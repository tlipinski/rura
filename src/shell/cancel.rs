use log::debug;
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Canceller {
    latest_gen: AtomicU64,
    running: Mutex<Running>,
}

#[derive(Default)]
struct Running {
    generation: u64,
    pid: Option<u32>,
}

impl Canceller {
    pub fn begin(&self, generation: u64) {
        self.running.lock().unwrap().generation = generation;
    }

    pub fn register(&self, pid: u32) {
        let mut running = self.running.lock().unwrap();
        if self.is_stale(running.generation) {
            kill(pid);
        }
        running.pid = Some(pid);
    }

    pub fn cancel(&self) -> u64 {
        let generation = self.latest_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let running = self.running.lock().unwrap();
        if let Some(pid) = running.pid
            && running.generation < generation
        {
            kill(pid);
        }
        generation
    }

    pub fn latest(&self) -> u64 {
        self.latest_gen.load(Ordering::SeqCst)
    }

    pub fn is_stale(&self, generation: u64) -> bool {
        self.latest() > generation
    }

    pub fn is_cancelled(&self) -> bool {
        self.is_stale(self.running.lock().unwrap().generation)
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
