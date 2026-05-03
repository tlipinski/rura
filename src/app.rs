use crate::Args;
use crate::app::Action::{CommandCompleted, ResetHighlight, StdinRead, UserInput};
use crate::config::{KeyBindingsConfig, ThemeConfig, history_path};
use crate::debouncer::debouncer_task;
use crate::history::History;
use crate::rura::ExecuteType;
use crate::rura_widget::RuraWidget;
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use Action::Debounced;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::tty::IsTty;
use log::debug;
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::Color::Red;
use ratatui::prelude::Stylize;
use ratatui::prelude::{Position, Span};
use ratatui::style::Color::Yellow;
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Scrollbar, ScrollbarOrientation};
use ratatui::widgets::{ScrollbarState, Wrap};
use ratatui::{DefaultTerminal, Frame};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::error::Error;
use std::io::{Read, Write, stdin};
use std::ops::Range;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use tui_input::Input;
use tui_popup::Popup;

pub struct App {
    rura_widget: RuraWidget,
    stdin: String,
    output: Output,
    error_output_opt: Option<Output>,
    offset: Position,
    wrap: bool,
    exit: bool,
    action_rx: Receiver<Action>,
    command_tx: Sender<(String, String)>,
    theme: Theme,
    key_bindings: KeyBindings,
    command_line_placement: CommandLinePlacement,
    kb_config: KeyBindingsConfig,
    help: bool,
    input_mode: InputMode,
    debouncer_tx: Sender<()>,
    error_display_mode: ErrorDisplayMode,
}

impl App {
    pub fn new(
        args: Args,
        theme_config: &ThemeConfig,
        kb_config: KeyBindingsConfig,
        command_line_placement: CommandLinePlacement,
    ) -> Self {
        let (action_tx, action_rx) = std::sync::mpsc::channel::<Action>();
        let (command_tx, command_rx) = std::sync::mpsc::channel::<(String, String)>();
        let (highlight_reset_tx, highlight_reset_rx) = std::sync::mpsc::channel::<()>();
        let (debouncer_tx, debouncer_rx) = std::sync::mpsc::channel::<()>();

        let s1 = action_tx.clone();
        thread::spawn(move || handle_input_task(s1).unwrap());

        let s2 = action_tx.clone();
        thread::spawn(move || handle_command_task(command_rx, s2).unwrap());

        let s3 = action_tx.clone();
        thread::spawn(move || read_stdin_task(args.file, s3).unwrap());

        let s4 = action_tx.clone();
        thread::spawn(move || reset_highlight_task(highlight_reset_rx, s4).unwrap());

        thread::spawn(move || {
            debouncer_task(debouncer_rx, Duration::from_millis(500), move || {
                action_tx
                    .send(Debounced)
                    .expect("Sending to channel failed");
            })
            .unwrap()
        });

        let mut history = VecDeque::new();
        if let Some(path) = history_path() {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    if !line.is_empty() {
                        history.push_front(line.to_string());
                    }
                }
            }
        }

        Self {
            rura_widget: RuraWidget {
                command_input: Input::from(args.command.unwrap_or_default()),
                highlight_until: None,
                theme: Theme::from_config(theme_config),
                history: History::load(),
                key_bindings: KeyBindings::from_config(&kb_config),
                highlight_reset_tx,
            },
            stdin: "".to_string(),
            offset: Position::default(),
            output: Output::ok(""),
            error_output_opt: None,
            action_rx,
            command_tx,
            wrap: false,
            exit: false,
            theme: Theme::from_config(theme_config),
            key_bindings: KeyBindings::from_config(&kb_config),
            command_line_placement,
            kb_config,
            help: false,
            input_mode: InputMode::Normal,
            debouncer_tx,
            error_display_mode: ErrorDisplayMode::Pane,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<String, Box<dyn Error>> {
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
            CommandCompleted(output) => self.handle_command_output(output),
            ResetHighlight => self.rura_widget.highlight_until = None,
            StdinRead(stdin) => {
                self.stdin = stdin;
                self.output = Output::ok(&self.stdin);
            }
            Debounced => {
                match self.input_mode {
                    InputMode::Normal => {
                        // Should not happen in normal mode
                        // Probably user turned off live before debouncer responded
                    }
                    InputMode::LiveFull => self.handle_execute(ExecuteType::FullLive),
                    InputMode::LiveUntilCursor => self.handle_execute(ExecuteType::UntilCurrentLive),
                }
            }
        }
    }

    fn handle_command_output(&mut self, output: Output) {
        if self.output.len() != output.len() {
            self.offset.y = 0;
        }

        if output.ok {
            self.output = output;
            self.error_output_opt = None;
        } else {
            self.error_output_opt = Some(output);
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;
                let key_bindings = &self.key_bindings;

                match (code, mods) {
                    (KeyCode::Esc, KeyModifiers::NONE) => {
                        self.help = false;
                    }
                    (KeyCode::F(1), KeyModifiers::NONE) => {
                        self.help = !self.help;
                    }
                    (KeyCode::F(2), KeyModifiers::NONE) => match self.error_display_mode {
                        ErrorDisplayMode::Inline => self.error_display_mode = ErrorDisplayMode::Pane,
                        ErrorDisplayMode::Pane => self.error_display_mode = ErrorDisplayMode::Inline,
                    },
                    (KeyCode::F(11), KeyModifiers::NONE) => match self.input_mode {
                        InputMode::Normal => {
                            self.input_mode = InputMode::LiveUntilCursor;
                        }
                        InputMode::LiveFull => {
                            self.input_mode = InputMode::LiveUntilCursor;
                        }
                        InputMode::LiveUntilCursor => {
                            self.input_mode = InputMode::Normal;
                        }
                    },
                    (KeyCode::F(12), KeyModifiers::NONE) => match self.input_mode {
                        InputMode::Normal => {
                            self.input_mode = InputMode::LiveFull;
                        }
                        InputMode::LiveFull => {
                            self.input_mode = InputMode::Normal;
                        }
                        InputMode::LiveUntilCursor => {
                            self.input_mode = InputMode::LiveFull;
                        }
                    },
                    (KeyCode::Char(_) | KeyCode::Backspace, KeyModifiers::NONE) => {
                        self.rura_widget.handle_event(event);
                        match self.input_mode {
                            InputMode::Normal => {}
                            InputMode::LiveFull | InputMode::LiveUntilCursor => {
                                self.debouncer_tx.send(()).unwrap();
                            }
                        }
                    }
                    _ => match to_ui_command(key_bindings, code, mods) {
                        None => {
                            self.rura_widget.handle_event(event);
                        }
                        Some(a) => match a {
                            UiCmd::Quit => {
                                self.exit = true;
                            }
                            UiCmd::ExecuteFull => {
                                self.handle_execute(ExecuteType::Full);
                            }
                            UiCmd::ExecuteUntilCurrent => {
                                self.handle_execute(ExecuteType::UntilCurrent)
                            }
                            UiCmd::ExecuteUntilPrev => {
                                self.handle_execute(ExecuteType::UntilCurrentPrev)
                            }
                            UiCmd::ResetInput => {
                                let new_output = Output::ok(&self.stdin);
                                if self.output.len() != new_output.len() {
                                    self.offset.y = 0;
                                }
                                self.output = new_output;
                                self.error_output_opt = None
                            }
                            UiCmd::ScrollDown => {
                                self.offset.y = self.offset.y.saturating_add(1);
                            }
                            UiCmd::ScrollDownPage => {
                                self.offset.y = self.offset.y.saturating_add(10);
                            }
                            UiCmd::ScrollUp => {
                                self.offset.y = self.offset.y.saturating_sub(1);
                            }
                            UiCmd::ScrollUpPage => {
                                self.offset.y = self.offset.y.saturating_sub(10);
                            }
                            UiCmd::ScrollLeft => {
                                self.offset.x = self.offset.x.saturating_sub(1);
                            }
                            UiCmd::ScrollRight => {
                                self.offset.x = self.offset.x.saturating_add(1);
                            }
                            UiCmd::ToggleWrap => {
                                self.wrap = !self.wrap;
                            }
                            UiCmd::HistoryNext => {
                                // disable history for live mode
                                if matches!(self.input_mode, InputMode::Normal) {
                                    self.rura_widget.handle_event(event);
                                }
                            }
                            UiCmd::HistoryPrev => {
                                // disable history for live mode
                                if matches!(self.input_mode, InputMode::Normal) {
                                    self.rura_widget.handle_event(event);
                                }
                            }
                            _ => {
                                self.rura_widget.handle_event(event);
                            }
                        },
                    },
                }
            }
            _ => {}
        }
    }

    fn handle_execute(&mut self, kind: ExecuteType) {
        match self.rura_widget.execute(kind) {
            Some(command) if command.is_empty() => {
                self.output = Output::ok(&self.stdin);
            }
            Some(c) => self.command_tx.send((c, self.stdin.clone())).unwrap(),
            None => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let theme = &self.theme;

        let margin = Margin::new(1, 1);

        let inner_area = area.inner(margin);

        let error_output_lines = match self.error_display_mode {
            ErrorDisplayMode::Inline => 0,
            ErrorDisplayMode::Pane => self
                .error_output_opt
                .as_ref()
                .map(|e| e.lines.len() + 2)
                .unwrap_or(0),
        };

        let (command_input_area, output_area, errors_area, status_area) =
            match self.command_line_placement {
                CommandLinePlacement::Top => {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![
                            Constraint::Length(self.rura_widget.height(inner_area.width) + 2),
                            Constraint::Length(error_output_lines as u16),
                            Constraint::Fill(1),
                            Constraint::Length(1),
                        ])
                        .split(area);

                    (layout[0], layout[2], layout[1], layout[3])
                }
                CommandLinePlacement::Bottom => {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![
                            Constraint::Fill(1),
                            Constraint::Length(error_output_lines as u16),
                            Constraint::Length(self.rura_widget.height(inner_area.width) + 2),
                            Constraint::Length(1),
                        ])
                        .split(area);

                    (layout[2], layout[0], layout[1], layout[3])
                }
            };

        let line_nums_width = self.output.len().to_string().len();
        let [line_nums_area, output_content_area, vscroll_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length((line_nums_width + 1) as u16),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(output_area);

        let command_input_block = if matches!(self.input_mode, InputMode::Normal) {
            Block::bordered()
        } else {
            Block::bordered()
                .border_style(Style::default().fg(Yellow))
                .border_type(BorderType::Double)
        };

        let inner_rect = command_input_area.inner(margin);

        frame.render_widget(command_input_block, command_input_area);
        frame.render_widget(&self.rura_widget, command_input_area.inner(margin));

        let (x, y) = self.rura_widget.cursor(inner_rect.width);
        frame.set_cursor_position((command_input_area.x + 1 + x, command_input_area.y + 1 + y));

        if matches!(self.error_display_mode, ErrorDisplayMode::Pane) {
            if let Some(err_output) = &self.error_output_opt {
                let block = Block::bordered()
                    .title(format!(" Error: {} ", err_output.status_code.unwrap_or(0)))
                    .border_style(Style::default().fg(Red));
                let mut output_par = Paragraph::new(err_output.lines.join("\n"))
                    .scroll((0, self.offset.x))
                    .block(block);

                if self.wrap {
                    output_par = output_par.wrap(Wrap::default())
                };
                frame.render_widget(output_par, errors_area);
            }
        }

        let output = self.main_output();

        let height = output_content_area.height.min(output.len() as u16);

        let range: Range<usize> = if height >= output.len() as u16 {
            0..output.len()
        } else {
            let from = (self.offset.y as usize).min(output.len());
            let to = (self.offset.y as usize + height as usize).min(output.len());
            from..to
        };

        // debug!("range: {range:?}");

        let line_nums = range
            .clone()
            .map(|i| format!("{: >pad$}", i + 1, pad = line_nums_width))
            .collect::<Vec<String>>();
        let lines_par = Paragraph::new(line_nums.join("\n")).style(theme.line_nums);
        if output.ok {
            frame.render_widget(lines_par, line_nums_area);
        }

        let mut output_par = Paragraph::new(output.lines[range].join("\n"))
            .scroll((0, self.offset.x))
            .block(Block::default());

        if self.wrap {
            output_par = output_par.wrap(Wrap::default())
        };
        frame.render_widget(output_par, output_content_area);

        let scroll_bar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut state = ScrollbarState::new(self.output.len());
        state = state.position(self.offset.y.into());
        frame.render_stateful_widget(scroll_bar, vscroll_area, &mut state);

        let status_text = match self.error_display_mode {
            ErrorDisplayMode::Inline => {
                if self.main_output().ok {
                    " OK ".white().on_green()
                } else {
                    match self.main_output().status_code {
                        None => " ERR ".white().on_red(),
                        Some(code) => format!(" ERR({code}) ").white().on_red(),
                    }
                }
            }
            ErrorDisplayMode::Pane => Span::from(""),
        };

        let [hints_area, exit_code_area, lines_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Fill(1),
                Constraint::Length(status_text.width() as u16 + 1),
                Constraint::Length(self.output.len().to_string().len() as u16 + 3),
            ])
            .areas(status_area);

        let hints = {

            let mut spans: Vec<Span> = vec![];

            spans.push(" ^C Quit".into());
            spans.push(" | ".into());
            spans.push("F2 Err:".into());

            match self.error_display_mode {
                ErrorDisplayMode::Pane => {
                    spans.push("Pane".white().on_dark_gray());
                    spans.push("/Inline".into());
                }
                ErrorDisplayMode::Inline => {
                    spans.push("Pane/".into());
                    spans.push("Inline".white().on_dark_gray());
                }
            };

            spans.push(" | ".into());

            spans.push("F11 ".into());
            match self.input_mode {
                InputMode::Normal | InputMode::LiveFull => {
                    spans.push("Live UC".into());
                }
                InputMode::LiveUntilCursor => {
                    spans.push("Live UC".on_yellow());
                }
            }

            spans.push(" | ".into());
            spans.push("F12 ".into());
            match self.input_mode {
                InputMode::Normal | InputMode::LiveUntilCursor => {
                    spans.push("Live".into());
                }
                InputMode::LiveFull => {
                    spans.push("Live".on_yellow());
                }
            }

            Line::from_iter(spans).centered()
        };
        frame.render_widget(hints, hints_area);

        match self.error_display_mode {
            ErrorDisplayMode::Pane => (),
            ErrorDisplayMode::Inline => frame.render_widget(status_text, exit_code_area),
        }

        frame.render_widget(
            format!("L:{} ", self.output.len())
                .bold()
                .into_right_aligned_line(),
            lines_area,
        );

        #[rustfmt::skip]
        let lines = Text::from(vec![
            Line::from(format!("{:09} - Exit", "ctrl+c")),
            Line::from(""),
            Line::from(format!("{:09} - Execute full command", self.kb_config.execute_full.first().unwrap().to_string())),
            Line::from(format!("{:09} - Execute until cursor", self.kb_config.execute_until_current.first().unwrap().to_string())),
            Line::from(format!("{:09} - Execute before cursor", self.kb_config.execute_until_prev.first().unwrap().to_string())),
            Line::from(format!("{:09} - Reset input", self.kb_config.reset_input.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Go to previous subcommand", self.kb_config.subcommand_prev.first().unwrap().to_string())),
            Line::from(format!("{:09} - Go to next subcommand", self.kb_config.subcommand_next.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - History previous item", self.kb_config.history_prev.first().unwrap().to_string())),
            Line::from(format!("{:09} - History next item", self.kb_config.history_next.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Scroll up", self.kb_config.scroll_up.first().unwrap().to_string())),
            Line::from(format!("{:09} - Scroll down", self.kb_config.scroll_down.first().unwrap().to_string())),
            Line::from(format!("{:09} - Scroll page up", self.kb_config.scroll_up_page.first().unwrap().to_string())),
            Line::from(format!("{:09} - Scroll down", self.kb_config.scroll_down.first().unwrap().to_string())),
            Line::from(format!("{:09} - Scroll page down", self.kb_config.scroll_down_page.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Scroll right", self.kb_config.scroll_right.first().unwrap().to_string())),
            Line::from(format!("{:09} - Scroll left", self.kb_config.scroll_left.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Wrap output lines", self.kb_config.toggle_wrap.first().unwrap().to_string())),
        ]);

        if self.help {
            let popup = Popup::new(lines)
                .title(" Keys ")
                .style(Style::new().white().on_blue());
            frame.render_widget(popup, frame.area());
        }
    }

    fn main_output(&self) -> &Output {
        match self.error_display_mode {
            ErrorDisplayMode::Inline => self.error_output_opt.as_ref().unwrap_or(&self.output),
            ErrorDisplayMode::Pane => &self.output,
        }
    }
}

#[derive(PartialEq, Eq)]
struct Output {
    lines: Vec<String>,
    status_code: Option<i32>,
    ok: bool,
}

impl Output {
    fn ok(str: &str) -> Self {
        Self {
            lines: Self::lines(str),
            status_code: Some(0),
            ok: true,
        }
    }

    fn err(str: &str, status_code: Option<i32>) -> Self {
        Self {
            lines: Self::lines(str),
            status_code,
            ok: false,
        }
    }

    fn len(&self) -> usize {
        self.lines.len()
    }

    fn lines(input: &str) -> Vec<String> {
        input.lines().map(|a| a.into()).collect()
    }
}

fn handle_command_task(
    command_rx: Receiver<(String, String)>,
    action_tx: Sender<Action>,
) -> Result<(), Box<dyn Error>> {
    loop {
        if let Ok((command, stdin)) = command_rx.recv() {
            debug!("executing command: {command}");

            let mut cmd = Command::new("sh");
            cmd.args(["-c", &command]);

            let mut child = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to spawn command");

            let mut child_stdin = child.stdin.take().expect("handle present");

            let owned_stdin = stdin.to_owned();

            thread::spawn(move || {
                let _ = child_stdin.write_all(owned_stdin.as_bytes());
            });

            if let Ok(output) = child.wait_with_output() {
                if output.status.success() {
                    let stdout = output.stdout.as_slice();
                    let str = String::from_utf8_lossy(stdout);
                    action_tx.send(CommandCompleted(Output::ok(&str)))?;
                } else {
                    let stderr = output.stderr.as_slice();
                    let str = String::from_utf8_lossy(stderr);
                    action_tx.send(CommandCompleted(Output::err(&str, output.status.code())))?;
                }
            } else {
                action_tx.send(CommandCompleted(Output::err(
                    "Failed to execute command",
                    None,
                )))?;
            }
        }
    }
}

fn handle_input_task(tx: Sender<Action>) -> Result<(), Box<dyn Error>> {
    loop {
        if let Ok(event) = event::read() {
            // debug!("event: {:?}", event);
            tx.send(UserInput(event))?
        }
    }
}

fn read_stdin_task(file_opt: Option<String>, tx: Sender<Action>) -> Result<(), Box<dyn Error>> {
    if let Some(file) = file_opt {
        debug!("reading file {file}");
        let file_content = std::fs::read_to_string(file).expect("Failed to read file");
        tx.send(StdinRead(file_content))?;
        Ok(())
    } else {
        let mut input = String::new();
        let tty = stdin().is_tty();
        debug!("tty? {tty}");
        if !tty {
            debug!("reading input");
            stdin()
                .read_to_string(&mut input)
                .expect("Failed to read input");

            tx.send(StdinRead(input))?;
            Ok(())
        } else {
            debug!("skipping input");
            Ok(())
        }
    }
}

fn reset_highlight_task(rx: Receiver<()>, tx: Sender<Action>) -> Result<(), Box<dyn Error>> {
    loop {
        if let Ok(_) = rx.recv() {
            thread::sleep(Duration::from_millis(250));
            tx.send(ResetHighlight)?
        }
    }
}

enum Action {
    UserInput(Event),
    CommandCompleted(Output),
    StdinRead(String),
    ResetHighlight,
    Debounced,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandLinePlacement {
    #[default]
    Top,
    Bottom,
}

enum InputMode {
    Normal,
    LiveFull,
    LiveUntilCursor,
}

enum ErrorDisplayMode {
    Inline,
    Pane,
}
