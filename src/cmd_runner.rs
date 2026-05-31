use anyhow::{Result, anyhow};
use itertools::Itertools;
use log::{debug, info};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::SystemTime;

pub trait CmdRunner {
    fn run(&mut self, command: Vec<String>, stdin: &str) -> Result<CmdResult>;
}

pub struct CmdRunners;
impl CmdRunners {
    pub fn new(shell: &str, use_cache: bool) -> Box<dyn CmdRunner> {
        if use_cache {
            Box::new(CachedCmdRunner::new(shell))
        } else {
            Box::new(SimpleCmdRunner::new(shell))
        }
    }
}

pub struct CachedCmdRunner {
    exec: Box<dyn Exec>,
    cache: Vec<Output>,
}

impl CachedCmdRunner {
    pub fn new(shell: &str) -> Self {
        Self {
            exec: Box::new(SystemExec {
                shell: shell.into(),
            }),
            cache: vec![],
        }
    }

    #[cfg(test)]
    fn with_exec(exec: Box<dyn Exec>) -> Self {
        Self {
            exec,
            cache: vec![],
        }
    }
}

impl CmdRunner for CachedCmdRunner {
    fn run(&mut self, commands: Vec<String>, stdin: &str) -> Result<CmdResult> {
        info!("executing commands: '{commands:?}'");

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
                    current_stdin = stdin.as_bytes().to_vec();
                }
            } else {
                current_stdin = stdin.as_bytes().to_vec();
            }

            // starting from the first non-cached command, we don't want to use cache for any further commands
            skip_cache = true;
            self.cache.truncate(i);

            debug!("  executing sub command: '{command}'");

            let output = self.exec.exec(&command, current_stdin.clone())?;

            if output.ok {
                self.cache.push(output);
            } else {
                debug!("  failed - aborting further execution");
                return Ok(CmdResult {
                    command: commands.iter().join("|"),
                    output,
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

        debug!("cached commands: {:?}", cached_commands);

        let elapsed = now.elapsed()?;
        debug!("command exec took {elapsed:?}");

        Ok(CmdResult {
            command: commands.iter().join("|"),
            output: self.cache.get(commands.len() - 1).unwrap().clone(),
        })
    }
}

pub struct SimpleCmdRunner {
    exec: Box<dyn Exec>,
}

impl SimpleCmdRunner {
    pub fn new(shell: &str) -> Self {
        Self {
            exec: Box::new(SystemExec {
                shell: shell.into(),
            }),
        }
    }
    #[cfg(test)]
    fn with_exec(exec: Box<dyn Exec>) -> Self {
        Self { exec }
    }
}

impl CmdRunner for SimpleCmdRunner {
    fn run(&mut self, commands: Vec<String>, stdin: &str) -> Result<CmdResult> {
        let command = commands.clone().join("|");

        let now = SystemTime::now();

        info!("executing command: '{command}'");

        let output = self.exec.exec(&command, stdin.as_bytes().to_vec())?;

        let elapsed = now.elapsed()?;
        debug!("command exec took {elapsed:?}");

        Ok(CmdResult {
            command: commands.iter().join("|"),
            output,
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
        let mut cmd = Command::new("/usr/bin/env");
        cmd.args([&self.shell, "-c", command]);

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
                let stdout = output.stdout.as_slice();
                let str = String::from_utf8_lossy(stdout);
                Ok(Output::ok_command(&command, &str))
            } else {
                let stderr = output.stderr.as_slice();
                let str = String::from_utf8_lossy(stderr);
                Ok(Output::err_command(&command, &str, output.status.code()))
            }
        } else {
            Ok(Output::err_command(
                &command,
                "Failed to execute command",
                None,
            ))
        }
    }
}

pub struct CmdResult {
    pub command: String,
    pub output: Output,
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
    pub fn ok_command(command: &str, str: &str) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_command(command: &str, str: &str, status_code: Option<i32>) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code,
            ok: false,
        }
    }

    pub fn ok_stdin(str: &str) -> Self {
        Self {
            command: None,
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_stdin(str: &str) -> Self {
        Self {
            command: None,
            lines: Self::lines(str),
            bytes: str.as_bytes().to_vec(),
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
        calls: Rc<RefCell<Vec<(String, Vec<u8>)>>>,
    }

    impl Exec for MockExec {
        fn exec(&self, command: &str, stdin: Vec<u8>) -> Result<Output> {
            self.calls.borrow_mut().push((command.into(), stdin));
            if command.ends_with("err") {
                Ok(Output::err_command(
                    command,
                    &format!("{}-output", command),
                    Some(1),
                ))
            } else {
                Ok(Output::ok_command(command, &format!("{}-output", command)))
            }
        }
    }

    #[test]
    fn test_ok_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = SimpleCmdRunner::with_exec(Box::new(mock_exec));

        let result = runner.run(vec!["echo hello".into()], "").unwrap();

        assert_eq!(
            result.output,
            Output::ok_command("echo hello", "echo hello-output")
        )
    }

    #[test]
    fn test_cmd_runner_calling_three_subcommands() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let result = runner
            .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()], "stdin")
            .unwrap();

        // output of the last called command
        assert_eq!(result.output, Output::ok_command("cmd3", "cmd3-output"));

        // input for the command is the output of the previous command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".as_bytes().to_vec()),
                ("cmd2".into(), "cmd1-output".as_bytes().to_vec()),
                ("cmd3".into(), "cmd2-output".as_bytes().to_vec()),
            ]
        );

        // all commands are cached
        assert_eq!(
            runner.cache,
            vec![
                Output::ok_command("cmd1", "cmd1-output"),
                Output::ok_command("cmd2", "cmd2-output"),
                Output::ok_command("cmd3", "cmd3-output")
            ]
        );
    }

    #[test]
    fn test_cmd_runner_shorter_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let _init_run = runner
            .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()], "stdin")
            .unwrap();

        calls.borrow_mut().clear();

        // second run
        let result = runner.run(vec!["cmd1".into()], "stdin").unwrap();

        // output of the last called command - cmd3
        assert_eq!(result.output, Output::ok_command("cmd1", "cmd1-output"));

        // no calls since the command is cached
        assert_eq!(*calls.borrow(), vec![]);

        // all commands are still cached
        assert_eq!(
            runner.cache,
            vec![
                Output::ok_command("cmd1", "cmd1-output"),
                Output::ok_command("cmd2", "cmd2-output"),
                Output::ok_command("cmd3", "cmd3-output")
            ]
        );
    }

    #[test]
    fn test_cmd_runner_extended_command() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let _init_run = runner
            .run(vec!["cmd1".into(), "cmd2".into()], "stdin")
            .unwrap();

        calls.borrow_mut().clear();

        // second run for less commands - keep whole cache
        let result = runner
            .run(
                vec!["cmd1".into(), "cmd2".into(), "cmd3".into(), "cmd4".into()],
                "stdin",
            )
            .unwrap();

        // output of the last called command
        assert_eq!(result.output, Output::ok_command("cmd4", "cmd4-output"));

        // only cmd3 is called since is's the only one not cached
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd3".into(), "cmd2-output".as_bytes().to_vec()),
                ("cmd4".into(), "cmd3-output".as_bytes().to_vec()),
            ]
        );

        // all commands are still cached
        assert_eq!(
            runner.cache,
            vec![
                Output::ok_command("cmd1", "cmd1-output"),
                Output::ok_command("cmd2", "cmd2-output"),
                Output::ok_command("cmd3", "cmd3-output"),
                Output::ok_command("cmd4", "cmd4-output")
            ]
        );
    }

    #[test]
    fn test_cmd_runner_modified_in_the_middle() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let _init_run = runner
            .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()], "stdin")
            .unwrap();
        calls.borrow_mut().clear();

        // second run for shorter command - keep whole cache
        let result = runner
            .run(vec!["cmd1".into(), "cmd2mod".into()], "stdin")
            .unwrap();

        // output of the last called command
        assert_eq!(
            result.output,
            Output::ok_command("cmd2mod", "cmd2mod-output")
        );

        // cmd2mod is called since it's modified
        assert_eq!(
            *calls.borrow(),
            vec![("cmd2mod".into(), "cmd1-output".as_bytes().to_vec()),]
        );

        // cmd2 replaced with cmd2mod and cmd3 removed since it's invalid after modified command
        assert_eq!(
            runner.cache,
            vec![
                Output::ok_command("cmd1", "cmd1-output"),
                Output::ok_command("cmd2mod", "cmd2mod-output"),
            ]
        );
    }

    #[test]
    fn test_cmd_runner_modified_in_the_middle_and_extended() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let _init_run = runner
            .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()], "stdin")
            .unwrap();
        calls.borrow_mut().clear();

        // second run for shorter command - keep whole cache
        let result = runner
            .run(
                vec!["cmd1".into(), "cmd2mod".into(), "cmd3".into()],
                "stdin",
            )
            .unwrap();

        // output of the last called command
        assert_eq!(result.output, Output::ok_command("cmd3", "cmd3-output"));

        // cmd2mod is called since it's modified
        // cmd3 is also called because it was after modified command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd2mod".into(), "cmd1-output".as_bytes().to_vec()),
                ("cmd3".into(), "cmd2mod-output".as_bytes().to_vec()),
            ]
        );

        // cmd2 replaced with cmd2mod and cmd3 replaced with updated output
        assert_eq!(
            runner.cache,
            vec![
                Output::ok_command("cmd1", "cmd1-output"),
                Output::ok_command("cmd2mod", "cmd2mod-output"),
                Output::ok_command("cmd3", "cmd3-output"),
            ]
        );
    }

    #[test]
    fn test_cmd_runner_errors() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let result = runner
            .run(
                vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()],
                "stdin",
            )
            .unwrap();

        // output of the last called command
        assert_eq!(
            result.output,
            Output::err_command("cmd2err", "cmd2err-output", Some(1))
        );

        // cmd2mod is called since it's modified
        // cmd3 is also called because it was after modified command
        assert_eq!(
            *calls.borrow(),
            vec![
                ("cmd1".into(), "stdin".as_bytes().to_vec()),
                ("cmd2err".into(), "cmd1-output".as_bytes().to_vec()),
            ]
        );

        // only cmd1 is cached since it didn't fail
        assert_eq!(
            runner.cache,
            vec![Output::ok_command("cmd1", "cmd1-output"),]
        );
    }

    #[test]
    fn test_cmd_runner_errors_clear_cache() {
        let calls = Rc::new(RefCell::new(vec![]));
        let mock_exec = MockExec {
            calls: calls.clone(),
        };
        let mut runner = CachedCmdRunner::with_exec(Box::new(mock_exec));

        let _init_run = runner
            .run(vec!["cmd1".into(), "cmd2".into(), "cmd3".into()], "stdin")
            .unwrap();
        calls.borrow_mut().clear();

        let result = runner
            .run(
                vec!["cmd1".into(), "cmd2err".into(), "cmd3".into()],
                "stdin",
            )
            .unwrap();

        assert_eq!(
            result.output,
            Output::err_command("cmd2err", "cmd2err-output", Some(1))
        );

        // cmd1 not called because it's cached
        assert_eq!(
            *calls.borrow(),
            vec![("cmd2err".into(), "cmd1-output".as_bytes().to_vec()),]
        );

        // only cmd1 is cached since it didn't fail
        // entry for cmd3 is cleared because cmd2err failed before
        assert_eq!(
            runner.cache,
            vec![Output::ok_command("cmd1", "cmd1-output"),]
        );
    }
}
