use crate::shell::pipeline_runner::PipelineRun;
use itertools::Itertools;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::Widget;
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Cell, Row, Table};

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
            pipeline_run: PipelineRun::default(),
            show_stdin: true,
        }
    }
}

impl Widget for &DetailsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let header = Row::new([
            Cell::from("Command"),
            Cell::from(Text::from("Time").right_aligned()),
            Cell::from(Text::from("Lines").right_aligned()),
            Cell::from(Text::from("Size").right_aligned()),
        ])
        .style(Style::new().bold());

        let mut rows = vec![];

        if self.show_stdin {
            rows.push(Row::new([
                Cell::from(String::from("<stdin>")),
                Cell::from(Text::from(String::from("-")).right_aligned()),
                Cell::from(
                    Text::from(format_thousands(self.pipeline_run.stdin.lines as u128))
                        .right_aligned(),
                ),
                Cell::from(
                    Text::from(format_file_size(self.pipeline_run.stdin.bytes.len() as u64))
                        .right_aligned(),
                ),
            ]))
        }

        let step_rows = self
            .pipeline_run
            .steps
            .iter()
            .map(|step| {
                let mut duration = format_duration(step.duration.as_millis().try_into().unwrap());

                if step.cached {
                    duration = format!("{} *", duration);
                }

                Row::new(vec![
                    Cell::from(step.command.clone()),
                    Cell::from(Text::from(duration).right_aligned()),
                    Cell::from(Text::from(format_thousands(step.lines as u128)).right_aligned()),
                    Cell::from(
                        Text::from(format_file_size(step.bytes.len() as u64)).right_aligned(),
                    ),
                ])
            })
            .collect_vec();

        rows.extend(step_rows);

        if let Some(failure) = &self.pipeline_run.failure {
            rows.push(
                Row::new([
                    Cell::from(failure.command.clone()),
                    Cell::from(
                        Text::from(format_duration(
                            failure.duration.as_millis().try_into().unwrap(),
                        ))
                        .right_aligned(),
                    ),
                    Cell::from(Text::from(format_thousands(failure.lines as u128)).right_aligned()),
                    Cell::from(
                        Text::from(format_file_size(failure.bytes.len() as u64)).right_aligned(),
                    ),
                ])
                .style(Style::new().red()),
            )
        }

        let _total_duration = self.pipeline_run.total_duration().as_millis().to_string();

        let widths = [
            Constraint::Max(40),
            Constraint::Length(10),
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
    use crate::shell::pipeline_runner::{PipelineRun, Stdin, StepFailure, StepOutput};
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
        widget.pipeline_run = PipelineRun {
            stdin: Stdin::default(),
            steps: vec![],
            failure: None,
        };

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
            stdin: Stdin::new(Arc::from("stdin".as_bytes())),
            steps: vec![
                StepOutput::new(
                    "cmd1".into(),
                    Arc::from("1234567890".as_bytes()),
                    Duration::from_millis(1),
                    true,
                ),
                StepOutput::new(
                    "cmd2".into(),
                    Arc::from("1234567890".as_bytes()),
                    Duration::from_millis(20000),
                    true,
                ),
                StepOutput::new(
                    "cmd3".into(),
                    Arc::from("x".as_bytes()),
                    Duration::from_millis(1000),
                    false,
                ),
            ],
            failure: Some(StepFailure::new(
                "failed".into(),
                Arc::from("failed".as_bytes()),
                None,
                Duration::from_millis(10),
            )),
        };

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }
}

pub fn format_thousands(n: u128) -> String {
    let num_str = n.to_string();
    let mut result = String::new();

    for (i, c) in num_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result.chars().rev().collect()
}

pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let suffixes = ["B", "KiB", "MiB", "GiB"];
    let bytes_f = bytes as f64;
    let scale = ((bytes_f.log2() / 10.0).floor() as usize).min(suffixes.len() - 1);
    let value = bytes_f / 1024.0_f64.powi(scale as i32);
    format!("{:.1} {}", value, suffixes[scale])
}

pub fn format_duration(ms: u64) -> String {
    const MS_PER_SEC: u64 = 1_000;
    const MS_PER_MIN: u64 = 60_000;
    const MS_PER_HOUR: u64 = 3_600_000;

    if ms < MS_PER_SEC {
        format!("{}ms", ms)
    } else if ms < MS_PER_MIN {
        let secs = ms as f64 / 1000.0;
        format!("{:.1}s", secs)
    } else if ms < MS_PER_HOUR {
        let mins = ms / MS_PER_MIN;
        let remaining_ms = ms % MS_PER_MIN;
        let secs = remaining_ms / MS_PER_SEC;
        format!("{}m {}s", mins, secs)
    } else {
        let hours = ms / MS_PER_HOUR;
        let remaining_ms = ms % MS_PER_HOUR;
        let mins = remaining_ms / MS_PER_MIN;
        let secs = (remaining_ms % MS_PER_MIN) / MS_PER_SEC;
        format!("{}h {}m {}s", hours, mins, secs)
    }
}
