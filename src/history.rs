use crate::config::history_path;
use log::debug;
use std::collections::VecDeque;
use std::io::Write;

pub struct History {
    history: VecDeque<String>,
    position: Option<usize>,
    current: Option<String>,
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
            current: None,
            save_to_file: true,
        }
    }

    pub fn previous(&mut self, current: &str) -> String {
        if self.history.is_empty() {
            return current.to_string();
        }

        match self.position {
            None => {
                self.current = Some(current.to_string());
                let mut new_pos = 0;
                if let Some(front) = self.history.front() {
                    if front == current && self.history.len() > 1 {
                        new_pos = 1;
                    }
                }
                self.position = Some(new_pos);
                self.history[new_pos].clone()
            }
            Some(pos) => {
                let new_pos = (pos + 1).min(self.history.len() - 1);
                self.position = Some(new_pos);
                self.history[new_pos].clone()
            }
        }
    }

    pub fn next(&mut self, current: &str) -> String {
        match self.position {
            None => current.to_string(),
            Some(pos) => {
                if pos == 0 {
                    self.position = None;
                    self.current.take().unwrap_or_default()
                } else {
                    let new_pos = pos - 1;
                    self.position = Some(new_pos);
                    self.history[new_pos].clone()
                }
            }
        }
    }

    pub fn push(&mut self, value: &str) {
        self.position = None;
        self.current = Some("".to_string());
        match self.history.front() {
            Some(most_recent) if most_recent != &value => {
                self.history.push_front(value.into());
                if self.save_to_file {
                    save_to_history(value.into());
                }
            }
            Some(_duplicate) => {}
            None => {
                self.history.push_front(value.into());
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
            current: None,
            save_to_file: false,
        };

        assert_eq!(history.previous("current"), "current");
        assert_eq!(history.next("current"), "current");
    }

    #[test]
    fn test_history_init() {
        let mut history = History {
            history: VecDeque::from(vec!["test1".into(), "test2".into(), "test3".into()]),
            position: None,
            current: None,
            save_to_file: false,
        };

        assert_eq!(history.history.len(), 3);

        let item = history.next("current");
        assert_eq!(item, "current");

        let item = history.previous(&item);
        assert_eq!(item, "test1");

        let item = history.previous(&item);
        assert_eq!(item, "test2");

        let item = history.previous(&item);
        assert_eq!(item, "test3");

        let item = history.previous(&item);
        assert_eq!(item, "test3"); // stays on the oldest value

        let item = history.next(&item);
        assert_eq!(item, "test2");

        let item = history.next(&format!("{item} edited")); // edit for non-current item will be ignored
        assert_eq!(item, "test1");

        let item = history.next(&item);
        assert_eq!(item, "current");

        let item = history.previous("current edited");
        assert_eq!(item, "test1");

        let item = history.next(&item);
        assert_eq!(item, "current edited");

        let _item = history.push(&item);

        let item = history.previous(&item);
        assert_eq!(item, "test1");

        let item = history.previous(&item);
        assert_eq!(item, "test2");

        let item = history.next(&item);
        assert_eq!(item, "test1");

        let item = history.next(&item);
        assert_eq!(item, "current edited");

        let item = history.next(&item);
        assert_eq!(item, "current edited");
    }
}
