use crate::config::KeyBindingsConfig;
use crossterm::event::{KeyCode, KeyModifiers};
use log::debug;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    ScrollLeftPage,
    ScrollRight,
    ScrollRightPage,
    ToggleWrap,
    HistoryPrev,
    HistoryNext,
    SubcommandNext,
    SubcommandPrev,
    Complete,
    CompletePrev,
    SearchNext,
    SearchPrev,
    SaveOutput,
    SaveCommand,
    FormatCommand,
    SubcommandCut,
    SubcommandCopy,
    SubcommandPaste,
    ToggleDiff,
    DiffBase,
    DiffBaseStdin,
    ToggleLive,
    ToggleLiveUntilCursor,
    TogglePresets,
    ToggleLineNums,
    ToggleStdinReading,
    ToggleFollow,
}

pub struct KeyBindings {
    pub bindings: HashMap<UiCmd, Vec<(KeyCode, KeyModifiers)>>,
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
        bindings.insert(
            UiCmd::ScrollLeftPage,
            parse_bindings(&config.scroll_left_page),
        );
        bindings.insert(UiCmd::ScrollRight, parse_bindings(&config.scroll_right));
        bindings.insert(
            UiCmd::ScrollRightPage,
            parse_bindings(&config.scroll_right_page),
        );
        bindings.insert(UiCmd::ToggleWrap, parse_bindings(&config.toggle_wrap));
        bindings.insert(UiCmd::HistoryPrev, parse_bindings(&config.history_prev));
        bindings.insert(UiCmd::HistoryNext, parse_bindings(&config.history_next));
        bindings.insert(
            UiCmd::SubcommandNext,
            parse_bindings(&config.subcommand_next),
        );
        bindings.insert(
            UiCmd::SubcommandPrev,
            parse_bindings(&config.subcommand_prev),
        );
        bindings.insert(UiCmd::Complete, parse_bindings(&config.complete));
        bindings.insert(UiCmd::CompletePrev, parse_bindings(&config.complete_prev));
        bindings.insert(UiCmd::SearchNext, parse_bindings(&config.search_next));
        bindings.insert(UiCmd::SearchPrev, parse_bindings(&config.search_prev));
        bindings.insert(UiCmd::SaveOutput, parse_bindings(&config.save_output));
        bindings.insert(UiCmd::SaveCommand, parse_bindings(&config.save_command));
        bindings.insert(UiCmd::FormatCommand, parse_bindings(&config.format_command));
        bindings.insert(UiCmd::SubcommandCut, parse_bindings(&config.subcommand_cut));
        bindings.insert(
            UiCmd::SubcommandCopy,
            parse_bindings(&config.subcommand_copy),
        );
        bindings.insert(
            UiCmd::SubcommandPaste,
            parse_bindings(&config.subcommand_paste),
        );
        bindings.insert(UiCmd::ToggleDiff, parse_bindings(&config.toggle_diff));
        bindings.insert(UiCmd::DiffBase, parse_bindings(&config.diff_base));
        bindings.insert(
            UiCmd::DiffBaseStdin,
            parse_bindings(&config.diff_base_stdin),
        );
        bindings.insert(UiCmd::ToggleLive, parse_bindings(&config.toggle_live));
        bindings.insert(
            UiCmd::ToggleLiveUntilCursor,
            parse_bindings(&config.toggle_live_until_cursor),
        );
        bindings.insert(UiCmd::TogglePresets, parse_bindings(&config.toggle_presets));
        bindings.insert(
            UiCmd::ToggleLineNums,
            parse_bindings(&config.toggle_line_nums),
        );
        bindings.insert(
            UiCmd::ToggleStdinReading,
            parse_bindings(&config.toggle_stdin_reading),
        );
        bindings.insert(UiCmd::ToggleFollow, parse_bindings(&config.toggle_follow));

        KeyBindings { bindings }
    }
}

fn parse_bindings(keys: &[String]) -> Vec<(KeyCode, KeyModifiers)> {
    keys.iter().filter_map(|s| parse_key_binding(s)).collect()
}

fn parse_key_binding(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    let parts: Vec<&str> = s.splitn(5, '+').collect();
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
        "backtab" => KeyCode::BackTab,
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

pub fn to_ui_command(bindings: &KeyBindings, code: KeyCode, mods: KeyModifiers) -> Option<UiCmd> {
    // Cleaning up data about key press to avoid leaking some of terminal related quirks into config
    // Not sure if this is the best way to do it, but it works for now
    let (c, m) = match (code, mods) {
        (KeyCode::BackTab, KeyModifiers::NONE) => (KeyCode::Tab, KeyModifiers::SHIFT),
        (KeyCode::BackTab, KeyModifiers::SHIFT) => (KeyCode::Tab, KeyModifiers::SHIFT),
        (KeyCode::Char(c), mods) => (KeyCode::Char(c.to_lowercase().next().unwrap()), mods),
        other => other,
    };
    let cmd_opt = bindings.bindings.iter().find_map(|(action, bindings)| {
        if bindings.contains(&(c, m)) {
            Some(*action)
        } else {
            None
        }
    });

    if let Some(action) = cmd_opt {
        debug!("Action: {:?} -> {:?}", (code, mods), action);
    } else {
        debug!("Key press: {:?}", (code, mods));
    };

    cmd_opt
}
