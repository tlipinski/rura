use crate::Args;
use crate::app::Action::{CommandCompleted, ResetHighlight, StdinRead, UserInput};
use crate::config::{KeyBindingsConfig, ThemeConfig};
use crate::rura::Rura;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::tty::IsTty;
use log::{debug, warn};
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::Position;
use ratatui::prelude::{Line, Stylize};
use ratatui::style::Color;
use ratatui::style::{Style, Styled};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation};
use ratatui::widgets::{ScrollbarState, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::collections::VecDeque;
use std::error::Error;
use std::io::{Read, Write, stdin};
use std::ops::Range;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

pub struct App {
    command_input: Input,
    stdin: String,
    output: Output,
    offset: Position,
    history: VecDeque<String>,
    history_index: usize,
    wrap: bool,
    exit: bool,
    action_rx: Receiver<Action>,
    command_tx: Sender<(String, String)>,
    highlight_until: Option<usize>,
    pub highlight_tx: Sender<()>,
    theme: Theme,
    kb: KeyBindings,
}

impl App {
    pub fn new(args: Args, theme_config: &ThemeConfig, kb_config: &KeyBindingsConfig) -> Self {
        let (action_tx, action_rx) = std::sync::mpsc::channel::<Action>();
        let (command_tx, command_rx) = std::sync::mpsc::channel::<(String, String)>();
        let (highlight_tx, highlight_rx) = std::sync::mpsc::channel::<()>();

        let s1 = action_tx.clone();
        thread::spawn(move || handle_input_task(s1).unwrap());

        let s2 = action_tx.clone();
        thread::spawn(move || handle_command_task(command_rx, s2).unwrap());

        let s3 = action_tx.clone();
        thread::spawn(move || read_stdin_task(args.file, s3).unwrap());

        let s4 = action_tx.clone();
        thread::spawn(move || reset_highlight_task(highlight_rx, s4).unwrap());

        Self {
            command_input: Input::from(""),
            stdin: "".to_string(),
            offset: Position::default(),
            output: Output::ok(""),
            history: VecDeque::new(),
            action_rx,
            command_tx,
            highlight_tx,
            history_index: 0,
            wrap: false,
            exit: false,
            highlight_until: None,
            theme: Theme::from_config(theme_config),
            kb: KeyBindings::from_config(kb_config),
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<String, Box<dyn Error>> {
        while !self.exit {
            terminal.draw(|frame| self.render(frame, frame.area()))?;

            let action = self.action_rx.recv()?;
            self.handle_action(action);
        }

        Ok(self.command_input.value().to_string())
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            UserInput(event) => self.handle_key_event(&event),
            CommandCompleted(output) => self.handle_command_output(output),
            ResetHighlight => self.highlight_until = None,
            StdinRead(stdin) => {
                self.stdin = stdin;
                self.output = Output::ok(&self.stdin);
            }
        }
    }

    fn handle_command_output(&mut self, output: Output) {
        if self.output.len() != output.len() {
            self.offset.y = 0;
        }

        self.output = output;
    }

    pub fn handle_key_event(&mut self, event: &Event) {
        if let Event::Key(key_event) = event {
            let code = key_event.code;
            let mods = key_event.modifiers;
            let kb = &self.kb;

            if key_matches(&kb.quit, code, mods) {
                self.exit = true;
            } else if key_matches(&kb.execute_until_prev, code, mods) {
                if self.command_input.value().is_empty() {
                    self.output = Output::ok(&self.stdin);
                    return;
                }
                self.push_history();
                match Rura::new(self.command_input.value(), self.command_input.visual_cursor()) {
                    Ok(r) => {
                        let (cmd, cmd_index) = r.command_until_current_prev();
                        self.highlight_until = Some(cmd_index);
                        self.highlight_tx.send(()).unwrap();
                        self.command_tx.send((cmd, self.stdin.clone())).unwrap()
                    }
                    Err(_) => warn!("Invalid command: {}", self.command_input.value()),
                };
            } else if key_matches(&kb.execute_until_current, code, mods) {
                if self.command_input.value().is_empty() {
                    self.output = Output::ok(&self.stdin);
                    return;
                }
                self.push_history();
                match Rura::new(self.command_input.value(), self.command_input.visual_cursor()) {
                    Ok(r) => {
                        let (cmd, cmd_index) = r.command_until_current();
                        self.highlight_until = Some(cmd_index);
                        self.highlight_tx.send(()).unwrap();
                        self.command_tx.send((cmd, self.stdin.clone())).unwrap()
                    }
                    Err(_) => warn!("Invalid command: {}", self.command_input.value()),
                };
            } else if key_matches(&kb.execute_full, code, mods) {
                if self.command_input.value().is_empty() {
                    self.output = Output::ok(&self.stdin);
                    return;
                }
                self.push_history();
                match Rura::new(self.command_input.value(), self.command_input.visual_cursor()) {
                    Ok(r) => {
                        let (cmd, cmd_index) = r.command_full();
                        self.highlight_until = Some(cmd_index);
                        self.highlight_tx.send(()).unwrap();
                        self.command_tx.send((cmd, self.stdin.clone())).unwrap()
                    }
                    Err(_) => warn!("Invalid command: {}", self.command_input.value()),
                };
            } else if key_matches(&kb.reset_input, code, mods) {
                let new_output = Output::ok(&self.stdin);
                if self.output.len() != new_output.len() {
                    self.offset.y = 0;
                }
                self.output = new_output;
            } else if key_matches(&kb.scroll_down, code, mods) {
                self.offset.y = self.offset.y.saturating_add(1);
            } else if key_matches(&kb.scroll_down_page, code, mods) {
                self.offset.y = self.offset.y.saturating_add(10);
            } else if key_matches(&kb.scroll_up, code, mods) {
                self.offset.y = self.offset.y.saturating_sub(1);
            } else if key_matches(&kb.scroll_up_page, code, mods) {
                self.offset.y = self.offset.y.saturating_sub(10);
            } else if key_matches(&kb.scroll_left, code, mods) {
                self.offset.x = self.offset.x.saturating_sub(1);
            } else if key_matches(&kb.scroll_right, code, mods) {
                self.offset.x = self.offset.x.saturating_add(1);
            } else if key_matches(&kb.toggle_wrap, code, mods) {
                self.wrap = !self.wrap;
            } else if key_matches(&kb.history_prev, code, mods) {
                if !self.history.is_empty() {
                    self.history_index =
                        (self.history_index + 1).min(self.history.len() - 1);
                    self.command_input =
                        Input::from(self.history[self.history_index].clone());
                }
            } else if key_matches(&kb.history_next, code, mods) {
                if !self.history.is_empty() {
                    self.history_index = self.history_index.saturating_sub(1);
                    self.command_input =
                        Input::from(self.history[self.history_index].clone());
                }
            } else {
                self.command_input.handle_event(event);
            }
        }
    }

    fn push_history(&mut self) {
        let should_add = match self.history.front() {
            Some(most_recent) if most_recent != self.command_input.value() => true,
            Some(_duplicate) => false,
            None => true,
        };
        if should_add {
            self.history.push_front(self.command_input.value().to_string());
            self.history_index = 0;
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let theme = &self.theme;

        let [command_input_area, output_area, status_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(3),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(area);

        let line_nums_width = self.output.len().to_string().len();
        let [line_nums_area, output_content_area, vscroll_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length((line_nums_width + 1) as u16),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(output_area);

        let max_cursor_pos = (command_input_area.inner(Margin::new(1, 0)).width - 1) as usize;

        let command_input_par = {
            let block = Block::bordered();
            let line = Line::from(self.command_input.value());
            let offset = self
                .command_input
                .visual_cursor()
                .saturating_sub(max_cursor_pos);

            let rura = Rura::new(
                self.command_input.value(),
                self.command_input.visual_cursor(),
            );

            match rura {
                Ok(ref r) => {
                    let mut spans = vec![];

                    for (index, part) in r.subcommands.iter().enumerate() {
                        match self.highlight_until {
                            None => {
                                if index > 0 {
                                    spans.push("|".set_style(theme.cmd_regular_pipe));
                                }

                                if index == r.current {
                                    spans.push(part.clone().set_style(theme.cmd_regular_current));
                                } else {
                                    spans.push(part.clone().set_style(theme.cmd_regular));
                                }
                            }
                            Some(until) => {
                                if index <= until {
                                    if index > 0 {
                                        spans.push("|".set_style(theme.cmd_highlight_pipe));
                                    }

                                    if index == r.current {
                                        spans.push(
                                            part.clone().set_style(theme.cmd_highlight_current),
                                        );
                                    } else {
                                        spans.push(part.clone().set_style(theme.cmd_highlight));
                                    }
                                } else {
                                    if index > 0 {
                                        spans.push("|".set_style(theme.cmd_regular_pipe));
                                    }

                                    if index == r.current {
                                        spans.push(
                                            part.clone().set_style(theme.cmd_regular_current),
                                        );
                                    } else {
                                        spans.push(part.clone().set_style(theme.cmd_regular));
                                    }
                                }
                            }
                        }
                    }

                    Paragraph::new(Line::from_iter(spans))
                        .scroll((0, offset as u16))
                        .block(block)
                }
                Err(_) => Paragraph::new(line)
                    .scroll((0, offset as u16))
                    .block(block)
                    .set_style(theme.cmd_invalid),
            }
        };
        let x = self.command_input.visual_cursor().min(max_cursor_pos);
        // debug!("vcur: {}", self.command_input.visual_cursor());
        frame.set_cursor_position((area.x + (x + 1) as u16, area.y + 1));
        frame.render_widget(command_input_par, command_input_area);

        let height = output_content_area.height.min(self.output.len() as u16);
        // debug!("height: {height:?}");
        let range: Range<usize> = if height >= self.output.len() as u16 {
            0..self.output.len()
        } else {
            let from = (self.offset.y as usize).min(self.output.len());
            let to = (self.offset.y as usize + height as usize).min(self.output.len());
            from..to
        };

        // debug!("range: {range:?}");

        let line_nums = range
            .clone()
            .map(|i| format!("{: >pad$}", i + 1, pad = line_nums_width))
            .collect::<Vec<String>>();
        let lines_par = Paragraph::new(line_nums.join("\n")).set_style(theme.line_nums);
        if self.output.ok {
            frame.render_widget(lines_par, line_nums_area);
        }

        let mut output_par =
            Paragraph::new(self.output.lines[range].join("\n")).scroll((0, self.offset.x));

        if self.wrap {
            output_par = output_par.wrap(Wrap::default())
        };
        frame.render_widget(output_par, output_content_area);

        let scroll_bar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut state = ScrollbarState::new(self.output.len());
        state = state.position(self.offset.y.into());
        frame.render_stateful_widget(scroll_bar, vscroll_area, &mut state);

        frame.render_widget(
            format!("Lines: {} ", self.output.len())
                .bold()
                .into_right_aligned_line(),
            status_area,
        )
    }
}

struct Output {
    lines: Vec<String>,
    ok: bool,
}

impl Output {
    fn ok(str: &str) -> Self {
        Self {
            lines: Self::lines(str),
            ok: true,
        }
    }

    fn err(str: &str) -> Self {
        Self {
            lines: Self::lines(str),
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
                    action_tx.send(CommandCompleted(Output::err(&str)))?;
                }
            } else {
                action_tx.send(CommandCompleted(Output::err("Failed to execute command")))?;
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
            thread::sleep(Duration::from_millis(150));
            tx.send(ResetHighlight)?
        }
    }
}

enum Action {
    UserInput(Event),
    CommandCompleted(Output),
    StdinRead(String),
    ResetHighlight,
}

struct Theme {
    pub cmd_regular: Style,
    pub cmd_regular_pipe: Style,
    pub cmd_regular_current: Style,

    pub cmd_highlight: Style,
    pub cmd_highlight_pipe: Style,
    pub cmd_highlight_current: Style,

    pub cmd_invalid: Style,

    pub line_nums: Style,
}

impl Theme {
    fn from_config(config: &ThemeConfig) -> Self {
        Theme {
            cmd_regular: style_from_config(&config.cmd_regular),
            cmd_regular_pipe: style_from_config(&config.cmd_regular_pipe),
            cmd_regular_current: style_from_config(&config.cmd_regular_current),
            cmd_highlight: style_from_config(&config.cmd_highlight),
            cmd_highlight_pipe: style_from_config(&config.cmd_highlight_pipe),
            cmd_highlight_current: style_from_config(&config.cmd_highlight_current),
            cmd_invalid: style_from_config(&config.cmd_invalid),
            line_nums: style_from_config(&config.line_nums),
        }
    }
}

fn style_from_config(sc: &crate::config::StyleConfig) -> Style {
    let mut s = Style::default();
    if let Some(c) = sc.fg.as_deref().and_then(parse_color) { s = s.fg(c); }
    if let Some(c) = sc.bg.as_deref().and_then(parse_color) { s = s.bg(c); }
    if sc.bold.unwrap_or(false) { s = s.bold(); }
    if sc.italic.unwrap_or(false) { s = s.italic(); }
    if sc.underlined.unwrap_or(false) { s = s.underlined(); }
    if sc.dim.unwrap_or(false) { s = s.dim(); }
    s
}

fn parse_color(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "dark_gray" => Some(Color::DarkGray),
        "lightred" | "light_red" => Some(Color::LightRed),
        "lightgreen" | "light_green" => Some(Color::LightGreen),
        "lightyellow" | "light_yellow" => Some(Color::LightYellow),
        "lightblue" | "light_blue" => Some(Color::LightBlue),
        "lightmagenta" | "light_magenta" => Some(Color::LightMagenta),
        "lightcyan" | "light_cyan" => Some(Color::LightCyan),
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).ok()?;
            let g = u8::from_str_radix(&s[3..5], 16).ok()?;
            let b = u8::from_str_radix(&s[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        s => s.parse::<u8>().ok().map(Color::Indexed),
    }
}

struct KeyBindings {
    quit: Vec<(KeyCode, KeyModifiers)>,
    execute_full: Vec<(KeyCode, KeyModifiers)>,
    execute_until_current: Vec<(KeyCode, KeyModifiers)>,
    execute_until_prev: Vec<(KeyCode, KeyModifiers)>,
    reset_input: Vec<(KeyCode, KeyModifiers)>,
    scroll_down: Vec<(KeyCode, KeyModifiers)>,
    scroll_down_page: Vec<(KeyCode, KeyModifiers)>,
    scroll_up: Vec<(KeyCode, KeyModifiers)>,
    scroll_up_page: Vec<(KeyCode, KeyModifiers)>,
    scroll_left: Vec<(KeyCode, KeyModifiers)>,
    scroll_right: Vec<(KeyCode, KeyModifiers)>,
    toggle_wrap: Vec<(KeyCode, KeyModifiers)>,
    history_prev: Vec<(KeyCode, KeyModifiers)>,
    history_next: Vec<(KeyCode, KeyModifiers)>,
}

impl KeyBindings {
    fn from_config(config: &KeyBindingsConfig) -> Self {
        KeyBindings {
            quit: parse_bindings(&config.quit),
            execute_full: parse_bindings(&config.execute_full),
            execute_until_current: parse_bindings(&config.execute_until_current),
            execute_until_prev: parse_bindings(&config.execute_until_prev),
            reset_input: parse_bindings(&config.reset_input),
            scroll_down: parse_bindings(&config.scroll_down),
            scroll_down_page: parse_bindings(&config.scroll_down_page),
            scroll_up: parse_bindings(&config.scroll_up),
            scroll_up_page: parse_bindings(&config.scroll_up_page),
            scroll_left: parse_bindings(&config.scroll_left),
            scroll_right: parse_bindings(&config.scroll_right),
            toggle_wrap: parse_bindings(&config.toggle_wrap),
            history_prev: parse_bindings(&config.history_prev),
            history_next: parse_bindings(&config.history_next),
        }
    }
}

fn parse_bindings(keys: &[String]) -> Vec<(KeyCode, KeyModifiers)> {
    keys.iter().filter_map(|s| parse_key_binding(s)).collect()
}

fn parse_key_binding(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    // Split into parts; everything before the last segment is a modifier.
    // Use splitn with a high limit to get all segments.
    let parts: Vec<&str> = s.splitn(10, '+').collect();
    if parts.is_empty() {
        return None;
    }

    let (modifier_parts, key_parts) = parts.split_at(parts.len() - 1);
    let key_str = key_parts[0].to_lowercase();

    let mut modifiers = KeyModifiers::NONE;
    for part in modifier_parts {
        match part.to_lowercase().as_str() {
            "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => return None,
        }
    }

    let code = match key_str.as_str() {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        s if s.chars().count() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        _ => return None,
    };

    Some((code, modifiers))
}

fn key_matches(bindings: &[(KeyCode, KeyModifiers)], code: KeyCode, mods: KeyModifiers) -> bool {
    bindings.iter().any(|(c, m)| *c == code && *m == mods)
}
