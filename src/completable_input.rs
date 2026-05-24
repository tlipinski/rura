use crate::completion::FileOnlyShCompleter;
use crate::completion::ShCompleter;
use crate::completion::{Completer, CompletionResult};
use crossterm::event::Event;
use tui_input::backend::crossterm::to_input_request;
use tui_input::{Input, InputRequest, InputResponse, StateChanged};

pub struct CompletableInput {
    input: Input,
    completions: Option<(CompletionResult, usize)>,
    completer: Box<dyn Completer>,
}

impl From<String> for CompletableInput {
    fn from(value: String) -> Self {
        Self {
            input: Input::new(value),
            completions: None,
            completer: Box::new(ShCompleter {}),
        }
    }
}

impl From<&str> for CompletableInput {
    fn from(value: &str) -> Self {
        Self {
            input: Input::new(value.to_string()),
            completions: None,
            completer: Box::new(ShCompleter {}),
        }
    }
}

impl CompletableInput {
    pub fn file_only(str: &str) -> Self {
        Self {
            input: Input::new(str.to_string()),
            completions: None,
            completer: Box::new(FileOnlyShCompleter {}),
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
        } else if let Some(res) = self.completer.completions(&current_value, cursor_pos) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode::Char;
    use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tui_input::Input;

    struct TestCompleter;

    impl Completer for TestCompleter {
        fn completions(&self, _input: &str, _cursor_pos: usize) -> Option<CompletionResult> {
            Some(CompletionResult {
                completions: vec!["command".to_string(), "command_other".to_string()],
                word_start: 0,
            })
        }
    }

    impl Default for CompletableInput {
        fn default() -> Self {
            CompletableInput {
                input: Input::from(""),
                completions: None,
                completer: Box::new(TestCompleter {}),
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
