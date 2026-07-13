use crate::shell::output::ExecOutput;
use anyhow::anyhow;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

pub trait Exec {
    fn exec(&self, command: Command, stdin: Arc<[u8]>) -> anyhow::Result<ExecOutput>;
}

pub struct SystemExec;

impl Exec for SystemExec {
    fn exec(&self, mut command: Command, stdin: Arc<[u8]>) -> anyhow::Result<ExecOutput> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn command [{command:?}]: {e}"))?;

        let mut child_stdin = child
            .stdin
            .take()
            .ok_or(anyhow!("Failed to take stdin handle"))?;

        thread::spawn(move || {
            let _ = child_stdin.write_all(&stdin);
        });

        match child.wait_with_output() {
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
