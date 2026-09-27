use log::debug;
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Coordinates cancellation of pipeline runs between the UI and the pipeline thread.
///
/// Every run request gets a generation number. Calling `cancel` invalidates all runs requested
/// so far and kills the process of the run that is currently executing, if it's one of them.
#[derive(Default)]
pub struct Canceller {
    latest: AtomicU64,
    running: Mutex<Running>,
}

#[derive(Default)]
struct Running {
    generation: u64,
    pid: Option<u32>,
}

impl Canceller {
    /// Invalidates all runs requested so far and kills the running process, if any.
    /// Returns the generation to be used for the next run request.
    pub fn cancel(&self) -> u64 {
        let generation = self.latest.fetch_add(1, Ordering::SeqCst) + 1;
        let running = self.running.lock().unwrap();
        if let Some(pid) = running.pid
            && running.generation < generation
        {
            kill(pid);
        }
        generation
    }

    pub fn latest(&self) -> u64 {
        self.latest.load(Ordering::SeqCst)
    }

    pub fn is_stale(&self, generation: u64) -> bool {
        generation < self.latest()
    }

    /// Marks the start of a run with the given generation.
    pub fn begin(&self, generation: u64) {
        self.running.lock().unwrap().generation = generation;
    }

    /// Whether the current run has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.is_stale(self.running.lock().unwrap().generation)
    }

    /// Registers a spawned process of the current run.
    /// The process is killed right away if the run has been cancelled in the meantime.
    pub fn register(&self, pid: u32) {
        let mut running = self.running.lock().unwrap();
        if self.is_stale(running.generation) {
            kill(pid);
        }
        running.pid = Some(pid);
    }

    pub fn unregister(&self) {
        self.running.lock().unwrap().pid = None;
    }
}

#[cfg(unix)]
fn kill(pid: u32) {
    // Stages are spawned as process group leaders, so killing the group
    // also kills subprocesses started by the shell.
    debug!("killing process group {pid}");
    unsafe {
        libc::killpg(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn kill(pid: u32) {
    debug!("killing process tree {pid}");
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output();
}

/// Error returned by pipeline runners when a run has been cancelled.
#[derive(Debug)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("run cancelled")
    }
}

impl std::error::Error for Cancelled {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_makes_previous_runs_stale() {
        let canceller = Canceller::default();

        let first = canceller.cancel();
        assert!(!canceller.is_stale(first));

        let second = canceller.cancel();
        assert!(canceller.is_stale(first));
        assert!(!canceller.is_stale(second));
    }

    #[test]
    fn current_run_is_cancelled_by_newer_request() {
        let canceller = Canceller::default();

        canceller.begin(canceller.cancel());
        assert!(!canceller.is_cancelled());

        canceller.cancel();
        assert!(canceller.is_cancelled());

        canceller.begin(canceller.latest());
        assert!(!canceller.is_cancelled());
    }
}
