use crate::app::{Action, PipelineRunnerAction};
use anyhow::Error;
use anyhow::Result;
use crossterm::tty::IsTty;
use log::{debug, info};
use std::io::{BufReader, Read, stdin};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime};

pub fn start_input_read_task(
    file_arg: Option<String>,
    action_tx: &Sender<Action>,
    command_tx: &Sender<PipelineRunnerAction>,
    update_interval: Duration,
) -> Sender<StdinControllerAction> {
    let (stdin_action_tx, stdin_action_rx) = std::sync::mpsc::channel::<StdinControllerAction>();
    let (reader_tx, reader_rx) = std::sync::mpsc::channel::<ReaderMsg>();
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
                    action_tx_c1,
                    command_tx_c2,
                    backpressure_tx,
                    stdin_action_rx,
                    update_interval,
                    false,
                )
                .unwrap()
            });

            thread::spawn(move || {
                stdin_reader_task(reader_tx.clone(), backpressure_rx).unwrap();
            });
        };
    });

    stdin_action_tx
}

fn stdin_controller_task(
    reader_rx: Receiver<ReaderMsg>,
    action_tx: Sender<Action>,
    command_tx: Sender<PipelineRunnerAction>,
    backpressure_tx: Sender<BackpressureMsg>,
    stdin_controller_rx: Receiver<StdinControllerAction>,
    duration: Duration,
    pause_after_first_update: bool,
) -> Result<()> {
    let mut stdin_bytes = vec![];
    let mut first_stdin_update = false;
    let mut new_data = false;
    loop {
        let mut now = SystemTime::now();
        'receiving: loop {
            if now.elapsed()? > duration && new_data {
                new_data = false;
                command_tx.send(PipelineRunnerAction::UpdateStdin(Arc::from(
                    stdin_bytes.clone(),
                )))?;
                now = SystemTime::now();
                if pause_after_first_update && !first_stdin_update {
                    first_stdin_update = true;
                    backpressure_tx.send(BackpressureMsg::Hold)?;
                    break 'receiving;
                }
            } else {
                match reader_rx.try_recv() {
                    Ok(b) => match b {
                        ReaderMsg::Read(b) => {
                            stdin_bytes.extend_from_slice(&b);
                            new_data = true;
                        }
                        ReaderMsg::Completed => {
                            action_tx.send(Action::StdinCompleted)?;
                            command_tx
                                .send(PipelineRunnerAction::UpdateStdin(Arc::from(stdin_bytes)))?;
                            return Ok(());
                        }
                    },
                    Err(_) => {}
                }

                match stdin_controller_rx.try_recv() {
                    Ok(StdinControllerAction::Toggle) => {
                        backpressure_tx.send(BackpressureMsg::Hold)?;
                        break 'receiving;
                    }
                    Err(_) => {}
                }
            }
        }

        'pause: loop {
            match stdin_controller_rx.recv() {
                Ok(StdinControllerAction::Toggle) => {
                    backpressure_tx.send(BackpressureMsg::Continue)?;
                    now = SystemTime::now();
                    break 'pause;
                }
                Err(_) => {}
            }
        }
    }
}

fn stdin_reader_task(
    reader_tx: Sender<ReaderMsg>,
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

            'hold: loop {
                debug!("stdin reader paused");
                match backpressure_rx.recv() {
                    Ok(BackpressureMsg::Continue) => break 'hold,
                    _ => {}
                }
            }
        }
    } else {
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
