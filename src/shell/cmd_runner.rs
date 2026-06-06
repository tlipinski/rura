use crate::rura::RuraCommand;
use crate::shell::cached_runner::CachedCmdRunner;
use crate::shell::output::Output;
use crate::shell::split_runner::SplitCmdRunner;
use anyhow::Result;

pub trait CmdRunner {
    fn run(&self, command: &RuraCommand) -> Result<CmdResult>;
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
        use crate::shell::simple_runner::SimpleCmdRunner;

        Box::new(SimpleCmdRunner::new(shell, stdin))
    }
}

pub struct CmdResult {
    pub output: Output,
    pub failed_subcommand: Option<usize>,
}
