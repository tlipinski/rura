use crate::rura::RuraCommand;
use crate::shell::builder::{CommandBuilder, UsrBinEnvCommandBuilder};
use crate::shell::cmd_runner::{CmdResult, CmdRunner};
use crate::shell::exec::{Exec, SystemExec};
use crate::shell::output::Output;
use log::{debug, info};
use std::sync::Arc;
use std::time::SystemTime;

pub struct SplitCmdRunner {
    exec: Box<dyn Exec>,
    builder: Box<dyn CommandBuilder>,
    stdin: Arc<[u8]>,
}

impl SplitCmdRunner {
    pub fn new(shell: &str, stdin: Arc<[u8]>) -> Self {
        Self {
            exec: Box::new(SystemExec {}),
            builder: Box::new(UsrBinEnvCommandBuilder {
                shell: shell.into(),
            }),
            stdin,
        }
    }
}

impl CmdRunner for SplitCmdRunner {
    fn run(&self, command: &RuraCommand) -> anyhow::Result<CmdResult> {
        info!("executing commands: '{command:?}'");

        if command.is_empty() {
            return Ok(CmdResult {
                stdin: self.stdin.clone(),
                ok_outputs: vec![self.stdin.clone()],
                error_output: None,
            });
        }

        let now = SystemTime::now();

        let mut current_stdin = self.stdin.clone();

        let mut outputs: Vec<Arc<[u8]>> = Vec::new();

        for subcommand in command.trimmed().iter() {
            debug!("exec: '{subcommand}'");

            let now_sub = SystemTime::now();

            let cmd = self.builder.build(subcommand);
            let output = self.exec.exec(cmd, current_stdin.clone())?;

            debug!("t: {:?}", now_sub.elapsed()?);

            match output {
                Output::Ok(bytes) => {
                    current_stdin = bytes.clone();
                    outputs.push(bytes);
                }
                Output::Err(bytes, code) => {
                    debug!("failed - aborting further execution");
                    return Ok(CmdResult {
                        stdin: self.stdin.clone(),
                        ok_outputs: outputs,
                        error_output: Some((bytes, code)),
                    });
                }
            }
        }

        let elapsed = now.elapsed()?;
        debug!("total: {elapsed:?}");

        Ok(CmdResult {
            stdin: self.stdin.clone(),
            ok_outputs: outputs,
            error_output: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::builder::TestBuilder;
    use crate::shell::cmd_runner::CmdRunner;
    use crate::shell::exec::{Exec, MockExec};
    use crate::shell::split_runner::SplitCmdRunner;
    use itertools::Itertools;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    fn runner(exec: Box<dyn Exec>, stdin: Arc<[u8]>) -> SplitCmdRunner {
        SplitCmdRunner {
            exec,
            stdin,
            builder: Box::new(TestBuilder),
        }
    }

    fn as_strings(o: Vec<Arc<[u8]>>) -> Vec<String> {
        o.iter()
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect_vec()
    }

    #[test]
    fn test_run_empty_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), Arc::from("stdin".as_bytes()));

        let result = runner.run(&vec![].into()).unwrap();

        assert_eq!(result.stdin, Arc::from("stdin".as_bytes()));

        assert_eq!(as_strings(result.ok_outputs), vec!["stdin"]);

        assert_eq!(*calls.borrow(), vec![])
    }

    #[test]
    fn test_cmd_runner_calling_three_subcommands() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), Arc::from("stdin".as_bytes()));

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin, Arc::from("stdin".as_bytes()));

        assert_eq!(
            as_strings(result.ok_outputs),
            vec!["cmd1-output", "cmd2-output", "cmd3-output"]
        );

        // input for the command is the output of the previous command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into()),
                ("cmd2".into(), "cmd1-output".into()),
                ("cmd3".into(), "cmd2-output".into()),
            ]
        );
    }

    #[test]
    fn test_cmd_runner_errors() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), Arc::from("stdin".as_bytes()));

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin, Arc::from("stdin".as_bytes()));

        assert_eq!(as_strings(result.ok_outputs), vec!["cmd1-output",]);
        assert_eq!(
            result.error_output,
            Some((Arc::from("cmd2err-output".as_bytes()), Some(1)))
        );
    }
}
