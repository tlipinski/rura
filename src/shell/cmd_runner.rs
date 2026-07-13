use crate::rura::RuraCommand;
use crate::shell::cached_runner::CachedCmdRunner;
use anyhow::Result;
use humansize::FormatSize;
use itertools::Itertools;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::time::Duration;

pub trait CmdRunner {
    fn run(&self, command: &RuraCommand) -> Result<CmdResult>;
    fn update_stdin(&mut self, stdin: Arc<[u8]>);
}

pub struct CmdRunners;
impl CmdRunners {
    #[cfg(unix)]
    pub fn new(shell: &str, stdin: Arc<[u8]>, no_cache: bool) -> Box<dyn CmdRunner> {
        Box::new(CachedCmdRunner::new(shell, stdin, !no_cache))
    }

    #[cfg(windows)]
    pub fn new(shell: &str, stdin: Arc<[u8]>, _no_cache: bool) -> Box<dyn CmdRunner> {
        use crate::shell::simple_runner::SimpleCmdRunner;

        Box::new(SimpleCmdRunner::new(shell, stdin))
    }
}

pub struct OkOutput {
    pub(crate) command: String,
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) duration: Option<Duration>,
}

impl Debug for OkOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ command: \"{}\", duration: {:?}, size: {} ({}) }}",
            self.command,
            self.duration,
            self.bytes.len(),
            self.bytes.len().format_size(humansize::BINARY),
        )
    }
}

pub struct CmdResult {
    pub(crate) stdin: Arc<[u8]>,
    pub(crate) ok_outputs: Vec<OkOutput>,
    pub(crate) error_output: Option<(Arc<[u8]>, Option<i32>)>,
}

impl Debug for CmdResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CmdResult {{ stdin: {} ({}), ok_outputs: {:?}, duration: {:?} }}",
            self.stdin.len(),
            self.stdin.len().format_size(humansize::DECIMAL),
            self.ok_outputs,
            self.total_duration()
        )
    }
}

impl CmdResult {
    pub fn new() -> CmdResult {
        CmdResult {
            stdin: Arc::from("".as_bytes()),
            ok_outputs: vec![],
            error_output: None,
        }
    }

    pub fn error(err: String, code: Option<i32>) -> CmdResult {
        CmdResult {
            stdin: Arc::from("".as_bytes()),
            ok_outputs: vec![],
            error_output: Some((Arc::from(err.as_bytes()), code)),
        }
    }

    pub fn error_bytes(err: Arc<[u8]>, code: Option<i32>) -> CmdResult {
        CmdResult {
            stdin: Arc::from("".as_bytes()),
            ok_outputs: vec![],
            error_output: Some((Arc::from(err), code)),
        }
    }

    pub fn total_duration(&self) -> Duration {
        self.ok_outputs
            .iter()
            .map(|o| o.duration.unwrap_or(Duration::ZERO))
            .sum()
    }

    pub fn all_ok(&self) -> bool {
        self.error_output.is_none()
    }

    pub fn ok_outputs(&self) -> Vec<Arc<[u8]>> {
        self.ok_outputs
            .iter()
            .map(|a| a.bytes.clone())
            .collect_vec()
    }

    pub fn failed_subcommand(&self) -> Option<usize> {
        if self.error_output.is_some() {
            Some(self.ok_outputs.len())
        } else {
            None
        }
    }
}

#[cfg(test)]
impl CmdResult {
    pub fn from_bytes(bytes: Arc<[u8]>) -> CmdResult {
        CmdResult {
            stdin: Arc::from("".as_bytes()),
            ok_outputs: vec![OkOutput {
                bytes,
                command: "test-cmd".into(),
                duration: Some(Duration::from_millis(1)),
            }],
            error_output: None,
        }
    }

    pub fn from_bytes_with_stdin(bytes: Arc<[u8]>, stdin: Arc<[u8]>) -> CmdResult {
        CmdResult {
            stdin,
            ok_outputs: vec![OkOutput {
                bytes,
                command: "test-cmd".into(),
                duration: Some(Duration::from_millis(1)),
            }],
            error_output: None,
        }
    }
}
