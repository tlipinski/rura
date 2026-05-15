use crossterm::event::KeyCode::Char;
use crossterm::event::{Event, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::widgets::{Block, Paragraph};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

#[derive(Default)]
pub struct SearchWidget {
    pub input: Input,
    pub case_sensitive: bool,
    current: usize,
    total: usize,
}

impl Widget for &SearchWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let par = Paragraph::new(self.input.value()).block(Block::bordered().title(format!(
            " Search: {} / {} | {} ",
            if self.total == 0 { 0 } else { self.current + 1 },
            self.total,
            if self.case_sensitive { "[Cc]" } else { "Cc" }
        )));
        par.render(area, buf);
    }
}

impl SearchWidget {
    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key_event) => {
                let code = key_event.code;
                let mods = key_event.modifiers;

                match (code, mods) {
                    (Char('c'), KeyModifiers::ALT) => {
                        self.case_sensitive = !self.case_sensitive;
                    }
                    _ => {
                        self.input.handle_event(event);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn update_highlight_info(&mut self, info: (usize, usize)) {
        self.current = info.0;
        self.total = info.1;
    }
}
