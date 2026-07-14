use crate::shell::pipeline_runner::PipelineRun;
use humansize::FormatSize;
use itertools::Itertools;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::Widget;
use ratatui::style::Style;
use ratatui::widgets::{Row, Table};

pub struct DetailsWidget {
    pub pipeline_run: PipelineRun,
    show_stdin: bool,
}

impl DetailsWidget {
    pub fn height(&self) -> u16 {
        self.pipeline_run.steps.len() as u16
            + if self.pipeline_run.failure.is_some() {
                1
            } else {
                0
            }
            + if self.show_stdin { 1 } else { 0 }
            + 1 /* header */
    }
}

impl Default for DetailsWidget {
    fn default() -> Self {
        DetailsWidget {
            pipeline_run: PipelineRun::new(),
            show_stdin: true,
        }
    }
}

impl Widget for &DetailsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header = Row::new(["Command", "Time", "Size"]).style(Style::new().bold());

        let mut rows = vec![];

        if self.show_stdin {
            rows.push(Row::new([
                String::from("<stdin>"),
                String::from("-"),
                self.pipeline_run.stdin.len().format_size(humansize::BINARY),
            ]))
        }

        let step_rows = self
            .pipeline_run
            .steps
            .iter()
            .map(|step| {
                let duration = step
                    .duration
                    .map(|d| format!("{} ms", d.as_millis().to_string()))
                    .unwrap_or("-".into());
                Row::new([
                    step.command.clone(),
                    duration,
                    step.bytes.len().format_size(humansize::BINARY),
                ])
            })
            .collect_vec();

        rows.extend(step_rows);

        if let Some(failure) = &self.pipeline_run.failure {
            rows.push(
                Row::new([
                    failure.command.clone(),
                    format!("{} ms", failure.duration.as_millis()),
                    failure.bytes.len().format_size(humansize::BINARY),
                ])
                .style(Style::new().red()),
            )
        }

        let _total_duration = self.pipeline_run.total_duration().as_millis().to_string();

        let widths = [
            Constraint::Max(40),
            Constraint::Length(10),
            Constraint::Length(10),
        ];
        let table = Table::new(rows, widths).header(header).column_spacing(1);

        table.render(area, buf);
    }
}

#[cfg(test)]
mod test {
    use crate::details_widget::DetailsWidget;
    use crate::shell::pipeline_runner::{PipelineRun, StepFailure, StepOutput};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::prelude::Widget;
    use std::sync::Arc;
    use std::time::Duration;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(50, 10)).unwrap())
        }
    }

    #[test]
    fn empty_run() {
        let mut widget = DetailsWidget::default();
        widget.pipeline_run = PipelineRun::new();

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn non_empty_run() {
        let mut widget = DetailsWidget::default();
        widget.pipeline_run = PipelineRun {
            stdin: Arc::from("stdin".as_bytes()),
            steps: vec![
                StepOutput {
                    command: "cmd1".to_string(),
                    bytes: Arc::from("1234567890".as_bytes()),
                    duration: None,
                },
                StepOutput {
                    command: "cmd2".to_string(),
                    bytes: Arc::from("1234567890".as_bytes()),
                    duration: Some(Duration::from_millis(1000)),
                },
                StepOutput {
                    command: "cmd3".to_string(),
                    bytes: Arc::from("x".as_bytes()),
                    duration: Some(Duration::from_millis(100)),
                },
            ],
            failure: Some(StepFailure {
                command: "failed".to_string(),
                bytes: Arc::from("failed".as_bytes()),
                duration: Duration::from_millis(10),
                code: Some(1),
            }),
        };

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }
}
