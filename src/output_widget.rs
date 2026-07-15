use crate::config::ThemeConfig;
use crate::content_widget::{ContentLine, ContentWidget, Position};
use crate::shell::pipeline_runner::PipelineRun;
use crate::theme::Theme;
use itertools::Itertools;
use log::{debug, info};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect, Size};
use ratatui::prelude::Color::Red;
use ratatui::prelude::{Style, Widget};
use ratatui::widgets::{Block, Paragraph};
use similar::TextDiff;
use similar::{Algorithm, ChangeTag};
use std::cell::Cell;
use std::time::Duration;

pub struct OutputWidget {
    content: ContentWidget<String>,
    diff: ContentWidget<(ChangeTag, String)>,
    error_output_opt: Option<(Vec<String>, Option<i32>)>,
    theme: Theme,
    error_pane_placement: ErrorPanePlacement,
    pipeline_run: PipelineRun,
    pub content_mode: ContentMode,
    diff_base: Option<usize>,
    diff_ready: bool,
}

impl ContentLine for String {
    fn string(&self) -> String {
        self.into()
    }

    fn style(&self, _theme: &Theme) -> Style {
        Style::default()
    }
}

impl ContentLine for (ChangeTag, String) {
    fn string(&self) -> String {
        self.clone().1
    }

    fn style(&self, theme: &Theme) -> Style {
        match self.0 {
            ChangeTag::Equal => theme.diff_equal,
            ChangeTag::Insert => theme.diff_addition,
            ChangeTag::Delete => theme.diff_deletion,
        }
    }
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
                line_nums: true,
                main_output_width: 0,
            },
            diff: ContentWidget {
                offset: Position::default(),
                lines: vec![],
                wrap: false,
                highlight_positions: vec![],
                highlight_index: 0,
                theme: Theme::from_config(theme_config),
                output_content_area_size: Cell::new(Size::default()),
                line_nums: true,
                main_output_width: 0,
            },
            error_output_opt: None,
            theme: Theme::from_config(theme_config),
            error_pane_placement,
            pipeline_run: PipelineRun::default(),
            content_mode: ContentMode::Normal,
            diff_base: None,
            diff_ready: false,
        }
    }

    pub fn toggle_diff(&mut self) {
        match self.content_mode {
            ContentMode::Normal => {
                self.content_mode = ContentMode::Diff;
                self.diff()
            }
            ContentMode::Diff => self.content_mode = ContentMode::Normal,
        }
    }

    pub fn diff_base(&self) -> Option<usize> {
        self.diff_base
    }

    pub fn diff(&mut self) {
        let now = std::time::Instant::now();
        if self.diff_ready {
            return;
        }
        let step_bytes = self.pipeline_run.step_bytes().clone();
        let last_bytes = step_bytes.last().unwrap_or(&self.pipeline_run.stdin.bytes);
        let vec = self.pipeline_run.step_bytes();
        let stdin_bytes = if let Some(base) = self.diff_base {
            if let Some(b) = vec.get(base) {
                b.as_ref()
            } else {
                return;
            }
        } else {
            self.pipeline_run.stdin.bytes.as_ref()
        };

        let old = String::from_utf8_lossy(&stdin_bytes);
        let new = String::from_utf8_lossy(&last_bytes);

        let text_diff: TextDiff<str> = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .timeout(Duration::from_millis(5000))
            .diff_lines(&old, &new);

        debug!("Diff took {}ms", now.elapsed().as_millis());

        let old_lines = old.lines().collect_vec();
        let new_lines = new.lines().collect_vec();

        let lines: Vec<(ChangeTag, String)> = text_diff
            .ops()
            .into_iter()
            .flat_map(|op| {
                op.iter_slices(&old_lines, &new_lines)
                    .flat_map(|(tag, slice)| slice.iter().map(move |&s| (tag, s.to_string())))
            })
            .collect_vec();

        self.diff_ready = true;
        self.diff.with_content(lines);
    }

    pub fn highlight_info(&self) -> (usize, usize) {
        match self.content_mode {
            ContentMode::Normal => self.content.highlight_info(),
            ContentMode::Diff => self.diff.highlight_info(),
        }
    }

    pub fn set_diff_base(&mut self, base: Option<usize>) {
        self.diff_base = base;
        self.content_mode = ContentMode::Diff;
        self.diff_ready = false;
        self.diff();
    }

    pub fn clear_highlight(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.clear_highlight(),
            ContentMode::Diff => self.diff.clear_highlight(),
        }
    }

    pub fn highlight_next(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.highlight_next(),
            ContentMode::Diff => self.diff.highlight_next(),
        }
    }

    pub fn highlight_prev(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.highlight_prev(),
            ContentMode::Diff => self.diff.highlight_prev(),
        }
    }

    pub fn highlight(&mut self, search_str: &str, case_sensitive: bool, regex: bool) {
        match self.content_mode {
            ContentMode::Normal => self.content.highlight(search_str, case_sensitive, regex),
            ContentMode::Diff => self.diff.highlight(search_str, case_sensitive, regex),
        }
    }

    pub fn output_len(&self) -> usize {
        match self.content_mode {
            ContentMode::Normal => self.content.output_len(),
            ContentMode::Diff => self.diff.output_len(),
        }
    }

    pub fn handle_pipeline_run(&mut self, pipeline_run: PipelineRun, follow: bool) {
        info!("Command result: {:?}", pipeline_run);
        self.pipeline_run = pipeline_run;

        debug!(
            "handle_command_result: {:?}",
            self.pipeline_run.step_bytes().len()
        );

        if let Some(step_failure) = &self.pipeline_run.failure {
            let str = String::from_utf8_lossy(&step_failure.bytes);
            let lines = str.lines().map(|a| a.into()).collect_vec();

            self.error_output_opt = Some((lines, step_failure.code));
        } else {
            if let Some(bytes) = &self.pipeline_run.step_bytes().last() {
                let str = String::from_utf8_lossy(&bytes);
                let lines = str.lines().map(|a| a.into()).collect_vec();
                self.content.with_content(lines);

                self.error_output_opt = None;
            } else {
                let str = String::from_utf8_lossy(&self.pipeline_run.stdin.bytes);
                let lines = str.lines().map(|a| a.into()).collect_vec();
                self.content.with_content(lines);

                self.error_output_opt = None;
            }
        }

        self.diff_ready = false;

        match self.content_mode {
            ContentMode::Normal => {
                if follow {
                    self.content.follow()
                }
            }
            ContentMode::Diff => {
                self.diff();
                if follow {
                    self.diff.follow()
                }
            }
        }

        self.clear_highlight();
    }

    pub fn scroll_down(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_down(),
            ContentMode::Diff => self.diff.scroll_down(),
        }
    }

    pub fn scroll_page_down(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_page_down(),
            ContentMode::Diff => self.diff.scroll_page_down(),
        }
    }

    pub fn scroll_up(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_up(),
            ContentMode::Diff => self.diff.scroll_up(),
        }
    }

    pub fn scroll_page_up(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_page_up(),
            ContentMode::Diff => self.diff.scroll_page_up(),
        }
    }

    pub fn scroll_left(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_left(),
            ContentMode::Diff => self.diff.scroll_left(),
        }
    }

    pub fn scroll_page_left(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_page_left(),
            ContentMode::Diff => self.diff.scroll_page_left(),
        }
    }

    pub fn scroll_right(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_right(),
            ContentMode::Diff => self.diff.scroll_right(),
        }
    }

    pub fn scroll_page_right(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.scroll_page_right(),
            ContentMode::Diff => self.diff.scroll_page_right(),
        }
    }

    pub fn toggle_wrap(&mut self) {
        match self.content_mode {
            ContentMode::Normal => self.content.wrap = !self.content.wrap,
            ContentMode::Diff => self.diff.wrap = !self.diff.wrap,
        }
    }

    pub fn toggle_line_nums(&mut self) {
        self.content.toggle_line_nums();
        self.diff.toggle_line_nums();
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

        match self.content_mode {
            ContentMode::Normal => self.content.render(output_content_area, buf),
            ContentMode::Diff => self.diff.render(output_content_area, buf),
        }
    }
}

pub enum ContentMode {
    Normal,
    Diff,
}

pub enum ErrorPanePlacement {
    Top,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn result_ok(s: &str) -> PipelineRun {
        PipelineRun::from_bytes(Arc::from(s.as_bytes()))
    }

    fn result_err(s: &str) -> PipelineRun {
        PipelineRun::error_bytes(Arc::from(s.as_bytes()), Some(1))
    }

    #[test]
    fn errors_pane_top() {
        let mut terminal = TestTerminal::default().0;

        let mut widget = OutputWidget::default();
        widget.error_pane_placement = ErrorPanePlacement::Top;

        widget.handle_pipeline_run(result_ok("out1\nout2\nout3"), false);
        widget.handle_pipeline_run(result_err("errors1\nerrors2\nerrors3"), false);

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

        widget.handle_pipeline_run(result_ok("out1\nout2\nout3"), false);
        widget.handle_pipeline_run(result_err("errors1\nerrors2\nerrors3"), false);

        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn highlighting_in_diff_mode() {
        let mut widget = OutputWidget::default();
        let stdin = Arc::from("line1\nline2\nline3".as_bytes());
        let output = "line1\nline2 modified\nline3";
        widget.handle_pipeline_run(
            PipelineRun::from_bytes_with_stdin(Arc::from(output.as_bytes()), stdin),
            false,
        );

        widget.toggle_diff(); // Switch to diff mode
        assert!(matches!(widget.content_mode, ContentMode::Diff));

        widget.highlight("modified", false, false);
        let info = widget.highlight_info();
        assert_eq!(info.1, 1); // 1 match found

        // Test output_len
        assert_eq!(widget.output_len(), 4);

        // Test clear_highlight
        widget.clear_highlight();
        assert_eq!(widget.highlight_info().1, 0);
    }
}
