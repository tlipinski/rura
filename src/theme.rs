use ratatui::prelude::{Color, Style};
use crate::config::ThemeConfig;

pub struct Theme {
    pub cmd_regular: Style,
    pub cmd_regular_pipe: Style,
    pub cmd_regular_current: Style,

    pub cmd_highlight: Style,
    pub cmd_highlight_pipe: Style,
    pub cmd_highlight_current: Style,

    pub cmd_invalid: Style,

    pub line_nums: Style,
}

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        Theme {
            cmd_regular: style_from_config(&config.cmd_regular),
            cmd_regular_pipe: style_from_config(&config.cmd_regular_pipe),
            cmd_regular_current: style_from_config(&config.cmd_regular_current),
            cmd_highlight: style_from_config(&config.cmd_highlight),
            cmd_highlight_pipe: style_from_config(&config.cmd_highlight_pipe),
            cmd_highlight_current: style_from_config(&config.cmd_highlight_current),
            cmd_invalid: style_from_config(&config.cmd_invalid),
            line_nums: style_from_config(&config.line_nums),
        }
    }
}

fn style_from_config(sc: &crate::config::StyleConfig) -> Style {
    let mut s = Style::default();
    if let Some(c) = sc.fg.as_deref().and_then(parse_color) {
        s = s.fg(c);
    }
    if let Some(c) = sc.bg.as_deref().and_then(parse_color) {
        s = s.bg(c);
    }
    if sc.bold.unwrap_or(false) {
        s = s.bold();
    }
    if sc.italic.unwrap_or(false) {
        s = s.italic();
    }
    if sc.underlined.unwrap_or(false) {
        s = s.underlined();
    }
    if sc.dim.unwrap_or(false) {
        s = s.dim();
    }
    s
}

fn parse_color(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "dark_gray" => Some(Color::DarkGray),
        "lightred" | "light_red" => Some(Color::LightRed),
        "lightgreen" | "light_green" => Some(Color::LightGreen),
        "lightyellow" | "light_yellow" => Some(Color::LightYellow),
        "lightblue" | "light_blue" => Some(Color::LightBlue),
        "lightmagenta" | "light_magenta" => Some(Color::LightMagenta),
        "lightcyan" | "light_cyan" => Some(Color::LightCyan),
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).ok()?;
            let g = u8::from_str_radix(&s[3..5], 16).ok()?;
            let b = u8::from_str_radix(&s[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        s => s.parse::<u8>().ok().map(Color::Indexed),
    }
}

