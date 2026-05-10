use crate::config::{KeyBindingsConfig, ThemeConfig};
use crate::theme::Theme;
use crate::uicmd::{KeyBindings, UiCmd, to_ui_command};
use crossterm::event::Event::Key;
use crossterm::event::{Event, KeyCode};
use itertools::Itertools;
use log::debug;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::prelude::Color::Red;
use ratatui::prelude::{StatefulWidget, Style, Text, Widget};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub struct OutputWidget {
    output: Output,
    error_output_opt: Option<Output>,
    offset: Position,
    wrap: bool,
    theme: Theme,
    key_bindings: KeyBindings,
    output_height: u16,
    error_pane_placement: ErrorPanePlacement,
    search: String,
    search_positions: Vec<(usize, Range<usize>)>,
    search_index: usize,
    visible_range: Range<usize>,
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
            search: String::new(),
            search_positions: vec![],
            visible_range: 0..0,
            search_index: 0,
        }
    }

    pub fn search_next(&mut self, search_str: &str) {
        debug!("next {}", search_str);
        if (self.search == search_str) {
            if !self.search_positions.is_empty() {
                self.search_index = (self.search_index + 1) % self.search_positions.len();
                let (line, _) = self.search_positions[self.search_index];
                if !(self.visible_range.contains(&line)) {
                    self.offset.y = line.saturating_sub(3) as u16;
                }
            }
        } else {
            self.search(search_str);
        }
    }

    pub fn search_prev(&mut self, search_str: &str) {
        debug!("prev {}", search_str);
        if (self.search == search_str) {
            if !self.search_positions.is_empty() {
                if self.search_index == 0 {
                    self.search_index = self.search_positions.len().saturating_sub(1);
                } else {
                    self.search_index = self.search_index.saturating_sub(1);
                }

                let (line, _) = self.search_positions[self.search_index];

                if !(self.visible_range.contains(&line)) {
                    self.offset.y = line.saturating_sub(3) as u16;
                }
            }
        } else {
            self.search(search_str);
        }
    }

    fn search(&mut self, search_str: &str) {
        self.search = search_str.to_string();
        if !search_str.is_empty() {
            let pattern = Regex::new(&regex::escape(&search_str.to_lowercase())).unwrap();

            let positions = self
                .output
                .lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    let matches = pattern
                        .find_iter(&line.to_lowercase())
                        .map(|m| (i, (m.start()..m.start() + search_str.len())))
                        .collect_vec();
                    if !matches.is_empty() {
                        Some(matches)
                    } else {
                        None
                    }
                })
                .flatten()
                .collect::<Vec<(usize, Range<usize>)>>();

            self.search_index = self.search_index.min(positions.len() - 1); // todo first index after offset
            self.search_positions = positions;
        } else {
            self.search_positions = vec![];
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

        let output_len = self.main_output().len();

        let height = output_content_area.height.min(output_len as u16);

        let visible_range: Range<usize> = if height >= output_len as u16 {
            0..output_len
        } else {
            let from = (self.offset.y as usize).min(output_len);
            let to = (self.offset.y as usize + height as usize).min(output_len);
            from..to
        };

        self.visible_range = visible_range.clone();

        let output = self.main_output();

        let line_nums = visible_range
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
            let mut par = if !self.search_positions.is_empty() {
                let lines = (&output.lines[visible_range.clone()])
                    .iter()
                    .enumerate()
                    .map(|(line_index, line)| {
                        // todo simplify
                        let logical_line_num = line_index + visible_range.start;

                        let a @ (current_match_line, current_match_range) =
                            self.search_positions.get(self.search_index).unwrap();

                        let line_highlight_ranges: Vec<&Range<usize>> = self
                            .search_positions
                            .iter()
                            .filter(|(row, _)| *row == logical_line_num)
                            .map(|(_, range)| range)
                            .collect();

                        let current_match_num = if (logical_line_num == *current_match_line) {
                            self.search_positions
                                .iter()
                                .filter(|(row, _)| *row == logical_line_num)
                                .find_position(|(_, range)| range == current_match_range)
                                .map(|(i, _)| i)
                        } else {
                            None
                        };

                        let spans = split_by_ranges(line, line_highlight_ranges, current_match_num)
                            .into_iter()
                            .map(|part| match part {
                                Part::InsideRangeX(value) => {
                                    Span::from(value).style(Style::default().bg(Color::Yellow))
                                }
                                Part::InsideRange(value) => {
                                    Span::from(value).style(Style::default().bg(Color::Magenta))
                                }
                                Part::OutsideRange(value) => {
                                    Span::from(value).style(Style::default())
                                }
                            })
                            .collect_vec();

                        Line::from(spans)
                    })
                    .collect::<Vec<Line>>();

                Paragraph::new(Text::from(lines))
                    .scroll((0, self.offset.x))
                    .block(Block::default())
            } else {
                Paragraph::new(output.lines[visible_range].join("\n"))
                    .scroll((0, self.offset.x))
                    .block(Block::default())
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

    #[test]
    fn split_line_into_parts_by_ranges_test() {
        let str = "01234567890123456789";

        let spans = split_by_ranges(str, vec![], None);
        assert_eq!(spans, vec![Part::OutsideRange(str.to_string())]);

        let spans = split_by_ranges(str, vec![0..2, 7..11, 14..18], None);
        assert_eq!(
            spans,
            vec![
                Part::InsideRangeX("01".into()),
                Part::OutsideRange("23456".into()),
                Part::InsideRangeX("7890".into()),
                Part::OutsideRange("123".into()),
                Part::InsideRangeX("4567".into()),
                Part::OutsideRange("89".into())
            ]
        );

        let spans = split_by_ranges(str, vec![&(1..2), &(7..11), &(14..18)], None);
        assert_eq!(
            spans,
            vec![
                Part::OutsideRange("0".into()),
                Part::InsideRange("1".into()),
                Part::OutsideRange("23456".into()),
                Part::InsideRange("7890".into()),
                Part::OutsideRange("123".into()),
                Part::InsideRange("4567".into()),
                Part::OutsideRange("89".into())
            ]
        );
    }
}

#[derive(Debug, PartialEq)]
enum Part {
    InsideRangeX(String),
    InsideRange(String),
    OutsideRange(String),
}

fn split_by_ranges(str: &str, ranges: Vec<&Range<usize>>, c: Option<usize>) -> Vec<Part> {
    let mut results = vec![];
    let mut last_end = 0;

    for (i, range) in ranges.iter().enumerate() {
        if last_end < range.start {
            results.push(Part::OutsideRange(str[last_end..range.start].to_string()));
        }

        if let Some(ccc) = c
            && ccc == i
        {
            results.push(Part::InsideRangeX(str[range.start..range.end].to_string()));
        } else {
            results.push(Part::InsideRange(str[range.start..range.end].to_string()));
        }
        last_end = range.end;
    }

    if last_end < str.len() {
        results.push(Part::OutsideRange(str[last_end..].to_string()));
    }

    results
}
