use crate::rura::RuraCommand;
use anyhow::{Result, anyhow};
use itertools::Itertools;
use log::{debug, info};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::SystemTime;

pub struct CmdRunner {
    shell: String,
    split_commands: bool,
}

impl CmdRunner {
    pub fn new(shell: &str, split_commands: bool) -> Self {
        Self {
            shell: shell.into(),
            split_commands,
        }
    }

    pub fn run(&self, command: RuraCommand, stdin: &str) -> Result<Output> {
        let now = SystemTime::now();
        let result = if self.split_commands {
            self.run_split(command.to_run, stdin)
                .map(|output| output.last().unwrap().clone())
        } else {
            self.run_full(&command.to_run.iter().join("|"), stdin)
        };
        let elapsed = now.elapsed()?;
        debug!("command exec took {elapsed:?}");
        result
    }

    fn run_split(&self, commands: Vec<String>, stdin: &str) -> Result<Vec<Output>> {
        info!("executing commands: '{commands:?}'");

        let full_command = commands.join("|");

        let mut outputs = vec![];

        let mut current_stdin = stdin.as_bytes().to_vec();

        for command in commands {
            info!("  sub command: '{command:?}'");

            let mut cmd = Command::new("/usr/bin/env");
            cmd.args([&self.shell, "-c", &command]);

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

            let owned_stdin = current_stdin.clone();

            thread::spawn(move || {
                let _ = child_stdin.write_all(&owned_stdin);
            });

            if let Ok(output) = child.wait_with_output() {
                if output.status.success() {
                    let stdout = output.stdout.as_slice();
                    let str = String::from_utf8_lossy(stdout);
                    outputs.push(Output::ok_command(&full_command, &str));
                    current_stdin = stdout.to_vec();
                } else {
                    let stderr = output.stderr.as_slice();
                    let str = String::from_utf8_lossy(stderr);
                    outputs.push(Output::err_command(
                        &full_command,
                        &str,
                        output.status.code(),
                    ));
                    current_stdin = stderr.to_vec();
                }
            } else {
                outputs.push(Output::err_command(
                    &full_command,
                    "Failed to execute command",
                    None,
                ))
            }
        }

        Ok(outputs)
    }

    fn run_full(&self, command: &str, stdin: &str) -> Result<Output> {
        info!("executing command: '{command}'");

        let mut cmd = Command::new("/usr/bin/env");
        cmd.args([&self.shell, "-c", &command]);

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

        let owned_stdin = stdin.to_owned();

        thread::spawn(move || {
            let _ = child_stdin.write_all(owned_stdin.as_bytes());
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

#[derive(Clone, PartialEq, Eq)]
pub struct Output {
    pub command: Option<String>,
    pub lines: Vec<String>,
    pub status_code: Option<i32>,
    pub ok: bool,
}

impl Output {
    pub fn ok_command(command: &str, str: &str) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(str),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_command(command: &str, str: &str, status_code: Option<i32>) -> Self {
        Self {
            command: Some(command.into()),
            lines: Self::lines(str),
            status_code,
            ok: false,
        }
    }

    pub fn ok_stdin(str: &str) -> Self {
        Self {
            command: None,
            lines: Self::lines(str),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err_stdin(str: &str) -> Self {
        Self {
            command: None,
            lines: Self::lines(str),
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
