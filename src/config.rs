use crate::props::APP_NAME;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct StyleConfig {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub dim: Option<bool>,
}

impl StyleConfig {
    fn fg(fg: &str) -> Self {
        StyleConfig {
            fg: Some(fg.to_string()),
            ..Default::default()
        }
    }

    fn bg(bg: &str) -> Self {
        StyleConfig {
            bg: Some(bg.to_string()),
            ..Default::default()
        }
    }

    fn fg_bg(fg: &str, bg: &str) -> Self {
        StyleConfig {
            fg: Some(fg.to_string()),
            bg: Some(bg.to_string()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub cmd_regular: StyleConfig,
    pub cmd_regular_pipe: StyleConfig,
    pub cmd_regular_current: StyleConfig,
    pub cmd_highlight: StyleConfig,
    pub cmd_highlight_pipe: StyleConfig,
    pub cmd_highlight_current: StyleConfig,
    pub cmd_quoted: StyleConfig,
    pub cmd_invalid: StyleConfig,
    pub line_nums: StyleConfig,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            cmd_regular: StyleConfig::default(),
            cmd_regular_pipe: StyleConfig::fg("green"),
            cmd_regular_current: StyleConfig::bg("gray"),
            cmd_highlight: StyleConfig::fg_bg("black", "yellow"),
            cmd_highlight_pipe: StyleConfig::bg("yellow"),
            cmd_highlight_current: StyleConfig::fg_bg("black", "yellow"),
            cmd_quoted: StyleConfig::fg("yellow"),
            cmd_invalid: StyleConfig::default(),
            line_nums: StyleConfig::fg("magenta"),
        }
    }
}

/// Key binding strings use the format: `[modifier+]*key`
/// Modifiers: `ctrl`, `alt`, `shift`
/// Named keys: `enter`, `esc`, `backspace`, `delete`, `tab`, `backtab`, `up`, `down`, `left`, `right`,
///             `pageup`, `pagedown`, `home`, `end`, `f1`–`f12`
/// Single characters: `a`–`z`, `0`–`9`, symbols like `|`, `\`, etc.
/// Examples: `"ctrl+c"`, `"alt+j"`, `"enter"`, `"pagedown"`, `"alt+\\"`
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct KeyBindingsConfig {
    pub quit: Vec<String>,
    pub execute_full: Vec<String>,
    pub execute_until_current: Vec<String>,
    pub execute_until_prev: Vec<String>,
    pub reset_input: Vec<String>,
    pub scroll_down: Vec<String>,
    pub scroll_down_page: Vec<String>,
    pub scroll_up: Vec<String>,
    pub scroll_up_page: Vec<String>,
    pub scroll_left: Vec<String>,
    pub scroll_right: Vec<String>,
    pub toggle_wrap: Vec<String>,
    pub history_prev: Vec<String>,
    pub history_next: Vec<String>,
    pub subcommand_next: Vec<String>,
    pub subcommand_prev: Vec<String>,
}

impl Default for KeyBindingsConfig {
    fn default() -> Self {
        KeyBindingsConfig {
            quit: vec!["ctrl+c".into()],
            execute_full: vec!["enter".into()],
            execute_until_current: vec!["alt+\\".into()],
            execute_until_prev: vec!["alt+|".into()],
            reset_input: vec!["alt+i".into()],
            scroll_down: vec!["down".into(), "alt+j".into()],
            scroll_down_page: vec!["pagedown".into(), "ctrl+d".into(), "alt+down".into()],
            scroll_up: vec!["up".into(), "alt+k".into()],
            scroll_up_page: vec!["pageup".into(), "ctrl+u".into(), "alt+up".into()],
            scroll_left: vec!["alt+left".into(), "alt+h".into()],
            scroll_right: vec!["alt+right".into(), "alt+l".into()],
            toggle_wrap: vec!["alt+w".into()],
            history_prev: vec!["ctrl+p".into()],
            history_next: vec!["ctrl+n".into()],
            subcommand_next: vec!["tab".into()],
            subcommand_prev: vec!["shift+backtab".into(), "shift+tab".into(), "backtab".into()],
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub keybindings: KeyBindingsConfig,
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_NAME).join("config.toml"))
}

pub fn history_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(APP_NAME).join("history.txt"))
}

pub fn load_config() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };

    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let default = Config::default();
        if let Ok(content) = toml::to_string_pretty(&default) {
            let _ = fs::write(&path, content);
        }
        return default;
    }

    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}
