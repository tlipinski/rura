use crate::rura::State::{
    Backslash, Comment, Delimiter, DoubleQuoted, DoubleQuotedBackslash, Pipe, SingleQuoted,
    Unquoted, UnquotedBackslash,
};
use std::{fmt, mem};

#[derive(Debug)]
pub struct Rura {
    pub subcommands: Vec<String>,
    pub current: usize,
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
        })
    }

    pub fn command_full(&self) -> (String, usize) {
        (self.subcommands.join("|"), self.subcommands.len() - 1)
    }

    pub fn command_until_current(&self) -> (String, usize) {
        if !self.subcommands.is_empty() {
            (
                self.subcommands[0..self.current + 1].join("|"),
                self.current,
            )
        } else {
            (String::new(), 0)
        }
    }

    pub fn command_until_current_prev(&self) -> Option<(String, usize)> {
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
    use crate::rura::Rura;

    #[test]
    fn commands() {
        let rura = Rura::new("a|b|c", 0).unwrap();
        assert_eq!(rura.command_full(), ("a|b|c".into(), 2));
        assert_eq!(rura.command_until_current(), ("a".into(), 0));
        assert_eq!(rura.command_until_current_prev(), None);

        let rura = Rura::new("a|b|c", 1).unwrap();
        assert_eq!(rura.command_full(), ("a|b|c".into(), 2));
        assert_eq!(rura.command_until_current(), ("a".into(), 0));
        assert_eq!(rura.command_until_current_prev(), None);

        let rura = Rura::new("a|b|c", 2).unwrap();
        assert_eq!(rura.command_full(), ("a|b|c".into(), 2));
        assert_eq!(rura.command_until_current(), ("a|b".into(), 1));
        assert_eq!(rura.command_until_current_prev(), Some(("a".into(), 0)));

        let rura = Rura::new("a|b|c", 3).unwrap();
        assert_eq!(rura.command_full(), ("a|b|c".into(), 2));
        assert_eq!(rura.command_until_current(), ("a|b".into(), 1));
        assert_eq!(rura.command_until_current_prev(), Some(("a".into(), 0)));

        let rura = Rura::new("a|b|c", 4).unwrap();
        assert_eq!(rura.command_full(), ("a|b|c".into(), 2));
        assert_eq!(rura.command_until_current(), ("a|b|c".into(), 2));
        assert_eq!(rura.command_until_current_prev(), Some(("a|b".into(), 1)));
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
