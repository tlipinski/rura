use crate::presets::{FilePresetsStore, Preset, PresetsStore};
use crate::theme::Theme;
use crossterm::event::Event;
use itertools::Itertools;
use log::error;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint::{Fill, Length};
use ratatui::layout::{Direction, Layout, Margin, Rect};
use ratatui::prelude::Stylize;
use ratatui::prelude::{Text, Widget};
use ratatui::style::{Style, Styled};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListState, Paragraph, Shadow, StatefulWidget};
use std::cell::Cell;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

pub struct PresetsWidget {
    pub display_mode: DisplayMode,
    command_input: Input,
    shortcut_input: Input,
    store: Box<dyn PresetsStore>,
    presets: Vec<Preset>,
    selected: Option<usize>,
    theme: Theme,
    edit_mode: EditMode,
    cursor: Cell<(u16, u16)>,
}

impl PresetsWidget {
    pub fn new(theme: Theme) -> Self {
        Self {
            store: Box::new(FilePresetsStore::default()),
            command_input: Input::from(""),
            shortcut_input: Input::from(""),
            display_mode: DisplayMode::Select,
            presets: vec![],
            selected: None,
            theme,
            edit_mode: EditMode::Command,
            cursor: Cell::new((0, 0)),
        }
    }

    pub fn load(&mut self) {
        self.selected = None;
        match self.store.load() {
            Ok(presets) => {
                self.presets = presets.clone();
            }
            Err(e) => {
                error!("Failed to load presets: {e}")
            }
        }
    }

    pub fn next(&mut self) {
        if !self.presets.is_empty() {
            if let Some(s) = self.selected {
                self.selected = Some((s + 1).min(self.presets.len() - 1));
            } else {
                self.selected = Some(0);
            }
        }
    }

    pub fn previous(&mut self) {
        if !self.presets.is_empty() {
            if let Some(s) = self.selected {
                self.selected = Some(s.saturating_sub(1));
            } else {
                self.selected = Some(self.presets.len() - 1)
            }
        }
    }

    pub fn confirm(&self) -> Option<String> {
        if let Some(s) = self.selected {
            self.presets.get(s).map(|p| p.command.clone())
        } else {
            None
        }
    }

    pub fn find_by_shortcut(&self, shortcut: char) -> Option<String> {
        self.presets
            .iter()
            .find(|p| p.shortcut == Some(shortcut))
            .map(|p| p.command.clone())
    }

    pub fn confirm_delete(&mut self) {
        if let Some(s) = self.selected {
            self.display_mode = DisplayMode::ConfirmDelete(s);
        }
    }

    pub fn cancel_delete(&mut self) {
        self.display_mode = DisplayMode::Select;
    }

    pub fn cancel_edit(&mut self) {
        if let DisplayMode::Edit(index, editing_new) = self.display_mode {
            if editing_new {
                self.presets.remove(index);
                self.selected = None;
            }
            self.display_mode = DisplayMode::Select;
        }
    }

    pub fn delete(&mut self) {
        if let Some(s) = self.selected {
            self.presets.remove(s);
            self.store.save(&self.presets).unwrap();
            self.load();
            self.display_mode = DisplayMode::Select;
        }
    }

    pub fn clone(&mut self) {
        if let Some(s) = self.selected {
            let mut preset = self.presets[s].clone();
            preset.shortcut = None;
            self.presets.insert(s + 1, preset);
            self.store.save(&self.presets).unwrap();
            self.load();
            self.selected = Some(s + 1);
        }
    }

    pub fn new_empty(&mut self) {
        self.new_from("");
    }

    pub fn new_from(&mut self, value: &str) {
        self.command_input = self.command_input.clone().with_value(value.into());
        self.shortcut_input = self.shortcut_input.clone().with_value("".into());
        self.edit_mode = EditMode::Command;
        let preset = Preset {
            command: self.command_input.value().to_string().trim().into(),
            shortcut: self.shortcut_input.value().parse().ok(),
        };
        if let Some(s) = self.selected {
            self.display_mode = DisplayMode::Edit(s + 1, true); // insert after selected
            self.presets.insert(s + 1, preset);
            self.selected = Some(s + 1);
        } else {
            self.display_mode = DisplayMode::Edit(self.presets.len(), true);
            self.presets.insert(self.presets.len(), preset);
            self.selected = Some(self.presets.len() - 1);
        }
    }

    pub fn edit(&mut self) {
        if let Some(s) = self.selected {
            self.command_input = self
                .command_input
                .clone()
                .with_value(self.presets[s].command.clone());
            self.shortcut_input = self.shortcut_input.clone().with_value(
                self.presets[s]
                    .shortcut
                    .map(|a| a.to_string())
                    .unwrap_or_default(),
            );
            self.display_mode = DisplayMode::Edit(s, false);
            self.edit_mode = EditMode::Command;
        }
    }

    pub fn cursor(&self) -> Option<(u16, u16)> {
        match self.display_mode {
            DisplayMode::Select => None,
            DisplayMode::ConfirmDelete(_) => None,
            DisplayMode::Edit { .. } => Some(self.cursor.get()),
        }
    }

    pub fn toggle_edit_mode(&mut self) {
        self.edit_mode = match self.edit_mode {
            EditMode::Command => EditMode::Shortcut,
            EditMode::Shortcut => EditMode::Command,
        };
    }

    pub fn save_edit(&mut self) {
        match self.display_mode {
            DisplayMode::Select => {}
            DisplayMode::ConfirmDelete(_) => {}
            DisplayMode::Edit(index, _) => {
                let current_value = self.command_input.value().trim();
                if !current_value.is_empty() {
                    self.presets[index].command = current_value.into();
                    self.presets[index].shortcut = self.shortcut_input.value().parse().ok();
                    self.display_mode = DisplayMode::Select;
                    let _ = self.store.save(&self.presets);
                }
            }
        }
    }

    pub fn move_up(&mut self) {
        if let DisplayMode::Select = self.display_mode {
            if let Some(index) = self.selected {
                self.presets.swap(index, index.saturating_sub(1));
                self.selected = Some(index.saturating_sub(1));
                let _ = self.store.save(&self.presets);
            }
        }
    }

    pub fn move_down(&mut self) {
        if let DisplayMode::Select = self.display_mode {
            if let Some(index) = self.selected {
                let index_down = (index + 1).min(self.presets.len() - 1);
                self.presets.swap(index, index_down);
                self.selected = Some(index_down);
                let _ = self.store.save(&self.presets);
            }
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        match self.edit_mode {
            EditMode::Command => {
                self.command_input.handle_event(event);
            }
            EditMode::Shortcut => {
                let existing_shortcuts = self
                    .presets
                    .iter()
                    .filter_map(|p| p.shortcut.as_ref())
                    .collect::<Vec<_>>();
                self.shortcut_input.handle_event(event);
                let value = self.shortcut_input.value();
                let valid_value = value
                    .chars()
                    .last()
                    .filter(|c| matches!(c, 'a'..='z'))
                    .filter(|c| !existing_shortcuts.contains(&c))
                    .map(|c| c.to_ascii_lowercase())
                    .map(|c| c.to_string())
                    .unwrap_or_default();

                self.shortcut_input = self.shortcut_input.clone().with_value(valid_value);
            }
        }
    }
}

impl Widget for &PresetsWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let max_width = (area.width as f32 * 0.8) as usize;
        let max_visible_command_width = max_width - 5;

        let shift = self
            .command_input
            .cursor()
            .saturating_sub(max_visible_command_width - 2);

        let presets = match self.display_mode {
            DisplayMode::Select => self.presets.clone(),
            DisplayMode::ConfirmDelete(_) => self.presets.clone(),
            DisplayMode::Edit(index, _) => {
                let mut p = self.presets.clone();
                p.remove(index);
                p.insert(
                    index,
                    Preset {
                        command: self.command_input.value()[shift..].into(),
                        shortcut: Some(self.shortcut_input.value().chars().last().unwrap_or(' ')),
                    },
                );
                p
            }
        };

        let height = presets.len() + 2;
        let min_width = 10;
        let max_command_width = presets
            .iter()
            .map(|p| p.command.len())
            .max()
            .unwrap_or(min_width)
            .max(min_width);
        let margins_plus_shortcut_width = 7;
        let width = max_command_width + margins_plus_shortcut_width;
        let centered_area =
            area.centered(Length(width.min(max_width) as u16), Length(height as u16));

        let centered_inner_area = centered_area.inner(Margin::new(1, 1));

        let [list_area, _, _buttons_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Fill(1), Length(0), Length(0)])
            .areas(centered_inner_area);

        Clear.render(centered_area, buf);

        let items: Vec<Line> = presets
            .iter()
            .map(|p| match p.shortcut {
                Some(sh) => Line::from(vec![
                    Span::from("[").style(Style::default().bold()),
                    Span::from(String::from(sh)).style(Style::default().bold()),
                    Span::from("] ").style(Style::default().bold()),
                    Span::from(p.command.clone()),
                ]),
                None => Line::from(vec![Span::from("    "), Span::from(p.command.clone())]),
            })
            .collect_vec();

        let list = List::new(items).highlight_style(Style::default().reversed());

        Block::default()
            .borders(Borders::ALL)
            .title(" Presets ")
            .set_style(self.theme.popup)
            .shadow(Shadow::dark_shade())
            .render(centered_area, buf);

        let mut state = ListState::default();
        state.select(self.selected);

        StatefulWidget::render(list, list_area, buf, &mut state);

        match self.display_mode {
            DisplayMode::Select => {}
            DisplayMode::ConfirmDelete(index) => {
                let width: u16 = self.presets[index]
                    .command
                    .len()
                    .min(area.width as usize)
                    .max(25) as u16;
                let centered_area = area.centered(Length(width), Length(8));
                Clear.render(centered_area, buf);

                Block::default()
                    .borders(Borders::ALL)
                    .title(" Confirm ")
                    .set_style(self.theme.popup)
                    .render(centered_area, buf);

                let inner = centered_area.inner(Margin::new(1, 1));

                let text: Vec<Line> = vec![
                    Line::from(" Are you sure you want to ").red().on_white(),
                    Line::from(" delete this preset? ").red().on_white(),
                    "".into(),
                    self.presets[index].command.clone().into(),
                    "".into(),
                    "[Y]es [N]o".into(),
                ];
                Paragraph::new(Text::from(text))
                    .centered()
                    .render(inner, buf);
            }
            DisplayMode::Edit(index, _) => match self.edit_mode {
                EditMode::Command => {
                    let x = self
                        .command_input
                        .cursor()
                        .min(max_visible_command_width - 2);
                    self.cursor.set((
                        centered_inner_area.x + x as u16 + 4,
                        centered_inner_area.y + index as u16,
                    ));
                }
                EditMode::Shortcut => {
                    self.cursor.set((
                        centered_inner_area.x + 1,
                        centered_inner_area.y + index as u16,
                    ));
                }
            },
        }
    }
}

pub enum DisplayMode {
    Select,
    ConfirmDelete(usize),
    Edit(usize, bool),
}

enum EditMode {
    Command,
    Shortcut,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;
    use crate::theme::Theme;
    use crossterm::event::KeyCode::Char;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::io::Error;

    struct TestTerminal(Terminal<TestBackend>);

    impl Default for TestTerminal {
        fn default() -> Self {
            TestTerminal(Terminal::new(TestBackend::new(40, 10)).unwrap())
        }
    }

    struct InMemPresetsStore {
        presets: Vec<Preset>,
    }

    impl PresetsStore for InMemPresetsStore {
        fn load(&mut self) -> Result<Vec<Preset>, Error> {
            Ok(self.presets.clone())
        }

        fn save(&mut self, values: &Vec<Preset>) -> Result<(), Error> {
            self.presets = values.clone();
            Ok(())
        }
    }

    impl PresetsWidget {
        fn with_presets(presets: Vec<Preset>) -> Self {
            let theme_config = ThemeConfig::default();
            Self {
                store: Box::new(InMemPresetsStore { presets }),
                command_input: Input::from(""),
                shortcut_input: Input::from(""),
                display_mode: DisplayMode::Select,
                presets: vec![],
                selected: None,
                theme: Theme::from_config(&theme_config),
                edit_mode: EditMode::Command,
                cursor: Cell::new((0, 0)),
            }
        }
    }

    #[test]
    fn no_presets() {
        let presets = vec![];

        let widget = PresetsWidget::with_presets(presets);

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn presets() {
        let presets = vec![
            Preset {
                command: "jq -c ''".to_string(),
                shortcut: Some('j'),
            },
            Preset {
                command: "grep | sort | uniq -c | sort -nr".to_string(),
                shortcut: None,
            },
        ];

        let mut widget = PresetsWidget::with_presets(presets);

        widget.load();

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn new_empty() {
        let presets = vec![
            Preset {
                command: "jq -c ''".to_string(),
                shortcut: Some('j'),
            },
            Preset {
                command: "grep | sort | uniq -c | sort -nr".to_string(),
                shortcut: None,
            },
        ];

        let mut widget = PresetsWidget::with_presets(presets);

        widget.new_empty();
        input_text(&mut widget, "some -x command | grep other");

        widget.toggle_edit_mode();
        input_text(&mut widget, "x");

        widget.save_edit();

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn delete() {
        let presets = vec![
            Preset {
                command: "jq -c ''".to_string(),
                shortcut: Some('j'),
            },
            Preset {
                command: "grep | sort | uniq -c | sort -nr".to_string(),
                shortcut: None,
            },
        ];

        let mut widget = PresetsWidget::with_presets(presets);
        widget.load();

        widget.next();
        widget.confirm_delete();

        let mut terminal = TestTerminal::default().0;
        terminal
            .draw(|frame| widget.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        assert_snapshot!(terminal.backend());
    }

    fn input_text(app: &mut PresetsWidget, text: &str) {
        for c in text.chars() {
            app.handle_event(&Event::Key(KeyEvent {
                code: Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }));
        }
    }
}
