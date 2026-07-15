use crate::rura::Rura;
use crate::shell::builder::CommandBuilder;
use crate::shell::exec::Exec;
use crate::shell::output::ExecOutput;
use crate::shell::pipeline_runner::{PipelineRun, PipelineRunner, Stdin, StepFailure, StepOutput};
use log::info;
use std::sync::Arc;
use std::time::SystemTime;

#[allow(dead_code)]
pub struct SimplePipelineRunner {
    exec: Box<dyn Exec>,
    builder: Box<dyn CommandBuilder>,
    stdin: Stdin,
}

impl SimplePipelineRunner {
    #[cfg(windows)]
    pub fn new(shell: &str, stdin: Arc<[u8]>) -> Self {
        use crate::shell::builder::PwshCommandBuilder;
        use crate::shell::exec::SystemExec;
        SimplePipelineRunner {
            exec: Box::new(SystemExec),
            builder: Box::new(PwshCommandBuilder {
                shell: shell.into(),
            }),
            stdin: Stdin {
                bytes: stdin.clone(),
                lines: stdin.iter().filter(|c| **c == b'\n').count(),
            },
        }
    }
}

impl PipelineRunner for SimplePipelineRunner {
    fn run(&self, rura: &Rura) -> anyhow::Result<PipelineRun> {
        info!("executing: '{rura:?}'");

        if rura.is_empty() {
            return Ok(PipelineRun {
                stdin: self.stdin.clone(),
                steps: vec![],
                failure: None,
            });
        }

        let now = SystemTime::now();

        let command = self.builder.build(&rura.to_string());
        let exec_output = self.exec.exec(command, self.stdin.bytes.clone())?;

        let elapsed = now.elapsed()?;

        match exec_output {
            ExecOutput::Ok(bytes) => Ok(PipelineRun {
                stdin: self.stdin.clone(),
                steps: vec![StepOutput::new(rura.to_string(), bytes, Some(elapsed))],
                failure: None,
            }),
            ExecOutput::Err(bytes, code) => Ok(PipelineRun {
                stdin: self.stdin.clone(),
                steps: vec![],
                failure: Some(StepFailure {
                    command: rura.to_string(),
                    bytes,
                    code,
                    duration: elapsed,
                }),
            }),
        }
    }

    fn update_stdin(&mut self, stdin: Arc<[u8]>) {
        self.stdin = Stdin::new(stdin);
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::builder::TestBuilder;
    use crate::shell::exec::Exec;
    use crate::shell::exec::MockExec;
    use crate::shell::pipeline_runner::{PipelineRunner, Stdin};
    use crate::shell::simple_runner::SimplePipelineRunner;
    use itertools::Itertools;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    fn simple_runner(exec: Box<dyn Exec>, stdin: Arc<[u8]>) -> SimplePipelineRunner {
        SimplePipelineRunner {
            exec,
            stdin: Stdin::new(stdin),
            builder: Box::new(TestBuilder {}),
        }
    }

    fn as_strings(o: Vec<Arc<[u8]>>) -> Vec<String> {
        o.iter()
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect_vec()
    }

    #[test]
    fn test_ok_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = simple_runner(Box::new(mock_exec), Arc::from("stdin".as_bytes()));

        let result = runner.run(&"echo hello".into()).unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));
        assert_eq!(as_strings(result.step_bytes()), vec!["echo hello-output"])
    }

    #[test]
    fn test_run_empty_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = simple_runner(Box::new(mock_exec), Arc::from("stdin".as_bytes()));

        let result = runner.run(&vec![].into()).unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));
        assert_eq!(as_strings(result.step_bytes()), Vec::<String>::new())
    }
}
