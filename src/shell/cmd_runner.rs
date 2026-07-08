use crate::rura::RuraCommand;
use crate::shell::cached_runner::CachedCmdRunner;
use crate::shell::split_runner::SplitCmdRunner;
use anyhow::Result;
use std::sync::Arc;

pub trait CmdRunner {
    fn run(&self, command: &RuraCommand) -> Result<CmdResult>;
}

pub struct CmdRunners;
impl CmdRunners {
    #[cfg(unix)]
    pub fn new(shell: &str, stdin: Arc<[u8]>, no_cache: bool) -> Box<dyn CmdRunner> {
        if no_cache {
            Box::new(SplitCmdRunner::new(shell, stdin))
        } else {
            Box::new(CachedCmdRunner::new(shell, stdin))
        }
    }

    #[cfg(windows)]
    pub fn new(shell: &str, stdin: Arc<[u8]>, _no_cache: bool) -> Box<dyn CmdRunner> {
        use crate::shell::simple_runner::SimpleCmdRunner;

        Box::new(SimpleCmdRunner::new(shell, stdin))
    }
}

#[derive(Clone, Debug)]
pub struct CmdResult {
    pub stdin: Arc<[u8]>,
    pub ok_outputs: Vec<Arc<[u8]>>,
    pub error_output: Option<(Arc<[u8]>, Option<i32>)>,
}

impl CmdResult {
    pub fn all_ok(&self) -> bool {
        self.error_output.is_none()
    }

    pub fn failed_subcommand(&self) -> Option<usize> {
        if self.error_output.is_some() {
            Some(self.ok_outputs.len())
        } else {
            None
        }
    }
}
