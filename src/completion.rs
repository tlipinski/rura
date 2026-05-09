use itertools::Itertools;
use log::{debug, error};
use std::process::Command;

pub trait Completer {
    fn completions(&self, input: &str, cursor_pos: usize) -> Option<CompletionResult>;
}

#[derive(Debug, PartialEq)]
pub struct CompletionResult {
    pub completions: Vec<String>,
    pub word_start: usize,
}

pub struct ShCompleter;

impl Completer for ShCompleter {
    fn completions(&self, input: &str, cursor_pos: usize) -> Option<CompletionResult> {
        let (prefix, completion_type, word_start) =
            ShCompleter::find_completion_prefix(input, cursor_pos);

        debug!(
            "Completion prefix for '{}' @ {}: '{}', type: {:?}, word start: {}",
            input, cursor_pos, prefix, completion_type, word_start
        );

        let comp_type_str = match completion_type {
            CompletionType::Command => "-c",
            CompletionType::File => "-f",
        };

        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("compgen {} -- \"{}\"", comp_type_str, prefix))
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let completions: Vec<String> = stdout
                    .lines()
                    .map(|s| s.to_string())
                    .unique()
                    .sorted_by(|a, b| a.len().cmp(&b.len()))
                    .sorted()
                    .collect();

                debug!(
                    "completion results [{}]: {:?}",
                    completions.len(),
                    completions
                );

                if completions.is_empty() {
                    None
                } else {
                    Some(CompletionResult {
                        completions,
                        word_start,
                    })
                }
            }
            Err(e) => {
                error!("Failed fetching completions {}", e);
                None
            }
        }
    }
}

impl ShCompleter {
    fn find_completion_prefix(input: &str, cursor_pos: usize) -> (String, CompletionType, usize) {
        let input_up_to_cursor = &input[..cursor_pos];

        let word_start = input_up_to_cursor
            .rfind(|c: char| c.is_whitespace() || c == '|')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &input_up_to_cursor[word_start..];

        // It's a command if it's the first word after the beginning of the string or after a pipe.
        // todo env vars before command
        let before_word = &input_up_to_cursor[..word_start].trim_end();
        let completion_type = if before_word.is_empty() || before_word.ends_with('|') {
            CompletionType::Command
        } else {
            CompletionType::File
        };

        (prefix.to_string(), completion_type, word_start)
    }
}

#[derive(Debug, PartialEq)]
enum CompletionType {
    Command,
    File,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_completion_prefix() {
        assert_eq!(
            ShCompleter::find_completion_prefix("grep ", 5),
            ("".to_string(), CompletionType::File, 5)
        );
        assert_eq!(
            ShCompleter::find_completion_prefix("grep f", 6),
            ("f".to_string(), CompletionType::File, 5)
        );
        assert_eq!(
            ShCompleter::find_completion_prefix("ls|gr", 5),
            ("gr".to_string(), CompletionType::Command, 3)
        );
        assert_eq!(
            ShCompleter::find_completion_prefix("ls | gr", 7),
            ("gr".to_string(), CompletionType::Command, 5)
        );
        assert_eq!(
            ShCompleter::find_completion_prefix("ls | ", 5),
            ("".to_string(), CompletionType::Command, 5)
        );
        assert_eq!(
            ShCompleter::find_completion_prefix("grep foo", 8),
            ("foo".to_string(), CompletionType::File, 5)
        );
        assert_eq!(
            ShCompleter::find_completion_prefix("grep foo ", 9),
            ("".to_string(), CompletionType::File, 9)
        );
    }
}
