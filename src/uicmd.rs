use std::collections::HashMap;
use crossterm::event::{KeyCode, KeyModifiers};
use crate::config::KeyBindingsConfig;

#[derive(PartialEq, Eq, Hash)]
pub enum UiCmd {
    Quit,
    ExecuteFull,
    ExecuteUntilCurrent,
    ExecuteUntilPrev,
    ResetInput,
    ScrollDown,
    ScrollDownPage,
    ScrollUp,
    ScrollUpPage,
    ScrollLeft,
    ScrollRight,
    ToggleWrap,
    HistoryPrev,
    HistoryNext,
}

pub struct KeyBindings {
    bindings: HashMap<UiCmd, Vec<(KeyCode, KeyModifiers)>>,
}

impl KeyBindings {
    pub fn from_config(config: &KeyBindingsConfig) -> Self {
        let mut bindings: HashMap<UiCmd, Vec<(KeyCode, KeyModifiers)>> = HashMap::new();
        bindings.insert(UiCmd::Quit, parse_bindings(&config.quit));
        bindings.insert(UiCmd::ExecuteFull, parse_bindings(&config.execute_full));
        bindings.insert(
            UiCmd::ExecuteUntilCurrent,
            parse_bindings(&config.execute_until_current),
        );
        bindings.insert(
            UiCmd::ExecuteUntilPrev,
            parse_bindings(&config.execute_until_prev),
        );
        bindings.insert(UiCmd::ResetInput, parse_bindings(&config.reset_input));
        bindings.insert(UiCmd::ScrollDown, parse_bindings(&config.scroll_down));
        bindings.insert(
            UiCmd::ScrollDownPage,
            parse_bindings(&config.scroll_down_page),
        );
        bindings.insert(UiCmd::ScrollUp, parse_bindings(&config.scroll_up));
        bindings.insert(UiCmd::ScrollUpPage, parse_bindings(&config.scroll_up_page));
        bindings.insert(UiCmd::ScrollLeft, parse_bindings(&config.scroll_left));
        bindings.insert(UiCmd::ScrollRight, parse_bindings(&config.scroll_right));
        bindings.insert(UiCmd::ToggleWrap, parse_bindings(&config.toggle_wrap));
        bindings.insert(UiCmd::HistoryPrev, parse_bindings(&config.history_prev));
        bindings.insert(UiCmd::HistoryNext, parse_bindings(&config.history_next));
        KeyBindings { bindings }
    }
}

fn parse_bindings(keys: &[String]) -> Vec<(KeyCode, KeyModifiers)> {
    keys.iter().filter_map(|s| parse_key_binding(s)).collect()
}

fn parse_key_binding(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    // Split into parts; everything before the last segment is a modifier.
    // Use splitn with a high limit to get all segments.
    let parts: Vec<&str> = s.splitn(10, '+').collect();
    if parts.is_empty() {
        return None;
    }

    let (modifier_parts, key_parts) = parts.split_at(parts.len() - 1);
    let key_str = key_parts[0].to_lowercase();

    let mut modifiers = KeyModifiers::NONE;
    for part in modifier_parts {
        match part.to_lowercase().as_str() {
            "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => return None,
        }
    }

    let code = match key_str.as_str() {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        s if s.chars().count() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        _ => return None,
    };

    Some((code, modifiers))
}

pub fn to_ui_command(bindings: &KeyBindings, code: KeyCode, mods: KeyModifiers) -> Option<&UiCmd> {
    bindings.bindings.iter().find_map(|(action, bindings)| {
        if bindings.contains(&(code, mods)) {
            Some(action)
        } else {
            None
        }
    })
}
