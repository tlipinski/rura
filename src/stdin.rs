use crate::app::{Action, PipelineRunnerAction};
use anyhow::Error;
use anyhow::Result;
use crossterm::tty::IsTty;
use log::{debug, info};
use std::io::{BufReader, Read, stdin};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub fn start_input_read_task(
    file_arg: Option<String>,
    action_tx: &Sender<Action>,
    command_tx: &Sender<PipelineRunnerAction>,
    update_interval: Duration,
) -> Sender<StdinControllerAction> {
    let (stdin_action_tx, stdin_action_rx) = std::sync::mpsc::channel::<StdinControllerAction>();
    // single channel for messages from the UI and the stdin reader so the controller can block on it
    let (controller_tx, controller_rx) = std::sync::mpsc::channel::<ControllerMsg>();
    let reader_tx = controller_tx.clone();
    let (backpressure_tx, backpressure_rx) = std::sync::mpsc::channel::<BackpressureMsg>();

    let command_tx_c1 = command_tx.clone();
    let command_tx_c2 = command_tx.clone();
    let action_tx_c1 = action_tx.clone();

    thread::spawn(move || {
        if let Some(file) = file_arg {
            match read_input_file(file) {
                Ok(stdin) => {
                    let arc: Arc<[u8]> = Arc::from(stdin);
                    // In case of reading file there's just one update of stdin therefore,
                    // we're waiting for the command_rx to accept commands
                    while let Err(_) =
                        command_tx_c1.send(PipelineRunnerAction::UpdateStdin(arc.clone()))
                    {
                        thread::sleep(Duration::from_millis(100));
                        debug!("Waiting for command_rx to accept commands");
                    }
                }
                Err(e) => {
                    action_tx_c1.send(Action::Failure(e.to_string())).unwrap();
                }
            }
        } else {
            thread::spawn(move || forward_ui_actions(stdin_action_rx, controller_tx));

            thread::spawn(move || {
                stdin_controller_task(
                    controller_rx,
                    action_tx_c1,
                    command_tx_c2,
                    backpressure_tx,
                    update_interval,
                    false,
                )
                .unwrap()
            });

            thread::spawn(move || {
                stdin_reader_task(reader_tx, backpressure_rx).unwrap();
            });
        };
    });

    stdin_action_tx
}

/// Passes actions sent by the UI to the controller until either side is gone.
fn forward_ui_actions(
    stdin_action_rx: Receiver<StdinControllerAction>,
    controller_tx: Sender<ControllerMsg>,
) {
    for action in stdin_action_rx {
        if controller_tx.send(ControllerMsg::Ui(action)).is_err() {
            break;
        }
    }
}

fn stdin_controller_task(
    controller_rx: Receiver<ControllerMsg>,
    action_tx: Sender<Action>,
    command_tx: Sender<PipelineRunnerAction>,
    backpressure_tx: Sender<BackpressureMsg>,
    interval: Duration,
    pause_after_first_update: bool,
) -> Result<()> {
    let mut stdin_bytes = vec![];
    let mut first_stdin_update = false;
    let mut new_data = false;
    let mut paused = false;
    let mut completed = false;
    let mut last_update = Instant::now();

    loop {
        // completion is deferred until resumed so output doesn't change while paused
        if completed && !paused {
            action_tx.send(Action::StdinCompleted)?;
            command_tx.send(PipelineRunnerAction::UpdateStdin(Arc::from(stdin_bytes)))?;
            return Ok(());
        }

        // stdin is updated at most once per interval, so when there's new data
        // wait only until the next update is due, otherwise block until a message comes
        let msg = if new_data && !paused {
            let next_update = last_update + interval;
            controller_rx.recv_timeout(next_update.saturating_duration_since(Instant::now()))
        } else {
            controller_rx.recv().map_err(RecvTimeoutError::from)
        };

        // backpressure send errors are ignored since they only mean the reader has finished
        match msg {
            Ok(ControllerMsg::Reader(ReaderMsg::Read(bytes))) => {
                stdin_bytes.extend_from_slice(&bytes);
                new_data = true;
            }
            Ok(ControllerMsg::Reader(ReaderMsg::Completed)) => {
                completed = true;
            }
            Ok(ControllerMsg::Ui(StdinControllerAction::Toggle)) => {
                paused = !paused;
                if paused {
                    let _ = backpressure_tx.send(BackpressureMsg::Hold);
                } else {
                    let _ = backpressure_tx.send(BackpressureMsg::Continue);
                    last_update = Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                new_data = false;
                command_tx.send(PipelineRunnerAction::UpdateStdin(Arc::from(
                    stdin_bytes.clone(),
                )))?;
                last_update = Instant::now();
                if pause_after_first_update && !first_stdin_update {
                    first_stdin_update = true;
                    paused = true;
                    let _ = backpressure_tx.send(BackpressureMsg::Hold);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn stdin_reader_task(
    reader_tx: Sender<ControllerMsg>,
    backpressure_rx: Receiver<BackpressureMsg>,
) -> Result<()> {
    let tty = stdin().is_tty();
    if !tty {
        let mut buf: [u8; 1048576] = [0; 1048576];
        let mut reader = BufReader::new(stdin());
        loop {
            'reading: loop {
                match backpressure_rx.try_recv() {
                    Ok(BackpressureMsg::Hold) => break 'reading,
                    _ => {}
                }

                let bytes_read = reader.read(&mut buf)?;
                // debug!("Read {} bytes from stdin", bytes_read);
                if bytes_read > 0 {
                    reader_tx.send(ControllerMsg::Reader(ReaderMsg::Read(Arc::from(
                        &buf[..bytes_read],
                    ))))?
                } else {
                    debug!("Completed reading from stdin");
                    reader_tx.send(ControllerMsg::Reader(ReaderMsg::Completed))?;
                    return Ok(());
                }
            }

            debug!("stdin reader paused");
            'hold: loop {
                match backpressure_rx.recv() {
                    Ok(BackpressureMsg::Continue) => break 'hold,
                    Ok(BackpressureMsg::Hold) => {}
                    Err(_) => return Ok(()),
                }
            }
        }
    } else {
        reader_tx.send(ControllerMsg::Reader(ReaderMsg::Completed))?;
        Ok(())
    }
}

fn read_input_file(file: String) -> Result<Vec<u8>> {
    info!("reading input file {file}");
    match std::fs::read(file.clone()) {
        Ok(content) => Ok(content),
        Err(e) => Err(Error::msg(format!(
            "Failed reading input file {}: {}",
            file,
            e.to_string()
        ))),
    }
}

enum BackpressureMsg {
    Hold,
    Continue,
}

// StdinControllerAction is public while ReaderMsg is private
// ControllerMsg joins them so that messages are read using a single channel
enum ControllerMsg {
    Ui(StdinControllerAction),
    Reader(ReaderMsg),
}

pub enum StdinControllerAction {
    Toggle,
}

enum ReaderMsg {
    Read(Arc<[u8]>),
    Completed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;

    struct Controller {
        tx: Sender<ControllerMsg>,
        action_rx: Receiver<Action>,
        command_rx: Receiver<PipelineRunnerAction>,
        backpressure_rx: Receiver<BackpressureMsg>,
        handle: JoinHandle<Result<()>>,
    }

    fn start_controller(interval: Duration) -> Controller {
        let (tx, rx) = channel();
        let (action_tx, action_rx) = channel();
        let (command_tx, command_rx) = channel();
        let (backpressure_tx, backpressure_rx) = channel();
        let handle = thread::spawn(move || {
            stdin_controller_task(rx, action_tx, command_tx, backpressure_tx, interval, false)
        });
        Controller {
            tx,
            action_rx,
            command_rx,
            backpressure_rx,
            handle,
        }
    }

    fn read(s: &str) -> ControllerMsg {
        ControllerMsg::Reader(ReaderMsg::Read(Arc::from(s.as_bytes())))
    }

    const COMPLETED: ControllerMsg = ControllerMsg::Reader(ReaderMsg::Completed);
    const TOGGLE: ControllerMsg = ControllerMsg::Ui(StdinControllerAction::Toggle);

    fn next_stdin(c: &Controller, timeout: Duration) -> Option<String> {
        match c.command_rx.recv_timeout(timeout) {
            Ok(PipelineRunnerAction::UpdateStdin(bytes)) => {
                Some(String::from_utf8_lossy(&bytes).into())
            }
            _ => None,
        }
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn sends_accumulated_stdin_after_interval() {
        let c = start_controller(Duration::from_millis(200));

        c.tx.send(read("a")).unwrap();
        c.tx.send(read("b")).unwrap();
        assert_eq!(next_stdin(&c, WAIT), Some("ab".into()));

        // no new data, no update
        assert_eq!(next_stdin(&c, Duration::from_millis(400)), None);

        c.tx.send(read("c")).unwrap();
        c.tx.send(COMPLETED).unwrap();
        assert!(matches!(
            c.action_rx.recv_timeout(WAIT),
            Ok(Action::StdinCompleted)
        ));
        assert_eq!(next_stdin(&c, WAIT), Some("abc".into()));

        assert!(c.handle.join().unwrap().is_ok());
    }

    #[test]
    fn paused_controller_defers_updates_and_completion() {
        let c = start_controller(Duration::from_millis(10));

        c.tx.send(TOGGLE).unwrap();
        assert!(matches!(
            c.backpressure_rx.recv_timeout(WAIT),
            Ok(BackpressureMsg::Hold)
        ));

        c.tx.send(read("a")).unwrap();
        c.tx.send(COMPLETED).unwrap();
        assert_eq!(next_stdin(&c, Duration::from_millis(200)), None);
        assert!(c.action_rx.try_recv().is_err());

        c.tx.send(TOGGLE).unwrap();
        assert!(matches!(
            c.backpressure_rx.recv_timeout(WAIT),
            Ok(BackpressureMsg::Continue)
        ));
        assert!(matches!(
            c.action_rx.recv_timeout(WAIT),
            Ok(Action::StdinCompleted)
        ));
        assert_eq!(next_stdin(&c, WAIT), Some("a".into()));

        assert!(c.handle.join().unwrap().is_ok());
    }

    #[test]
    fn forwards_ui_actions_until_ui_is_gone() {
        let (ui_tx, ui_rx) = channel();
        let (controller_tx, controller_rx) = channel();
        let handle = thread::spawn(move || forward_ui_actions(ui_rx, controller_tx));

        ui_tx.send(StdinControllerAction::Toggle).unwrap();
        assert!(matches!(
            controller_rx.recv_timeout(WAIT),
            Ok(ControllerMsg::Ui(StdinControllerAction::Toggle))
        ));

        drop(ui_tx);
        handle.join().unwrap();
        assert!(controller_rx.recv().is_err());
    }

    #[test]
    fn exits_when_all_senders_are_dropped() {
        let c = start_controller(Duration::from_millis(10));

        drop(c.tx);

        assert!(c.handle.join().unwrap().is_ok());
    }
}
