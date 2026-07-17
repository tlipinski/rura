use crate::rura::Rura;
use crate::shell::builder::{CommandBuilder, UsrBinEnvCommandBuilder};
use crate::shell::exec::{Exec, SystemExec};
use crate::shell::output::ExecOutput;
use crate::shell::pipeline_runner::{PipelineRun, PipelineRunner, Stdin, StepFailure, StepOutput};
use log::{debug, info};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct CachedPipelineRunner {
    exec: Box<dyn Exec>,
    builder: Box<dyn CommandBuilder>,
    stdin: Stdin,
    cache: RefCell<Vec<(String, Arc<[u8]>, Duration)>>,
    use_cache: bool,
}

impl CachedPipelineRunner {
    pub fn new(shell: &str, stdin: Arc<[u8]>, use_cache: bool) -> Self {
        Self {
            exec: Box::new(SystemExec),
            builder: Box::new(UsrBinEnvCommandBuilder {
                shell: shell.into(),
            }),
            stdin: Stdin::new(stdin),
            cache: RefCell::new(vec![]),
            use_cache,
        }
    }
}

impl PipelineRunner for CachedPipelineRunner {
    fn run(&self, rura: &Rura) -> anyhow::Result<PipelineRun> {
        let mut cache = self.cache.borrow_mut();

        info!("executing: '{rura:?}'");

        if rura.is_empty() {
            return Ok(PipelineRun {
                stdin: self.stdin.clone(),
                steps: vec![],
                failure: None,
            });
        }

        // check how many subcommands are equal between command and cache
        // and truncate cache to only keep those subcommands
        for (i, (cached_command_str, _, _)) in cache.iter().enumerate() {
            if let Some(command_str) = rura.trimmed().get(i) {
                if cached_command_str != command_str {
                    cache.truncate(i);
                    break;
                }
            }
        }

        let mut steps: Vec<StepOutput> = vec![];

        for (i, step) in rura.trimmed().iter().enumerate() {
            if let Some((_, bytes, duration)) = cache.get(i) {
                steps.push(StepOutput::new(
                    step.clone(),
                    bytes.clone(),
                    *duration,
                    true,
                ));
                continue;
            }

            let current_stdin = if let Some(cached_bytes) = steps.last() {
                &cached_bytes.bytes
            } else {
                &self.stdin.bytes
            };

            let now_sub = SystemTime::now();

            let command = self.builder.build(step);
            let exec_output = self.exec.exec(command, current_stdin.clone())?;

            let exec_duration = now_sub.elapsed()?;

            match exec_output {
                ExecOutput::Ok(bytes) => {
                    if self.use_cache {
                        cache.push((step.clone(), bytes.clone(), exec_duration.clone()));
                    }
                    steps.push(StepOutput::new(step.clone(), bytes, exec_duration, false));
                }
                ExecOutput::Err(bytes, code) => {
                    debug!("  failed - aborting further execution");
                    return Ok(PipelineRun {
                        stdin: self.stdin.clone(),
                        steps,
                        failure: Some(StepFailure::new(step.clone(), bytes, code, exec_duration)),
                    });
                }
            }
        }

        // Keep all following items in cache since user might have called for instance
        // "until cursor prev" action so the full command might be still called
        // with all subcommands

        Ok(PipelineRun {
            stdin: self.stdin.clone(),
            steps,
            failure: None,
        })
    }

    fn update_stdin(&mut self, stdin: Arc<[u8]>) {
        self.stdin = Stdin::new(stdin);
        self.cache.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::builder::TestBuilder;
    use crate::shell::exec::MockExec;
    use itertools::Itertools;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::shell::exec::Exec;
    use crate::shell::pipeline_runner::PipelineRunner;

    fn cached_runner(exec: Box<dyn Exec>, stdin: Arc<[u8]>) -> CachedPipelineRunner {
        CachedPipelineRunner {
            exec,
            builder: Box::new(TestBuilder {}),
            stdin: Stdin::new(stdin),
            cache: RefCell::new(vec![]),
            use_cache: true,
        }
    }

    fn cache_entry(command: &str, stdin: &str) -> (String, Arc<[u8]>) {
        (command.into(), stdin.as_bytes().into())
    }

    fn cache_no_duration(entries: Vec<(String, Arc<[u8]>, Duration)>) -> Vec<(String, Arc<[u8]>)> {
        entries
            .into_iter()
            .map(|(command, bytes, _)| (command, bytes))
            .collect_vec()
    }

    fn as_strings(o: Vec<Arc<[u8]>>) -> Vec<String> {
        o.iter()
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect_vec()
    }

    #[test]
    fn test_run_empty_command_cached() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let result = runner.run(&vec![].into()).unwrap();

        assert_eq!(result.step_bytes(), vec![])
    }

    #[test]
    fn test_runner_calling_three_subcommands() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        assert_eq!(
            as_strings(result.step_bytes()),
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

        // all commands are cached
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![
                cache_entry("cmd1", "cmd1-output"),
                cache_entry("cmd2", "cmd2-output"),
                cache_entry("cmd3", "cmd3-output")
            ]
        );
    }

    #[test]
    fn test_runner_shorter_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();

        calls.borrow_mut().clear();

        // second run
        let result = runner.run(&vec!["cmd1".into()].into()).unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // only cmd1 is in the output
        assert_eq!(as_strings(result.step_bytes()), vec!["cmd1-output",]);

        // no calls since the command is cached
        assert_eq!(*calls.borrow(), vec![]);

        // all commands are still cached
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![
                cache_entry("cmd1", "cmd1-output"),
                cache_entry("cmd2", "cmd2-output"),
                cache_entry("cmd3", "cmd3-output")
            ]
        );
    }

    #[test]
    fn test_runner_extended_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into()].into())
            .unwrap();

        calls.borrow_mut().clear();

        // second run for less commands - keep whole cache
        let result = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into(), "cmd4".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        assert_eq!(
            as_strings(result.step_bytes()),
            vec!["cmd1-output", "cmd2-output", "cmd3-output", "cmd4-output",]
        );

        // only cmd3 is called since is's the only one not cached
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd3".into(), "cmd2-output".into()),
                ("cmd4".into(), "cmd3-output".into()),
            ]
        );

        // all commands are still cached
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![
                cache_entry("cmd1", "cmd1-output"),
                cache_entry("cmd2", "cmd2-output"),
                cache_entry("cmd3", "cmd3-output"),
                cache_entry("cmd4", "cmd4-output")
            ]
        );
    }

    #[test]
    fn test_runner_modified_in_the_middle() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();
        calls.borrow_mut().clear();

        // second run for shorter command - keep whole cache
        let result = runner
            .run(&vec!["cmd1".into(), "cmd2mod".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command
        assert_eq!(
            as_strings(result.step_bytes()),
            vec!["cmd1-output", "cmd2mod-output",]
        );

        // cmd2mod is called since it's modified
        assert_eq!(
            *calls.borrow(),
            vec![("cmd2mod".into(), "cmd1-output".into()),]
        );

        // cmd2 replaced with cmd2mod and cmd3 removed since it's invalid after modified command
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![
                cache_entry("cmd1", "cmd1-output"),
                cache_entry("cmd2mod", "cmd2mod-output"),
            ]
        );
    }

    #[test]
    fn test_runner_modified_in_the_middle_and_extended() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();
        calls.borrow_mut().clear();

        // second run for shorter command - keep whole cache
        let result = runner
            .run(&vec!["cmd1".into(), "cmd2mod".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command
        assert_eq!(
            as_strings(result.step_bytes()),
            vec!["cmd1-output", "cmd2mod-output", "cmd3-output"]
        );

        // cmd2mod is called since it's modified
        // cmd3 is also called because it was after modified command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd2mod".into(), "cmd1-output".into()),
                ("cmd3".into(), "cmd2mod-output".into()),
            ]
        );

        // cmd2 replaced with cmd2mod and cmd3 replaced with updated output
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![
                cache_entry("cmd1", "cmd1-output"),
                cache_entry("cmd2mod", "cmd2mod-output"),
                cache_entry("cmd3", "cmd3-output"),
            ]
        );
    }

    #[test]
    fn test_runner_errors() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command - breaks on first error
        assert_eq!(as_strings(result.step_bytes()), vec!["cmd1-output",]);
        assert_eq!(
            result.failure_bytes(),
            Some((Arc::from("cmd2err-output".as_bytes()), Some(1))),
        );

        // cmd2mod is called since it's modified
        // cmd3 is also called because it was after modified command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into()),
                ("cmd2err".into(), "cmd1-output".into()),
            ]
        );

        // only cmd1 is cached since it didn't fail
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![cache_entry("cmd1", "cmd1-output"),]
        );
    }

    #[test]
    fn test_runner_errors_clear_cache() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = cached_runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();
        calls.borrow_mut().clear();

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command - breaks on first error
        assert_eq!(as_strings(result.step_bytes()), vec!["cmd1-output",]);
        assert_eq!(
            result.failure_bytes(),
            Some((Arc::from("cmd2err-output".as_bytes()), Some(1))),
        );

        // cmd1 not called because it's cached
        assert_eq!(
            *calls.borrow(),
            vec![("cmd2err".into(), "cmd1-output".into()),]
        );

        // only cmd1 is cached since it didn't fail
        // entry for cmd3 is cleared because cmd2err failed before
        assert_eq!(
            cache_no_duration(runner.cache.borrow().clone()),
            vec![cache_entry("cmd1", "cmd1-output"),]
        );
    }
}

#[cfg(test)]
mod tests_no_cache {
    use super::*;
    use crate::shell::builder::TestBuilder;
    use crate::shell::exec::MockExec;
    use itertools::Itertools;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::shell::exec::Exec;
    use crate::shell::pipeline_runner::PipelineRunner;

    fn runner(exec: Box<dyn Exec>, stdin: Arc<[u8]>) -> CachedPipelineRunner {
        CachedPipelineRunner {
            exec,
            builder: Box::new(TestBuilder {}),
            stdin: Stdin::new(stdin),
            cache: RefCell::new(vec![]),
            use_cache: false,
        }
    }

    fn as_strings(o: Vec<Arc<[u8]>>) -> Vec<String> {
        o.iter()
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect_vec()
    }

    #[test]
    fn test_run_empty_command_cached() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let result = runner.run(&vec![].into()).unwrap();

        assert_eq!(result.step_bytes(), vec![])
    }

    #[test]
    fn test_runner_calling_three_subcommands() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        assert_eq!(
            as_strings(result.step_bytes()),
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

        assert_eq!(*runner.cache.borrow(), vec![]);
    }

    #[test]
    fn test_runner_shorter_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();

        calls.borrow_mut().clear();

        // second run
        let result = runner.run(&vec!["cmd1".into()].into()).unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // only cmd1 is in the output
        assert_eq!(as_strings(result.step_bytes()), vec!["cmd1-output",]);

        assert_eq!(*calls.borrow(), vec![("cmd1".into(), "stdin".into()),]);

        assert_eq!(*runner.cache.borrow(), vec![]);
    }

    #[test]
    fn test_runner_extended_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into()].into())
            .unwrap();

        calls.borrow_mut().clear();

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into(), "cmd4".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        assert_eq!(
            as_strings(result.step_bytes()),
            vec!["cmd1-output", "cmd2-output", "cmd3-output", "cmd4-output",]
        );

        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into()),
                ("cmd2".into(), "cmd1-output".into()),
                ("cmd3".into(), "cmd2-output".into()),
                ("cmd4".into(), "cmd3-output".into()),
            ]
        );

        assert_eq!(*runner.cache.borrow(), vec![]);
    }

    #[test]
    fn test_runner_modified_in_the_middle() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();
        calls.borrow_mut().clear();

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2mod".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command
        assert_eq!(
            as_strings(result.step_bytes()),
            vec!["cmd1-output", "cmd2mod-output",]
        );

        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into(),),
                ("cmd2mod".into(), "cmd1-output".into()),
            ]
        );

        assert_eq!(*runner.cache.borrow(), vec![]);
    }

    #[test]
    fn test_runner_modified_in_the_middle_and_extended() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();
        calls.borrow_mut().clear();

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2mod".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command
        assert_eq!(
            as_strings(result.step_bytes()),
            vec!["cmd1-output", "cmd2mod-output", "cmd3-output"]
        );

        // cmd2mod is called since it's modified
        // cmd3 is also called because it was after modified command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into()),
                ("cmd2mod".into(), "cmd1-output".into()),
                ("cmd3".into(), "cmd2mod-output".into()),
            ]
        );

        assert_eq!(*runner.cache.borrow(), vec![]);
    }

    #[test]
    fn test_runner_errors() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command - breaks on first error
        assert_eq!(as_strings(result.step_bytes()), vec!["cmd1-output",]);
        assert_eq!(
            result.failure_bytes(),
            Some((Arc::from("cmd2err-output".as_bytes()), Some(1))),
        );

        // cmd2mod is called since it's modified
        // cmd3 is also called because it was after modified command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into()),
                ("cmd2err".into(), "cmd1-output".into()),
            ]
        );

        assert_eq!(*runner.cache.borrow(), vec![]);
    }

    #[test]
    fn test_runner_errors_clear_cache() {
        let calls: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let runner = runner(Box::new(mock_exec), "stdin".as_bytes().into());

        let _init_run = runner
            .run(&vec!["cmd1".into(), "cmd2".into(), "cmd3".into()].into())
            .unwrap();
        calls.borrow_mut().clear();

        let result = runner
            .run(&vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()].into())
            .unwrap();

        assert_eq!(result.stdin.bytes, Arc::from("stdin".as_bytes()));

        // all outputs of the last called command - breaks on first error
        assert_eq!(as_strings(result.step_bytes()), vec!["cmd1-output",]);
        assert_eq!(
            result.failure_bytes(),
            Some((Arc::from("cmd2err-output".as_bytes()), Some(1))),
        );

        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".into()),
                ("cmd2err".into(), "cmd1-output".into()),
            ]
        );

        assert_eq!(*runner.cache.borrow(), vec![]);
    }
}
