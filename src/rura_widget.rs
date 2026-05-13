use crate::completion::{Completer, CompletionResult};
use crate::history::History;
use crate::rura::{ExecuteType, Part, Rura};
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use crossterm::event::Event;
use error::Error;
use itertools::Itertools;
use log::info;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::prelude::{Line, Style, Widget};
use ratatui::style::Styled;
use ratatui::text::StyledGrapheme;
use std::error;
use std::sync::mpsc::Sender;
use tui_input::backend::crossterm::EventHandler;
use tui_input::{Input, InputRequest};
use unicode_width::UnicodeWidthStr;

pub struct RuraWidget {
    pub command_input: Input,
    pub highlight_until: Option<usize>,
    pub theme: Theme,
    pub key_bindings: KeyBindings,
    pub history: History,
    pub highlight_reset_tx: Sender<()>,
    pub completions: Option<(CompletionResult, usize)>,
    pub completer: Box<dyn Completer>,
}

impl Widget for &RuraWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let command_input_line = {
            match Rura::new(
                self.command_input.value(),
                self.command_input.visual_cursor(),
            ) {
                Ok(r) => to_line(r, self.highlight_until, &self.theme),
                Err(_) => Line::from(self.command_input.value()),
            }
        };

        let graphemes = command_input_line
            .styled_graphemes(Style::default())
            .collect_vec();

        let chunks = graphemes.chunks(area.width as usize);

        for (i, c) in chunks.enumerate() {
            render_line(c.to_vec(), area, buf, i as u16)
        }
    }
}

impl RuraWidget {
    pub fn height(&self, width: u16) -> u16 {
        (self.command_input.value().len() as u16 / width) + 1
    }

    pub fn cursor(&self, width: u16) -> (u16, u16) {
        let cursor = self.command_input.visual_cursor() as u16;
        (cursor % width, cursor / width)
    }

    // returns bool - indicates if the command value was modified
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;
                let key_bindings = &self.key_bindings;

                match to_ui_command(key_bindings, code, mods) {
                    None => {
                        self.completions = None;
                        self.command_input
                            .handle_event(event)
                            .map(|change| change.value)
                            .unwrap_or(false)
                    }
                    Some(ui_cmd) => self.handle_ui_command(ui_cmd),
                }
            }
            _ => false,
        }
    }

    fn handle_ui_command(&mut self, ui_cmd: UiCmd) -> bool {
        match ui_cmd {
            UiCmd::Complete | UiCmd::CompletePrev => {
                let current_value = self.command_input.value().to_string();
                let cursor_pos = self.command_input.visual_cursor();

                if let Some((res, index)) = self.completions.as_mut() {
                    if ui_cmd == UiCmd::Complete {
                        *index = (*index + 1) % res.completions.len();
                    } else {
                        *index = if *index == 0 {
                            res.completions.len() - 1
                        } else {
                            *index - 1
                        };
                    }
                    let completion = &res.completions[*index];
                    let new_value = format!(
                        "{}{}{}",
                        &current_value[..res.word_start],
                        completion,
                        &current_value[cursor_pos..]
                    );
                    self.command_input = Input::from(new_value);
                    self.command_input
                        .handle(InputRequest::SetCursor(res.word_start + completion.len()));
                } else if let Some(res) = self.completer.completions(&current_value, cursor_pos) {
                    let index = if ui_cmd == UiCmd::Complete {
                        0
                    } else {
                        res.completions.len() - 1
                    };
                    let word_start = res.word_start;
                    let completion = res.completions[index].clone();
                    let new_value = format!(
                        "{}{}{}",
                        &current_value[..word_start],
                        completion,
                        &current_value[cursor_pos..]
                    );
                    self.completions = Some((res, index));
                    self.command_input = Input::from(new_value);
                    self.command_input
                        .handle(InputRequest::SetCursor(word_start + completion.len()));
                }

                true
            }
            UiCmd::SubcommandNext => {
                if let Ok(r) = Rura::new(
                    self.command_input.value(),
                    self.command_input.visual_cursor(),
                ) {
                    if let Some(cursor) = r.cursor_next() {
                        self.command_input.handle(InputRequest::SetCursor(cursor));
                    }
                }

                self.completions = None;

                false
            }
            UiCmd::SubcommandPrev => {
                if let Ok(r) = Rura::new(
                    self.command_input.value(),
                    self.command_input.visual_cursor(),
                ) {
                    if let Some(cursor) = r.cursor_prev() {
                        self.command_input.handle(InputRequest::SetCursor(cursor));
                    }
                }

                self.completions = None;

                false
            }
            UiCmd::HistoryPrev => {
                self.command_input = Input::from(self.history.previous(self.command_input.value()));

                self.completions = None;

                false
            }
            UiCmd::HistoryNext => {
                self.command_input = Input::from(self.history.next(self.command_input.value()));

                self.completions = None;

                false
            }
            _ => false,
        }
    }

    pub fn execute(&mut self, execute_type: ExecuteType) -> Result<Option<String>, Box<dyn Error>> {
        if self.command_input.value().is_empty() {
            return Ok(None);
        }
        match Rura::new(
            self.command_input.value(),
            self.command_input.visual_cursor(),
        ) {
            Ok(r) => match r.command(&execute_type) {
                None => Ok(None),
                Some(command) => {
                    if !matches!(
                        execute_type,
                        ExecuteType::FullLive | ExecuteType::UntilCurrentLive
                    ) {
                        self.highlight_until = Some(command.until);
                        let _ = self.highlight_reset_tx.send(());
                    }
                    Ok(Some(command.to_run))
                }
            },
            Err(e) => {
                info!("invalid command: '{}'", self.command_input.value());
                Err(e.into())
            }
        }
    }
}

fn render_line(line: Vec<StyledGrapheme>, area: Rect, buf: &mut Buffer, y: u16) {
    let mut x = 0;
    for StyledGrapheme { symbol, style } in line {
        let width = symbol.width();
        if width == 0 {
            continue;
        }
        // Make sure to overwrite any previous character with a space (rather than a zero-width)
        let symbol = if symbol.is_empty() { " " } else { symbol };
        let position = Position::new(area.left() + x, area.top() + y);
        buf[position].set_symbol(symbol).set_style(style);
        x += u16::try_from(width).unwrap_or(u16::MAX);
    }
}

fn to_line<'a>(r: Rura, highlight_until: Option<usize>, theme: &Theme) -> Line<'a> {
    let mut spans = vec![];

    for (index, parts) in r.subcommands.iter().enumerate() {
        let is_current = index == r.current;
        let is_highlighted = highlight_until.map_or(false, |until| index <= until);

        if index > 0 {
            let pipe_style = if is_highlighted {
                theme.cmd_highlight_pipe
            } else {
                theme.cmd_regular_pipe
            };
            spans.push("|".set_style(pipe_style));
        }

        let base_style = if is_highlighted {
            if is_current {
                theme.cmd_highlight_current
            } else {
                theme.cmd_highlight
            }
        } else if is_current {
            theme.cmd_regular_current
        } else {
            theme.cmd_regular
        };

        for part in parts {
            let style = match part {
                Part::Unquoted(_) => base_style,
                Part::Quoted(_) => theme.cmd_quoted.patch(base_style),
            };
            spans.push(part.content().to_string().set_style(style));
        }
    }

    Line::from_iter(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{KeyBindingsConfig, ThemeConfig};
    use crate::history::History;
    use crate::theme::Theme;
    use crate::uicmd::KeyBindings;
    use crossterm::event::KeyCode::Char;
    use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tui_input::Input;

    struct TestTerminal(Terminal<TestBackend>);

    struct TestCompleter;

    impl Completer for TestCompleter {
        fn completions(&self, _input: &str, _cursor_pos: usize) -> Option<CompletionResult> {
            Some(CompletionResult {
                completions: vec!["command".to_string(), "command_other".to_string()],
                word_start: 0,
            })
        }
    }

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(20, 4)).unwrap())
        }
    }

    impl Default for RuraWidget {
        fn default() -> Self {
            let (highlight_reset_tx, _) = std::sync::mpsc::channel::<()>();
            let theme_config = ThemeConfig::default();
            let kb_config = KeyBindingsConfig::default();
            RuraWidget {
                command_input: Input::from(""),
                highlight_until: None,
                theme: Theme::from_config(&theme_config),
                history: History::in_mem(),
                key_bindings: KeyBindings::from_config(&kb_config),
                highlight_reset_tx,
                completions: None,
                completer: Box::new(TestCompleter {}),
            }
        }
    }

    #[test]
    fn command_input() {
        let mut widget = RuraWidget::default();

        input_text(&mut widget, "ls -la | grep a");

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn command_input_wrap_line() {
        let mut widget = RuraWidget::default();

        input_text(&mut widget, "ls -la | grep a | sort | uniq");

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn execute_add_to_history() {
        let mut widget = RuraWidget::default();

        input_text(&mut widget, "cmd1");
        widget.history.push("cmd1");

        input_text(&mut widget, " | cmd2");
        widget.history.push("cmd1 | cmd2");

        assert_eq!(widget.command_input.value(), "cmd1 | cmd2");

        widget.handle_ui_command(UiCmd::HistoryPrev);
        assert_eq!(widget.command_input.value(), "cmd1");

        widget.handle_ui_command(UiCmd::HistoryNext);
        assert_eq!(widget.command_input.value(), "cmd1 | cmd2");
    }

    #[test]
    fn completer() {
        let mut widget = RuraWidget::default();

        input_text(&mut widget, "co");

        widget.handle_ui_command(UiCmd::Complete);
        assert_eq!(widget.command_input.value(), "command");

        widget.handle_ui_command(UiCmd::Complete);
        assert_eq!(widget.command_input.value(), "command_other");

        widget.handle_ui_command(UiCmd::CompletePrev);
        assert_eq!(widget.command_input.value(), "command");
    }

    fn input_text(app: &mut RuraWidget, text: &str) {
        for c in text.chars() {
            app.handle_event(&Event::Key(KeyEvent {
                code: Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }));
        }
    }
}
