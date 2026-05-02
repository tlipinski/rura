use crate::rura::ExecuteType::{Full, UntilCurrent, UntilCurrentPrev};
use crate::rura::State::{
    Backslash, Comment, Delimiter, DoubleQuoted, DoubleQuotedBackslash, Pipe, SingleQuoted,
    Unquoted, UnquotedBackslash,
};
use std::{fmt, mem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Unquoted(String),
    Quoted(String),
}

impl Part {
    pub fn content(&self) -> &str {
        match self {
            Part::Unquoted(s) => s,
            Part::Quoted(s) => s,
        }
    }
}

#[derive(Debug)]
pub struct Rura {
    pub subcommands: Vec<Vec<Part>>,
    pub current: usize,
    pub cursor: usize,
}

impl Rura {
    pub fn new(command: &str, cursor: usize) -> Result<Rura, ParseError> {
        let subcommands = split_command(command)?;

        let mut sum = 0;
        let mut current_subcommand = 0;

        for (index, parts) in subcommands.iter().enumerate() {
            let part_len: usize = parts.iter().map(|p| p.content().len()).sum();
            sum += part_len;
            if cursor <= sum {
                current_subcommand = index;
                break;
            }
            sum += 1 // for additional pipe character
        }

        Ok(Rura {
            subcommands,
            current: current_subcommand,
            cursor,
        })
    }

    pub fn command(&self, execute_type: ExecuteType) -> Option<(String, usize)> {
        let join_parts = |parts: &[Vec<Part>]| -> String {
            parts
                .iter()
                .map(|p| p.iter().map(|part| part.content()).collect::<String>())
                .collect::<Vec<String>>()
                .join("|")
        };

        match execute_type {
            Full => {
                if self.subcommands.is_empty() {
                    None
                } else {
                    Some((join_parts(&self.subcommands), self.subcommands.len() - 1))
                }
            }
            UntilCurrent => {
                if !self.subcommands.is_empty() {
                    Some((
                        join_parts(&self.subcommands[0..self.current + 1]),
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
                        Some((join_parts(&self.subcommands[0..self.current]), self.current - 1))
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
        for (i, parts) in self.subcommands.iter().enumerate() {
            let part_len: usize = parts.iter().map(|p| p.content().len()).sum();
            if i == self.subcommands.len() - 1 {
                ends.push(sum + part_len);
            } else {
                ends.push(sum + part_len - 1);
            }
            sum += part_len + 1;
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
        for (index, parts) in self.subcommands.iter().enumerate() {
            let part_len: usize = parts.iter().map(|p| p.content().len()).sum();
            let end_of_subcommand = if index == self.subcommands.len() - 1 {
                sum + part_len
            } else {
                sum + part_len - 1
            };

            if self.cursor < end_of_subcommand {
                // In the middle of current subcommand
                return Some(end_of_subcommand);
            } else if self.cursor == end_of_subcommand {
                // At the end of current subcommand
                if index + 1 < self.subcommands.len() {
                    let next_sum = sum + part_len + 1;
                    let next_parts = &self.subcommands[index + 1];
                    let next_part_len: usize = next_parts.iter().map(|p| p.content().len()).sum();
                    if index + 1 == self.subcommands.len() - 1 {
                        return Some(next_sum + next_part_len);
                    } else {
                        return Some(next_sum + next_part_len - 1);
                    }
                } else {
                    // Wrap to first
                    let first_parts = &self.subcommands[0];
                    let first_part_len: usize = first_parts.iter().map(|p| p.content().len()).sum();
                    if self.subcommands.len() == 1 {
                        return Some(first_part_len);
                    } else {
                        return Some(first_part_len - 1);
                    }
                }
            } else if self.cursor == sum + part_len {
                // At the pipe character
                if index + 1 < self.subcommands.len() {
                    let next_sum = sum + part_len + 1;
                    let next_parts = &self.subcommands[index + 1];
                    let next_part_len: usize = next_parts.iter().map(|p| p.content().len()).sum();
                    if index + 1 == self.subcommands.len() - 1 {
                        return Some(next_sum + next_part_len);
                    } else {
                        return Some(next_sum + next_part_len - 1);
                    }
                }
            }
            sum += part_len + 1;
        }
        None
    }

}

pub enum ExecuteType {
    Full,
    UntilCurrent,
    UntilCurrentPrev,
}

// Inspired by https://github.com/tmiasko/shell-words
fn split_command(s: &str) -> Result<Vec<Vec<Part>>, ParseError> {
    let mut commands = Vec::new();
    let mut current_command = Vec::new();
    let mut current_part = String::new();
    let mut chars = s.chars();
    let mut state = Delimiter;

    loop {
        let c = chars.next();

        state = match state {
            Delimiter => match c {
                None => break,
                Some('\'') => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    current_part.push('\'');
                    SingleQuoted
                }
                Some('\"') => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    current_part.push('\"');
                    DoubleQuoted
                }
                Some('\\') => {
                    current_part.push('\\');
                    Backslash
                }
                Some('\t') | Some(' ') | Some('\n') => {
                    current_part.push(c.unwrap());
                    Delimiter
                }
                Some('#') => {
                    current_part.push('#');
                    Comment
                }
                Some('|') => return Err(ParseError),
                Some(_) => {
                    current_part.push(c.unwrap());
                    Unquoted
                }
            },
            Backslash => match c {
                None => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    commands.push(mem::take(&mut current_command));
                    break;
                }
                Some('\n') => {
                    current_part.push('\n');
                    Delimiter
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    Unquoted
                }
            },
            Unquoted => match c {
                None => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    commands.push(mem::take(&mut current_command));
                    break;
                }
                Some('\'') => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    current_part.push('\'');
                    SingleQuoted
                }
                Some('\"') => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    current_part.push('\"');
                    DoubleQuoted
                }
                Some('\\') => {
                    current_part.push('\\');
                    UnquotedBackslash
                }
                Some('|') => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    commands.push(mem::take(&mut current_command));
                    Pipe
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    Unquoted
                }
            },
            UnquotedBackslash => match c {
                None => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    commands.push(mem::take(&mut current_command));
                    break;
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    Unquoted
                }
            },
            SingleQuoted => match c {
                None => return Err(ParseError),
                Some('\'') => {
                    current_part.push('\'');
                    current_command.push(Part::Quoted(mem::take(&mut current_part)));
                    Unquoted
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    SingleQuoted
                }
            },
            DoubleQuoted => match c {
                None => return Err(ParseError),
                Some('\"') => {
                    current_part.push('\"');
                    current_command.push(Part::Quoted(mem::take(&mut current_part)));
                    Unquoted
                }
                Some('\\') => {
                    current_part.push('\\');
                    DoubleQuotedBackslash
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    DoubleQuoted
                }
            },
            DoubleQuotedBackslash => match c {
                None => return Err(ParseError),
                Some('\n') => {
                    current_part.push('\n');
                    DoubleQuoted
                }
                Some('$') | Some('`') | Some('"') | Some('\\') => {
                    current_part.push(c.unwrap());
                    DoubleQuoted
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    DoubleQuoted
                }
            },
            Comment => match c {
                None => {
                    if !current_part.is_empty() {
                        current_command.push(Part::Unquoted(mem::take(&mut current_part)));
                    }
                    commands.push(mem::take(&mut current_command));
                    break;
                }
                Some('\n') => {
                    current_part.push('\n');
                    Delimiter
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    Comment
                }
            },
            Pipe => match c {
                None => return Err(ParseError),
                Some('\n') => {
                    current_part.push('\n');
                    Delimiter
                }
                Some(_) => {
                    current_part.push(c.unwrap());
                    Unquoted
                }
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
    use crate::rura::ExecuteType::{Full, UntilCurrent, UntilCurrentPrev};
    use crate::rura::{Part, Rura, split_command};

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
        assert_eq!(rura.cursor_next(), Some(14));

        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_next(), Some(2));

        let rura = Rura::new("aaa|bbbb|ccccc", 2).unwrap();
        assert_eq!(rura.cursor_next(), Some(7));
    }

    #[test]
    fn test_cursor_prev() {
        let rura = Rura::new("aaa|bbbb|ccccc", 14).unwrap();
        assert_eq!(rura.cursor_prev(), Some(7));
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
        assert_eq!(rura.cursor_prev(), Some(14));
    }

    use super::*;

    #[test]
    fn test_split_command() {
        let cmd = "ls -l";
        let split = split_command(cmd).unwrap();
        assert_eq!(split, vec![vec![Part::Unquoted(String::from("ls -l"))]]);
    }

    #[test]
    fn test_split_command_preserve_all_whitespaces() {
        let cmd = " \t\n ls    -la    |   grep    rw     ";
        let split = split_command(cmd).unwrap();
        assert_eq!(
            split,
            vec![
                vec![Part::Unquoted(String::from(" \t\n ls    -la    "))],
                vec![Part::Unquoted(String::from("   grep    rw     "))]
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
                vec![Part::Unquoted(String::from("some_cmd "))],
                vec![
                    Part::Unquoted(String::from(" jq ")),
                    Part::Quoted(String::from("'.. | .name'")),
                    Part::Unquoted(String::from(" -r")),
                ]
            ]
        );
    }

    #[test]
    fn test_split_pipe_in_wrong_place() {
        let cmd = " | some_cmd \n| ls ";
        let split = split_command(cmd);
        assert_eq!(split, Err(ParseError));
    }

    #[test]
    fn test_split_command_quoted() {
        let cmd = "ls \"file name\" 'other file'";
        let split = split_command(cmd).unwrap();
        assert_eq!(
            split,
            vec![vec![
                Part::Unquoted(String::from("ls ")),
                Part::Quoted(String::from("\"file name\"")),
                Part::Unquoted(String::from(" ")),
                Part::Quoted(String::from("'other file'")),
            ]]
        );
    }
}
