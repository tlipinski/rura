use crate::completable_input::CompletableInput;
use crate::file_saver::FileSaver;
use crate::theme::Theme;
use crossterm::event::Event;
use log::error;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint::Length;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::{Line, Stylize, Widget};
use ratatui::style::Styled;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Shadow};
use std::cell::Cell;
use std::path::PathBuf;

pub struct SaveToFileWidget {
    file_saver: Box<dyn FileSaver>,
    title: String,
    theme: Theme,
    file_path_input: CompletableInput,
    error_message: Option<String>,
    cursor: Cell<(u16, u16)>,
    overwrite_confirm: bool,
}

impl SaveToFileWidget {
    pub fn new(file_saver: Box<dyn FileSaver>, title: String, shell: String, theme: Theme) -> Self {
        Self {
            file_saver,
            title,
            theme,
            file_path_input: CompletableInput::file_only("", &shell),
            error_message: None,
            cursor: Cell::new((0, 0)),
            overwrite_confirm: false,
        }
    }

    pub fn overwrite_confirm(&self) -> bool {
        self.overwrite_confirm
    }

    pub fn cancel(&mut self) {
        self.clear();
    }

    pub fn complete(&mut self) {
        self.file_path_input.complete(true)
    }

    pub fn complete_prev(&mut self) {
        self.file_path_input.complete(false)
    }

    pub fn cursor(&self) -> Option<(u16, u16)> {
        if self.overwrite_confirm {
            None
        } else {
            Some(self.cursor.get())
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        if !self.overwrite_confirm {
            self.file_path_input.handle_event(event);
        }
        ()
    }

    pub fn confirm(&mut self, content: Vec<u8>, executable: bool, overwrite: bool) -> bool {
        let path = PathBuf::from(self.file_path_input.value().trim());
        match self.file_saver.save(path, content, executable, overwrite) {
            Ok(()) => {
                self.clear();
                true
            }
            Err(e) if e.to_string().contains("File exists") => {
                self.error_message = Some(e.to_string());
                self.overwrite_confirm = true;
                false
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
                error!("Error saving command to file: {}", e);
                false
            }
        }
    }

    fn clear(&mut self) {
        self.overwrite_confirm = false;
        self.error_message = None;
    }
}

impl Widget for &SaveToFileWidget {
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
            .set_style(self.theme.popup)
            .shadow(Shadow::dark_shade())
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

        if self.overwrite_confirm {
            Line::from("                            ")
                .right_aligned()
                .render(buttons_area, buf);

            Line::from(vec!["Overwrite? ".bold(), "[Y]es ".bold(), "[N]o ".bold()])
                .right_aligned()
                .render(buttons_area, buf);
        } else {
            self.cursor.set((
                (path_input_area.x + self.file_path_input.cursor() as u16)
                    .min(path_input_area.width + path_area.x),
                path_input_area.y,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;
    use crossterm::event::Event::Key;
    use crossterm::event::KeyCode::Char;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    struct MockFileSaver;

    impl FileSaver for MockFileSaver {
        fn save(
            &self,
            path: PathBuf,
            _content: Vec<u8>,
            _executable: bool,
            overwrite: bool,
        ) -> anyhow::Result<()> {
            if path.to_str().unwrap().contains("exists") {
                if overwrite {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("File exists"))
                }
            } else {
                Ok(())
            }
        }
    }

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(100, 20)).unwrap())
        }
    }

    #[test]
    fn open_dialog() {
        let mut widget = SaveToFileWidget::new(
            Box::new(MockFileSaver {}),
            " Test ".into(),
            "shell".into(),
            Theme::from_config(&ThemeConfig::default()),
        );

        input_text(&mut widget, "test.txt");

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn save_success() {
        let mut widget = SaveToFileWidget::new(
            Box::new(MockFileSaver {}),
            " Test ".into(),
            "shell".into(),
            Theme::from_config(&ThemeConfig::default()),
        );

        input_text(&mut widget, "test.txt");

        widget.confirm(vec![], false, false);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn save_exists_show_overwrite_confirm() {
        let mut widget = SaveToFileWidget::new(
            Box::new(MockFileSaver {}),
            " Test ".into(),
            "shell".into(),
            Theme::from_config(&ThemeConfig::default()),
        );

        input_text(&mut widget, "test-exists.txt");

        widget.confirm(vec![], false, false);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn save_exists_cancel_overwrite() {
        let mut widget = SaveToFileWidget::new(
            Box::new(MockFileSaver {}),
            " Test ".into(),
            "shell".into(),
            Theme::from_config(&ThemeConfig::default()),
        );

        input_text(&mut widget, "test-exists.txt");

        widget.confirm(vec![], false, false);

        widget.cancel();

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn save_exists_confirm_overwrite() {
        let mut widget = SaveToFileWidget::new(
            Box::new(MockFileSaver {}),
            " Test ".into(),
            "shell".into(),
            Theme::from_config(&ThemeConfig::default()),
        );

        input_text(&mut widget, "test-exists.txt");

        widget.confirm(vec![], false, false);

        widget.confirm(vec![], false, true);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    fn input_text(widget: &mut SaveToFileWidget, text: &str) {
        for c in text.chars() {
            widget.handle_event(&Key(KeyEvent {
                code: Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }))
        }
    }
}
