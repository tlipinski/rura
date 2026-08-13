use crossterm::event::{Event, KeyCode};
use modalkit::actions::{Action, Editable};
use modalkit::editing::application::EmptyInfo;
use modalkit::editing::buffer::CursorGroupId;
use modalkit::editing::cursor::Cursor;
use modalkit::editing::store::{SharedBuffer, Store};
use modalkit::env::vim::VimMode;
use modalkit::env::vim::keybindings::{VimMachine, default_vim_keys};
use modalkit::key::TerminalKey;
use modalkit::keybindings::BindingMachine;
use modalkit::prelude::ViewportContext;
use serde::{Deserialize, Serialize};
use tui_input::backend::crossterm::EventHandler;
use tui_input::{Input, InputRequest};

pub trait TextInput {
    fn value(&self) -> String;
    fn cursor(&self) -> usize;
    fn set_cursor(&mut self, cursor: usize);
    fn handle_event(&mut self, evt: &Event) -> bool;
    fn set_value(&mut self, value: &str);
    fn mode(&self) -> ModalInputMode;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum EditingMode {
    #[default]
    Std,
    Vim,
}

pub struct Inputs {}
impl Inputs {
    pub fn from(tp: EditingMode, value: String) -> Box<dyn TextInput> {
        match tp {
            EditingMode::Std => Box::new(StandardInput::new(value.into())),
            EditingMode::Vim => Box::new(VimInput::new(value.into())),
        }
    }
}

struct StandardInput {
    input: Input,
}

impl StandardInput {
    fn new(value: String) -> Self {
        Self {
            input: Input::new(value.into()),
        }
    }
}

impl TextInput for StandardInput {
    fn value(&self) -> String {
        self.input.value().into()
    }

    fn cursor(&self) -> usize {
        self.input.visual_cursor()
    }

    fn set_cursor(&mut self, cursor: usize) {
        self.input.handle(InputRequest::SetCursor(cursor));
    }

    fn handle_event(&mut self, evt: &Event) -> bool {
        if let Some(state_changed) = EventHandler::handle_event(&mut self.input, evt) {
            state_changed.value
        } else {
            false
        }
    }

    fn set_value(&mut self, value: &str) {
        self.input = Input::new(value.into());
    }

    fn mode(&self) -> ModalInputMode {
        ModalInputMode::Normal
    }
}

pub struct VimInput {
    store: Store<EmptyInfo>,
    buffer: SharedBuffer<EmptyInfo>,
    cgi: CursorGroupId,
    view_ctx: ViewportContext<Cursor>,
    machine: VimMachine<TerminalKey>,
}

impl TextInput for VimInput {
    fn value(&self) -> String {
        self.value()
    }

    fn cursor(&self) -> usize {
        self.cursor()
    }

    fn set_cursor(&mut self, cursor: usize) {
        self.buffer
            .write()
            .unwrap()
            .set_leader(self.cgi, Cursor::new(0, cursor));
    }

    fn handle_event(&mut self, evt: &Event) -> bool {
        let before = self.value();
        self.handle_event(evt);
        let after = self.value();

        before != after
    }

    fn set_value(&mut self, value: &str) {
        self.buffer.write().unwrap().set_text(String::from(value));
        self.buffer
            .write()
            .unwrap()
            .set_leader(self.cgi, Cursor::new(0, value.len()));
    }

    fn mode(&self) -> ModalInputMode {
        match self.machine.mode() {
            VimMode::Insert => ModalInputMode::Insert,
            _ => ModalInputMode::Normal,
        }
    }
}

impl VimInput {
    pub fn new(init: String) -> Self {
        let mut store = Store::<EmptyInfo>::default();
        let buffer = store.load_buffer(String::from("*scratch*"));
        let cgi = buffer.write().unwrap().create_group();
        let mut cursor = Cursor::default();
        cursor.set_x(init.len());
        buffer.write().unwrap().set_text(init);
        buffer.write().unwrap().set_leader(cgi, cursor.clone());
        let mut machine = default_vim_keys();

        // enter insert mode by default
        machine.input_key(TerminalKey::from(KeyCode::Char('i')));

        Self {
            store,
            buffer,
            cgi,
            machine,
            view_ctx: ViewportContext::default(),
        }
    }

    pub fn cursor(&self) -> usize {
        self.buffer.write().unwrap().get_leader(self.cgi).x
    }

    pub fn value(&self) -> String {
        let value = self.buffer.read().unwrap().get_text();
        value[..value.len() - 1].into()
    }

    pub fn handle_event(&mut self, evt: &Event) {
        match evt {
            Event::Key(key) => {
                self.machine.input_key(key.clone().into());

                while let Some((act, ctx)) = self.machine.pop() {
                    match act {
                        Action::Editor(action) => {
                            self.buffer
                                .editor_command(
                                    &action,
                                    &(self.cgi, &self.view_ctx, &ctx),
                                    &mut self.store,
                                )
                                .unwrap();
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

pub enum ModalInputMode {
    Insert,
    Normal,
}

#[cfg(test)]
mod tests {
    use crate::text_input::VimInput;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn vim_mode() {
        let mut input = VimInput::new("aaa/bbb/ccc/ddd".into());

        assert_eq!("aaa/bbb/ccc/ddd", input.value());

        input.handle_event(&key(KeyCode::Esc)); // exit default insert mode

        "0wwd2f/".chars().for_each(|c| {
            input.handle_event(&key(KeyCode::Char(c)));
        });

        assert_eq!("aaa/ddd", input.value());

        "$T/cawzzz".chars().for_each(|c| {
            input.handle_event(&key(KeyCode::Char(c)));
        });

        assert_eq!("aaa/zzz", input.value());
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }
}
