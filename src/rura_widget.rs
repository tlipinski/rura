use crate::history::History;
use crate::rura::{ExecuteType, Rura};
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use crossterm::event::Event;
use itertools::Itertools;
use log::warn;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::prelude::{Line, Style, Widget};
use ratatui::style::Styled;
use ratatui::text::StyledGrapheme;
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

    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;
                let key_bindings = &self.key_bindings;

                match to_ui_command(key_bindings, code, mods) {
                    None => {
                        self.command_input.handle_event(event);
                    }
                    Some(a) => match a {
                        UiCmd::SubcommandNext => match Rura::new(
                            self.command_input.value(),
                            self.command_input.visual_cursor(),
                        ) {
                            Ok(r) => {
                                if let Some(cursor) = r.cursor_next() {
                                    self.command_input.handle(InputRequest::SetCursor(cursor));
                                }
                            }
                            Err(_) => {}
                        },
                        UiCmd::SubcommandPrev => match Rura::new(
                            self.command_input.value(),
                            self.command_input.visual_cursor(),
                        ) {
                            Ok(r) => {
                                if let Some(cursor) = r.cursor_prev() {
                                    self.command_input.handle(InputRequest::SetCursor(cursor));
                                }
                            }
                            Err(_) => {}
                        },
                        UiCmd::HistoryPrev => {
                            self.command_input = Input::from(self.history.previous());
                        }
                        UiCmd::HistoryNext => {
                            self.command_input = Input::from(self.history.next());
                        }
                        _ => {}
                    },
                }
            }
            _ => {}
        }
    }

    pub fn command(&mut self, execute_type: ExecuteType) -> Option<String> {
        if self.command_input.value().is_empty() {
            return Some(String::new()); // todo replace with enum?
        }
        match Rura::new(
            self.command_input.value(),
            self.command_input.visual_cursor(),
        ) {
            Ok(r) => match r.command(execute_type) {
                None => Some(String::new()),
                Some((cmd, cmd_index)) => {
                    self.highlight_until = Some(cmd_index);
                    self.highlight_reset_tx.send(()).unwrap();
                    self.history.push(self.command_input.value());
                    Some(cmd)
                }
            },
            Err(_) => {
                warn!("Invalid command: {}", self.command_input.value());
                None
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

    for (index, part) in r.subcommands.iter().enumerate() {
        match highlight_until {
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
                        spans.push(part.clone().set_style(theme.cmd_highlight_current));
                    } else {
                        spans.push(part.clone().set_style(theme.cmd_highlight));
                    }
                } else {
                    if index > 0 {
                        spans.push("|".set_style(theme.cmd_regular_pipe));
                    }

                    if index == r.current {
                        spans.push(part.clone().set_style(theme.cmd_regular_current));
                    } else {
                        spans.push(part.clone().set_style(theme.cmd_regular));
                    }
                }
            }
        }
    }

    Line::from_iter(spans)
}
