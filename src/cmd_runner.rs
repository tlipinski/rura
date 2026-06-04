use anyhow::{Result, anyhow};
use itertools::Itertools;
use log::{debug, info};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::SystemTime;

pub trait CmdRunner {
    fn run(&mut self, command: Vec<String>) -> Result<CmdResult>;
}

pub struct CmdRunners;
impl CmdRunners {
    #[cfg(unix)]
    pub fn new(shell: &str, stdin: Vec<u8>, no_cache: bool) -> Box<dyn CmdRunner> {
        if no_cache {
            Box::new(SplitCmdRunner::new(shell, stdin))
        } else {
            Box::new(CachedCmdRunner::new(shell, stdin))
        }
    }

    #[cfg(windows)]
    pub fn new(shell: &str, stdin: Vec<u8>, _no_cache: bool) -> Box<dyn CmdRunner> {
        Box::new(SimpleCmdRunner::new(shell, stdin))
    }
}

pub struct SplitCmdRunner {
    exec: Box<dyn Exec>,
    stdin: Vec<u8>,
}

impl SplitCmdRunner {
    pub fn new(shell: &str, stdin: Vec<u8>) -> Self {
        Self {
            exec: Box::new(SystemExec {
                shell: shell.into(),
            }),
            stdin,
        }
    }
}

impl CmdRunner for SplitCmdRunner {
    fn run(&mut self, commands: Vec<String>) -> Result<CmdResult> {
        info!("executing commands: '{commands:?}'");

        let now = SystemTime::now();

        let mut current_stdin = self.stdin.clone();

        let mut output_opt: Option<Output> = None;

        for (i, command) in commands.iter().enumerate() {
            let command = command.trim();

            debug!("  executing sub command: '{command}'");

            let now_sub = SystemTime::now();

            let output = self.exec.exec(&command, current_stdin.clone())?;

            debug!("    time: {:?}, ", now_sub.elapsed()?);

            if output.ok {
                current_stdin = output.bytes.clone();
                output_opt = Some(output);
            } else {
                debug!("  failed - aborting further execution");
                return Ok(CmdResult {
                    command: commands.iter().join("|"),
                    output,
                    failed_subcommand: Some(i),
                });
            }
        }

        if let Some(output) = output_opt {
            let elapsed = now.elapsed()?;
            debug!("command exec took {elapsed:?}");

            Ok(CmdResult {
                command: commands.iter().join("|"),
                output,
                failed_subcommand: None,
            })
        } else {
            Ok(CmdResult {
                command: "".into(),
                output: Output::ok_stdin(self.stdin.clone()),
                failed_subcommand: None,
            })
        }
    }
}

pub struct CachedCmdRunner {
    exec: Box<dyn Exec>,
    stdin: Vec<u8>,
    cache: Vec<Output>,
}

impl CachedCmdRunner {
    pub fn new(shell: &str, stdin: Vec<u8>) -> Self {
        Self {
            exec: Box::new(SystemExec {
                shell: shell.into(),
            }),
            stdin,
            cache: vec![],
        }
    }
}

impl CmdRunner for CachedCmdRunner {
    fn run(&mut self, commands: Vec<String>) -> Result<CmdResult> {
        info!("executing commands: '{commands:?}'");

        if commands.is_empty() {
            return Ok(CmdResult {
                command: "".into(),
                output: Output::ok_stdin(self.stdin.clone()),
                failed_subcommand: None,
            });
        }

        let now = SystemTime::now();

        let mut skip_cache = false;

        for (i, command) in commands.iter().enumerate() {
            let command = command.trim();
            let cached = self.cache.get(i);

            if let Some(c) = cached
                && !skip_cache
                && c.command == Some(command.into())
            {
                debug!("  using cached output for command: '{command:?}'");
                continue;
            }

            let current_stdin;

            if i > 0 {
                if let Some(c) = self.cache.get(i - 1) {
                    current_stdin = c.bytes.clone();
                } else {
                    current_stdin = self.stdin.clone();
                }
            } else {
                current_stdin = self.stdin.clone();
            }

            // starting from the first non-cached command, we don't want to use cache for any further commands
            skip_cache = true;
            self.cache.truncate(i);

            debug!("  executing sub command: '{command}'");

            let now_sub = SystemTime::now();

            let output = self.exec.exec(&command, current_stdin.clone())?;

            debug!("    time: {:?}, ", now_sub.elapsed()?);

            if output.ok {
                self.cache.push(output);
            } else {
                debug!("  failed - aborting further execution");
                return Ok(CmdResult {
                    command: commands.iter().join("|"),
                    output,
                    failed_subcommand: Some(i),
                });
            }
        }

        // Keep all following items in cache since user might have called for instance
        // "until cursor prev" action so the full command might be still called
        // with all subcommands

        let cached_commands = self
            .cache
            .iter()
            .map(|c| c.command.clone())
            .flatten()
            .collect_vec();

        debug!("  cached commands: {:?}", cached_commands);

        let elapsed = now.elapsed()?;
        debug!("  command exec took {elapsed:?}");

        Ok(CmdResult {
            command: commands.iter().join("|"),
            output: self.cache.get(commands.len() - 1).unwrap().clone(),
            failed_subcommand: None,
        })
    }
}

#[allow(dead_code)]
pub struct SimpleCmdRunner {
    exec: Box<dyn Exec>,
    stdin: Vec<u8>,
}

impl SimpleCmdRunner {
    #[allow(dead_code)]
    pub fn new(shell: &str, stdin: Vec<u8>) -> Self {
        Self {
            exec: Box::new(SystemExec {
                shell: shell.into(),
            }),
            stdin,
        }
    }
}

impl CmdRunner for SimpleCmdRunner {
    fn run(&mut self, commands: Vec<String>) -> Result<CmdResult> {
        if commands.is_empty() {
            return Ok(CmdResult {
                command: "".into(),
                output: Output::ok_stdin(self.stdin.clone()),
                failed_subcommand: None,
            });
        }

        let command = commands.clone().join("|");

        let now = SystemTime::now();

        info!("executing command: '{command}'");

        let output = self.exec.exec(&command, self.stdin.clone())?;

        let elapsed = now.elapsed()?;
        debug!("command exec took {elapsed:?}");

        Ok(CmdResult {
            command: commands.iter().join("|"),
            output,
            failed_subcommand: None,
        })
    }
}

trait Exec {
    fn exec(&self, command: &str, stdin: Vec<u8>) -> Result<Output>;
}

struct SystemExec {
    shell: String,
}

impl Exec for SystemExec {
    fn exec(&self, command: &str, stdin: Vec<u8>) -> Result<Output> {
        let mut cmd = build_command(&self.shell, command);

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn command [{cmd:?}]: {e}"))?;

        let mut child_stdin = child
            .stdin
            .take()
            .ok_or(anyhow!("Failed to take stdin handle"))?;

        thread::spawn(move || {
            let _ = child_stdin.write_all(&stdin);
        });

        if let Ok(output) = child.wait_with_output() {
            if output.status.success() {
                Ok(Output::ok_command(&command, output.stdout))
            } else {
                Ok(Output::err_command(
                    &command,
                    output.stderr,
                    output.status.code(),
                ))
            }
        } else {
            Ok(Output::err_command(
                &command,
                "Failed to execute command".bytes().collect_vec(),
                None,
            ))
        }
    }
}

#[cfg(unix)]
fn build_command(shell: &str, command: &str) -> Command {
    let mut cmd = Command::new("/usr/bin/env");
    cmd.args([shell, "-c", command]);
    cmd
}

#[cfg(windows)]
fn build_command(shell: &str, command: &str) -> Command {
    let mut cmd = Command::new(shell);
    cmd.env("NO_COLOR", "1");
    cmd.arg("-NonInteractive");
    cmd.arg("-NoProfile");
    cmd.arg("-NoLogo");
    cmd.args(["/C", &command]);
    cmd
}

pub struct CmdResult {
    pub command: String,
    pub output: Output,
    pub failed_subcommand: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub command: Option<String>,
    pub lines: Vec<String>,
    pub bytes: Vec<u8>,
    pub status_code: Option<i32>,
    pub ok: bool,
}

impl Output {
    pub fn ok_command(command: &str, bytes: Vec<u8>) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(&String::from_utf8_lossy(&bytes)),
            bytes,
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_command(command: &str, bytes: Vec<u8>, status_code: Option<i32>) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(&String::from_utf8_lossy(&bytes)),
            bytes,
            status_code,
            ok: false,
        }
    }

    pub fn ok_stdin(bytes: Vec<u8>) -> Self {
        Self {
            command: None,
            lines: Self::lines(&String::from_utf8_lossy(&bytes)),
            bytes,
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_stdin(bytes: Vec<u8>) -> Self {
        Self {
            command: None,
            lines: Self::lines(&String::from_utf8_lossy(&bytes)),
            bytes,
            status_code: None,
            ok: false,
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    fn lines(input: &str) -> Vec<String> {
        input.lines().map(|a| a.into()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct MockExec {
        calls: Rc<RefCell<Vec<(String, String)>>>,
    }

    impl Exec for MockExec {
        fn exec(&self, command: &str, stdin: Vec<u8>) -> Result<Output> {
            self.calls.borrow_mut().push((
                command.into(),
                String::from_utf8_lossy(stdin.as_slice()).into(),
            ));
            if command.ends_with("err") {
                Ok(Output::err_command_str(
                    command,
                    &format!("{}-output", command),
                    Some(1),
                ))
            } else {
                Ok(Output::ok_command_str(
                    command,
                    &format!("{}-output", command),
                ))
            }
        }
    }

    mod simple_runner {
        use crate::cmd_runner::tests::MockExec;
        use crate::cmd_runner::{CmdRunner, Exec, Output, SimpleCmdRunner};
        use std::cell::RefCell;
        use std::rc::Rc;

        fn simple_runner(exec: Box<dyn Exec>, stdin: Vec<u8>) -> SimpleCmdRunner {
            SimpleCmdRunner { exec, stdin }
        }

        #[test]
        fn test_ok_command() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = simple_runner(Box::new(mock_exec), "stdin".into());

            let result = runner.run(vec!["echo hello".into()]).unwrap();

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
            let mut runner = simple_runner(Box::new(mock_exec), "stdin".into());

            let result = runner.run(vec![]).unwrap();

            assert_eq!(result.output, Output::ok_stdin_str("stdin"))
        }
    }

    mod split_runner {
        use crate::cmd_runner::tests::MockExec;
        use crate::cmd_runner::{CmdRunner, Exec, Output, SplitCmdRunner};
        use std::cell::RefCell;
        use std::rc::Rc;

        fn runner(exec: Box<dyn Exec>, stdin: Vec<u8>) -> SplitCmdRunner {
            SplitCmdRunner { exec, stdin }
        }

        #[test]
        fn test_run_empty_command() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = runner(Box::new(mock_exec), "stdin".into());

            let result = runner.run(vec![]).unwrap();

            assert_eq!(result.output, Output::ok_stdin_str("stdin"));

            assert_eq!(*calls.borrow(), vec![])
        }

        #[test]
        fn test_cmd_runner_calling_three_subcommands() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = runner(Box::new(mock_exec), "stdin".into());

            let result = runner
                .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()])
                .unwrap();

            // output of the last called command
            assert_eq!(result.output, Output::ok_command_str("cmd3", "cmd3-output"));

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
            let mut runner = runner(Box::new(mock_exec), "stdin".into());

            let result = runner
                .run(vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()])
                .unwrap();

            // output of the last called command
            assert_eq!(
                result.output,
                Output::err_command_str("cmd2err", "cmd2err-output", Some(1))
            );
        }
    }

    mod cached_runner {
        use crate::cmd_runner::tests::MockExec;
        use crate::cmd_runner::{CachedCmdRunner, CmdRunner, Exec, Output};
        use std::cell::RefCell;
        use std::rc::Rc;

        fn cached_runner(exec: Box<dyn Exec>, stdin: Vec<u8>) -> CachedCmdRunner {
            CachedCmdRunner {
                exec,
                stdin,
                cache: vec![],
            }
        }

        #[test]
        fn test_run_empty_command_cached() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let result = runner.run(vec![]).unwrap();

            assert_eq!(result.output, Output::ok_stdin_str("stdin"))
        }

        #[test]
        fn test_cmd_runner_calling_three_subcommands() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let result = runner
                .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()])
                .unwrap();

            // output of the last called command
            assert_eq!(result.output, Output::ok_command_str("cmd3", "cmd3-output"));

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
                runner.cache,
                vec![
                    Output::ok_command_str("cmd1", "cmd1-output"),
                    Output::ok_command_str("cmd2", "cmd2-output"),
                    Output::ok_command_str("cmd3", "cmd3-output")
                ]
            );
        }

        #[test]
        fn test_cmd_runner_shorter_command() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let _init_run = runner
                .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()])
                .unwrap();

            calls.borrow_mut().clear();

            // second run
            let result = runner.run(vec!["cmd1".into()]).unwrap();

            // output of the last called command - cmd3
            assert_eq!(result.output, Output::ok_command_str("cmd1", "cmd1-output"));

            // no calls since the command is cached
            assert_eq!(*calls.borrow(), vec![]);

            // all commands are still cached
            assert_eq!(
                runner.cache,
                vec![
                    Output::ok_command_str("cmd1", "cmd1-output"),
                    Output::ok_command_str("cmd2", "cmd2-output"),
                    Output::ok_command_str("cmd3", "cmd3-output")
                ]
            );
        }

        #[test]
        fn test_cmd_runner_extended_command() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let _init_run = runner.run(vec!["cmd1".into(), "cmd2".into()]).unwrap();

            calls.borrow_mut().clear();

            // second run for less commands - keep whole cache
            let result = runner
                .run(vec![
                    "cmd1".into(),
                    "cmd2".into(),
                    "cmd3".into(),
                    "cmd4".into(),
                ])
                .unwrap();

            // output of the last called command
            assert_eq!(result.output, Output::ok_command_str("cmd4", "cmd4-output"));

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
                runner.cache,
                vec![
                    Output::ok_command_str("cmd1", "cmd1-output"),
                    Output::ok_command_str("cmd2", "cmd2-output"),
                    Output::ok_command_str("cmd3", "cmd3-output"),
                    Output::ok_command_str("cmd4", "cmd4-output")
                ]
            );
        }

        #[test]
        fn test_cmd_runner_modified_in_the_middle() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let _init_run = runner
                .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()])
                .unwrap();
            calls.borrow_mut().clear();

            // second run for shorter command - keep whole cache
            let result = runner.run(vec!["cmd1".into(), "cmd2mod".into()]).unwrap();

            // output of the last called command
            assert_eq!(
                result.output,
                Output::ok_command_str("cmd2mod", "cmd2mod-output")
            );

            // cmd2mod is called since it's modified
            assert_eq!(
                *calls.borrow(),
                vec![("cmd2mod".into(), "cmd1-output".into()),]
            );

            // cmd2 replaced with cmd2mod and cmd3 removed since it's invalid after modified command
            assert_eq!(
                runner.cache,
                vec![
                    Output::ok_command_str("cmd1", "cmd1-output"),
                    Output::ok_command_str("cmd2mod", "cmd2mod-output"),
                ]
            );
        }

        #[test]
        fn test_cmd_runner_modified_in_the_middle_and_extended() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let _init_run = runner
                .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()])
                .unwrap();
            calls.borrow_mut().clear();

            // second run for shorter command - keep whole cache
            let result = runner
                .run(vec!["cmd1".into(), "cmd2mod".into(), "cmd3".into()])
                .unwrap();

            // output of the last called command
            assert_eq!(result.output, Output::ok_command_str("cmd3", "cmd3-output"));

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
                runner.cache,
                vec![
                    Output::ok_command_str("cmd1", "cmd1-output"),
                    Output::ok_command_str("cmd2mod", "cmd2mod-output"),
                    Output::ok_command_str("cmd3", "cmd3-output"),
                ]
            );
        }

        #[test]
        fn test_cmd_runner_errors() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let result = runner
                .run(vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()])
                .unwrap();

            // output of the last called command
            assert_eq!(
                result.output,
                Output::err_command_str("cmd2err", "cmd2err-output", Some(1))
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
                runner.cache,
                vec![Output::ok_command_str("cmd1", "cmd1-output"),]
            );
        }

        #[test]
        fn test_cmd_runner_errors_clear_cache() {
            let calls = Rc::new(RefCell::new(vec![]));
            let mock_exec = MockExec {
                calls: calls.clone(),
            };
            let mut runner = cached_runner(Box::new(mock_exec), "stdin".into());

            let _init_run = runner
                .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()])
                .unwrap();
            calls.borrow_mut().clear();

            let result = runner
                .run(vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()])
                .unwrap();

            assert_eq!(
                result.output,
                Output::err_command_str("cmd2err", "cmd2err-output", Some(1))
            );

            // cmd1 not called because it's cached
            assert_eq!(
                *calls.borrow(),
                vec![("cmd2err".into(), "cmd1-output".into()),]
            );

            // only cmd1 is cached since it didn't fail
            // entry for cmd3 is cleared because cmd2err failed before
            assert_eq!(
                runner.cache,
                vec![Output::ok_command_str("cmd1", "cmd1-output"),]
            );
        }
    }
}

#[cfg(test)]
impl Output {
    pub fn ok_command_str(command: &str, str: &str) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_command_str(command: &str, str: &str, status_code: Option<i32>) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code,
            ok: false,
        }
    }

    pub fn ok_stdin_str(str: &str) -> Self {
        Self {
            command: None,
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_stdin_str(str: &str) -> Self {
        Self {
            command: None,
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code: None,
            ok: false,
        }
    }
}
