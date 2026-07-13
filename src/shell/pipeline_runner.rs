use crate::rura::Rura;
use crate::shell::cached_runner::CachedPipelineRunner;
use anyhow::Result;
use humansize::FormatSize;
use itertools::Itertools;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::time::Duration;

pub trait PipelineRunner {
    fn run(&self, rura: &Rura) -> Result<PipelineRun>;
    fn update_stdin(&mut self, stdin: Arc<[u8]>);
}

pub struct PipelineRunners;
impl PipelineRunners {
    #[cfg(unix)]
    pub fn new(shell: &str, stdin: Arc<[u8]>, no_cache: bool) -> Box<dyn PipelineRunner> {
        Box::new(CachedPipelineRunner::new(shell, stdin, !no_cache))
    }

    #[cfg(windows)]
    pub fn new(shell: &str, stdin: Arc<[u8]>, _no_cache: bool) -> Box<dyn PipelineRunner> {
        use crate::shell::simple_runner::SimplePipelineRunner;

        Box::new(SimplePipelineRunner::new(shell, stdin))
    }
}

pub struct StepOutput {
    pub command: String,
    pub bytes: Arc<[u8]>,
    pub duration: Option<Duration>,
}

#[derive(Clone)]
pub struct StepFailure {
    pub command: String,
    pub bytes: Arc<[u8]>,
    pub code: Option<i32>,
    pub duration: Duration,
}

impl Debug for StepOutput {
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

impl Debug for StepFailure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ command: \"{}\", duration: {:?}, size: {} ({}), error_code: {:?} }}",
            self.command,
            self.duration,
            self.bytes.len(),
            self.bytes.len().format_size(humansize::BINARY),
            self.code
        )
    }
}

pub struct PipelineRun {
    pub stdin: Arc<[u8]>,
    pub steps: Vec<StepOutput>,
    pub failure: Option<StepFailure>,
}

impl Debug for PipelineRun {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PipelineRun {{ stdin: {} ({}), steps: {:?}, failure: {:?}, duration: {:?} }}",
            self.stdin.len(),
            self.stdin.len().format_size(humansize::DECIMAL),
            self.steps,
            self.failure,
            self.total_duration()
        )
    }
}

impl PipelineRun {
    pub fn new() -> PipelineRun {
        PipelineRun {
            stdin: Arc::from("".as_bytes()),
            steps: vec![],
            failure: None,
        }
    }

    pub fn error(err: String, code: Option<i32>) -> PipelineRun {
        PipelineRun {
            stdin: Arc::from("".as_bytes()),
            steps: vec![],
            failure: Some(StepFailure {
                command: "".into(),
                bytes: Arc::from(err.as_bytes()),
                code,
                duration: Duration::from_millis(1),
            }),
        }
    }

    pub fn error_bytes(err: Arc<[u8]>, code: Option<i32>) -> PipelineRun {
        PipelineRun {
            stdin: Arc::from("".as_bytes()),
            steps: vec![],
            failure: Some(StepFailure {
                command: "".into(),
                bytes: Arc::from(err),
                code,
                duration: Duration::from_millis(1),
            }),
        }
    }

    pub fn total_duration(&self) -> Duration {
        self.steps
            .iter()
            .map(|o| o.duration.unwrap_or(Duration::ZERO))
            .sum()
    }

    pub fn succeeded(&self) -> bool {
        self.failure.is_none()
    }

    pub fn step_bytes(&self) -> Vec<Arc<[u8]>> {
        self.steps.iter().map(|a| a.bytes.clone()).collect_vec()
    }

    pub fn failure_bytes(&self) -> Option<(Arc<[u8]>, Option<i32>)> {
        self.failure
            .as_ref()
            .map(|o| (o.bytes.clone(), o.code.clone()))
    }

    pub fn failed_step_index(&self) -> Option<usize> {
        if self.failure.is_some() {
            Some(self.steps.len())
        } else {
            None
        }
    }
}

#[cfg(test)]
impl PipelineRun {
    pub fn from_bytes(bytes: Arc<[u8]>) -> PipelineRun {
        PipelineRun {
            stdin: Arc::from("".as_bytes()),
            steps: vec![StepOutput {
                bytes,
                command: "test-cmd".into(),
                duration: Some(Duration::from_millis(1)),
            }],
            failure: None,
        }
    }

    pub fn from_bytes_with_stdin(bytes: Arc<[u8]>, stdin: Arc<[u8]>) -> PipelineRun {
        PipelineRun {
            stdin,
            steps: vec![StepOutput {
                bytes,
                command: "test-cmd".into(),
                duration: Some(Duration::from_millis(1)),
            }],
            failure: None,
        }
    }
}
