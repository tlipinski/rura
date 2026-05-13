use crate::Args;
use crate::app::Action::{CommandCompleted, Debounced, ResetHighlight, StdinRead, UserInput};
use crate::completion::ShCompleter;
use crate::config::{KeyBindingsConfig, ThemeConfig};
use crate::debouncer::debouncer_task;
use crate::history::History;
use crate::output_widget::{ErrorDisplayMode, ErrorPanePlacement, Output, OutputWidget};
use crate::rura::ExecuteType;
use crate::rura_widget::RuraWidget;
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use KeyCode::{Enter, Esc, F};
use crossterm::event::KeyCode::Char;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::tty::IsTty;
use log::{debug, info};
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::Span;
use ratatui::prelude::Stylize;
use ratatui::style::Color::Yellow;
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};
use ratatui::{DefaultTerminal, Frame};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::{Read, Write, stdin};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;
use tui_popup::Popup;

pub struct App {
    rura_widget: RuraWidget,
    output_widget: OutputWidget,
    stdin: String,
    exit: bool,
    action_rx: Receiver<Action>,
    command_tx: Sender<(String, String)>,
    key_bindings: KeyBindings,
    command_line_placement: CommandLinePlacement,
    kb_config: KeyBindingsConfig,
    help: bool,
    input_mode: InputMode,
    debouncer_tx: Sender<()>,
    confirming_live: Option<InputMode>,
    search_input: Input,
    searching: bool,
    case_sensitive: bool,
}

impl App {
    pub fn new(
        args: Args,
        theme_config: &ThemeConfig,
        kb_config: KeyBindingsConfig,
        command_line_placement: CommandLinePlacement,
        error_display_mode: ErrorDisplayMode,
        highlight_duration_ms: u64,
        debounce_duration_ms: u64,
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
        thread::spawn(move || {
            reset_highlight_task(highlight_reset_rx, s4, highlight_duration_ms).unwrap()
        });

        thread::spawn(move || {
            debouncer_task(
                debouncer_rx,
                Duration::from_millis(debounce_duration_ms),
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
                command_input: Input::from(args.command.unwrap_or_default()),
                highlight_until: None,
                theme: Theme::from_config(theme_config),
                history: History::load(),
                key_bindings: KeyBindings::from_config(&kb_config),
                highlight_reset_tx,
                completions: None,
                completer: Box::new(ShCompleter {}),
            },
            output_widget: OutputWidget::new(
                theme_config,
                &kb_config,
                match command_line_placement {
                    CommandLinePlacement::Top => ErrorPanePlacement::Top,
                    CommandLinePlacement::Bottom => ErrorPanePlacement::Bottom,
                },
                error_display_mode,
            ),
            stdin: "".into(),
            action_rx,
            command_tx,
            debouncer_tx,
            exit: false,
            key_bindings: KeyBindings::from_config(&kb_config),
            command_line_placement,
            kb_config,
            help: false,
            input_mode: InputMode::Normal,
            confirming_live: None,
            search_input: Input::from(""),
            searching: false,
            case_sensitive: false,
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
            CommandCompleted(output) => {
                if output.ok {
                    if let Some(c) = &output.command {
                        self.rura_widget.history.push(c)
                    }
                }
                self.output_widget.handle_command_output(output)
            }
            ResetHighlight => self.rura_widget.highlight_until = None,
            StdinRead(output) => {
                self.output_widget
                    .handle_command_output(Output::ok_stdin(&output));
                self.stdin = output;
            }
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
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;
                let key_bindings = &self.key_bindings;

                if let Some(confirming_live) = self.confirming_live.clone() {
                    match (code, mods) {
                        (Esc | Char('n'), KeyModifiers::NONE) => self.confirming_live = None,
                        (Char('y'), KeyModifiers::NONE) => {
                            self.confirming_live = None;
                            self.input_mode = confirming_live;
                        }
                        _ => match to_ui_command(key_bindings, code, mods) {
                            Some(UiCmd::Quit) => {
                                self.exit = true;
                            }
                            _ => {}
                        },
                    }
                    return;
                }

                match (code, mods) {
                    (Esc, KeyModifiers::NONE) => {
                        self.help = false;
                        if self.searching {
                            self.searching = false;
                        } else {
                            self.searching = false;
                            self.output_widget.clear_highlight();
                        }
                    }
                    (F(1), KeyModifiers::NONE) => {
                        self.help = !self.help;
                    }
                    (F(11), KeyModifiers::NONE) => match self.input_mode {
                        InputMode::Normal => {
                            // self.input_mode = InputMode::LiveUntilCursor;
                            self.confirming_live = Some(InputMode::LiveUntilCursor);
                        }
                        InputMode::LiveFull => {
                            self.input_mode = InputMode::LiveUntilCursor;
                        }
                        InputMode::LiveUntilCursor => {
                            self.input_mode = InputMode::Normal;
                        }
                    },
                    (F(12), KeyModifiers::NONE) => match self.input_mode {
                        InputMode::Normal => {
                            // self.input_mode = InputMode::LiveFull;
                            self.confirming_live = Some(InputMode::LiveFull);
                        }
                        InputMode::LiveFull => {
                            self.input_mode = InputMode::Normal;
                        }
                        InputMode::LiveUntilCursor => {
                            self.input_mode = InputMode::LiveFull;
                        }
                    },
                    (Enter, KeyModifiers::NONE) if self.searching => {
                        self.output_widget
                            .highlight(self.search_input.value(), self.case_sensitive);
                    }
                    (F(3), KeyModifiers::NONE) => {
                        if self.searching {
                            self.output_widget.highlight_next();
                        } else {
                            self.searching = true;
                        }
                    }
                    (F(4), KeyModifiers::NONE) => {
                        if self.searching {
                            self.output_widget.highlight_prev();
                        } else {
                            self.searching = true;
                        }
                    }
                    (Char('c'), KeyModifiers::ALT) => {
                        self.case_sensitive = !self.case_sensitive;
                        self.output_widget
                            .highlight(self.search_input.value(), self.case_sensitive);
                    }
                    _ => match to_ui_command(key_bindings, code, mods) {
                        None => {
                            if self.searching {
                                self.search_input.handle_event(event);
                                self.output_widget
                                    .highlight(self.search_input.value(), self.case_sensitive);
                            } else {
                                if self.rura_widget.handle_event(event) {
                                    match self.input_mode {
                                        InputMode::Normal => {}
                                        InputMode::LiveFull | InputMode::LiveUntilCursor => {
                                            self.debouncer_tx.send(()).unwrap();
                                        }
                                    }
                                }
                            }
                        }
                        Some(a) => match a {
                            UiCmd::Quit => {
                                self.exit = true;
                            }
                            UiCmd::ExecuteFull if !self.searching => {
                                self.handle_execute(ExecuteType::Full);
                            }
                            UiCmd::ExecuteUntilCurrent if !self.searching => {
                                self.handle_execute(ExecuteType::UntilCurrent)
                            }
                            UiCmd::ExecuteUntilPrev if !self.searching => {
                                self.handle_execute(ExecuteType::UntilCurrentPrev)
                            }
                            UiCmd::ResetInput if !self.searching => {
                                self.output_widget
                                    .handle_command_output(Output::ok_stdin(&self.stdin));
                            }
                            UiCmd::SubcommandNext | UiCmd::SubcommandPrev if !self.searching => {
                                self.rura_widget.handle_event(event);
                            }
                            UiCmd::HistoryNext
                            | UiCmd::HistoryPrev
                            | UiCmd::Complete
                            | UiCmd::CompletePrev
                                if !self.searching =>
                            {
                                // disable history and completions in live mode
                                if matches!(self.input_mode, InputMode::Normal) {
                                    self.rura_widget.handle_event(event);
                                }
                            }
                            _ => {
                                self.output_widget.handle_event(event);
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
            Ok(Some(c)) => self.command_tx.send((c, self.stdin.clone())).unwrap(),
            Ok(None) => self
                .output_widget
                .handle_command_output(Output::ok_stdin(&self.stdin)),
            Err(_) => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let margin = Margin::new(1, 1);

        let inner_area = area.inner(margin);

        let (command_input_area, search_input_area, output_area, status_area) =
            match self.command_line_placement {
                CommandLinePlacement::Top => {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![
                            Constraint::Length(self.rura_widget.height(inner_area.width) + 2), // command
                            Constraint::Length(if self.searching { 3 } else { 0 }), // search
                            Constraint::Fill(1),                                    // output
                            Constraint::Length(1),                                  // status
                        ])
                        .split(area);

                    (layout[0], layout[1], layout[2], layout[3])
                }
                CommandLinePlacement::Bottom => {
                    let layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![
                            Constraint::Fill(1),                                               // output
                            Constraint::Length(if self.searching { 3 } else { 0 }), // search
                            Constraint::Length(self.rura_widget.height(inner_area.width) + 2), // command
                            Constraint::Length(1), // status
                        ])
                        .split(area);

                    (layout[2], layout[1], layout[0], layout[3])
                }
            };

        if self.searching {
            let (current, total) = self.output_widget.highlight_info();
            let par =
                Paragraph::new(self.search_input.value()).block(Block::bordered().title(format!(
                    " Search: {} / {} | {} ",
                    if total == 0 { 0 } else { current + 1 },
                    total,
                    if self.case_sensitive { "[Cc]" } else { "Cc" }
                )));
            par.render(search_input_area, frame.buffer_mut());
        }

        let command_input_block = if matches!(self.input_mode, InputMode::Normal) {
            Block::bordered()
        } else {
            Block::bordered()
                .border_style(Style::default().fg(Yellow))
                .border_type(BorderType::Thick)
        };

        frame.render_widget(command_input_block, command_input_area);
        frame.render_widget(&self.rura_widget, command_input_area.inner(margin));

        if self.searching {
            frame.render_widget(
                Block::default().reversed(),
                command_input_area.inner(margin),
            );
        }

        if self.searching {
            let x = self.search_input.visual_cursor() as u16;
            frame.set_cursor_position((search_input_area.x + 1 + x, search_input_area.y + 1));
        } else {
            let inner_rect = command_input_area.inner(margin);
            let (x, y) = self.rura_widget.cursor(inner_rect.width);
            frame.set_cursor_position((command_input_area.x + 1 + x, command_input_area.y + 1 + y));
        }

        frame.render_widget(&mut self.output_widget, output_area);

        let status_text = match self.output_widget.error_display_mode {
            ErrorDisplayMode::Inline => {
                if self.output_widget.main_output().ok {
                    " OK ".white().on_green()
                } else {
                    match self.output_widget.main_output().status_code {
                        None => " ERR ".white().on_red(),
                        Some(code) => format!(" ERR({code}) ").white().on_red(),
                    }
                }
            }
            ErrorDisplayMode::Pane => Span::from(""),
        };

        let [_, exit_code_area, hints_area, lines_area, _] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Length(status_text.width() as u16 + 1),
                Constraint::Fill(1),
                Constraint::Length(self.output_widget.output_len().to_string().len() as u16 + 3),
                Constraint::Length(1),
            ])
            .areas(status_area);

        frame.render_widget(self.hints_widget(), hints_area);

        match self.output_widget.error_display_mode {
            ErrorDisplayMode::Pane => (),
            ErrorDisplayMode::Inline => frame.render_widget(status_text, exit_code_area),
        }

        frame.render_widget(
            format!("L:{}", self.output_widget.output_len())
                .bold()
                .into_right_aligned_line(),
            lines_area,
        );

        self.render_help(frame);
        self.render_live_confirm(frame);
    }

    fn render_live_confirm(&self, frame: &mut Frame) {
        if self.confirming_live.is_some() {
            let body = Text::from(vec![
                Line::from("").centered(),
                Line::from("   Warning: This might be dangerous!   ")
                    .centered()
                    .bold(),
                Line::from("").centered(),
                Line::from("   Commands will be executed automatically as you type.   ").centered(),
                Line::from("").centered(),
                Line::from("[Y]es / [N]o").centered(),
                Line::from("").centered(),
            ]);
            let popup = Popup::new(body)
                .title(" Confirm entering LIVE mode ")
                .style(Style::new().white().on_yellow());
            frame.render_widget(popup, frame.area());
        }
    }

    fn render_help(&self, frame: &mut Frame) {
        if self.help {
            #[rustfmt::skip]
        let lines = Text::from(vec![
            Line::from(format!("{:09} - Execute full command", self.kb_config.execute_full.first().unwrap().to_string())),
            Line::from(format!("{:09} - Execute until cursor", self.kb_config.execute_until_current.first().unwrap().to_string())),
            Line::from(format!("{:09} - Execute before cursor", self.kb_config.execute_until_prev.first().unwrap().to_string())),
            Line::from(format!("{:09} - Reset input", self.kb_config.reset_input.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Search ↓", "F3")),
            Line::from(format!("{:09} - Search ↑", "F4")),
            Line::from(format!("{:09} - Switch case sensitivity", "alt+c")),
            Line::from(""),
            Line::from(format!("{:09} - Complete forward", self.kb_config.complete.first().unwrap().to_string())),
            Line::from(format!("{:09} - Complete backward", self.kb_config.complete_prev.first().unwrap().to_string())),
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
            Line::from(format!("{:09} - Scroll page down", self.kb_config.scroll_down_page.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Scroll right", self.kb_config.scroll_right.first().unwrap().to_string())),
            Line::from(format!("{:09} - Scroll left", self.kb_config.scroll_left.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:09} - Wrap output lines", self.kb_config.toggle_wrap.first().unwrap().to_string())),
        ]);

            let popup = Popup::new(lines)
                .title(" Keys ")
                .style(Style::new().white().on_blue());
            frame.render_widget(popup, frame.area());
        }
    }

    fn hints_widget(&self) -> Line<'_> {
        let mut spans: Vec<Span> = vec![];

        spans.push(" ".into());
        spans.push("^C".bold());
        spans.push(" Quit ".into());
        spans.push("Enter".bold());
        spans.push(" Execute ".into());
        spans.push("F1".bold());
        spans.push(" Help ".into());
        spans.push("F3".bold());
        spans.push("/".into());
        spans.push("F4".bold());
        spans.push(" Search ↓/↑ ".into());
        spans.push("F11 ".bold());
        match self.input_mode {
            InputMode::Normal | InputMode::LiveFull => {
                spans.push("Live UC".into());
            }
            InputMode::LiveUntilCursor => {
                spans.push("Live UC".reversed());
            }
        }

        spans.push(" ".into());
        spans.push("F12 ".bold());
        match self.input_mode {
            InputMode::Normal | InputMode::LiveUntilCursor => {
                spans.push("Live".into());
            }
            InputMode::LiveFull => {
                spans.push("Live".reversed());
            }
        }

        Line::from_iter(spans).centered().dim()
    }
}

fn handle_command_task(
    command_rx: Receiver<(String, String)>,
    action_tx: Sender<Action>,
) -> Result<(), Box<dyn Error>> {
    loop {
        if let Ok((command, stdin)) = command_rx.recv() {
            info!("executing command: '{command}'");

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
                    action_tx.send(CommandCompleted(Output::ok_command(&command, &str)))?;
                } else {
                    let stderr = output.stderr.as_slice();
                    let str = String::from_utf8_lossy(stderr);
                    action_tx.send(CommandCompleted(Output::err_command(
                        &command,
                        &str,
                        output.status.code(),
                    )))?;
                }
            } else {
                action_tx.send(CommandCompleted(Output::err_command(
                    &command,
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
            debug!("event: {:?}", event);
            tx.send(UserInput(event))?
        }
    }
}

fn read_stdin_task(file_opt: Option<String>, tx: Sender<Action>) -> Result<(), Box<dyn Error>> {
    if let Some(file) = file_opt {
        info!("reading file {file}");
        let file_content = std::fs::read_to_string(file);
        match file_content {
            Ok(content) => {
                tx.send(StdinRead(content))?;
            }
            Err(e) => {
                tx.send(StdinRead(e.to_string()))?;
            }
        }
        Ok(())
    } else {
        let mut buff = String::new();
        let tty = stdin().is_tty();
        if !tty {
            let result = stdin().read_to_string(&mut buff);

            match result {
                Ok(_) => {
                    tx.send(StdinRead(buff))?;
                }
                Err(e) => {
                    tx.send(StdinRead(e.to_string()))?;
                }
            }
            Ok(())
        } else {
            Ok(())
        }
    }
}

fn reset_highlight_task(
    rx: Receiver<()>,
    tx: Sender<Action>,
    duration_ms: u64,
) -> Result<(), Box<dyn Error>> {
    loop {
        if let Ok(_) = rx.recv() {
            thread::sleep(Duration::from_millis(duration_ms));
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
    use crossterm::event::Event::Key;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(100, 30)).unwrap())
        }
    }

    impl Default for App {
        fn default() -> Self {
            let (_, action_rx) = std::sync::mpsc::channel::<Action>();
            let (command_tx, _) = std::sync::mpsc::channel::<(String, String)>();
            let (highlight_reset_tx, _) = std::sync::mpsc::channel::<()>();
            let (debouncer_tx, _) = std::sync::mpsc::channel::<()>();

            let theme_config = ThemeConfig::default();
            let kb_config = KeyBindingsConfig::default();

            Self {
                rura_widget: RuraWidget {
                    command_input: Input::from(""),
                    highlight_until: None,
                    theme: Theme::from_config(&theme_config),
                    history: History::load(),
                    key_bindings: KeyBindings::from_config(&kb_config),
                    highlight_reset_tx,
                    completions: None,
                    completer: Box::new(ShCompleter {}),
                },
                output_widget: OutputWidget::new(
                    &theme_config,
                    &kb_config,
                    ErrorPanePlacement::Bottom,
                    ErrorDisplayMode::Pane,
                ),
                search_input: Input::new("".into()),
                searching: false,
                stdin: "".into(),
                action_rx,
                command_tx,
                debouncer_tx,
                exit: false,
                key_bindings: KeyBindings::from_config(&kb_config),
                command_line_placement: CommandLinePlacement::Bottom,
                kb_config,
                help: false,
                input_mode: InputMode::Normal,
                confirming_live: None,
                case_sensitive: true,
            }
        }
    }

    #[test]
    fn main_screen() {
        let mut app = App::default();

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
