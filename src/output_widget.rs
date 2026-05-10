use crate::config::{KeyBindingsConfig, ThemeConfig};
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use crossterm::event::Event::Key;
use crossterm::event::{Event, KeyCode};
use itertools::Itertools;
use log::{debug, info};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::prelude::Color::Red;
use ratatui::prelude::{StatefulWidget, Style, Text, Widget};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use serde::{Deserialize, Serialize};
use std::ops::Range;
use regex::Regex;

pub struct OutputWidget {
    output: Output,
    error_output_opt: Option<Output>,
    offset: Position,
    wrap: bool,
    theme: Theme,
    key_bindings: KeyBindings,
    output_height: u16,
    error_pane_placement: ErrorPanePlacement,
    highlight: Option<String>,
    search_positions: Vec<usize>,
    search_index: usize,
    pub error_display_mode: ErrorDisplayMode,
}

impl OutputWidget {
    pub fn new(
        theme_config: &ThemeConfig,
        kb_config: &KeyBindingsConfig,
        error_pane_placement: ErrorPanePlacement,
        error_display_mode: ErrorDisplayMode,
    ) -> Self {
        Self {
            offset: Position::default(),
            output: Output::ok(""),
            error_output_opt: None,
            wrap: false,
            theme: Theme::from_config(theme_config),
            key_bindings: KeyBindings::from_config(&kb_config),
            error_display_mode,
            output_height: 0u16,
            error_pane_placement,
            highlight: None,
            search_positions: vec![],
            search_index: 0,
        }
    }

    pub fn search(&mut self, search_str: &str) {
        if search_str.is_empty() {
            self.highlight = None;
        } else {
            self.highlight = Some(search_str.into());

            let positions = self
                .output
                .lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    let z = line.match_indices(search_str).collect_vec();
                    if !z.is_empty() {
                        debug!("matches {} {:?}", i, z);
                    }
                    if line.contains(search_str) { Some(i) } else { None }
                })
                .collect::<Vec<usize>>();

            info!(
                "searching for {} in {} lines: {:?}",
                search_str,
                self.output.lines.len(),
                positions
            );

            self.search_positions = positions;
            // self.search_index = 0; // todo first index after offset
        }
    }

    pub fn output_len(&self) -> usize {
        self.output.lines.len()
    }

    pub fn handle_command_output(&mut self, output: Output) {
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
            Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;
                let key_bindings = &self.key_bindings;

                match key_event.code {
                    KeyCode::F(3) => {
                        if !self.search_positions.is_empty() {
                            self.search_index =
                                (self.search_index + 1) % self.search_positions.len();
                            self.offset.y = self.search_positions[self.search_index] as u16;
                        }
                    }
                    _ => match to_ui_command(key_bindings, code, mods) {
                        Some(ui_cmd) => self.handle_ui_command(ui_cmd),
                        None => {}
                    },
                }
            }
            _ => {}
        }
    }

    pub fn handle_ui_command(&mut self, ui_cmd: UiCmd) {
        match ui_cmd {
            UiCmd::ScrollDown => {
                let max_offset = self
                    .main_output()
                    .lines
                    .len()
                    .saturating_sub(self.output_height as usize);
                self.offset.y = self.offset.y.saturating_add(1).min(max_offset as u16);
            }
            UiCmd::ScrollDownPage => {
                let max_offset = self
                    .main_output()
                    .lines
                    .len()
                    .saturating_sub(self.output_height as usize);
                let page_size = self.output_height / 2;
                self.offset.y = self
                    .offset
                    .y
                    .saturating_add(page_size)
                    .min(max_offset as u16);
            }
            UiCmd::ScrollUp => {
                self.offset.y = self.offset.y.saturating_sub(1);
            }
            UiCmd::ScrollUpPage => {
                let page_size = self.output_height / 2;
                self.offset.y = self.offset.y.saturating_sub(page_size);
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
            _ => {}
        }
    }

    pub fn main_output(&self) -> &Output {
        match self.error_display_mode {
            ErrorDisplayMode::Inline => self.error_output_opt.as_ref().unwrap_or(&self.output),
            ErrorDisplayMode::Pane => &self.output,
        }
    }
}

impl Widget for &mut OutputWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = &self.theme;

        let error_output_lines = match self.error_display_mode {
            ErrorDisplayMode::Inline => 0,
            ErrorDisplayMode::Pane => self
                .error_output_opt
                .as_ref()
                .map(|e| e.lines.len() + 2)
                .unwrap_or(0),
        };

        let (output_area, errors_area) = match self.error_pane_placement {
            ErrorPanePlacement::Top => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![
                        Constraint::Length(error_output_lines.min(10) as u16),
                        Constraint::Fill(1),
                    ])
                    .split(area);

                (layout[1], layout[0])
            }
            ErrorPanePlacement::Bottom => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![
                        Constraint::Fill(1),
                        Constraint::Length(error_output_lines.min(10) as u16),
                    ])
                    .split(area);

                (layout[0], layout[1])
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

        self.output_height = output_content_area.height; // save this value for scroll logic

        // it screen was resized (height increased) then adjust current offset
        let current_max_y_offset =
            (self.main_output().lines.len() as u16).saturating_sub(output_content_area.height);
        if self.offset.y > current_max_y_offset {
            self.offset.y = current_max_y_offset
        }

        if matches!(self.error_display_mode, ErrorDisplayMode::Pane) {
            if let Some(err_output) = &self.error_output_opt {
                let block = Block::bordered()
                    .title(format!(" Error: {} ", err_output.status_code.unwrap_or(0)))
                    .border_style(Style::default().fg(Red));
                let mut err_output_par = Paragraph::new(err_output.lines.join("\n"))
                    .scroll((0, self.offset.x))
                    .block(block);

                if self.wrap {
                    err_output_par = err_output_par.wrap(Wrap::default())
                };
                err_output_par.render(errors_area, buf);
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

        let line_nums = range
            .clone()
            .flat_map(|i| {
                let visual_line_count = if self.wrap {
                    Paragraph::new(output.lines[i].as_str())
                        .wrap(Wrap::default())
                        .line_count(output_content_area.width)
                } else {
                    1
                };
                std::iter::once(format!("{: >pad$}", i + 1, pad = line_nums_width)).chain(
                    std::iter::repeat_n(String::new(), visual_line_count.saturating_sub(1)),
                )
            })
            .collect::<Vec<String>>();
        let lines_par = Paragraph::new(line_nums.join("\n")).style(theme.line_nums);
        if output.ok {
            lines_par.render(line_nums_area, buf);
        }

        let output_par = {
            let mut par = match &self.highlight {
                Some(st) => {
                    let z = (&output.lines[range])
                        .iter()
                        .map(|line| {
                            let regular_spans = line.split(st).map(Span::from);
                            let all_spans = regular_spans.intersperse(
                                Span::from(st).style(Style::default().bg(Color::Magenta)),
                            );
                            let sp = all_spans.collect::<Vec<_>>();
                            let line: Line = Line::from(sp);
                            line
                        })
                        .collect::<Vec<Line>>();

                    Paragraph::new(Text::from(z))
                        .scroll((0, self.offset.x))
                        .block(Block::default())
                }
                None => Paragraph::new(output.lines[range].join("\n"))
                    .scroll((0, self.offset.x))
                    .block(Block::default()),
            };

            if self.wrap {
                par = par.wrap(Wrap::default())
            };

            par
        };

        output_par.render(output_content_area, buf);

        let scroll_bar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut state = ScrollbarState::new(
            self.output
                .len()
                .saturating_sub(self.output_height as usize),
        );
        state = state.position(self.offset.y.into());
        scroll_bar.render(vscroll_area, buf, &mut state)
    }
}

#[derive(Default, Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorDisplayMode {
    Inline,
    #[default]
    Pane,
}

pub enum ErrorPanePlacement {
    Top,
    Bottom,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Output {
    pub lines: Vec<String>,
    pub status_code: Option<i32>,
    pub ok: bool,
}

impl Output {
    pub fn ok(str: &str) -> Self {
        Self {
            lines: Self::lines(str),
            status_code: Some(0),
            ok: true,
        }
    }

    pub fn err(str: &str, status_code: Option<i32>) -> Self {
        Self {
            lines: Self::lines(str),
            status_code,
            ok: false,
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    fn lines(input: &str) -> Vec<String> {
        input.lines().map(|a| a.into()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(100, 30)).unwrap())
        }
    }

    impl Default for OutputWidget {
        fn default() -> Self {
            let theme_config = ThemeConfig::default();
            let kb_config = KeyBindingsConfig::default();

            OutputWidget::new(
                &theme_config,
                &kb_config,
                ErrorPanePlacement::Top,
                ErrorDisplayMode::Pane,
            )
        }
    }

    #[test]
    fn errors_pane_top() {
        let mut terminal = TestTerminal::default().0;

        let mut widget = OutputWidget::default();
        widget.error_pane_placement = ErrorPanePlacement::Top;
        widget.error_display_mode = ErrorDisplayMode::Pane;

        widget.handle_command_output(Output::ok("out1\nout2\nout3"));
        widget.handle_command_output(Output::err("errors1\nerrors2\nerrors3", Some(1)));

        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn errors_pane_bottom() {
        let mut terminal = TestTerminal::default().0;

        let mut widget = OutputWidget::default();
        widget.error_pane_placement = ErrorPanePlacement::Bottom;
        widget.error_display_mode = ErrorDisplayMode::Pane;

        widget.handle_command_output(Output::ok("out1\nout2\nout3"));
        widget.handle_command_output(Output::err("errors1\nerrors2\nerrors3", Some(1)));

        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    fn generate_lines(count: usize) -> String {
        (1..=count)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn errors_inline() {
        let mut terminal = TestTerminal::default().0;

        let mut widget = OutputWidget::default();
        widget.error_display_mode = ErrorDisplayMode::Inline;

        widget.handle_command_output(Output::ok("out1\nout2\nout3"));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("after ok", terminal.backend());

        widget.handle_command_output(Output::ok(&generate_lines(3)));

        widget.handle_command_output(Output::err("errors1\nerrors2\nerrors3", Some(1)));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn scrolling() {
        let mut terminal = Terminal::new(TestBackend::new(10, 5)).unwrap();

        let mut widget = OutputWidget::default();
        widget.error_display_mode = ErrorDisplayMode::Inline;

        widget.handle_command_output(Output::ok(&generate_lines(10)));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll base", terminal.backend());

        widget.handle_ui_command(UiCmd::ScrollDown);
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll down line", terminal.backend());

        widget.handle_ui_command(UiCmd::ScrollDownPage);
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll down page", terminal.backend());

        widget.handle_ui_command(UiCmd::ScrollUp);
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll up line", terminal.backend());

        widget.handle_ui_command(UiCmd::ScrollUpPage);
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll up page", terminal.backend());
    }
}
