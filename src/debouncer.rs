use anyhow::Result;
use log::{debug, error};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::sleep;
use std::time::Duration;

pub fn debouncer_task<F>(rx: Receiver<()>, duration: Duration, on_debounce: F) -> Result<()>
where
    F: Fn() -> () + Send + 'static,
{
    'outer: loop {
        sleep(duration);

        let mut last: Option<()> = None;

        // Receive as much as possible within outer loop cycle
        'debouncing: loop {
            match rx.try_recv() {
                Ok(request) => last = Some(request),

                Err(TryRecvError::Empty) => break 'debouncing,

                Err(TryRecvError::Disconnected) => {
                    error!("Disconnected");
                    break 'outer Ok(());
                }
            }
        }

        if last.is_some() {
            debug!("Debounced");
            on_debounce();
        }
    }
}
