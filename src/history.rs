use crate::config::history_path;
use log::debug;
use std::collections::VecDeque;
use std::io::Write;

pub struct History {
    history: VecDeque<String>,
    position: Option<usize>,
    save_to_file: bool,
}

impl History {
    pub fn load() -> Self {
        let mut history = VecDeque::new();
        if let Some(path) = history_path() {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    if !line.is_empty() {
                        debug!("line: {}", line);
                        history.push_front(line.to_string());
                    }
                }
            }
        }
        History {
            history,
            position: None,
            save_to_file: true,
        }
    }

    pub fn previous(&mut self) -> String {
        match self.position {
            None => {
                if !self.history.is_empty() {
                    let new_pos = 0;
                    self.position = Some(new_pos);
                    self.history.get(new_pos).cloned().unwrap_or_default()
                } else {
                    "".into()
                }
            }
            Some(pos) => {
                let new_pos = pos.saturating_add(1);
                if let Some(previous) = self.history.get(new_pos).cloned() {
                    self.position = Some(new_pos);
                    previous
                } else {
                    self.history.back().cloned().unwrap_or_default()
                }
            }
        }
    }

    pub fn next(&mut self) -> String {
        match self.position {
            None => String::new(),
            Some(pos) => {
                let new_pos = pos.saturating_sub(1);
                let next = self.history.get(new_pos).cloned();
                self.position = Some(new_pos);
                next.unwrap_or_default()
            }
        }
    }

    pub fn push(&mut self, value: &str) {
        match self.history.front() {
            Some(most_recent) if most_recent != &value => {
                self.history.push_front(value.into());
                // set to previous history item, not the one just executed
                self.position = Some(0);
                if self.save_to_file {
                    save_to_history(value.into());
                }
            }
            Some(_duplicate) => {}
            None => {
                self.history.push_front(value.into());
                self.position = Some(0);
                if self.save_to_file {
                    save_to_history(value.into());
                }
            }
        };
    }
}

fn save_to_history(value: String) {
    if let Some(path) = history_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{}", value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_history() {
        let mut history = History {
            history: VecDeque::new(),
            position: None,
            save_to_file: false,
        };

        assert_eq!(history.previous(), "");
        assert_eq!(history.next(), "");
    }

    #[test]
    fn test_history_push() {
        let mut history = History {
            history: VecDeque::from(vec!["test1".into(), "test2".into(), "test3".into()]),
            position: None,
            save_to_file: false,
        };

        assert_eq!(history.history.len(), 3);

        assert_eq!(history.next(), "");
        assert_eq!(history.previous(), "test1");
        assert_eq!(history.previous(), "test2");
        assert_eq!(history.previous(), "test3");
        assert_eq!(history.previous(), "test3"); // stays on the oldest value
        assert_eq!(history.next(), "test2");
        assert_eq!(history.next(), "test1");
        assert_eq!(history.next(), "test1");
        assert_eq!(history.previous(), "test2");
    }
}
