use itertools::Itertools;
use log::{debug, error};
use std::process::Command;

pub trait Completer {
    fn completions(&self, prefix: &str, completion_type: CompletionType) -> Vec<String>;
}

#[allow(dead_code)]
pub struct NoopCompleter;

impl Completer for NoopCompleter {
    fn completions(&self, prefix: &str, completion_type: CompletionType) -> Vec<String> {
        debug!(
            "calling noop completions [{:?}]: '{}'",
            completion_type, prefix
        );
        vec![]
    }
}

pub struct BashCompleter;

impl Completer for BashCompleter {
    fn completions(&self, prefix: &str, completion_type: CompletionType) -> Vec<String> {
        debug!(
            "calling bash completions [{:?}]: '{}'",
            completion_type, prefix
        );

        let comp_type_str = match completion_type {
            CompletionType::Command => "-c",
            CompletionType::File => "-f",
        };

        let output = Command::new("/usr/bin/env")
            .args([
                "bash",
                "-c",
                &format!("compgen {} -- \"{}\"", comp_type_str, prefix),
            ])
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

                completions
            }
            Err(e) => {
                error!("Failed fetching completions {}", e);
                vec![]
            }
        }
    }
}

pub struct ZshCompleter;

impl Completer for ZshCompleter {
    fn completions(&self, prefix: &str, completion_type: CompletionType) -> Vec<String> {
        debug!(
            "calling zsh completions [{:?}]: '{}'",
            completion_type, prefix
        );

        let cmd = match completion_type {
            CompletionType::Command => {
                format!("print -l ${{(k)commands[(I){}*]}}", prefix)
            }
            CompletionType::File => {
                if prefix.starts_with("~") {
                    format!("setopt extended_glob && print -l {}*(N)", prefix)
                } else {
                    format!("setopt extended_glob && print -l (#i){}*(N)", prefix)
                }
            }
        };

        let output = Command::new("/usr/bin/env")
            .args(["zsh", "-c", &cmd])
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let completions: Vec<String> = stdout
                    .lines()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect_vec();

                debug!(
                    "completion results [{}]: {:?}",
                    completions.len(),
                    completions
                );

                completions
            }
            Err(e) => {
                error!("Failed fetching zsh completions {}", e);
                vec![]
            }
        }
    }
}

pub struct FishCompleter;

impl Completer for FishCompleter {
    fn completions(&self, prefix: &str, completion_type: CompletionType) -> Vec<String> {
        debug!(
            "calling fish completions [{:?}]: '{}'",
            completion_type, prefix
        );

        let cmd = match completion_type {
            CompletionType::Command => {
                format!("complete -C {} | cut -f1", prefix)
            }
            CompletionType::File => {
                format!("complete -C \"cat {}\"", prefix)
            }
        };

        let output = Command::new("/usr/bin/env")
            .args(["fish", "-c", &cmd])
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let completions: Vec<String> = stdout
                    .lines()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect_vec();

                debug!(
                    "completion results [{}]: {:?}",
                    completions.len(),
                    completions
                );

                completions
            }
            Err(e) => {
                error!("Failed fetching fish completions {}", e);
                vec![]
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CompletionType {
    Command,
    File,
}

pub struct Completers;

impl Completers {
    pub fn for_shell(shell: &str) -> Box<dyn Completer> {
        match shell {
            "bash" => Box::new(BashCompleter {}),
            "zsh" => Box::new(ZshCompleter {}),
            "fish" => Box::new(FishCompleter {}),
            "sh" => Box::new(NoopCompleter {}),
            _ => Box::new(BashCompleter {}),
        }
    }
}
