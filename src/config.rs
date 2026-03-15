use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalendarEntry {
    pub name: String,
    pub url: String,
    pub color: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default, skip_serializing)]
    pub ics_url: Option<String>,
    #[serde(default)]
    pub calendars: Vec<CalendarEntry>,
}

pub fn config_path() -> PathBuf {
    let base = config_dir();
    base.join("caltui").join("config.toml")
}

fn config_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        let home = std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config")
    }
}

pub fn load_config() -> Config {
    let path = config_path();
    let mut config: Config = if let Ok(contents) = fs::read_to_string(&path) {
        toml::from_str(&contents).unwrap_or_default()
    } else {
        Config::default()
    };

    // Migrate legacy ics_url to calendars list
    if let Some(url) = config.ics_url.take() {
        if config.calendars.is_empty() && !url.is_empty() {
            config.calendars.push(CalendarEntry {
                name: "Calendar".into(),
                url,
                color: "blue".into(),
                enabled: true,
            });
        }
    }

    config
}

pub fn save_config(config: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let contents = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, contents).map_err(|e| e.to_string())
}
