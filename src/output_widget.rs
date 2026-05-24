use crate::cmd_runner::Output;
use crate::config::ThemeConfig;
use crate::theme::Theme;
use itertools::Itertools;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect, Size};
use ratatui::prelude::Color::Red;
use ratatui::prelude::{StatefulWidget, Style, Text, Widget};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Debug, Default)]
struct Position {
    col: usize,
    row: usize,
}

struct Viewport {
    rows: Range<usize>,
    cols: Range<usize>,
}

pub struct OutputWidget {
    pub output: Output,
    error_output_opt: Option<Output>,
    offset: Position,
    wrap: bool,
    theme: Theme,
    output_content_area_size: Size,
    error_pane_placement: ErrorPanePlacement,
    highlight_positions: Vec<(usize, Range<usize>)>,
    highlight_index: usize,
    pub error_display_mode: ErrorDisplayMode,
}

impl OutputWidget {
    pub fn new(
        theme_config: &ThemeConfig,
        error_pane_placement: ErrorPanePlacement,
        error_display_mode: ErrorDisplayMode,
    ) -> Self {
        Self {
            offset: Position::default(),
            output: Output::ok_stdin(""),
            error_output_opt: None,
            wrap: false,
            theme: Theme::from_config(theme_config),
            error_display_mode,
            output_content_area_size: Size::default(),
            error_pane_placement,
            highlight_positions: vec![],
            highlight_index: 0,
        }
    }

    pub fn highlight_info(&self) -> (usize, usize) {
        (self.highlight_index, self.highlight_positions.len())
    }

    pub fn clear_highlight(&mut self) {
        self.highlight_positions = vec![];
        self.highlight_index = 0;
    }

    pub fn highlight_next(&mut self) {
        if !self.highlight_positions.is_empty() {
            self.highlight_index = (self.highlight_index + 1) % self.highlight_positions.len();

            let (line, range) = self.highlight_positions[self.highlight_index].clone();
            self.adjust_viewport_for_highlight(line, range);
        }
    }

    pub fn highlight_prev(&mut self) {
        if !self.highlight_positions.is_empty() {
            if self.highlight_index == 0 {
                self.highlight_index = self.highlight_positions.len().saturating_sub(1);
            } else {
                self.highlight_index = self.highlight_index.saturating_sub(1);
            }

            let (line, range) = self.highlight_positions[self.highlight_index].clone();
            self.adjust_viewport_for_highlight(line, range);
        }
    }

    pub fn highlight(&mut self, search_str: &str, case_sensitive: bool, regex: bool) {
        if search_str.is_empty() {
            self.highlight_positions = vec![];
        } else {
            let mut search_str = String::from(search_str);

            if !case_sensitive {
                search_str = search_str.to_lowercase();
            }

            let pattern_res = if regex {
                Regex::new(&search_str)
            } else {
                Regex::new(&regex::escape(&search_str))
            };

            if let Ok(pattern) = pattern_res {
                let positions = self
                    .output
                    .lines
                    .iter()
                    .enumerate()
                    .filter_map(|(i, line)| {
                        let line_to_match = if case_sensitive {
                            line
                        } else {
                            &line.to_lowercase()
                        };
                        let matches = pattern
                            .find_iter(line_to_match)
                            .map(|m| (i, m.start()..m.end()))
                            .collect_vec();
                        if !matches.is_empty() {
                            Some(matches)
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect::<Vec<(usize, Range<usize>)>>();

                // find the first match in the visible range otherwise start from the beginning
                match positions
                    .iter()
                    .find_position(|(line, _range)| line >= &self.viewport().rows.start)
                {
                    Some((z, _)) => self.highlight_index = z,
                    None => self.highlight_index = 0,
                }

                self.highlight_positions = positions;

                // focus on the first match
                if !self.highlight_positions.is_empty() {
                    let (line, range) = self.highlight_positions[self.highlight_index].clone();
                    self.adjust_viewport_for_highlight(line, range);
                }
            } else {
                self.highlight_positions = vec![];
            }
        }
    }

    fn adjust_viewport_for_highlight(&mut self, line_num: usize, range: Range<usize>) {
        if !self.viewport().rows.contains(&line_num) {
            self.offset.row = line_num.saturating_sub(self.viewport().rows.len() / 2);
        }

        if !self.viewport().cols.contains(&range.start) {
            if range.start < self.viewport().cols.len() {
                // scroll fully to the left if highlight is in the first "horizontal page"
                self.offset.col = 0;
            } else {
                self.offset.col = range.start.saturating_sub(self.viewport().cols.len() / 4);
            }
        }
    }

    pub fn output_len(&self) -> usize {
        self.output.lines.len()
    }

    pub fn handle_command_output(&mut self, output: Output) {
        if self.output.len() != output.len() {
            self.offset = Position::default();
        }

        if output.ok {
            self.output = output;
            self.error_output_opt = None;
        } else {
            self.error_output_opt = Some(output);
        }

        self.highlight_index = 0;
        self.highlight_positions = vec![];
    }

    pub fn scroll_down(&mut self) {
        if self.main_output().len() > self.viewport().rows.len() {
            let max_offset = self.main_output().lines.len().saturating_sub(1); // keep at least one line visible
            self.offset.row = self.offset.row.saturating_add(1).min(max_offset);
        }
    }

    pub fn scroll_page_down(&mut self) {
        if self.main_output().len() > self.viewport().rows.len() {
            let max_offset = self.main_output().lines.len().saturating_sub(1); // keep at least one line visible
            let page_size = self.output_content_area_size.height as usize / 2;
            self.offset.row = self.offset.row.saturating_add(page_size).min(max_offset);
        }
    }

    pub fn scroll_up(&mut self) {
        self.offset.row = self.offset.row.saturating_sub(1);
    }

    pub fn scroll_page_up(&mut self) {
        let page_size = self.output_content_area_size.height as usize / 2;
        self.offset.row = self.offset.row.saturating_sub(page_size);
    }

    pub fn scroll_left(&mut self) {
        self.offset.col = self.offset.col.saturating_sub(1);
    }

    pub fn scroll_right(&mut self) {
        if self.main_output_width() > self.viewport().cols.len() {
            let max_offset = self.main_output_width().saturating_sub(1); // keep at least one line visible
            self.offset.col = self.offset.col.saturating_add(1).min(max_offset);
        }
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
    }

    pub fn main_output(&self) -> &Output {
        match self.error_display_mode {
            ErrorDisplayMode::Inline => self.error_output_opt.as_ref().unwrap_or(&self.output),
            ErrorDisplayMode::Pane => &self.output,
        }
    }

    fn main_output_width(&self) -> usize {
        let output = self.main_output();
        let mut max_len = 0;
        for line in &output.lines {
            max_len = max_len.max(line.len());
        }
        max_len
    }

    fn viewport(&self) -> Viewport {
        Viewport {
            cols: self.offset.col..self.offset.col + self.output_content_area_size.width as usize,
            rows: self.offset.row..self.offset.row + self.output_content_area_size.height as usize,
        }
    }

    pub fn layout(&self, area: Rect) -> [Rect; 5] {
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

        let line_nums_width = self.main_output().len().to_string().len();

        let lines_content_scroll_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Length((line_nums_width + 1) as u16),
                Constraint::Fill(1),
                Constraint::Length(1),
            ]);

        let [main_output_area, h_scroll_area] = {
            let [_, content, _] = lines_content_scroll_layout.areas(output_area);
            if !self.wrap && self.main_output_width() > content.width as usize {
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Fill(1), Constraint::Length(1)])
                    .areas(output_area)
            } else {
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Fill(1), Constraint::Length(0)])
                    .areas(output_area)
            }
        };

        let [line_nums_area, output_content_area, v_scrollbar_area] =
            lines_content_scroll_layout.areas(main_output_area);

        let [_, h_scrollbar_area, _] = lines_content_scroll_layout.areas(h_scroll_area);

        [
            line_nums_area,
            output_content_area,
            errors_area,
            v_scrollbar_area,
            h_scrollbar_area,
        ]
    }
}

impl Widget for &mut OutputWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = &self.theme;

        let [
            line_nums_area,
            output_content_area,
            errors_area,
            vscrollbar_area,
            hscrollbar_area,
        ] = self.layout(area);

        let line_nums_width = self.output.len().to_string().len();

        self.output_content_area_size = output_content_area.into(); // save this value for scroll logic

        if matches!(self.error_display_mode, ErrorDisplayMode::Pane) {
            if let Some(err_output) = &self.error_output_opt {
                let block = Block::bordered()
                    .title(format!(" Error: {} ", err_output.status_code.unwrap_or(0)))
                    .border_style(Style::default().fg(Red));
                let mut err_output_par = Paragraph::new(err_output.lines.join("\n"))
                    .scroll((0, self.offset.col as u16))
                    .block(block);

                if self.wrap {
                    err_output_par = err_output_par.wrap(Wrap::default())
                };
                err_output_par.render(errors_area, buf);
            }
        }

        let output_len = self.main_output().len();

        let height = output_content_area.height.min(output_len as u16);

        let visible_lines: Range<usize> = {
            let from = (self.offset.row).min(output_len);
            let to = (self.offset.row + height as usize).min(output_len);
            from..to
        };

        let output = self.main_output();

        let line_nums = visible_lines
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
            let mut par = if !self.highlight_positions.is_empty() {
                let lines = (&output.lines[visible_lines.clone()])
                    .iter()
                    .enumerate()
                    .map(|(line_index, line)| {
                        // todo simplify
                        let logical_line_num = line_index + visible_lines.start;

                        let (current_match_line, current_match_range) =
                            self.highlight_positions.get(self.highlight_index).unwrap();

                        let line_highlight_ranges: Vec<&Range<usize>> = self
                            .highlight_positions
                            .iter()
                            .filter(|(row, _)| *row == logical_line_num)
                            .map(|(_, range)| range)
                            .collect();

                        let current_match_num = if logical_line_num == *current_match_line {
                            self.highlight_positions
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
                                Part::InsideRangeCurrent(value) => {
                                    Span::from(value).style(theme.output_highlight_current)
                                }
                                Part::InsideRange(value) => {
                                    Span::from(value).style(theme.output_highlight)
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
                    .scroll((0, self.offset.col as u16)) // todo
                    .block(Block::default())
            } else {
                Paragraph::new(output.lines[visible_lines].join("\n"))
                    .scroll((0, self.offset.col as u16)) // todo
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
                .saturating_sub(self.output_content_area_size.height as usize),
        );
        state = state.position(self.offset.row.into());
        scroll_bar.render(vscrollbar_area, buf, &mut state);

        let scroll_bar_h = Scrollbar::new(ScrollbarOrientation::HorizontalTop);
        let mut state_h = ScrollbarState::new(self.main_output_width());
        state_h = state_h.position(self.offset.col.into());
        scroll_bar_h.render(hscrollbar_area, buf, &mut state_h)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_runner::Output;
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

            OutputWidget::new(
                &theme_config,
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

        widget.handle_command_output(Output::ok_command("", "out1\nout2\nout3"));
        widget.handle_command_output(Output::err_command(
            "",
            "errors1\nerrors2\nerrors3",
            Some(1),
        ));

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

        widget.handle_command_output(Output::ok_command("", "out1\nout2\nout3"));
        widget.handle_command_output(Output::err_command(
            "",
            "errors1\nerrors2\nerrors3",
            Some(1),
        ));

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

        widget.handle_command_output(Output::ok_stdin("out1\nout2\nout3"));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("after ok", terminal.backend());

        widget.handle_command_output(Output::ok_stdin(&generate_lines(3)));

        widget.handle_command_output(Output::err_command(
            "",
            "errors1\nerrors2\nerrors3",
            Some(1),
        ));
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

        widget.handle_command_output(Output::ok_stdin(&generate_lines(10)));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll base", terminal.backend());

        widget.scroll_down();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll down line", terminal.backend());

        widget.scroll_page_down();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll down page", terminal.backend());

        widget.scroll_up();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll up line", terminal.backend());

        widget.scroll_page_up();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("scroll up page", terminal.backend());
    }

    #[test]
    fn no_scrolling_when_content_fits_viewport() {
        let mut terminal = Terminal::new(TestBackend::new(10, 10)).unwrap();

        let mut widget = OutputWidget::default();
        widget.output_content_area_size = terminal.size().unwrap();

        widget.handle_command_output(Output::ok_stdin(&generate_lines(8)));

        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("no scrolling base", terminal.backend());

        // neither of this commands is supposed to move viewport if content fits it
        widget.scroll_down();
        widget.scroll_page_down();
        widget.scroll_right();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("no scrolling", terminal.backend());
    }

    #[test]
    fn highlighting() {
        let mut terminal = Terminal::new(TestBackend::new(20, 10)).unwrap();

        let mut widget = OutputWidget::default();

        widget.handle_command_output(Output::ok_stdin(&generate_lines(50)));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight base", terminal.backend());

        widget.highlight("line2", false, false);
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight", terminal.backend());

        widget.highlight_next();
        widget.highlight_next();
        widget.highlight_next();
        widget.highlight_next();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight next 4x", terminal.backend());

        // in visible area - should not move offset
        widget.highlight_next();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight next 1x", terminal.backend());

        widget.highlight_prev();
        widget.highlight_prev();
        widget.highlight_prev();
        widget.highlight_prev();
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight prev 4x", terminal.backend());

        widget.highlight("line50", false, false);
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight another highlight", terminal.backend());
    }

    #[test]
    fn highlighting_horizontal_scroll() {
        let mut terminal = Terminal::new(TestBackend::new(15, 6)).unwrap();

        let mut widget = OutputWidget::default();

        let out = vec![
            "  hl1                          ",
            "                hl2 hl3        ",
            "  hl4               hl5        ",
        ];

        widget.handle_command_output(Output::ok_stdin(&out.join("\n")));
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_snapshot!("highlight horizontal base", terminal.backend());

        widget.highlight("hl", false, false);
        for i in 1..6 {
            widget.highlight_next();
            terminal
                .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
                .unwrap();
            assert_snapshot!(format!("highlight {i}"), terminal.backend());
        }
    }

    #[test]
    fn split_line_into_parts_by_ranges_test() {
        let str = "01234567890123456789";

        let spans = split_by_ranges(str, vec![], None);
        assert_eq!(spans, vec![Part::OutsideRange(str.to_string())]);

        let spans = split_by_ranges(str, vec![&(0..2), &(7..11), &(14..18)], None);
        assert_eq!(
            spans,
            vec![
                Part::InsideRange("01".into()),
                Part::OutsideRange("23456".into()),
                Part::InsideRange("7890".into()),
                Part::OutsideRange("123".into()),
                Part::InsideRange("4567".into()),
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
    InsideRangeCurrent(String),
    InsideRange(String),
    OutsideRange(String),
}

fn split_by_ranges(str: &str, ranges: Vec<&Range<usize>>, current_opt: Option<usize>) -> Vec<Part> {
    let mut results = vec![];
    let mut last_end = 0;

    for (i, range) in ranges.iter().enumerate() {
        if last_end < range.start {
            results.push(Part::OutsideRange(str[last_end..range.start].to_string()));
        }

        if let Some(current) = current_opt
            && current == i
        {
            results.push(Part::InsideRangeCurrent(
                str[range.start..range.end].to_string(),
            ));
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
