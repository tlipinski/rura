use crate::rura::RuraCommand;
use crate::shell::builder::CommandBuilder;
use crate::shell::cmd_runner::{CmdResult, CmdRunner};
use crate::shell::exec::{CommandOutput, Exec};
use crate::shell::output::Output;
use log::{debug, info};
use std::time::SystemTime;

#[allow(dead_code)]
pub struct SimpleCmdRunner {
    exec: Box<dyn Exec>,
    builder: Box<dyn CommandBuilder>,
    stdin: Vec<u8>,
}

impl SimpleCmdRunner {
    #[cfg(windows)]
    pub fn new(shell: &str, stdin: Vec<u8>) -> Self {
        use crate::shell::builder::PwshCommandBuilder;
        use crate::shell::exec::SystemExec;
        SimpleCmdRunner {
            exec: Box::new(SystemExec),
            builder: Box::new(PwshCommandBuilder {
                shell: shell.into(),
            }),
            stdin,
        }
    }
}

impl CmdRunner for SimpleCmdRunner {
    fn run(&self, command: &RuraCommand) -> anyhow::Result<CmdResult> {
        info!("executing: '{command:?}'");

        if command.is_empty() {
            return Ok(CmdResult {
                output: Output::ok_stdin(self.stdin.clone()),
                failed_subcommand: None,
            });
        }

        let now = SystemTime::now();

        let cmd = self.builder.build(&command.to_string());
        let output = self.exec.exec(cmd, self.stdin.clone())?;

        let elapsed = now.elapsed()?;
        debug!("command exec took {elapsed:?}");

        match output {
            CommandOutput::Stdout(bytes) => Ok(CmdResult {
                output: Output::ok_command(&command.to_string(), bytes),
                failed_subcommand: None,
            }),
            CommandOutput::Stderr(bytes, code) => Ok(CmdResult {
                output: Output::err_command(&command.to_string(), bytes, code),
                failed_subcommand: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::builder::TestBuilder;
    use crate::shell::cmd_runner::CmdRunner;
    use crate::shell::exec::Exec;
    use crate::shell::exec::MockExec;
    use crate::shell::output::Output;
    use crate::shell::simple_runner::SimpleCmdRunner;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn simple_runner(exec: Box<dyn Exec>, stdin: Vec<u8>) -> SimpleCmdRunner {
        SimpleCmdRunner {
            exec,
            stdin,
            builder: Box::new(TestBuilder {}),
        }
    }

    #[test]
    fn test_ok_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = simple_runner(Box::new(mock_exec), "stdin".into());

        let result = runner.run(&"echo hello".into()).unwrap();

        assert_eq!(
            result.output,
            Output::ok_command_str("echo hello", "echo hello-output")
        )
    }

    #[test]
    fn test_run_empty_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = simple_runner(Box::new(mock_exec), "stdin".into());

        let result = runner.run(&vec![].into()).unwrap();

        assert_eq!(result.output, Output::ok_stdin_str("stdin"))
    }
}
