use crate::rura::ExecuteType::{Full, FullLive, UntilCurrent, UntilCurrentLive, UntilCurrentPrev};
use crate::rura::State::{
    Backslash, Comment, Delimiter, DoubleQuoted, DoubleQuotedBackslash, Pipe, SingleQuoted,
    Unquoted, UnquotedBackslash,
};
use itertools::Itertools;
use std::{fmt, mem};

#[derive(Debug)]
pub struct Rura {
    subcommands: Vec<String>,
    current: usize,
    cursor: usize,
    cursor_relative: usize,
}

impl Rura {
    pub fn new(command: &str, cursor: usize) -> Result<Rura, ParseError> {
        let subcommands = split_command(command)?;

        let mut sum = 0;
        let mut current_subcommand = 0;
        let mut cursor_relative = cursor;

        for (index, subcommand) in subcommands.iter().enumerate() {
            sum += subcommand.len();
            if cursor <= sum {
                current_subcommand = index;
                cursor_relative = cursor - (sum - subcommand.len());
                break;
            }
            sum += 1 // for additional pipe character
        }

        Ok(Rura {
            subcommands,
            current: current_subcommand,
            cursor,
            cursor_relative,
        })
    }

    pub fn current(&self) -> usize {
        *&self.current
    }

    pub fn subcommands(&self) -> &Vec<String> {
        &self.subcommands
    }

    pub fn command(&self, execute_type: &ExecuteType) -> RuraCommand {
        match execute_type {
            Full | FullLive => RuraCommand {
                sub: self.subcommands.clone(),
            },
            UntilCurrent | UntilCurrentLive => {
                if !self.subcommands.is_empty() {
                    RuraCommand {
                        sub: self
                            .subcommands
                            .iter()
                            .take(self.current + 1)
                            .cloned()
                            .collect(),
                    }
                } else {
                    RuraCommand::empty()
                }
            }
            UntilCurrentPrev => {
                if self.subcommands.is_empty() || self.current == 0 {
                    RuraCommand::empty()
                } else {
                    RuraCommand {
                        sub: self
                            .subcommands
                            .iter()
                            .take(self.current)
                            .cloned()
                            .collect(),
                    }
                }
            }
        }
    }

    pub fn cursor_prev(&self, cycle: bool) -> Option<usize> {
        let mut sum = 0;
        let mut ends = Vec::new();
        for (i, subcommand) in self.subcommands.iter().enumerate() {
            if i == self.subcommands.len() - 1 {
                ends.push(sum + subcommand.len());
            } else {
                ends.push(sum + subcommand.len() - 1);
            }
            sum += subcommand.len() + 1;
        }

        if ends.is_empty() {
            return None;
        }

        for i in 0..ends.len() {
            if self.cursor > ends[i] && (i + 1 == ends.len() || self.cursor <= ends[i + 1]) {
                // Cursor is after ends[i] and before or at ends[i+1]
                return Some(ends[i]);
            }
        }

        // If cursor <= ends[0], wrap to last
        if self.cursor <= ends[0] {
            if cycle {
                return Some(*ends.last().unwrap());
            } else {
                return Some(ends[0]);
            }
        }

        None
    }

    pub fn cursor_next(&self, cycle: bool) -> Option<usize> {
        let mut sum = 0;
        for (index, subcommand) in self.subcommands.iter().enumerate() {
            let end_of_subcommand = if index == self.subcommands.len() - 1 {
                sum + subcommand.len()
            } else {
                sum + subcommand.len() - 1
            };

            if self.cursor < end_of_subcommand {
                // In the middle of current subcommand
                return Some(end_of_subcommand);
            } else if self.cursor == end_of_subcommand {
                // At the end of current subcommand
                if index + 1 < self.subcommands.len() {
                    let next_sum = sum + subcommand.len() + 1;
                    let next_subcommand = &self.subcommands[index + 1];
                    if index + 1 == self.subcommands.len() - 1 {
                        return Some(next_sum + next_subcommand.len());
                    } else {
                        return Some(next_sum + next_subcommand.len() - 1);
                    }
                } else {
                    if cycle {
                        // Wrap to first
                        let first_subcomman = &self.subcommands[0];
                        if self.subcommands.len() == 1 {
                            return Some(first_subcomman.len());
                        } else {
                            return Some(first_subcomman.len() - 1);
                        }
                    } else {
                        return Some(end_of_subcommand);
                    }
                }
            } else if self.cursor == sum + subcommand.len() {
                // At the pipe character
                if index + 1 < self.subcommands.len() {
                    let next_sum = sum + subcommand.len() + 1;
                    let next_subcommand = &self.subcommands[index + 1];
                    if index + 1 == self.subcommands.len() - 1 {
                        return Some(next_sum + next_subcommand.len());
                    } else {
                        return Some(next_sum + next_subcommand.len() - 1);
                    }
                }
            }
            sum += subcommand.len() + 1;
        }
        None
    }

    pub fn current_subcommand(&self) -> Option<String> {
        self.subcommands.get(self.current).map(|s| s.to_owned())
    }

    pub fn delete_current(&mut self) -> Option<(String, usize)> {
        if self.subcommands.is_empty() {
            None
        } else {
            let removed = self.subcommands.remove(self.current);
            self.current = self.current.saturating_sub(1);

            if let Some(new_cursor) = self.cursor_ends().get(self.current) {
                Some((removed, *new_cursor))
            } else {
                Some((removed, 0))
            }
        }
    }

    #[allow(dead_code)]
    pub fn insert_before(&mut self, insert: &str) {
        self.subcommands.insert(self.current, insert.to_owned());
    }

    pub fn insert_after(&mut self, insert: &str) -> usize {
        let insert_index = (self.current + 1).min(self.subcommands.len());
        self.subcommands.insert(insert_index, insert.to_owned());

        if let Some(cursor) = self.cursor_ends().get(self.current + 1) {
            *cursor
        } else {
            // inserted into empty input
            self.subcommands.first().map(|s| s.len()).unwrap_or(0)
        }
    }

    pub fn format(&mut self) -> usize {
        if self.subcommands.is_empty() {
            return 0;
        }
        let current_sub = &self.subcommands[self.current];
        let trimmed = current_sub.trim();
        let leading_whitespace = current_sub.len() - current_sub.trim_start().len();

        let mut rel_in_trimmed = if self.cursor_relative < leading_whitespace {
            0
        } else {
            self.cursor_relative - leading_whitespace
        };
        rel_in_trimmed = rel_in_trimmed.min(trimmed.len());

        let size = self.subcommands.len();
        for (i, subcommand) in self.subcommands.iter_mut().enumerate() {
            *subcommand = match i {
                0 => format!("{} ", subcommand.trim()),
                a if a == size - 1 => format!(" {}", subcommand.trim()),
                _ => format!(" {} ", subcommand.trim()),
            }
        }

        let mut new_cursor = 0;
        for i in 0..self.current {
            new_cursor += self.subcommands[i].len() + 1;
        }

        if self.current > 0 {
            new_cursor += 1;
        }
        new_cursor += rel_in_trimmed;

        self.cursor = new_cursor;
        self.cursor_relative = if self.current > 0 {
            rel_in_trimmed + 1
        } else {
            rel_in_trimmed
        };

        new_cursor
    }

    pub fn to_string(&self) -> String {
        self.subcommands.join("|")
    }

    fn cursor_ends(&self) -> Vec<usize> {
        let mut ends = Vec::new();
        let mut sum = 0;
        for (i, subcommand) in self.subcommands.iter().enumerate() {
            if i == 0 {
                sum += subcommand.len() - 1;
            } else {
                sum += subcommand.len() + 1;
            }
            ends.push(sum);
        }
        ends
    }
}

pub enum ExecuteType {
    Full,
    FullLive,
    UntilCurrent,
    UntilCurrentLive,
    UntilCurrentPrev,
}

// Inspired by https://github.com/tmiasko/shell-words
fn split_command(s: &str) -> Result<Vec<String>, ParseError> {
    let mut commands = Vec::new();
    let mut command = String::new();
    let mut chars = s.chars();
    let mut state = Delimiter;

    loop {
        let c = chars.next();
        if let Some(c) = c {
            command.push(c);
        }
        state = match state {
            Delimiter => match c {
                None => break,
                Some('\'') => SingleQuoted,
                Some('\"') => DoubleQuoted,
                Some('\\') => Backslash,
                Some('\t') | Some(' ') | Some('\n') => Delimiter,
                Some('#') => Comment,
                Some('|') => return Err(ParseError),
                Some(_) => Unquoted,
            },
            Backslash => match c {
                None => {
                    commands.push(mem::replace(&mut command, String::new()));
                    break;
                }
                Some('\n') => Delimiter,
                Some(_) => Unquoted,
            },
            Unquoted => match c {
                None => {
                    commands.push(mem::replace(&mut command, String::new()));
                    break;
                }
                Some('\'') => SingleQuoted,
                Some('\"') => DoubleQuoted,
                Some('\\') => UnquotedBackslash,
                Some('|') => {
                    command.remove(command.len() - 1);
                    commands.push(mem::replace(&mut command, String::new()));
                    Pipe
                }
                Some(_) => Unquoted,
            },
            UnquotedBackslash => match c {
                None => {
                    commands.push(mem::replace(&mut command, String::new()));
                    break;
                }
                Some(_) => Unquoted,
            },
            SingleQuoted => match c {
                None => return Err(ParseError),
                Some('\'') => Unquoted,
                Some(_) => SingleQuoted,
            },
            DoubleQuoted => match c {
                None => return Err(ParseError),
                Some('\"') => Unquoted,
                Some('\\') => DoubleQuotedBackslash,
                Some(_) => DoubleQuoted,
            },
            DoubleQuotedBackslash => match c {
                None => return Err(ParseError),
                Some('\n') => DoubleQuoted,
                Some('$') | Some('`') | Some('"') | Some('\\') => DoubleQuoted,
                Some(_) => DoubleQuoted,
            },
            Comment => match c {
                None => break,
                Some('\n') => Delimiter,
                Some(_) => Comment,
            },
            Pipe => match c {
                None => return Err(ParseError),
                Some('\n') => Delimiter,
                Some(_) => Unquoted,
            },
        }
    }

    if commands.iter().any(|c| c.trim().is_empty()) {
        Err(ParseError)
    } else {
        Ok(commands)
    }
}

enum State {
    /// Within a delimiter.
    Delimiter,
    /// After backslash, but before starting word.
    Backslash,
    /// Within an unquoted word.
    Unquoted,
    /// After backslash in an unquoted word.
    UnquotedBackslash,
    /// Within a single quoted word.
    SingleQuoted,
    /// Within a double quoted word.
    DoubleQuoted,
    /// After backslash inside a double quoted word.
    DoubleQuotedBackslash,
    /// Inside a comment.
    Comment,
    Pipe,
}

/// An error returned when shell parsing fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("missing closing quote")
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RuraCommand {
    sub: Vec<String>,
}

impl RuraCommand {
    pub fn empty() -> Self {
        Self { sub: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.sub.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sub.len()
    }

    pub fn to_string(&self) -> String {
        self.sub.join("|")
    }

    pub fn trimmed(&self) -> Vec<String> {
        self.sub.iter().map(|s| s.trim().into()).collect_vec()
    }
}

impl From<Vec<String>> for RuraCommand {
    fn from(to_run: Vec<String>) -> Self {
        Self { sub: to_run }
    }
}

impl From<&str> for RuraCommand {
    fn from(to_run: &str) -> Self {
        Self {
            sub: vec![to_run.into()],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rura::ExecuteType::{Full, UntilCurrent, UntilCurrentPrev};
    use crate::rura::{Rura, split_command};

    #[test]
    fn commands() {
        let rura = Rura::new("a|b|c", 0).unwrap();
        assert_eq!(
            rura.command(&Full),
            RuraCommand {
                sub: vec!["a".into(), "b".into(), "c".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrent),
            RuraCommand {
                sub: vec!["a".into()],
            },
        );
        assert_eq!(rura.command(&UntilCurrentPrev), RuraCommand::empty());

        let rura = Rura::new("a|b|c", 1).unwrap();
        assert_eq!(
            rura.command(&Full),
            RuraCommand {
                sub: vec!["a".into(), "b".into(), "c".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrent),
            RuraCommand {
                sub: vec!["a".into()],
            },
        );
        assert_eq!(rura.command(&UntilCurrentPrev), RuraCommand::empty());

        let rura = Rura::new("a|b|c", 2).unwrap();
        assert_eq!(
            rura.command(&Full),
            RuraCommand {
                sub: vec!["a".into(), "b".into(), "c".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrent),
            RuraCommand {
                sub: vec!["a".into(), "b".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrentPrev),
            RuraCommand {
                sub: vec!["a".into()],
            },
        );

        let rura = Rura::new("a|b|c", 3).unwrap();
        assert_eq!(
            rura.command(&Full),
            RuraCommand {
                sub: vec!["a".into(), "b".into(), "c".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrent),
            RuraCommand {
                sub: vec!["a".into(), "b".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrentPrev),
            RuraCommand {
                sub: vec!["a".into()],
            },
        );

        let rura = Rura::new("a|b|c", 4).unwrap();
        assert_eq!(
            rura.command(&Full),
            RuraCommand {
                sub: vec!["a".into(), "b".into(), "c".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrent),
            RuraCommand {
                sub: vec!["a".into(), "b".into(), "c".into()],
            },
        );
        assert_eq!(
            rura.command(&UntilCurrentPrev),
            RuraCommand {
                sub: vec!["a".into(), "b".into()],
            },
        );
    }

    #[test]
    fn test_cursor_next() {
        let rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        assert_eq!(rura.cursor_next(true), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 1).unwrap();
        assert_eq!(rura.cursor_next(true), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_next(true), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_next(true), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 4).unwrap();
        assert_eq!(rura.cursor_next(true), Some(7));

        let rura = Rura::new("aaa|bbbb|ccccc", 7).unwrap();
        assert_eq!(rura.cursor_next(true), Some(14));

        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_next(true), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_next(true), Some(7));
    }

    #[test]
    fn test_cursor_next_no_cycle() {
        let rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        assert_eq!(rura.cursor_next(false), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 1).unwrap();
        assert_eq!(rura.cursor_next(false), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_next(false), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_next(false), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 4).unwrap();
        assert_eq!(rura.cursor_next(false), Some(7));

        let rura = Rura::new("aaa|bbbb|ccccc", 7).unwrap();
        assert_eq!(rura.cursor_next(false), Some(14));

        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_next(false), Some(14));
    }

    #[test]
    fn test_cursor_prev_cycle() {
        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 13).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 12).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(7));

        let rura = Rura::new("aaa|bbbb|ccccc", 8).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 7).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 6).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_prev(true), Some(14));
    }

    #[test]
    fn test_cursor_prev_no_cycle() {
        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 13).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 12).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(7));

        let rura = Rura::new("aaa|bbbb|ccccc", 8).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 7).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 6).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_prev(false), Some(2));
    }

    #[test]
    fn test_delete_current() {
        let mut rura = Rura::new("", 0).unwrap();
        assert_eq!(rura.delete_current(), None);

        let mut rura = Rura::new("   ", 0).unwrap();
        assert_eq!(rura.delete_current(), None);

        let mut rura = Rura::new("aaa", 0).unwrap();
        assert_eq!(rura.delete_current(), Some(("aaa".into(), 0)));

        let mut rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        assert_eq!(rura.delete_current(), Some(("aaa".into(), 3))); // cursor at end of bbbb
        assert_eq!(rura.subcommands, vec!["bbbb", "ccccc"]);

        let mut rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.delete_current(), Some(("aaa".into(), 3))); // cursor at end of bbbb
        assert_eq!(rura.subcommands, vec!["bbbb", "ccccc"]);

        let mut rura = Rura::new("aaa|bbbb|ccccc", 4).unwrap();
        assert_eq!(rura.delete_current(), Some(("bbbb".into(), 2))); // cursor at end of ccccc
        assert_eq!(rura.subcommands, vec!["aaa", "ccccc"]);

        let mut rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.delete_current(), Some(("ccccc".into(), 7)));
        assert_eq!(rura.subcommands, vec!["aaa", "bbbb"]);
    }

    #[test]
    fn test_insert_before() {
        let mut rura = Rura::new("", 0).unwrap();
        rura.insert_before("aaa");
        assert_eq!(rura.subcommands, vec!["aaa"]);

        let mut rura = Rura::new("aaa", 0).unwrap();
        rura.insert_before("bbbb");
        assert_eq!(rura.subcommands, vec!["bbbb", "aaa"]);

        let mut rura = Rura::new("aaa|bbbb", 5).unwrap();
        rura.insert_before("ccccc");
        assert_eq!(rura.subcommands, vec!["aaa", "ccccc", "bbbb"]);
    }

    #[test]
    fn test_insert_after() {
        let mut rura = Rura::new("", 0).unwrap();
        let cursor = rura.insert_after("aaa");
        assert_eq!(rura.subcommands, vec!["aaa"]);
        assert_eq!(cursor, 3);

        let mut rura = Rura::new("aaa", 0).unwrap();
        let cursor = rura.insert_after("bbbb");
        assert_eq!(rura.subcommands, vec!["aaa", "bbbb"]);
        assert_eq!(cursor, 7);

        let mut rura = Rura::new("aaa|bbbb", 0).unwrap();
        let cursor = rura.insert_after("ccccc");
        assert_eq!(cursor, 8);
        assert_eq!(rura.subcommands, vec!["aaa", "ccccc", "bbbb"]);

        let mut rura = Rura::new("aaa|bbbb", 5).unwrap();
        let cursor = rura.insert_after("ccccc");
        assert_eq!(cursor, 13);
        assert_eq!(rura.subcommands, vec!["aaa", "bbbb", "ccccc"]);
    }

    #[test]
    fn test_ends() {
        let rura = Rura::new("", 0).unwrap();
        assert_eq!(rura.cursor_ends(), vec![]);

        let rura = Rura::new("    ", 0).unwrap();
        assert_eq!(rura.cursor_ends(), vec![]);

        let rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        assert_eq!(rura.cursor_ends(), vec![2, 7, 13]);
    }

    #[test]
    fn test_cursor_relative_position() {
        let rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        assert_eq!(rura.cursor_relative, 0);

        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_relative, 3);

        let rura = Rura::new("aaa|bbbb|ccccc", 4).unwrap();
        assert_eq!(rura.cursor_relative, 0);

        let rura = Rura::new("aaa|bbbb|ccccc", 9).unwrap();
        assert_eq!(rura.cursor_relative, 0);

        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_relative, 5);
    }

    #[test]
    fn test_format_and_cursor_position() {
        let mut rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        let cursor = rura.format();
        assert_eq!(rura.to_string(), "aaa | bbbb | ccccc");
        assert_eq!(cursor, 0);

        let mut rura = Rura::new("aaa|bbbb|ccccc", 5).unwrap();
        let cursor = rura.format();
        assert_eq!(rura.to_string(), "aaa | bbbb | ccccc");
        assert_eq!(cursor, 7);

        let mut rura = Rura::new("aaa     |bbbb|ccccc", 5).unwrap();
        let cursor = rura.format();
        assert_eq!(rura.to_string(), "aaa | bbbb | ccccc");
        assert_eq!(cursor, 3);

        let mut rura = Rura::new("aaa      |         bbbb       |      ccccc", 40).unwrap();
        let cursor = rura.format();
        assert_eq!(rura.to_string(), "aaa | bbbb | ccccc");
        assert_eq!(cursor, 16);
    }

    use super::*;

    #[test]
    fn test_split_command() {
        let cmd = "ls -l";
        let split = split_command(cmd).unwrap();
        assert_eq!(split, vec![String::from("ls -l")]);
    }

    #[test]
    fn test_split_command_preserve_all_whitespaces() {
        let cmd = " \t\n ls    -la    |   grep    rw     ";
        let split = split_command(cmd).unwrap();
        assert_eq!(
            split,
            vec![
                String::from(" \t\n ls    -la    "),
                String::from("   grep    rw     ")
            ]
        );
    }

    #[test]
    fn test_split_command_pipe_in_quotes() {
        let cmd = "some_cmd | jq '.. | .name' -r";
        let split = split_command(cmd).unwrap();
        assert_eq!(split, vec!["some_cmd ", " jq '.. | .name' -r"]);
    }

    #[test]
    fn test_empty_subcommands() {
        let cmd = " | cmd";
        let split = split_command(cmd);
        assert_eq!(split, Err(ParseError));

        let cmd = "cmd | | a";
        let split = split_command(cmd);
        assert_eq!(split, Err(ParseError));

        let cmd = "cmd | ";
        let split = split_command(cmd);
        assert_eq!(split, Err(ParseError));
    }

    #[test]
    fn test_split_command_quoted() {
        let cmd = "ls \"file name\" 'other file'";
        let split = split_command(cmd).unwrap();
        assert_eq!(split, vec!["ls \"file name\" 'other file'"]);
    }
}
