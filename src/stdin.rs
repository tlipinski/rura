use crate::app::{Action, PipelineRunnerAction};
use anyhow::Error;
use anyhow::Result;
use crossbeam_channel::{after, never, select};
use crossterm::tty::IsTty;
use log::{debug, info};
use std::io::{BufReader, Read, stdin};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub fn start_input_read_task(
    file_arg: Option<String>,
    action_tx: &Sender<Action>,
    command_tx: &Sender<PipelineRunnerAction>,
    update_interval: Duration,
) -> crossbeam_channel::Sender<StdinControllerAction> {
    // crossbeam channels so the controller can wait on both with select!
    let (stdin_action_tx, stdin_action_rx) =
        crossbeam_channel::unbounded::<StdinControllerAction>();
    let (reader_tx, reader_rx) = crossbeam_channel::unbounded::<ReaderMsg>();
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
            thread::spawn(move || {
                stdin_controller_task(
                    reader_rx,
                    stdin_action_rx,
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

fn stdin_controller_task(
    reader_rx: crossbeam_channel::Receiver<ReaderMsg>,
    controller_rx: crossbeam_channel::Receiver<StdinControllerAction>,
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

    // replaces reader_rx once reading is completed, since a disconnected channel
    // would be selected immediately over and over again
    let reader_done = never::<ReaderMsg>();

    loop {
        // completion is deferred until resumed so output doesn't change while paused
        if completed && !paused {
            action_tx.send(Action::StdinCompleted)?;
            command_tx.send(PipelineRunnerAction::UpdateStdin(Arc::from(stdin_bytes)))?;
            return Ok(());
        }

        // stdin is updated at most once per interval, so when there's new data
        // wait only until the next update is due, otherwise wait for messages only
        let update_due = if new_data && !paused {
            after((last_update + interval).saturating_duration_since(Instant::now()))
        } else {
            never()
        };

        let reader = if completed { &reader_done } else { &reader_rx };

        // backpressure send errors are ignored since they only mean the reader has finished
        select! {
            recv(reader) -> msg => match msg {
                Ok(ReaderMsg::Read(bytes)) => {
                    stdin_bytes.extend_from_slice(&bytes);
                    new_data = true;
                }
                // reader gone without completing (e.g. read error) - nothing more to read
                Ok(ReaderMsg::Completed) | Err(_) => {
                    completed = true;
                }
            },
            recv(controller_rx) -> msg => match msg {
                Ok(StdinControllerAction::Toggle) => {
                    paused = !paused;
                    if paused {
                        let _ = backpressure_tx.send(BackpressureMsg::Hold);
                    } else {
                        let _ = backpressure_tx.send(BackpressureMsg::Continue);
                        last_update = Instant::now();
                    }
                }
                // UI is gone
                Err(_) => return Ok(()),
            },
            recv(update_due) -> _ => {
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
            },
        }
    }
}

fn stdin_reader_task(
    reader_tx: crossbeam_channel::Sender<ReaderMsg>,
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
                    reader_tx.send(ReaderMsg::Read(Arc::from(&buf[..bytes_read])))?
                } else {
                    debug!("Completed reading from stdin");
                    reader_tx.send(ReaderMsg::Completed)?;
                    return Ok(());
                }
            }

            debug!("stdin reader paused");
            'hold: loop {
                match backpressure_rx.recv() {
                    Ok(BackpressureMsg::Continue) => break 'hold,
                    Ok(BackpressureMsg::Hold) => {}
                    // controller is gone, nobody will resume reading
                    Err(_) => return Ok(()),
                }
            }
        }
    } else {
        reader_tx.send(ReaderMsg::Completed)?;
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

enum ReaderMsg {
    Read(Arc<[u8]>),
    Completed,
}

enum BackpressureMsg {
    Hold,
    Continue,
}

pub enum StdinControllerAction {
    Toggle,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;

    struct Controller {
        reader_tx: crossbeam_channel::Sender<ReaderMsg>,
        ui_tx: crossbeam_channel::Sender<StdinControllerAction>,
        action_rx: Receiver<Action>,
        command_rx: Receiver<PipelineRunnerAction>,
        backpressure_rx: Receiver<BackpressureMsg>,
        handle: JoinHandle<Result<()>>,
    }

    fn start_controller(interval: Duration) -> Controller {
        let (reader_tx, reader_rx) = crossbeam_channel::unbounded();
        let (ui_tx, ui_rx) = crossbeam_channel::unbounded();
        let (action_tx, action_rx) = channel();
        let (command_tx, command_rx) = channel();
        let (backpressure_tx, backpressure_rx) = channel();
        let handle = thread::spawn(move || {
            stdin_controller_task(
                reader_rx,
                ui_rx,
                action_tx,
                command_tx,
                backpressure_tx,
                interval,
                false,
            )
        });
        Controller {
            reader_tx,
            ui_tx,
            action_rx,
            command_rx,
            backpressure_rx,
            handle,
        }
    }

    fn read(s: &str) -> ReaderMsg {
        ReaderMsg::Read(Arc::from(s.as_bytes()))
    }

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

        c.reader_tx.send(read("a")).unwrap();
        c.reader_tx.send(read("b")).unwrap();
        assert_eq!(next_stdin(&c, WAIT), Some("ab".into()));

        // no new data, no update
        assert_eq!(next_stdin(&c, Duration::from_millis(400)), None);

        c.reader_tx.send(read("c")).unwrap();
        c.reader_tx.send(ReaderMsg::Completed).unwrap();
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

        c.ui_tx.send(StdinControllerAction::Toggle).unwrap();
        assert!(matches!(
            c.backpressure_rx.recv_timeout(WAIT),
            Ok(BackpressureMsg::Hold)
        ));

        c.reader_tx.send(read("a")).unwrap();
        c.reader_tx.send(ReaderMsg::Completed).unwrap();
        assert_eq!(next_stdin(&c, Duration::from_millis(200)), None);
        assert!(c.action_rx.try_recv().is_err());

        c.ui_tx.send(StdinControllerAction::Toggle).unwrap();
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
    fn completes_when_reader_is_gone() {
        let c = start_controller(Duration::from_millis(10));

        c.reader_tx.send(read("a")).unwrap();
        drop(c.reader_tx);

        assert!(matches!(
            c.action_rx.recv_timeout(WAIT),
            Ok(Action::StdinCompleted)
        ));
        assert!(c.handle.join().unwrap().is_ok());
    }

    #[test]
    fn exits_when_ui_is_gone() {
        let c = start_controller(Duration::from_millis(10));

        drop(c.ui_tx);

        assert!(c.handle.join().unwrap().is_ok());
    }
}
