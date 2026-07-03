use crate::app::Action::{
    CommandCompleted, Debounced, Failure, ResetHighlight, StartProgress, StopProgress, UserInput,
};
use crate::args::Args;
use crate::completable_input::CompletableInput;
use crate::config::{Config, history_path, search_history_path};
use crate::debouncer::debouncer_task;
use crate::file_saver::{FileSaver, FileSavers};
use crate::help_widget::HelpWidget;
use crate::history::History;
use crate::output_widget::{ContentMode, ErrorPanePlacement, OutputWidget};
use crate::presets_widget::{DisplayMode, PresetsWidget};
use crate::rura::{ExecuteType, RuraCommand};
use crate::rura_widget::RuraWidget;
use crate::save_to_file_widget::SaveToFileWidget;
use crate::search_widget::SearchWidget;
use crate::shell::cmd_runner::{CmdResult, CmdRunner, CmdRunners};
use crate::shell::output::Output;
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use KeyCode::{Enter, Esc, F, Tab};
use anyhow::{Error, Result};
use cfg_if::cfg_if;
use crossterm::event::KeyCode::Char;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::tty::IsTty;
use itertools::Itertools;
use log::{debug, error, info};
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::Span;
use ratatui::prelude::Stylize;
use ratatui::style::Color::Yellow;
use ratatui::style::{Style, Styled};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Shadow, Widget};
use ratatui::{DefaultTerminal, Frame};
use serde::{Deserialize, Serialize};
use std::io::{Read, stdin};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, SystemTime};
use std::{env, thread};
use tui_input::Input;

pub struct App {
    rura_widget: RuraWidget,
    output_widget: OutputWidget,
    search_widget: SearchWidget,
    help_widget: HelpWidget,
    save_output_widget: SaveToFileWidget,
    save_command_widget: SaveToFileWidget,
    presets_widget: PresetsWidget,
    action_rx: Receiver<Action>,
    command_tx: Sender<RuraCommand>,
    key_bindings: KeyBindings,
    command_line_placement: CommandLinePlacement,
    input_mode: InputMode,
    debouncer_tx: Sender<()>,
    active_mode: ActiveMode,
    active_modal: ActiveModal,
    in_progress: Option<SystemTime>,
    theme: Theme,
    file_saver: Box<dyn FileSaver>,
    success_output_bytes: Arc<[u8]>,
    exit: bool,
}

impl App {
    pub fn new(args: Args, config: Config) -> Self {
        let default;
        cfg_if! {
            if #[cfg(unix)] {
                default = "sh";
            } else if #[cfg(windows)] {
                default = "powershell";
            }
        }
        let shell = args
            .shell
            .clone()
            .or(config.shell)
            .or({
                if let Ok(shell_var) = env::var("SHELL") {
                    let shell_path = PathBuf::from(shell_var);
                    shell_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(String::from)
                } else {
                    None
                }
            })
            .unwrap_or(default.into());

        debug!("Shell: {:?}", shell);

        let (action_tx, action_rx) = std::sync::mpsc::channel::<Action>();
        let (command_tx, command_rx) = std::sync::mpsc::channel::<RuraCommand>();
        let (highlight_reset_tx, highlight_reset_rx) = std::sync::mpsc::channel::<()>();
        let (debouncer_tx, debouncer_rx) = std::sync::mpsc::channel::<()>();

        let s1 = action_tx.clone();
        thread::spawn(move || handle_input_task(s1).unwrap());

        let no_cache = args.no_cache || config.no_cache;
        let value = shell.clone();
        let s2 = action_tx.clone();
        let s3 = action_tx.clone();
        let ctx = command_tx.clone();
        thread::spawn(move || {
            let stdin_res = if let Some(file) = args.file {
                read_file_task(file)
            } else {
                read_stdin_task()
            };

            match stdin_res {
                Ok(stdin) => {
                    thread::spawn(move || {
                        handle_command_task(
                            CmdRunners::new(&value, Arc::from(stdin), no_cache),
                            command_rx,
                            s2,
                        )
                        .unwrap();
                    });

                    while let Err(_) = ctx.send(RuraCommand::empty()) {
                        thread::sleep(Duration::from_millis(100));
                        debug!("Waiting for command_rx to accept commands");
                    }
                }
                Err(e) => {
                    s3.send(Failure(e.to_string())).unwrap();
                }
            }
        });

        let s4 = action_tx.clone();
        thread::spawn(move || {
            reset_highlight_task(highlight_reset_rx, s4, config.highlight_duration_ms).unwrap()
        });

        thread::spawn(move || {
            debouncer_task(
                debouncer_rx,
                Duration::from_millis(config.debounce_duration_ms),
                move || {
                    action_tx
                        .send(Debounced)
                        .expect("Sending to channel failed");
                },
            )
            .unwrap()
        });

        Self {
            rura_widget: RuraWidget {
                command_input: CompletableInput::from(&args.command.unwrap_or_default(), &shell),
                highlight_until: None,
                theme: Theme::from_config(&config.theme),
                history: history_path()
                    .map(|p| History::using_file(p))
                    .unwrap_or(History::in_mem()),
                highlight_reset_tx,
                failed_subcommand: None,
                diff_base_subcommand: None,
                copied: None,
            },
            output_widget: OutputWidget::new(
                &config.theme,
                match config.command_line_placement {
                    CommandLinePlacement::Top => ErrorPanePlacement::Top,
                    CommandLinePlacement::Bottom => ErrorPanePlacement::Bottom,
                },
            ),
            search_widget: SearchWidget {
                input: Input::default(),
                case_sensitive: false,
                regex: false,
                current: 0,
                total: 0,
                history: search_history_path()
                    .map(|p| History::using_file(p))
                    .unwrap_or(History::in_mem()),
            },
            save_output_widget: SaveToFileWidget::new(
                " Save output to file ".to_string(),
                shell.clone(),
                Theme::from_config(&config.theme),
            ),
            save_command_widget: SaveToFileWidget::new(
                " Save command to file ".to_string(),
                shell.clone(),
                Theme::from_config(&config.theme),
            ),
            presets_widget: PresetsWidget::new(Theme::from_config(&config.theme)),
            action_rx,
            command_tx,
            debouncer_tx,
            key_bindings: KeyBindings::from_config(&config.keybindings),
            command_line_placement: config.command_line_placement,
            help_widget: HelpWidget::new(config.keybindings, Theme::from_config(&config.theme)),
            input_mode: InputMode::Normal,
            active_mode: ActiveMode::default(),
            active_modal: ActiveModal::default(),
            in_progress: None,
            theme: Theme::from_config(&config.theme),
            file_saver: FileSavers::new(&shell),
            success_output_bytes: Arc::from(Vec::new()),
            exit: false,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<String> {
        while !self.exit {
            terminal.draw(|frame| self.render(frame, frame.area()))?;

            let action = self.action_rx.recv()?;
            self.handle_action(action);
        }

        Ok(self.rura_widget.command_input.value().to_string())
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            UserInput(event) => self.handle_event(&event),
            CommandCompleted(command, result) => {
                if !command.is_empty() {
                    if matches!(self.input_mode, InputMode::Normal) {
                        // in normal mode save all commands to history
                        self.rura_widget.history.push(&command.to_string())
                    } else {
                        // in live mode only save commands that were successfully executed
                        if let Some(Output::Ok(_)) = result.outputs.last() {
                            self.rura_widget.history.push(&command.to_string())
                        }
                    }
                }

                self.rura_widget.failed_subcommand = result.failed_subcommand();

                if let Some(Output::Ok(bytes)) = result.outputs.last() {
                    self.success_output_bytes = bytes.clone();
                }

                self.output_widget.handle_command_result(result);
            }
            ResetHighlight => self.rura_widget.highlight_until = None,
            Debounced => {
                match self.input_mode {
                    InputMode::Normal => {
                        // Should not happen in normal mode
                        // Probably user turned off live before debouncer responded
                    }
                    InputMode::LiveFull => self.handle_execute(ExecuteType::FullLive),
                    InputMode::LiveUntilCursor => {
                        self.handle_execute(ExecuteType::UntilCurrentLive)
                    }
                }
            }
            StartProgress(time) => {
                self.in_progress = Some(time);
            }
            StopProgress => {
                self.in_progress = None;
            }
            Failure(err) => {
                let result = CmdResult {
                    stdin: Arc::from("".as_bytes()),
                    outputs: vec![Output::Err(Arc::from(err.as_bytes()), None)],
                };
                self.output_widget.handle_command_result(result);
            }
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;

                match &self.active_modal {
                    ActiveModal::None => match &self.active_mode {
                        ActiveMode::Normal => self.handle_event_normal(event, code, mods),
                        ActiveMode::Search => self.handle_event_search(event, code, mods),
                    },
                    ActiveModal::LiveConfirmation(input_mode) => {
                        self.handle_event_live_confirmation(code, mods, input_mode.clone())
                    }
                    ActiveModal::Help => self.handle_event_help(code, mods),
                    ActiveModal::SaveOutput => self.handle_event_save_output(event, code, mods),
                    ActiveModal::SaveCommand => self.handle_event_save_command(event, code, mods),
                    ActiveModal::Presets => self.handle_event_presets(event, code, mods),
                }
            }
            _ => {}
        }
    }

    fn handle_event_save_command(&mut self, event: &Event, code: KeyCode, mods: KeyModifiers) {
        match (code, mods) {
            (Esc, KeyModifiers::NONE) => {
                self.active_modal = ActiveModal::default();
            }
            (Enter, KeyModifiers::NONE) => match self.save_command_to_file() {
                Ok(()) => {
                    self.active_modal = ActiveModal::default();
                    self.save_command_widget.error_message = None;
                }
                Err(e) => {
                    self.save_command_widget.error_message = Some(e.to_string());
                    error!("Error saving output to file: {}", e);
                }
            },
            _ => match to_ui_command(&self.key_bindings, code, mods) {
                Some(UiCmd::Quit) => {
                    self.exit = true;
                }
                Some(UiCmd::Complete) => {
                    self.save_command_widget.file_path_input.complete(true);
                }
                Some(UiCmd::CompletePrev) => {
                    self.save_command_widget.file_path_input.complete(false);
                }
                _ => {
                    self.save_command_widget.file_path_input.handle_event(event);
                }
            },
        }
    }

    fn handle_event_save_output(&mut self, event: &Event, code: KeyCode, mods: KeyModifiers) {
        match (code, mods) {
            (Esc, KeyModifiers::NONE) => {
                self.active_modal = ActiveModal::default();
            }
            (Enter, KeyModifiers::NONE) => match self.save_output_to_file() {
                Ok(()) => {
                    self.save_output_widget.error_message = None;
                    self.active_modal = ActiveModal::default();
                }
                Err(e) => {
                    self.save_output_widget.error_message = Some(e.to_string());
                    error!("Error saving output to file: {}", e);
                }
            },
            _ => match to_ui_command(&self.key_bindings, code, mods) {
                Some(UiCmd::Quit) => {
                    self.exit = true;
                }
                Some(UiCmd::Complete) => {
                    self.save_output_widget.file_path_input.complete(true);
                }
                Some(UiCmd::CompletePrev) => {
                    self.save_output_widget.file_path_input.complete(false);
                }
                _ => {
                    self.save_output_widget.file_path_input.handle_event(event);
                }
            },
        }
    }

    fn handle_event_presets(&mut self, event: &Event, code: KeyCode, mods: KeyModifiers) {
        match self.presets_widget.display_mode {
            DisplayMode::Select => match (code, mods) {
                (Esc, KeyModifiers::NONE) => {
                    self.active_modal = ActiveModal::default();
                }
                (Enter, KeyModifiers::NONE) => {
                    if let Some(preset) = self.presets_widget.confirm() {
                        self.rura_widget
                            .insert_after_current(&format!(" {} ", preset));
                        self.active_modal = ActiveModal::default();
                    }
                }
                (Enter, KeyModifiers::ALT) => {
                    if let Some(preset) = self.presets_widget.confirm() {
                        self.rura_widget
                            .insert_before_current(&format!(" {} ", preset));
                        self.active_modal = ActiveModal::default();
                    }
                }
                (KeyCode::Up, KeyModifiers::NONE) => self.presets_widget.previous(),
                (KeyCode::Down, KeyModifiers::NONE) => self.presets_widget.next(),
                (Char('t'), KeyModifiers::CONTROL) => {
                    self.presets_widget
                        .new_from(self.rura_widget.command_input.value());
                }
                (Char('n'), KeyModifiers::CONTROL) => {
                    self.presets_widget.new_empty();
                }
                (Char('d'), KeyModifiers::CONTROL) => {
                    self.presets_widget.confirm_delete();
                }
                (Char('e'), KeyModifiers::CONTROL) => {
                    self.presets_widget.edit();
                }
                (Char('k'), KeyModifiers::CONTROL) => {
                    self.presets_widget.clone();
                }
                (KeyCode::Up, KeyModifiers::CONTROL) => {
                    self.presets_widget.move_up();
                }
                (KeyCode::Down, KeyModifiers::CONTROL) => {
                    self.presets_widget.move_down();
                }
                (Char(ch), KeyModifiers::NONE) if ch.is_lowercase() => {
                    if let Some(preset) = self.presets_widget.find_by_shortcut(ch) {
                        self.rura_widget
                            .insert_after_current(&format!(" {} ", preset));
                        self.active_modal = ActiveModal::default();
                    }
                }
                (Char(ch), KeyModifiers::SHIFT) => {
                    if let Some(preset) = self
                        .presets_widget
                        .find_by_shortcut(ch.to_lowercase().next().unwrap())
                    {
                        self.rura_widget
                            .insert_before_current(&format!(" {} ", preset));
                        self.active_modal = ActiveModal::default();
                    }
                }
                _ => match to_ui_command(&self.key_bindings, code, mods) {
                    Some(UiCmd::Quit) => {
                        self.exit = true;
                    }
                    Some(UiCmd::TogglePresets) => {
                        self.active_modal = ActiveModal::default();
                    }
                    _ => {}
                },
            },
            DisplayMode::ConfirmDelete(_) => match (code, mods) {
                (Esc | Char('n'), KeyModifiers::NONE) => self.presets_widget.cancel_delete(),
                (Char('y'), KeyModifiers::NONE) => {
                    self.presets_widget.delete();
                }
                _ => match to_ui_command(&self.key_bindings, code, mods) {
                    Some(UiCmd::Quit) => {
                        self.exit = true;
                    }
                    _ => {}
                },
            },
            DisplayMode::Edit { .. } => match (code, mods) {
                (Esc, KeyModifiers::NONE) => self.presets_widget.cancel_edit(),
                (Tab, KeyModifiers::NONE) => {
                    self.presets_widget.toggle_edit_mode();
                }
                (Enter, KeyModifiers::NONE) => {
                    self.presets_widget.save_edit();
                }
                _ => match to_ui_command(&self.key_bindings, code, mods) {
                    Some(UiCmd::Quit) => {
                        self.exit = true;
                    }
                    _ => {
                        self.presets_widget.handle_event(&event);
                    }
                },
            },
        }
    }

    fn handle_event_help(&mut self, code: KeyCode, mods: KeyModifiers) {
        match (code, mods) {
            (Esc, KeyModifiers::NONE) => {
                self.active_modal = ActiveModal::default();
            }
            (F(1), KeyModifiers::NONE) => {
                self.active_modal = ActiveModal::default();
            }
            _ => match to_ui_command(&self.key_bindings, code, mods) {
                Some(UiCmd::Quit) => {
                    self.exit = true;
                }
                Some(UiCmd::ScrollUp) => {
                    self.help_widget.scroll_up();
                }
                Some(UiCmd::ScrollDown) => {
                    self.help_widget.scroll_down();
                }
                Some(UiCmd::ScrollUpPage) => {
                    self.help_widget.scroll_page_up();
                }
                Some(UiCmd::ScrollDownPage) => {
                    self.help_widget.scroll_page_down();
                }
                _ => {}
            },
        }
    }

    fn handle_event_live_confirmation(
        &mut self,
        code: KeyCode,
        mods: KeyModifiers,
        input_mode: InputMode,
    ) {
        match (code, mods) {
            (Esc | Char('n'), KeyModifiers::NONE) => self.active_modal = ActiveModal::default(),
            (Char('y'), KeyModifiers::NONE) => {
                self.input_mode = input_mode.clone();
                self.active_modal = ActiveModal::default();
            }
            _ => match to_ui_command(&self.key_bindings, code, mods) {
                Some(UiCmd::Quit) => {
                    self.exit = true;
                }
                _ => {}
            },
        }
    }

    fn handle_event_search(&mut self, event: &Event, code: KeyCode, mods: KeyModifiers) {
        match (code, mods) {
            (Esc, KeyModifiers::NONE) => {
                self.search_widget.update_history();
                self.active_mode = ActiveMode::Normal;
            }
            (F(1), KeyModifiers::NONE) => {
                self.active_modal = ActiveModal::Help;
            }
            (Char('c'), KeyModifiers::ALT) => {
                self.search_widget.toggle_case_sensitive();
                self.output_widget.highlight(
                    self.search_widget.input.value(),
                    self.search_widget.case_sensitive,
                    self.search_widget.regex,
                );
                self.search_widget
                    .update_highlight_info(self.output_widget.highlight_info());
            }
            (Char('x'), KeyModifiers::ALT) => {
                self.search_widget.toggle_regex();
                self.output_widget.highlight(
                    self.search_widget.input.value(),
                    self.search_widget.case_sensitive,
                    self.search_widget.regex,
                );
                self.search_widget
                    .update_highlight_info(self.output_widget.highlight_info());
            }
            (Enter, KeyModifiers::NONE) => {
                self.output_widget.highlight(
                    self.search_widget.input.value(),
                    self.search_widget.case_sensitive,
                    self.search_widget.regex,
                );
                self.search_widget
                    .update_highlight_info(self.output_widget.highlight_info());
                self.search_widget.update_history();
            }
            _ => match to_ui_command(&self.key_bindings, code, mods) {
                Some(ui_cmd) => match ui_cmd {
                    UiCmd::Quit => {
                        self.exit = true;
                    }
                    UiCmd::SearchNext => {
                        self.output_widget.highlight_next();
                        self.search_widget
                            .update_highlight_info(self.output_widget.highlight_info());
                    }
                    UiCmd::SearchPrev => {
                        self.output_widget.highlight_prev();
                        self.search_widget
                            .update_highlight_info(self.output_widget.highlight_info());
                    }
                    UiCmd::ScrollDown => {
                        self.output_widget.scroll_down();
                    }
                    UiCmd::ScrollDownPage => {
                        self.output_widget.scroll_page_down();
                    }
                    UiCmd::ScrollUp => {
                        self.output_widget.scroll_up();
                    }
                    UiCmd::ScrollUpPage => {
                        self.output_widget.scroll_page_up();
                    }
                    UiCmd::ScrollLeft => {
                        self.output_widget.scroll_left();
                    }
                    UiCmd::ScrollLeftPage => {
                        self.output_widget.scroll_page_left();
                    }
                    UiCmd::ScrollRight => {
                        self.output_widget.scroll_right();
                    }
                    UiCmd::ScrollRightPage => {
                        self.output_widget.scroll_page_right();
                    }
                    UiCmd::ToggleWrap => {
                        self.output_widget.toggle_wrap();
                    }
                    UiCmd::SaveOutput => {
                        self.active_modal = ActiveModal::SaveOutput;
                    }
                    UiCmd::SaveCommand => {
                        self.active_modal = ActiveModal::SaveCommand;
                    }
                    UiCmd::HistoryPrev => {
                        self.search_widget.history_prev();
                    }
                    UiCmd::HistoryNext => {
                        self.search_widget.history_next();
                    }
                    _ => {}
                },
                _ => {
                    if self.search_widget.handle_event(event) {
                        self.output_widget.highlight(
                            self.search_widget.input.value(),
                            self.search_widget.case_sensitive,
                            self.search_widget.regex,
                        );
                        self.search_widget
                            .update_highlight_info(self.output_widget.highlight_info());
                    };
                }
            },
        }
    }

    fn handle_event_normal(&mut self, event: &Event, code: KeyCode, mods: KeyModifiers) {
        match (code, mods) {
            (Esc, KeyModifiers::NONE) => {
                self.output_widget.clear_highlight();
            }
            (F(1), KeyModifiers::NONE) => {
                self.active_modal = ActiveModal::Help;
            }
            _ => match to_ui_command(&self.key_bindings, code, mods) {
                Some(ui_cmd) => match ui_cmd {
                    UiCmd::Quit => {
                        self.exit = true;
                    }
                    UiCmd::SearchNext | UiCmd::SearchPrev => {
                        self.active_mode = ActiveMode::Search;
                    }
                    UiCmd::ExecuteFull => {
                        self.handle_execute(ExecuteType::Full);
                    }
                    UiCmd::ExecuteUntilCurrent => self.handle_execute(ExecuteType::UntilCurrent),
                    UiCmd::ExecuteUntilPrev => self.handle_execute(ExecuteType::UntilCurrentPrev),
                    UiCmd::ResetInput => {
                        self.command_tx.send(RuraCommand::empty()).unwrap();
                    }
                    UiCmd::SubcommandNext => {
                        self.rura_widget.subcommand_next();
                    }
                    UiCmd::SubcommandPrev => {
                        self.rura_widget.subcommand_prev();
                    }
                    UiCmd::SubcommandCut => {
                        self.rura_widget.cut_current();
                    }
                    UiCmd::SubcommandCopy => {
                        self.rura_widget.copy_current();
                    }
                    UiCmd::SubcommandPaste => {
                        self.rura_widget.paste_after_current();
                    }
                    UiCmd::HistoryNext => {
                        // disable history in live mode
                        if matches!(self.input_mode, InputMode::Normal) {
                            self.rura_widget.history_next();
                        }
                    }
                    UiCmd::HistoryPrev => {
                        // disable history in live mode
                        if matches!(self.input_mode, InputMode::Normal) {
                            self.rura_widget.history_prev();
                        }
                    }
                    UiCmd::Complete => {
                        // disable completions in live mode
                        if matches!(self.input_mode, InputMode::Normal) {
                            self.rura_widget.command_input.complete(true);
                        }
                    }
                    UiCmd::CompletePrev => {
                        // disable completions in live mode
                        if matches!(self.input_mode, InputMode::Normal) {
                            self.rura_widget.command_input.complete(false);
                        }
                    }
                    UiCmd::ScrollDown => {
                        self.output_widget.scroll_down();
                    }
                    UiCmd::ScrollDownPage => {
                        self.output_widget.scroll_page_down();
                    }
                    UiCmd::ScrollUp => {
                        self.output_widget.scroll_up();
                    }
                    UiCmd::ScrollUpPage => {
                        self.output_widget.scroll_page_up();
                    }
                    UiCmd::ScrollLeft => {
                        self.output_widget.scroll_left();
                    }
                    UiCmd::ScrollLeftPage => {
                        self.output_widget.scroll_page_left();
                    }
                    UiCmd::ScrollRight => {
                        self.output_widget.scroll_right();
                    }
                    UiCmd::ScrollRightPage => self.output_widget.scroll_page_right(),
                    UiCmd::ToggleWrap => {
                        self.output_widget.toggle_wrap();
                    }
                    UiCmd::SaveOutput => {
                        self.active_modal = ActiveModal::SaveOutput;
                    }
                    UiCmd::SaveCommand => {
                        self.active_modal = ActiveModal::SaveCommand;
                    }
                    UiCmd::FormatCommand => {
                        self.rura_widget.format();
                    }
                    UiCmd::ToggleDiff => {
                        self.output_widget.toggle_diff();
                        match self.output_widget.content_mode {
                            ContentMode::Normal => {
                                self.rura_widget.diff_base_subcommand = None;
                            }
                            ContentMode::Diff => {
                                self.rura_widget.diff_base_subcommand =
                                    self.output_widget.diff_base();
                            }
                        }
                    }
                    UiCmd::DiffBaseStdin => {
                        self.output_widget.set_diff_base(None);
                        self.rura_widget.diff_base_subcommand = None;
                    }
                    UiCmd::DiffBase => {
                        let idx = self.rura_widget.current_index();
                        if let Some(idx) = idx {
                            self.output_widget.set_diff_base(Some(idx));
                            self.rura_widget.diff_base_subcommand = Some(idx);
                        }
                    }
                    UiCmd::ToggleLive => match self.input_mode {
                        InputMode::Normal => {
                            self.active_modal = ActiveModal::LiveConfirmation(InputMode::LiveFull);
                        }
                        InputMode::LiveFull => {
                            self.input_mode = InputMode::Normal;
                        }
                        InputMode::LiveUntilCursor => {
                            self.input_mode = InputMode::LiveFull;
                        }
                    },
                    UiCmd::ToggleLiveUntilCursor => match self.input_mode {
                        InputMode::Normal => {
                            self.active_modal =
                                ActiveModal::LiveConfirmation(InputMode::LiveUntilCursor);
                        }
                        InputMode::LiveFull => {
                            self.input_mode = InputMode::LiveUntilCursor;
                        }
                        InputMode::LiveUntilCursor => {
                            self.input_mode = InputMode::Normal;
                        }
                    },
                    UiCmd::TogglePresets => {
                        self.presets_widget.load();
                        self.active_modal = ActiveModal::Presets;
                    }
                    UiCmd::ToggleLineNums => {
                        self.output_widget.toggle_line_nums();
                    }
                },
                _ => {
                    if self.rura_widget.handle_event(event) {
                        match self.input_mode {
                            InputMode::Normal => {}
                            InputMode::LiveFull | InputMode::LiveUntilCursor => {
                                self.debouncer_tx.send(()).unwrap();
                            }
                        }
                    }
                }
            },
        }
    }

    fn save_output_to_file(&mut self) -> Result<()> {
        let path = PathBuf::from(self.save_output_widget.file_path_input.value().trim());
        self.file_saver
            .save(path, self.success_output_bytes.to_vec())
    }

    fn save_command_to_file(&mut self) -> Result<()> {
        let path = PathBuf::from(self.save_command_widget.file_path_input.value().trim());

        self.file_saver.save_script(
            path,
            self.rura_widget.command_input.value().bytes().collect_vec(),
        )
    }

    fn handle_execute(&mut self, kind: ExecuteType) {
        match self.rura_widget.execute(kind) {
            Ok(command) => self.command_tx.send(command).unwrap(),
            Err(_) => {}
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let margin = Margin::new(1, 1);

        let inner_area = area.inner(margin);

        let (command_input_area, search_input_area, output_area, status_area) = {
            let search_height = if matches!(self.active_mode, ActiveMode::Search) {
                self.search_widget.height(inner_area.width) + 2
            } else {
                0
            };
            match self.command_line_placement {
                CommandLinePlacement::Top => {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![
                            Constraint::Length(self.rura_widget.height(inner_area.width) + 2), // command
                            Constraint::Length(search_height), // search
                            Constraint::Fill(1),               // output
                            Constraint::Length(1),             // status
                        ])
                        .split(area);

                    (layout[0], layout[1], layout[2], layout[3])
                }
                CommandLinePlacement::Bottom => {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![
                            Constraint::Fill(1),                                               // output
                            Constraint::Length(search_height), // search
                            Constraint::Length(self.rura_widget.height(inner_area.width) + 2), // command
                            Constraint::Length(1), // status
                        ])
                        .split(area);

                    (layout[2], layout[1], layout[0], layout[3])
                }
            }
        };

        let command_input_block = if matches!(self.input_mode, InputMode::Normal) {
            Block::bordered()
        } else {
            Block::bordered()
                .border_style(Style::default().fg(Yellow))
                .border_type(BorderType::Thick)
        };

        frame.render_widget(command_input_block, command_input_area);
        frame.render_widget(&self.rura_widget, command_input_area.inner(margin));

        match self.active_mode {
            ActiveMode::Normal => {
                let inner_rect = command_input_area.inner(margin);
                let (x, y) = self.rura_widget.cursor(inner_rect.width);
                frame.set_cursor_position((
                    command_input_area.x + 1 + x,
                    command_input_area.y + 1 + y,
                ));
            }
            ActiveMode::Search => {
                self.search_widget
                    .render(search_input_area, frame.buffer_mut());

                frame.render_widget(
                    Block::default().reversed(),
                    command_input_area.inner(margin),
                );

                let inner_rect = search_input_area.inner(margin);
                let (x, y) = self.search_widget.cursor(inner_rect.width);
                frame.set_cursor_position((
                    search_input_area.x + 1 + x,
                    search_input_area.y + 1 + y,
                ));
            }
        }

        frame.render_widget(&self.output_widget, output_area);

        let [
            _,
            status_left_area,
            status_center_area,
            status_right_area,
            _,
        ] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(status_area);

        let [exec_area, _, diff_area, _, live_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length(3),
                Constraint::Length(1), // separator
                Constraint::Length(4),
                Constraint::Length(1), // separator
                Constraint::Length(7),
            ])
            .areas(status_left_area);

        let [_, lines_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Fill(1), // fill space to the left
                Constraint::Length(self.output_widget.output_len().to_string().len() as u16 + 3),
            ])
            .areas(status_right_area);

        frame.render_widget(self.hints_widget(), status_center_area);

        match self.output_widget.content_mode {
            ContentMode::Normal => {}
            ContentMode::Diff => {
                frame.render_widget("diff".reversed(), diff_area);
            }
        }

        match self.input_mode {
            InputMode::Normal => {}
            InputMode::LiveFull => {
                frame.render_widget("live".reversed(), live_area);
            }
            InputMode::LiveUntilCursor => {
                frame.render_widget("live uc".reversed(), live_area);
            }
        }

        // Render progress indicator only if command runs for more than defined time
        // It reduces flickering when command is fast and progress indicator is not needed.
        if let Some(time) = self.in_progress
            && time.elapsed().unwrap() > Duration::from_millis(100)
        {
            frame.render_widget(">>>".bold().rapid_blink(), exec_area);
        }

        frame.render_widget(
            format!("L:{}", self.output_widget.output_len())
                .bold()
                .into_right_aligned_line(),
            lines_area,
        );

        match self.active_modal {
            ActiveModal::LiveConfirmation(_) => {
                self.render_live_confirm(frame);
            }
            ActiveModal::Help => {
                frame.render_widget(&self.help_widget, frame.area());
            }
            ActiveModal::SaveOutput => {
                self.save_output_widget
                    .render(frame.area(), frame.buffer_mut());
                frame.set_cursor_position(self.save_output_widget.cursor())
            }
            ActiveModal::SaveCommand => {
                self.save_command_widget
                    .render(frame.area(), frame.buffer_mut());
                frame.set_cursor_position(self.save_command_widget.cursor())
            }
            ActiveModal::Presets => {
                self.presets_widget.render(frame.area(), frame.buffer_mut());
                if let Some(cursor) = self.presets_widget.cursor() {
                    frame.set_cursor_position(cursor)
                }
            }
            _ => {}
        }
    }

    fn render_live_confirm(&self, frame: &mut Frame) {
        let body = Text::from(vec![
            Line::from("").centered(),
            Line::from("  WARNING: THIS MIGHT BE DANGEROUS!   ")
                .centered()
                .yellow()
                .reversed(),
            Line::from("").centered(),
            Line::from("   Commands will be executed automatically as you type.   ").centered(),
            Line::from("").centered(),
            Line::from("[Y]es / [N]o").centered(),
            Line::from("").centered(),
        ]);

        let max_width = body
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or_default();

        let centered_area = frame.area().centered(
            Constraint::Length(max_width as u16 + 2),
            Constraint::Length(body.height() as u16 + 2),
        );
        Clear.render(centered_area, frame.buffer_mut());
        Block::default()
            .borders(Borders::ALL)
            .title(" Confirm entering LIVE mode ")
            .set_style(self.theme.popup)
            .shadow(Shadow::dark_shade())
            .render(centered_area, frame.buffer_mut());

        Paragraph::new(body).render(centered_area.inner(Margin::new(1, 1)), frame.buffer_mut());
    }

    fn hints_widget(&self) -> Line<'_> {
        let mut spans: Vec<Span> = vec![];

        spans.push("F1".bold());
        spans.push(" Help ".into());
        spans.push("Ctrl+C".bold());
        spans.push(" Quit ".into());

        Line::from_iter(spans).centered().dim()
    }
}

fn handle_command_task(
    cmd_runner: Box<dyn CmdRunner>,
    command_rx: Receiver<RuraCommand>,
    action_tx: Sender<Action>,
) -> Result<()> {
    loop {
        if let Ok(command) = command_rx.recv() {
            action_tx.send(StartProgress(SystemTime::now()))?;

            match cmd_runner.run(&command) {
                Ok(result) => {
                    let _ = action_tx.send(CommandCompleted(command, result));
                }
                Err(e) => {
                    error!("Failed running command {:?}: {}", command, e);
                    action_tx.send(Failure("Failed running command, check logs".into()))?;
                }
            }

            action_tx.send(StopProgress)?;
        }
    }
}

fn handle_input_task(tx: Sender<Action>) -> Result<()> {
    loop {
        if let Ok(event) = event::read() {
            match event {
                Event::Key(key_evt) if !key_evt.is_press() => {}
                event => tx.send(UserInput(event))?,
            }
        }
    }
}

fn read_stdin_task() -> Result<Vec<u8>> {
    let mut buff = vec![];
    let tty = stdin().is_tty();
    if !tty {
        let result = stdin().read_to_end(&mut buff);
        match result {
            Ok(_) => Ok(buff),
            Err(e) => Err(Error::msg(format!(
                "Failed reading stdin: {}",
                e.to_string()
            ))),
        }
    } else {
        Ok("".into())
    }
}

fn read_file_task(file: String) -> Result<Vec<u8>> {
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

fn reset_highlight_task(rx: Receiver<()>, tx: Sender<Action>, duration_ms: u64) -> Result<()> {
    loop {
        if let Ok(_) = rx.recv() {
            thread::sleep(Duration::from_millis(duration_ms));
            tx.send(ResetHighlight)?
        }
    }
}

enum Action {
    UserInput(Event),
    CommandCompleted(RuraCommand, CmdResult),
    Failure(String),
    ResetHighlight,
    Debounced,
    StartProgress(SystemTime),
    StopProgress,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandLinePlacement {
    Top,
    #[default]
    Bottom,
}

#[derive(Clone)]
enum InputMode {
    Normal,
    LiveFull,
    LiveUntilCursor,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{KeyBindingsConfig, ThemeConfig};
    use crossterm::event::Event::Key;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::VecDeque;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(100, 30)).unwrap())
        }
    }

    impl Default for App {
        fn default() -> Self {
            let (_, action_rx) = std::sync::mpsc::channel::<Action>();
            let (command_tx, _) = std::sync::mpsc::channel::<RuraCommand>();
            let (highlight_reset_tx, _) = std::sync::mpsc::channel::<()>();
            let (debouncer_tx, _) = std::sync::mpsc::channel::<()>();

            let theme_config = ThemeConfig::default();
            let kb_config = KeyBindingsConfig::default();

            Self {
                rura_widget: RuraWidget {
                    command_input: CompletableInput::from("", ""),
                    highlight_until: None,
                    theme: Theme::from_config(&theme_config),
                    history: History::in_mem(),
                    highlight_reset_tx,
                    failed_subcommand: None,
                    diff_base_subcommand: None,
                    copied: None,
                },
                output_widget: OutputWidget::new(&theme_config, ErrorPanePlacement::Bottom),
                save_output_widget: SaveToFileWidget::new(
                    " Save output to file ".into(),
                    "".into(),
                    Theme::from_config(&theme_config),
                ),
                save_command_widget: SaveToFileWidget::new(
                    " Save command to file ".into(),
                    "".into(),
                    Theme::from_config(&theme_config),
                ),
                presets_widget: PresetsWidget::new(Theme::from_config(&theme_config)),
                search_widget: SearchWidget {
                    input: Input::default(),
                    case_sensitive: false,
                    regex: false,
                    current: 0,
                    total: 0,
                    history: History::in_mem(),
                },
                action_rx,
                command_tx,
                debouncer_tx,
                exit: false,
                key_bindings: KeyBindings::from_config(&kb_config),
                command_line_placement: CommandLinePlacement::Bottom,
                help_widget: HelpWidget::new(kb_config, Theme::from_config(&theme_config)),
                input_mode: InputMode::Normal,
                active_mode: ActiveMode::default(),
                active_modal: ActiveModal::default(),
                theme: Theme::from_config(&theme_config),
                file_saver: FileSavers::new(""),
                success_output_bytes: Arc::from([]),
                in_progress: None,
            }
        }
    }

    #[test]
    fn main_screen() {
        let app = App::default();

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn main_screen_help() {
        let mut app = App::default();

        input_key(&mut app, F(1), KeyModifiers::NONE);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn main_screen_help_small_screen() {
        let mut app = App::default();

        input_key(&mut app, F(1), KeyModifiers::NONE);

        let mut terminal = Terminal::new(TestBackend::new(100, 15)).unwrap();
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn main_screen_help_narrow_screen() {
        let mut app = App::default();

        input_key(&mut app, F(1), KeyModifiers::NONE);

        let mut terminal = Terminal::new(TestBackend::new(40, 30)).unwrap();
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn main_screen_help_large_screen() {
        let mut app = App::default();

        input_key(&mut app, F(1), KeyModifiers::NONE);

        let mut terminal = Terminal::new(TestBackend::new(100, 50)).unwrap();
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn live_mode_confirm() {
        let mut app = App::default();

        input_key(&mut app, F(11), KeyModifiers::NONE);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn live_mode_full_confirm() {
        let mut app = App::default();

        input_key(&mut app, F(12), KeyModifiers::NONE);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn command_input() {
        let mut app = App::default();

        input_text(&mut app, "ls -la | grep a");

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn saving_to_history_only_ok_outputs_in_live_mode() {
        let mut app = App::default();
        app.input_mode = InputMode::LiveFull;

        let cmd_res = |output: Output| CmdResult {
            stdin: Arc::from("".as_bytes()),
            outputs: vec![output],
        };

        app.handle_action(CommandCompleted("g".into(), cmd_res(Output::err_str(""))));
        app.handle_action(CommandCompleted("gr".into(), cmd_res(Output::err_str(""))));
        app.handle_action(CommandCompleted("gre".into(), cmd_res(Output::err_str(""))));
        app.handle_action(CommandCompleted("grep".into(), cmd_res(Output::ok_str(""))));
        app.handle_action(CommandCompleted(
            "grep 'abc'".into(),
            cmd_res(Output::ok_str("")),
        ));
        app.handle_action(CommandCompleted(
            "gp 'abc'".into(),
            cmd_res(Output::err_str("")),
        ));

        assert_eq!(
            *app.rura_widget.history.history(),
            VecDeque::from(vec!["grep 'abc'".into(), "grep".into(),])
        );
    }

    #[test]
    fn saving_to_history_all_outputs_in_normal_mode() {
        let mut app = App::default();

        let cmd_res = |output: Output| CmdResult {
            stdin: Arc::from("".as_bytes()),
            outputs: vec![output],
        };

        app.handle_action(CommandCompleted("g".into(), cmd_res(Output::err_str(""))));
        app.handle_action(CommandCompleted("grep".into(), cmd_res(Output::ok_str(""))));

        assert_eq!(
            *app.rura_widget.history.history(),
            VecDeque::from(vec!["grep".into(), "g".into()])
        );
    }

    #[test]
    fn save_output_popup() {
        let mut app = App::default();

        input_key(&mut app, Char('s'), KeyModifiers::CONTROL);

        input_text(&mut app, "output-file.txt");

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn save_command_popup() {
        let mut app = App::default();

        input_key(
            &mut app,
            Char('s'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );

        input_text(&mut app, "command-file.txt");

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| app.render(frame, frame.area()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    // todo
    // add test checking that whatever was piped in through stdin
    // goes in exactly the same form out - jq input_line_number

    fn input_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_event(&Key(KeyEvent {
                code: Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }))
        }
    }

    fn input_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_event(&Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }
}

#[derive(Default)]
enum ActiveMode {
    #[default]
    Normal,
    Search,
}

#[derive(Default)]
enum ActiveModal {
    #[default]
    None,
    LiveConfirmation(InputMode),
    Help,
    SaveOutput,
    SaveCommand,
    Presets,
}
