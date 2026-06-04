use crate::completable_input::CompletableInput;
use crate::history::History;
use crate::rura::{ExecuteType, Rura, RuraCommand};
use crate::theme::Theme;
use anyhow::Result;
use crossterm::event::Event;
use itertools::Itertools;
use log::info;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::prelude::{Line, Style, Widget};
use ratatui::style::Styled;
use ratatui::text::StyledGrapheme;
use std::sync::mpsc::Sender;
use tui_input::InputRequest;
use unicode_width::UnicodeWidthStr;

pub struct RuraWidget {
    pub command_input: CompletableInput,
    pub highlight_until: Option<usize>,
    pub theme: Theme,
    pub history: History,
    pub highlight_reset_tx: Sender<()>,
    pub failed_subcommand: Option<usize>,
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
                Ok(r) => to_line(r, self.highlight_until, self.failed_subcommand, &self.theme),
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

    pub fn handle_event(&mut self, event: &Event) -> bool {
        let changed_value = self
            .command_input
            .handle_event(event)
            .map(|change| change.value)
            .unwrap_or(false);

        if changed_value {
            self.failed_subcommand = None
        }

        changed_value
    }

    pub fn subcommand_next(&mut self) {
        if let Ok(r) = Rura::new(
            self.command_input.value(),
            self.command_input.visual_cursor(),
        ) {
            if let Some(cursor) = r.cursor_next(false) {
                self.command_input.handle(InputRequest::SetCursor(cursor));
            }
        }

        self.command_input.clear_completions();
    }

    pub fn subcommand_prev(&mut self) {
        if let Ok(r) = Rura::new(
            self.command_input.value(),
            self.command_input.visual_cursor(),
        ) {
            if let Some(cursor) = r.cursor_prev(false) {
                self.command_input.handle(InputRequest::SetCursor(cursor));
            }
        }

        self.command_input.clear_completions();
    }

    pub fn history_next(&mut self) {
        self.command_input
            .with_value(self.history.next(self.command_input.value()));

        self.command_input.clear_completions();
        self.failed_subcommand = None;
    }

    pub fn history_prev(&mut self) {
        self.command_input
            .with_value(self.history.previous(self.command_input.value()));

        self.command_input.clear_completions();
        self.failed_subcommand = None;
    }

    pub fn execute(&mut self, execute_type: ExecuteType) -> Result<RuraCommand> {
        if self.command_input.value().is_empty() {
            return Ok(RuraCommand::empty());
        }
        match Rura::new(
            self.command_input.value(),
            self.command_input.visual_cursor(),
        ) {
            Ok(r) => {
                let command = r.command(&execute_type);
                if !matches!(
                    execute_type,
                    ExecuteType::FullLive | ExecuteType::UntilCurrentLive
                ) {
                    if !command.is_empty() {
                        self.highlight_until = Some(command.len() - 1);
                    }
                    let _ = self.highlight_reset_tx.send(());
                }
                Ok(command)
            }
            Err(e) => {
                info!("invalid command: '{}'", self.command_input.value());
                Err(e.into())
            }
        }
    }
}

pub fn render_line(line: Vec<StyledGrapheme>, area: Rect, buf: &mut Buffer, y: u16) {
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

fn to_line<'a>(
    r: Rura,
    highlight_until: Option<usize>,
    highlight_failed: Option<usize>,
    theme: &Theme,
) -> Line<'a> {
    let mut spans = vec![];

    for (index, subcommand) in r.subcommands.iter().enumerate() {
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

        let parts = split_quoted_parts(subcommand);

        for (part, is_quoted) in parts {
            let style = if is_quoted {
                theme.cmd_quoted.patch(base_style)
            } else {
                base_style
            };

            if let Some(failed_subcommand) = highlight_failed
                && index == failed_subcommand
            {
                spans.push(part.set_style(style.red()));
            } else {
                spans.push(part.set_style(style));
            }
        }
    }

    Line::from_iter(spans)
}

fn split_quoted_parts(s: &str) -> Vec<(String, bool)> {
    let mut parts = vec![];
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if !in_quotes && (c == '"' || c == '\'') {
            if !current.is_empty() {
                parts.push((current.clone(), false));
                current.clear();
            }
            in_quotes = true;
            quote_char = c;
            current.push(c);
        } else if in_quotes && c == quote_char {
            current.push(c);
            parts.push((current.clone(), true));
            current.clear();
            in_quotes = false;
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        parts.push((current, in_quotes));
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;
    use crate::history::History;
    use crate::theme::Theme;
    use crossterm::event::KeyCode::Char;
    use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(20, 4)).unwrap())
        }
    }

    impl Default for RuraWidget {
        fn default() -> Self {
            let (highlight_reset_tx, _) = std::sync::mpsc::channel::<()>();
            let theme_config = ThemeConfig::default();
            RuraWidget {
                command_input: CompletableInput::from("", ""),
                highlight_until: None,
                theme: Theme::from_config(&theme_config),
                history: History::in_mem(),
                highlight_reset_tx,
                failed_subcommand: None,
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

        widget.history_prev();
        assert_eq!(widget.command_input.value(), "cmd1");

        widget.history_next();
        assert_eq!(widget.command_input.value(), "cmd1 | cmd2");
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
