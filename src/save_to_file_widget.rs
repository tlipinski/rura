use crate::completable_input::CompletableInput;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint::Length;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::{Line, Stylize, Widget};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

pub struct SaveToFileWidget {
    pub title: String,
    pub file_path_input: CompletableInput,
    pub error_message: Option<String>,
    pub cursor: (u16, u16),
}

impl SaveToFileWidget {
    pub fn new(title: String, shell: String) -> Self {
        Self {
            title,
            file_path_input: CompletableInput::file_only("", &shell),
            error_message: None,
            cursor: (0, 0),
        }
    }

    pub fn save(&mut self, content: &str) -> anyhow::Result<()> {
        self.save_file(content, 0o644)
    }

    pub fn save_executable(&mut self, content: &str) -> anyhow::Result<()> {
        self.save_file(content, 0o755)
    }

    fn save_file(&mut self, content: &str, mode: u32) -> anyhow::Result<()> {
        let path = PathBuf::from(self.file_path_input.value().trim());
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .open(path)?;

        write!(file, "{}\n", content)?;

        self.error_message = None;

        Ok(())
    }
}

impl Widget for &mut SaveToFileWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let height = if self.error_message.is_some() { 7 } else { 6 };

        let centered_area = area.centered(Constraint::Percentage(60), Constraint::Length(height));

        let centered_inner_area = centered_area.inner(Margin::new(1, 1));

        let [path_area, error_area, buttons_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Length(3),
                Length(if self.error_message.is_some() { 1 } else { 0 }),
                Length(1),
            ])
            .areas(centered_inner_area);

        Clear.render(centered_area, buf);
        Block::default()
            .borders(Borders::ALL)
            .title(self.title.clone())
            .white()
            .on_blue()
            .render(centered_area, buf);

        let path_input_area = centered_inner_area.inner(Margin::new(1, 1));
        let shift = self
            .file_path_input
            .cursor()
            .saturating_sub(path_input_area.width.into()) as u16;
        Paragraph::new(self.file_path_input.value())
            .block(Block::default().borders(Borders::ALL))
            .scroll((0, shift))
            .render(path_area, buf);

        if let Some(error_message) = &self.error_message {
            Line::from(error_message.clone())
                .red()
                .on_white()
                .render(error_area, buf);
        }

        Line::from(vec![
            "Enter ".bold(),
            "Save | ".into(),
            "Esc ".bold(),
            "Cancel".into(),
        ])
        .right_aligned()
        .render(buttons_area, buf);

        self.cursor = (
            (path_input_area.x + self.file_path_input.cursor() as u16)
                .min(path_input_area.width + path_area.x),
            path_input_area.y,
        );
    }
}
