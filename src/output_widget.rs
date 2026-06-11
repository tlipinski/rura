use crate::config::ThemeConfig;
use crate::content_widget::{ContentWidget, Position};
use crate::shell::cmd_runner::CmdResult;
use crate::shell::output::Output;
use crate::theme::Theme;
use itertools::Itertools;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect, Size};
use ratatui::prelude::Color::Red;
use ratatui::prelude::{Style, Widget};
use ratatui::widgets::{Block, Paragraph};
use std::cell::Cell;

pub struct OutputWidget {
    content: ContentWidget,
    error_output_opt: Option<(Vec<String>, Option<i32>)>,
    theme: Theme,
    error_pane_placement: ErrorPanePlacement,
}

impl OutputWidget {
    pub fn new(theme_config: &ThemeConfig, error_pane_placement: ErrorPanePlacement) -> Self {
        Self {
            content: ContentWidget {
                offset: Position::default(),
                lines: vec![],
                wrap: false,
                highlight_positions: vec![],
                highlight_index: 0,
                theme: Theme::from_config(theme_config),
                output_content_area_size: Cell::new(Size::default()),
            },
            error_output_opt: None,
            theme: Theme::from_config(theme_config),
            error_pane_placement,
        }
    }

    pub fn highlight_info(&self) -> (usize, usize) {
        self.content.highlight_info()
    }

    pub fn clear_highlight(&mut self) {
        self.content.clear_highlight();
    }

    pub fn highlight_next(&mut self) {
        self.content.highlight_next();
    }

    pub fn highlight_prev(&mut self) {
        self.content.highlight_prev();
    }

    pub fn highlight(&mut self, search_str: &str, case_sensitive: bool, regex: bool) {
        self.content.highlight(search_str, case_sensitive, regex);
    }

    pub fn output_len(&self) -> usize {
        self.content.output_len()
    }

    pub fn handle_command_result(&mut self, result: &CmdResult) {
        match result.outputs.last() {
            Some(Output::Ok(bytes)) => {
                let str = String::from_utf8_lossy(&bytes);
                let lines = str.lines().map(|a| a.into()).collect_vec();
                if self.content.lines.len() != lines.len() {
                    self.content.offset = Position::default();
                }
                self.content.lines = lines;

                self.error_output_opt = None;
            }
            Some(Output::Err(bytes, code)) => {
                let str = String::from_utf8_lossy(&bytes);
                let lines = str.lines().map(|a| a.into()).collect_vec();

                self.error_output_opt = Some((lines, *code));
            }
            None => {
                let str = String::from_utf8_lossy(&result.stdin);
                let lines = str.lines().map(|a| a.into()).collect_vec();
                if self.content.lines.len() != lines.len() {
                    self.content.offset = Position::default();
                }
                self.content.lines = lines;

                self.error_output_opt = None;
            }
        }

        self.content.highlight_index = 0;
        self.content.highlight_positions = vec![];
    }

    pub fn scroll_down(&mut self) {
        self.content.scroll_down();
    }

    pub fn scroll_page_down(&mut self) {
        self.content.scroll_page_down();
    }

    pub fn scroll_up(&mut self) {
        self.content.scroll_up();
    }

    pub fn scroll_page_up(&mut self) {
        self.content.scroll_page_up()
    }

    pub fn scroll_left(&mut self) {
        self.content.scroll_left();
    }

    pub fn scroll_page_left(&mut self) {
        self.content.scroll_page_left();
    }

    pub fn scroll_right(&mut self) {
        self.content.scroll_right();
    }

    pub fn scroll_page_right(&mut self) {
        self.content.scroll_page_right();
    }

    pub fn toggle_wrap(&mut self) {
        self.content.wrap = !self.content.wrap;
    }

    pub fn layout(&self, area: Rect) -> [Rect; 2] {
        let error_output_lines = self
            .error_output_opt
            .as_ref()
            .map(|e| e.0.len() + 2)
            .unwrap_or(0);

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

        [output_area, errors_area]
    }
}

impl Widget for &OutputWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let _theme = &self.theme;

        let [output_content_area, errors_area] = self.layout(area);

        if let Some(err_output) = &self.error_output_opt {
            let block = Block::bordered()
                .title(format!(" Error: {} ", err_output.1.unwrap_or(0)))
                .border_style(Style::default().fg(Red));
            let err_output_par = Paragraph::new(err_output.0.join("\n")).block(block);

            err_output_par.render(errors_area, buf);
        }

        self.content.render(output_content_area, buf);
    }
}

pub enum ErrorPanePlacement {
    Top,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::output::Output;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::sync::Arc;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(100, 30)).unwrap())
        }
    }

    impl Default for OutputWidget {
        fn default() -> Self {
            let theme_config = ThemeConfig::default();

            OutputWidget::new(&theme_config, ErrorPanePlacement::Top)
        }
    }

    fn result(output: Output) -> CmdResult {
        CmdResult {
            stdin: Arc::from("".as_bytes()),
            outputs: vec![output],
        }
    }

    #[test]
    fn errors_pane_top() {
        let mut terminal = TestTerminal::default().0;

        let mut widget = OutputWidget::default();
        widget.error_pane_placement = ErrorPanePlacement::Top;

        widget.handle_command_result(&result(Output::ok_str("out1\nout2\nout3")));
        widget.handle_command_result(&result(Output::err_str("errors1\nerrors2\nerrors3")));

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

        widget.handle_command_result(&result(Output::ok_str("out1\nout2\nout3")));
        widget.handle_command_result(&result(Output::err_str("errors1\nerrors2\nerrors3")));

        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }
}
