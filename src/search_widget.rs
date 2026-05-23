use crossterm::event::Event;
use itertools::Itertools;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::prelude::{Style, Widget};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

#[derive(Default)]
pub struct SearchWidget {
    pub input: Input,
    pub case_sensitive: bool,
    pub regex: bool,
    current: usize,
    total: usize,
}

impl Widget for &SearchWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let paragraph = Paragraph::new(self.input.value()).block(Block::bordered().title(format!(
            " Search: {} / {} | {} {} ",
            if self.total == 0 { 0 } else { self.current + 1 },
            self.total,
            if self.regex { "[.*]" } else { ".*" },
            if self.case_sensitive { "[Cc]" } else { "Cc" },
        )));

        paragraph.render(area, buf);

        let inner_area = area.inner(Margin::new(1, 1));

        let line = Line::from(self.input.value());
        let graphemes = line.styled_graphemes(Style::default()).collect_vec();

        let chunks = graphemes.chunks(inner_area.width as usize);

        for (i, c) in chunks.enumerate() {
            crate::rura_widget::render_line(c.to_vec(), inner_area, buf, i as u16)
        }
    }
}

impl SearchWidget {
    pub fn handle_event(&mut self, event: &Event) -> bool {
        self.input
            .handle_event(event)
            .map(|change| change.value)
            .unwrap_or(false)
    }

    pub fn toggle_case_sensitive(&mut self) {
        self.case_sensitive = !self.case_sensitive;
    }

    pub fn toggle_regex(&mut self) {
        self.regex = !self.regex;
    }

    pub fn height(&self, width: u16) -> u16 {
        (self.input.value().len() as u16 / width) + 1
    }

    pub fn cursor(&self, width: u16) -> (u16, u16) {
        let cursor = self.input.visual_cursor() as u16;
        (cursor % width, cursor / width)
    }

    pub fn update_highlight_info(&mut self, info: (usize, usize)) {
        self.current = info.0;
        self.total = info.1;
    }
}
