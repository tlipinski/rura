use crate::rura::ExecuteType::{Full, UntilCurrent, UntilCurrentPrev};
use crate::rura::State::{
    Backslash, Comment, Delimiter, DoubleQuoted, DoubleQuotedBackslash, Pipe, SingleQuoted,
    Unquoted, UnquotedBackslash,
};
use std::{fmt, mem};

#[derive(Debug)]
pub struct Rura {
    pub subcommands: Vec<String>,
    pub current: usize,
    pub cursor: usize,
}

impl Rura {
    pub fn new(command: &str, cursor: usize) -> Result<Rura, ParseError> {
        let subcommands = split_command(command)?;

        let mut sum = 0;
        let mut current_subcommand = 0;

        for (index, part) in subcommands.iter().enumerate() {
            sum += part.len();
            if cursor <= sum {
                current_subcommand = index;
                break;
            }
            sum += 1 // for additional pipe character
        }

        Ok(Rura {
            subcommands,
            current: current_subcommand,
            cursor
        })
    }

    pub fn command(&self, execute_type: ExecuteType) -> Option<(String, usize)> {
        match execute_type {
            Full => {
                if self.subcommands.is_empty() {
                    None
                } else {
                    Some((self.subcommands.join("|"), self.subcommands.len() - 1))
                }
            }
            UntilCurrent => {
                if !self.subcommands.is_empty() {
                    Some((
                        self.subcommands[0..self.current + 1].join("|"),
                        self.current,
                    ))
                } else {
                    None
                }
            }
            UntilCurrentPrev => {
                if !self.subcommands.is_empty() {
                    if self.current == 0 {
                        None
                    } else {
                        Some((
                            self.subcommands[0..self.current].join("|"),
                            self.current - 1,
                        ))
                    }
                } else {
                    None
                }
            }
        }
    }

    pub fn cursor_prev(&self) -> Option<usize> {
        let mut sum = 0;
        let mut ends = Vec::new();
        for part in &self.subcommands {
            ends.push(sum + part.len() - 1);
            sum += part.len() + 1;
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
            return Some(*ends.last().unwrap());
        }

        None
    }

    pub fn cursor_next(&self) -> Option<usize> {
        let mut sum = 0;
        for (index, part) in self.subcommands.iter().enumerate() {
            let end_of_subcommand = sum + part.len() - 1;
            if self.cursor < end_of_subcommand {
                // In the middle of current subcommand
                return Some(end_of_subcommand);
            } else if self.cursor == end_of_subcommand {
                // At the end of current subcommand
                if index + 1 < self.subcommands.len() {
                    let next_sum = sum + part.len() + 1;
                    let next_part_len = self.subcommands[index + 1].len();
                    return Some(next_sum + next_part_len - 1);
                } else {
                    // Wrap to first
                    return Some(self.subcommands[0].len() - 1);
                }
            } else if self.cursor == sum + part.len() {
                // At the pipe character
                if index + 1 < self.subcommands.len() {
                    let next_sum = sum + part.len() + 1;
                    let next_part_len = self.subcommands[index + 1].len();
                    return Some(next_sum + next_part_len - 1);
                }
            }
            sum += part.len() + 1;
        }
        None
    }

    fn len(&self) -> usize {
        self.subcommands.join("|").len()
    }
}

pub enum ExecuteType {
    Full,
    UntilCurrent,
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

    Ok(commands)
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

#[cfg(test)]
mod tests {
    use crate::rura::ExecuteType::UntilCurrent;
    use crate::rura::Rura;

    #[test]
    fn commands() {
        let rura = Rura::new("a|b|c", 0).unwrap();
        assert_eq!(rura.command(Full), Some(("a|b|c".into(), 2)));
        assert_eq!(rura.command(UntilCurrent), Some(("a".into(), 0)));
        assert_eq!(rura.command(UntilCurrentPrev), None);

        let rura = Rura::new("a|b|c", 1).unwrap();
        assert_eq!(rura.command(Full), Some(("a|b|c".into(), 2)));
        assert_eq!(rura.command(UntilCurrent), Some(("a".into(), 0)));
        assert_eq!(rura.command(UntilCurrentPrev), None);

        let rura = Rura::new("a|b|c", 2).unwrap();
        assert_eq!(rura.command(Full), Some(("a|b|c".into(), 2)));
        assert_eq!(rura.command(UntilCurrent), Some(("a|b".into(), 1)));
        assert_eq!(rura.command(UntilCurrentPrev), Some(("a".into(), 0)));

        let rura = Rura::new("a|b|c", 3).unwrap();
        assert_eq!(rura.command(Full), Some(("a|b|c".into(), 2)));
        assert_eq!(rura.command(UntilCurrent), Some(("a|b".into(), 1)));
        assert_eq!(rura.command(UntilCurrentPrev), Some(("a".into(), 0)));

        let rura = Rura::new("a|b|c", 4).unwrap();
        assert_eq!(rura.command(Full), Some(("a|b|c".into(), 2)));
        assert_eq!(rura.command(UntilCurrent), Some(("a|b|c".into(), 2)));
        assert_eq!(rura.command(UntilCurrentPrev), Some(("a|b".into(), 1)));
    }

    #[test]
    fn test_cursor_next() {
        let rura = Rura::new("aaa|bbbb|ccccc", 0).unwrap();
        assert_eq!(rura.cursor_next(), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 1).unwrap();
        assert_eq!(rura.cursor_next(), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_next(), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_next(), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 4).unwrap();
        assert_eq!(rura.cursor_next(), Some(7));

        let rura = Rura::new("aaa|bbbb|ccccc", 7).unwrap();
        assert_eq!(rura.cursor_next(), Some(13));

        let rura = Rura::new("aaa|bbbb|ccccc", 13).unwrap();
        assert_eq!(rura.cursor_next(), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_next(), Some(7));
    }

    #[test]
    fn test_cursor_prev() {
        let rura = Rura::new("aaa|bbbb|ccccc", 13).unwrap();
        assert_eq!(rura.cursor_prev(), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 12).unwrap();
        assert_eq!(rura.cursor_prev(), Some(7));

        let rura = Rura::new("aaa|bbbb|ccccc", 8).unwrap();
        assert_eq!(rura.cursor_prev(), Some(7));
        let rura = Rura::new("aaa|bbbb|ccccc", 7).unwrap();
        assert_eq!(rura.cursor_prev(), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 6).unwrap();
        assert_eq!(rura.cursor_prev(), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 3).unwrap();
        assert_eq!(rura.cursor_prev(), Some(2));
        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_prev(), Some(13));
    }

    use super::*;
    use crate::rura::split_command;

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
        assert_eq!(
            split,
            vec![
                String::from("some_cmd "),
                String::from(" jq '.. | .name' -r")
            ]
        );
    }

    #[test]
    fn test_split_pipe_in_wrong_place() {
        let cmd = " | some_cmd \n| ls ";
        let split = split_command(cmd);
        assert_eq!(split, Err(ParseError));
    }
}
