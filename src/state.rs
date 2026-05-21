use crate::props::APP_NAME;
use log::warn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub struct CachedState {
    #[serde(default)]
    pub multiline: bool,
}

pub fn cache_state_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(APP_NAME).join("state.toml"))
}

pub fn load() -> CachedState {
    let Some(path) = cache_state_path() else {
        return CachedState::default();
    };
    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => CachedState::default(),
    }
}

pub fn save(state: &CachedState) {
    if cfg!(test) {
        return;
    }
    let Some(path) = cache_state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match toml::to_string_pretty(state) {
        Ok(content) => {
            if let Err(e) = fs::write(&path, content) {
                warn!("Failed to write cached state: {}", e);
            }
        }
        Err(e) => warn!("Failed to serialize cached state: {}", e),
    }
}
