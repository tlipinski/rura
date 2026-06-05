use crate::completion::Completers;
use crate::completion::{Completer, CompletionType};
use crossterm::event::Event;
use tui_input::backend::crossterm::to_input_request;
use tui_input::{Input, InputRequest, InputResponse, StateChanged};

pub struct CompletableInput {
    input: Input,
    completions: Option<(CompletionResult, usize)>,
    completer: Box<dyn Completer>,
    completion_type_rule: Option<CompletionType>,
}

impl CompletableInput {
    pub fn from(str: &str, shell: &str) -> Self {
        Self {
            input: Input::new(str.to_string()),
            completions: None,
            completer: Completers::for_shell(shell),
            completion_type_rule: None,
        }
    }

    pub fn file_only(str: &str, shell: &str) -> Self {
        Self {
            input: Input::new(str.to_string()),
            completions: None,
            completer: Completers::for_shell(shell),
            completion_type_rule: Some(CompletionType::File),
        }
    }

    pub fn cursor(&self) -> usize {
        self.input.cursor()
    }

    pub fn handle(&mut self, req: InputRequest) -> InputResponse {
        self.input.handle(req)
    }

    pub fn handle_event(&mut self, evt: &Event) -> Option<StateChanged> {
        self.completions = None;
        to_input_request(evt).and_then(|req| self.input.handle(req))
    }

    pub fn value(&self) -> &str {
        self.input.value()
    }

    pub fn with_value(&mut self, value: String) {
        self.input = Input::from(value);
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.input.handle(InputRequest::SetCursor(pos));
    }

    pub fn visual_cursor(&self) -> usize {
        self.input.visual_cursor()
    }

    pub fn clear_completions(&mut self) {
        self.completions = None;
    }

    pub fn complete(&mut self, next: bool) {
        let current_value = self.input.value().to_string();
        let cursor_pos = self.input.visual_cursor();

        if let Some((res, index)) = self.completions.as_mut() {
            if next {
                *index = (*index + 1) % res.completions.len();
            } else {
                *index = if *index == 0 {
                    res.completions.len() - 1
                } else {
                    *index - 1
                };
            }
            let completion = &res.completions[*index];
            let new_value = format!(
                "{}{}{}",
                &current_value[..res.word_start],
                completion,
                &current_value[cursor_pos..]
            );
            self.input = Input::from(new_value);
            self.input
                .handle(InputRequest::SetCursor(res.word_start + completion.len()));
        } else if let Some(res) = self.get_completions(&current_value, cursor_pos) {
            let index = if next { 0 } else { res.completions.len() - 1 };
            let word_start = res.word_start;
            let completion = res.completions[index].clone();
            let new_value = format!(
                "{}{}{}",
                &current_value[..word_start],
                completion,
                &current_value[cursor_pos..]
            );
            self.completions = Some((res, index));
            self.input = Input::from(new_value);
            self.input
                .handle(InputRequest::SetCursor(word_start + completion.len()));
        }
    }

    fn get_completions(&self, current_value: &str, cursor_pos: usize) -> Option<CompletionResult> {
        let (prefix, completion_type, word_start) = match self.completion_type_rule {
            None => find_completion_prefix_cmd_or_file(current_value, cursor_pos),
            Some(CompletionType::File) => {
                let p = find_completion_prefix_file(current_value, cursor_pos);
                (p.0, CompletionType::File, p.2) // todo order
            }
            Some(CompletionType::Command) => {
                todo!()
            }
        };

        let completions = self.completer.completions(&prefix, completion_type);

        if completions.is_empty() {
            None
        } else {
            Some(CompletionResult {
                completions,
                word_start,
            })
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct CompletionResult {
    pub completions: Vec<String>,
    pub word_start: usize,
}

pub fn find_completion_prefix_file(
    input: &str,
    cursor_pos: usize,
) -> (String, CompletionType, usize) {
    let input_up_to_cursor = &input[..cursor_pos];

    let word_start = input_up_to_cursor
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix = &input_up_to_cursor[word_start..];

    (prefix.to_string(), CompletionType::File, word_start)
}

pub fn find_completion_prefix_cmd_or_file(
    input: &str,
    cursor_pos: usize,
) -> (String, CompletionType, usize) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode::Char;
    use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tui_input::Input;

    struct TestCompleter;

    impl Completer for TestCompleter {
        fn completions(&self, _prefix: &str, _type: CompletionType) -> Vec<String> {
            vec!["command".to_string(), "command_other".to_string()]
        }
    }

    impl Default for CompletableInput {
        fn default() -> Self {
            CompletableInput {
                input: Input::from(""),
                completions: None,
                completer: Box::new(TestCompleter {}),
                completion_type_rule: None,
            }
        }
    }

    #[test]
    fn completer() {
        let mut input = CompletableInput::default();

        input_text(&mut input, "co");

        input.complete(true);
        assert_eq!(input.value(), "command");

        input.complete(true);
        assert_eq!(input.value(), "command_other");

        input.complete(false);
        assert_eq!(input.value(), "command");
    }

    #[test]
    fn test_find_completion_prefix_cmd_or_file() {
        assert_eq!(
            find_completion_prefix_cmd_or_file("grep ", 5),
            ("".to_string(), CompletionType::File, 5)
        );
        assert_eq!(
            find_completion_prefix_cmd_or_file("grep f", 6),
            ("f".to_string(), CompletionType::File, 5)
        );
        assert_eq!(
            find_completion_prefix_cmd_or_file("ls|gr", 5),
            ("gr".to_string(), CompletionType::Command, 3)
        );
        assert_eq!(
            find_completion_prefix_cmd_or_file("ls | gr", 7),
            ("gr".to_string(), CompletionType::Command, 5)
        );
        assert_eq!(
            find_completion_prefix_cmd_or_file("ls | ", 5),
            ("".to_string(), CompletionType::Command, 5)
        );
        assert_eq!(
            find_completion_prefix_cmd_or_file("grep foo", 8),
            ("foo".to_string(), CompletionType::File, 5)
        );
        assert_eq!(
            find_completion_prefix_cmd_or_file("grep foo ", 9),
            ("".to_string(), CompletionType::File, 9)
        );
    }

    #[test]
    fn test_find_completion_prefix_file() {
        assert_eq!(
            find_completion_prefix_file("file", 4),
            ("file".to_string(), CompletionType::File, 0)
        );
        assert_eq!(
            find_completion_prefix_file("file", 1),
            ("f".to_string(), CompletionType::File, 0)
        );
        assert_eq!(
            find_completion_prefix_file("file1 file2", 8),
            ("fi".to_string(), CompletionType::File, 6)
        );
    }

    fn input_text(app: &mut CompletableInput, text: &str) {
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
