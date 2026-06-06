use crate::rura::RuraCommand;
use crate::shell::builder::{CommandBuilder, UsrBinEnvCommandBuilder};
use crate::shell::cmd_runner::{CmdResult, CmdRunner};
use crate::shell::exec::{CommandOutput, Exec, SystemExec};
use crate::shell::output::Output;
use log::{debug, info};
use std::time::SystemTime;

pub struct SplitCmdRunner {
    exec: Box<dyn Exec>,
    builder: Box<dyn CommandBuilder>,
    stdin: Vec<u8>,
}

impl SplitCmdRunner {
    pub fn new(shell: &str, stdin: Vec<u8>) -> Self {
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

        let now = SystemTime::now();

        let mut current_stdin = self.stdin.clone();

        let mut output_opt: Option<(String, Vec<u8>)> = None;

        for (i, subcommand) in command.trimmed().iter().enumerate() {
            debug!("  executing sub command: '{subcommand}'");

            let now_sub = SystemTime::now();

            let cmd = self.builder.build(subcommand);
            let output = self.exec.exec(cmd, current_stdin.clone())?;

            debug!("    time: {:?}, ", now_sub.elapsed()?);

            match output {
                CommandOutput::Stdout(bytes) => {
                    current_stdin = bytes.clone();
                    output_opt = Some((subcommand.clone(), bytes));
                }
                CommandOutput::Stderr(bytes, code) => {
                    debug!("  failed - aborting further execution");
                    return Ok(CmdResult {
                        output: Output::err(bytes, code),
                        failed_subcommand: Some(i),
                    });
                }
            }
        }

        if let Some((_, output)) = output_opt {
            let elapsed = now.elapsed()?;
            debug!("command exec took {elapsed:?}");

            Ok(CmdResult {
                output: Output::ok(output),
                failed_subcommand: None,
            })
        } else {
            Ok(CmdResult {
                output: Output::ok(self.stdin.clone()),
                failed_subcommand: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::builder::TestBuilder;
    use crate::shell::cmd_runner::CmdRunner;
    use crate::shell::exec::{Exec, MockExec};
    use crate::shell::output::Output;
    use crate::shell::split_runner::SplitCmdRunner;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn runner(exec: Box<dyn Exec>, stdin: Vec<u8>) -> SplitCmdRunner {
        SplitCmdRunner {
            exec,
            stdin,
            builder: Box::new(TestBuilder),
        }
    }

    #[test]
    fn test_run_empty_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".into());

        let result = runner.run(&vec![].into()).unwrap();

        assert_eq!(result.output, Output::ok_str("stdin"));

        assert_eq!(*calls.borrow(), vec![])
    }

    #[test]
    fn test_cmd_runner_calling_three_subcommands() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".into());

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();

        // output of the last called command
        assert_eq!(result.output, Output::ok_str("cmd3-output"));

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
        let runner = runner(Box::new(mock_exec), "stdin".into());

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()].into())
            .unwrap();

        // output of the last called command
        assert_eq!(result.output, Output::err_str("cmd2err-output", Some(1)));
    }
}
