use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    music_dir: PathBuf,
}

/// Returns the path to the config file:
/// `$XDG_CONFIG_HOME/tuiplay/config.json` or `~/.config/tuiplay/config.json`.
fn config_path() -> PathBuf {
    let base = match env::var("XDG_CONFIG_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        }
    };
    base.join("tuiplay").join("config.json")
}

/// Try to load the saved music directory from the config file.
/// Returns `None` if the file doesn't exist, is empty, or contains invalid data.
pub fn load_music_dir() -> Option<PathBuf> {
    let path = config_path();
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(&path).ok()?;
    if data.trim().is_empty() {
        return None;
    }
    let config: Config = serde_json::from_str(&data).ok()?;
    // Only return if the saved directory actually exists.
    config.music_dir.is_dir().then_some(config.music_dir)
}

/// Persist the chosen music directory to the config file.
/// Creates parent directories as needed.
pub fn save_music_dir(dir: &PathBuf) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let config = Config {
        music_dir: dir.clone(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        let _ = fs::write(&path, json);
    }
}
