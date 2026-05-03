use crate::completion::{get_completions, CompletionType};
use crate::history::History;
use crate::rura::{ExecuteType, Part, Rura};
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
    pub completion_state: Option<CompletionState>,
}

pub struct CompletionState {
    pub word: String,
    pub start: usize,
    pub completions: Vec<String>,
    pub index: usize,
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
                        self.completion_state = None;
                        self.command_input.handle_event(event);
                    }
                    Some(a) => match a {
                        UiCmd::SubcommandNext => {
                            self.completion_state = None;
                            if let Ok(r) = Rura::new(
                                self.command_input.value(),
                                self.command_input.visual_cursor(),
                            ) {
                                if let Some(cursor) = r.cursor_next() {
                                    self.command_input.handle(InputRequest::SetCursor(cursor));
                                }
                            }
                        }
                        UiCmd::SubcommandPrev => {
                            self.completion_state = None;
                            if let Ok(r) = Rura::new(
                                self.command_input.value(),
                                self.command_input.visual_cursor(),
                            ) {
                                if let Some(cursor) = r.cursor_prev() {
                                    self.command_input.handle(InputRequest::SetCursor(cursor));
                                }
                            }
                        }
                        UiCmd::HistoryPrev => {
                            self.completion_state = None;
                            self.command_input = Input::from(self.history.previous(self.command_input.value()));
                        }
                        UiCmd::HistoryNext => {
                            self.completion_state = None;
                            self.command_input = Input::from(self.history.next(self.command_input.value()));
                        }
                        UiCmd::Complete => {
                            if let Ok(r) = Rura::new(
                                self.command_input.value(),
                                self.command_input.visual_cursor(),
                            ) {
                                let (word, start, is_cmd) = r.word_at_cursor();

                                if let Some(ref mut state) = self.completion_state {
                                    if state.start == start && word.starts_with(&state.word) {
                                        state.index = (state.index + 1) % state.completions.len();
                                        let completion = &state.completions[state.index];
                                        let mut val = self.command_input.value().to_string();
                                        val.replace_range(
                                            start..self.command_input.visual_cursor(),
                                            completion,
                                        );
                                        let new_cursor = start + completion.len();
                                        self.command_input = Input::from(val);
                                        self.command_input.handle(InputRequest::SetCursor(new_cursor));
                                        return;
                                    }
                                }

                                let kind = if is_cmd {
                                    CompletionType::Command
                                } else {
                                    CompletionType::File
                                };
                                let completions = get_completions(&word, kind);
                                if !completions.is_empty() {
                                    let completion = &completions[0];
                                    let mut val = self.command_input.value().to_string();
                                    val.replace_range(start..start + word.len(), completion);
                                    let new_cursor = start + completion.len();
                                    self.command_input = Input::from(val);
                                    self.command_input.handle(InputRequest::SetCursor(new_cursor));

                                    self.completion_state = Some(CompletionState {
                                        word: word.to_string(),
                                        start,
                                        completions,
                                        index: 0,
                                    });
                                }
                            }
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
