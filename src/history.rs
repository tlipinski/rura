use crate::config::history_path;
use std::collections::VecDeque;
use std::io::{Error, Write};
use log::debug;

pub struct History {
    store: Box<dyn HistoryStore>,
    position: Option<usize>,
    current: Option<String>,
}

trait HistoryStore {
    fn load(&mut self) -> Result<VecDeque<String>, Error>;
    fn save(&mut self, item: &str) -> Result<(), Error>;
}

#[derive(Default)]
struct FileHistoryStore {
    items: Option<VecDeque<String>>,
}

impl HistoryStore for FileHistoryStore {
    fn load(&mut self) -> Result<VecDeque<String>, Error> {
        debug!("items {:?}", self.items);
        if let Some(loaded) = self.items.clone() {
            Ok(loaded)
        } else {
            let mut history = VecDeque::new();
            if let Some(path) = history_path() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    for line in content.lines() {
                        if !line.is_empty() {
                            history.push_front(line.to_string());
                        }
                    }
                }
            }
            self.items = Some(history.clone());
            Ok(history)
        }
    }

    fn save(&mut self, value: &str) -> Result<(), Error> {
        debug!("items before push {:?}", self.items);
        self.items.as_mut().map(|items_mut| {
            items_mut.push_front(value.into());
        });
        debug!("items after push {:?}", self.items);

        if let Some(path) = history_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "{}", value);
                Ok(())
            } else {
                Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to open history file for writing",
                ))
            }
        } else {
            Err(Error::new(
                std::io::ErrorKind::Other,
                "History path not found",
            ))
        }
    }
}

impl History {
    pub fn using_file() -> Self {
        let store = Box::new(FileHistoryStore::default());

        History {
            position: None,
            current: None,
            store,
        }
    }

    fn history(&mut self) -> VecDeque<String> {
        self.store.load().unwrap_or_default()
    }
}

impl History {
    pub fn previous(&mut self, current: &str) -> String {
        if self.history().is_empty() {
            return current.to_string();
        }

        match self.position {
            None => {
                self.current = Some(current.to_string());
                let mut new_pos = 0;
                if let Some(front) = self.history().front() {
                    if front == current && self.history().len() > 1 {
                        new_pos = 1;
                    }
                }
                self.position = Some(new_pos);
                self.history()[new_pos].clone()
            }
            Some(pos) => {
                let new_pos = (pos + 1).min(self.history().len() - 1);
                self.position = Some(new_pos);
                self.history()[new_pos].clone()
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
                    self.history()[new_pos].clone()
                }
            }
        }
    }

    pub fn push(&mut self, value: &str) {
        self.position = None;
        self.current = Some("".to_string());
        match self.history().front() {
            Some(most_recent) if most_recent.trim() != value.trim() => {
                let _ = self.store.save(value);
            }
            Some(_duplicate) => {}
            None => {
                let _ = self.store.save(value);
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct InMemHistoryStore {
        items: VecDeque<String>,
    }

    impl History {
        pub fn in_mem() -> History {
            History {
                store: Box::new(InMemHistoryStore::default()),
                position: None,
                current: None,
            }
        }
    }

    impl HistoryStore for InMemHistoryStore {
        fn load(&mut self) -> Result<VecDeque<String>, Error> {
            Ok(self.items.clone())
        }

        fn save(&mut self, value: &str) -> Result<(), Error> {
            self.items.push_front(value.into());
            Ok(())
        }
    }

    impl InMemHistoryStore {
        fn stub(items: VecDeque<String>) -> Self {
            Self { items }
        }
    }

    #[test]
    fn test_empty_history() {
        let mut history = History {
            store: Box::new(InMemHistoryStore::stub(VecDeque::new())),
            position: None,
            current: None,
        };

        assert_eq!(history.previous("current"), "current");
        assert_eq!(history.next("current"), "current");
    }

    #[test]
    fn test_history_init() {
        let mut history = History {
            store: Box::new(InMemHistoryStore::stub(VecDeque::from(vec![
                "test1".into(),
                "test2".into(),
                "test3".into(),
            ]))),
            position: None,
            current: None,
        };

        assert_eq!(history.history().len(), 3);

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
