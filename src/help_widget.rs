use crate::config::KeyBindingsConfig;
use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Styled, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Shadow};
use std::cell::Cell;

pub struct HelpWidget {
    kb_config: KeyBindingsConfig,
    theme: Theme,
    scroll: usize,
    content_height: Cell<usize>,
}

impl HelpWidget {
    pub fn new(kb_config: KeyBindingsConfig, theme: Theme) -> Self {
        Self {
            kb_config,
            theme,
            scroll: 0,
            content_height: Cell::new(20),
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let total_lines = self.get_lines().len();
        let visible_lines = self.visible_content_height();
        if self.scroll + visible_lines < total_lines {
            self.scroll += 1;
        }
    }

    pub fn scroll_page_up(&mut self) {
        let visible_lines = self.visible_content_height();
        self.scroll = self.scroll.saturating_sub(visible_lines);
    }

    pub fn scroll_page_down(&mut self) {
        let total_lines = self.get_lines().len();
        let visible_lines = self.visible_content_height();
        self.scroll = (self.scroll + visible_lines).min(total_lines.saturating_sub(visible_lines));
    }

    fn visible_content_height(&self) -> usize {
        self.content_height.get()
    }

    #[rustfmt::skip]
    fn get_lines(&self) -> Vec<Line<'_>> {
        vec![
            Line::from("Commands").reversed().centered(),
            Line::from(format!("{:012} - Execute full command", self.kb_config.execute_full.first().unwrap().to_string())),
            Line::from(format!("{:012} - Execute until cursor", self.kb_config.execute_until_current.first().unwrap().to_string())),
            Line::from(format!("{:012} - Execute before cursor", self.kb_config.execute_until_prev.first().unwrap().to_string())),
            Line::from(format!("{:012} - Reset input", self.kb_config.reset_input.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:012} - Toggle Live mode", self.kb_config.toggle_live.first().unwrap().to_string())),
            Line::from(format!("{:012} - Toggle Live Until Cursor mode", self.kb_config.toggle_live_until_cursor.first().unwrap().to_string())),
            Line::from(""),
            Line::from("Subcommand edit").reversed().centered(),
            Line::from(format!("{:012} - Go to previous subcommand",self.kb_config.subcommand_prev.first().unwrap().to_string())),
            Line::from(format!("{:012} - Go to next subcommand",self.kb_config.subcommand_next.first().unwrap().to_string())),
            Line::from(format!("{:012} - Copy current subcommand",self.kb_config.subcommand_copy.first().unwrap().to_string())),
            Line::from(format!("{:012} - Cut current subcommand",self.kb_config.subcommand_cut.first().unwrap().to_string())),
            Line::from(format!("{:012} - Paste subcommand after",self.kb_config.subcommand_paste.first().unwrap().to_string())),
            Line::from(""),
            Line::from("Navigation").reversed().centered(),
            Line::from(format!("{:012} - Scroll up",self.kb_config.scroll_up.first().unwrap().to_string())),
            Line::from(format!("{:012} - Scroll down",self.kb_config.scroll_down.first().unwrap().to_string())),
            Line::from(format!("{:012} - Scroll page up",self.kb_config.scroll_up_page.first().unwrap().to_string())),
            Line::from(format!("{:012} - Scroll page down",self.kb_config.scroll_down_page.first().unwrap().to_string())),
            Line::from(format!("{:012} - Scroll right",self.kb_config.scroll_right.first().unwrap().to_string())),
            Line::from(format!("{:012} - Scroll left",self.kb_config.scroll_left.first().unwrap().to_string())),
            Line::from(""),
            Line::from(format!("{:012} - Wrap output lines",self.kb_config.toggle_wrap.first().unwrap().to_string())),
            Line::from(format!("{:012} - Toggle line numbers",self.kb_config.toggle_line_nums.first().unwrap().to_string())),
            Line::from(""),
            Line::from("Presets").reversed().centered(),
            Line::from(format!("{:012} - Show presets popup",self.kb_config.toggle_presets.first().unwrap().to_string())),
            Line::from("ctrl+n       - Create new preset".to_string()),
            Line::from("ctrl+t       - Create new preset from current command".to_string()),
            Line::from("ctrl+d       - Delete preset".to_string()),
            Line::from("ctrl+e       - Edit preset".to_string()),
            Line::from("ctrl+k       - Clone preset".to_string()),
            Line::from(""),
            Line::from("Completion").reversed().centered(),
            Line::from(format!("{:012} - Complete forward",self.kb_config.complete.first().unwrap().to_string())),
            Line::from(format!("{:012} - Complete backward",self.kb_config.complete_prev.first().unwrap().to_string())),
            Line::from(""),
            Line::from("History").reversed().centered(),
            Line::from(format!("{:012} - History previous item",self.kb_config.history_prev.first().unwrap().to_string())),
            Line::from(format!("{:012} - History next item",self.kb_config.history_next.first().unwrap().to_string())),
            Line::from(""),
            Line::from("Search").reversed().centered(),
            Line::from(format!("{:012} - Search next",self.kb_config.search_next.first().unwrap().to_string())),
            Line::from(format!("{:012} - Search previous",self.kb_config.search_prev.first().unwrap().to_string())),
            Line::from(format!("{:012} - Toggle regex mode", "alt+x")),
            Line::from(format!("{:012} - Toggle case sensitivity", "alt+c")),
            Line::from(""),
            Line::from("Saving to file").reversed().centered(),
            Line::from(format!("{:012} - Save output to file", self.kb_config.save_output.first().unwrap().to_string())),
            Line::from(format!("{:012} - Save command to file",self.kb_config.save_command.first().unwrap().to_string())),
            Line::from(""),
            Line::from("Stdin").reversed().centered(),
            Line::from(format!("{:012} - Toggle stdin reading", self.kb_config.toggle_stdin_reading.first().unwrap().to_string())),
            Line::from(format!("{:012} - Toggle stdin follow", self.kb_config.toggle_follow.first().unwrap().to_string())),
        ]
    }
}

impl Widget for &HelpWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let lines = self.get_lines();
        let total_lines = lines.len();

        let available_height_with_arrows = (area.height as usize).saturating_sub(4);
        let available_height_no_arrows = (area.height as usize).saturating_sub(2);

        let (content_height, show_arrows) = if total_lines <= available_height_no_arrows {
            (total_lines, false)
        } else {
            (available_height_with_arrows.max(1), true)
        };

        self.content_height.set(content_height);

        let start = self.scroll.min(total_lines.saturating_sub(content_height));
        let end = (start + content_height).min(total_lines);

        let mut display_lines = Vec::new();

        if show_arrows {
            if start > 0 {
                display_lines.push(Line::from("▲").centered().yellow());
            } else {
                display_lines.push(Line::from(" "));
            }

            display_lines.extend(lines[start..end].iter().cloned());

            if end < total_lines {
                display_lines.push(Line::from("▼").centered().yellow());
            } else {
                display_lines.push(Line::from(" "));
            }
        } else {
            display_lines.extend(lines[start..end].iter().cloned());
        }

        let max_width = display_lines
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or_default();

        let centered_area = area.centered(
            Constraint::Length(max_width as u16 + 2),
            Constraint::Length(display_lines.len() as u16 + 2),
        );
        Clear.render(centered_area, buf);
        Block::default()
            .borders(Borders::ALL)
            .title(" Keys ")
            .set_style(self.theme.popup)
            .shadow(Shadow::dark_shade())
            .render(centered_area, buf);

        Paragraph::new(display_lines).render(centered_area.inner(Margin::new(1, 1)), buf);
    }
}
