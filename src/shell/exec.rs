use crate::shell::cancel::Canceller;
use crate::shell::output::ExecOutput;
use anyhow::anyhow;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

pub trait Exec {
    fn exec(&self, command: Command, stdin: Arc<[u8]>) -> anyhow::Result<ExecOutput>;
}

pub struct SystemExec {
    pub canceller: Arc<Canceller>,
}

impl Exec for SystemExec {
    fn exec(&self, mut command: Command, stdin: Arc<[u8]>) -> anyhow::Result<ExecOutput> {
        if self.canceller.is_cancelled() {
            return Ok(ExecOutput::Cancelled);
        }

        // Own process group allows killing the shell together with its subprocesses on cancel
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn command [{command:?}]: {e}"))?;

        self.canceller.register(child.id());

        let mut child_stdin = child
            .stdin
            .take()
            .ok_or(anyhow!("Failed to take stdin handle"))?;

        thread::spawn(move || {
            let _ = child_stdin.write_all(&stdin);
        });

        let result = child.wait_with_output();

        self.canceller.unregister();

        if self.canceller.is_cancelled() {
            return Ok(ExecOutput::Cancelled);
        }

        match result {
            Ok(output) => {
                if output.status.success() {
                    Ok(ExecOutput::Ok(Arc::from(output.stdout)))
                } else {
                    // failed successfully!
                    Ok(ExecOutput::Err(
                        Arc::from(output.stderr),
                        output.status.code(),
                    ))
                }
            }
            Err(e) => Err(anyhow!("Failed to execute command '{command:?}': {e}")),
        }
    }
}

#[cfg(test)]
pub struct MockExec {
    pub calls: std::rc::Rc<std::cell::RefCell<Vec<(String, String)>>>,
}

#[cfg(test)]
impl Exec for MockExec {
    fn exec(&self, command: Command, stdin: Arc<[u8]>) -> anyhow::Result<ExecOutput> {
        let program = command.get_program().to_string_lossy().into_owned();
        self.calls
            .borrow_mut()
            .push((program.clone(), String::from_utf8_lossy(&stdin).into()));
        if program.ends_with("err") {
            Ok(ExecOutput::Err(
                Arc::from(format!("{}-output", program).into_bytes()),
                Some(1),
            ))
        } else {
            Ok(ExecOutput::Ok(Arc::from(
                format!("{}-output", program).into_bytes(),
            )))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[test]
    fn cancel_kills_running_command_with_subprocesses() {
        let canceller = Arc::new(Canceller::default());
        let exec = SystemExec {
            canceller: canceller.clone(),
        };
        canceller.begin(canceller.cancel());

        let c = canceller.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            c.cancel();
        });

        let start = Instant::now();
        // `; echo` keeps the shell from exec-ing sleep, so sleep runs as its subprocess
        let output = exec.exec(sh("sleep 10; echo done"), Arc::from([])).unwrap();

        assert!(matches!(output, ExecOutput::Cancelled));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancelled_run_does_not_spawn_command() {
        let canceller = Arc::new(Canceller::default());
        let exec = SystemExec {
            canceller: canceller.clone(),
        };
        canceller.begin(canceller.cancel());
        canceller.cancel();

        let output = exec
            .exec(Command::new("/nonexistent/command"), Arc::from([]))
            .unwrap();

        assert!(matches!(output, ExecOutput::Cancelled));
    }

    #[test]
    fn not_cancelled_command_completes() {
        let canceller = Arc::new(Canceller::default());
        let exec = SystemExec {
            canceller: canceller.clone(),
        };
        canceller.begin(canceller.cancel());

        let output = exec.exec(sh("cat"), Arc::from("abc".as_bytes())).unwrap();

        assert!(matches!(output, ExecOutput::Ok(bytes) if &*bytes == b"abc"));
    }
}
