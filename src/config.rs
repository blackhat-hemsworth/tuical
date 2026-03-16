use std::{collections::HashMap, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CalType {
    #[default]
    Ics,
    #[serde(alias = "caldav")]
    Google,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalendarEntry {
    pub name: String,
    pub url: String,
    pub color: String,
    pub enabled: bool,
    #[serde(default)]
    pub cal_type: CalType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_role: Option<String>,
}

impl CalendarEntry {
    pub fn is_writable(&self) -> bool {
        match self.access_role.as_deref() {
            Some("owner") | Some("writer") => true,
            Some(_) => false,
            None => self.cal_type == CalType::Google,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default, skip_serializing)]
    pub ics_url: Option<String>,
    #[serde(default)]
    pub calendars: Vec<CalendarEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GoogleTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum EventFormMode {
    Title,
    Date,
    StartTime,
    EndTime,
    Description,
    Confirm,
}

pub fn config_path() -> PathBuf {
    let base = config_dir();
    base.join("caltui").join("config.toml")
}

pub fn tokens_path() -> PathBuf {
    let base = config_dir();
    base.join("caltui").join("tokens.json")
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
                cal_type: CalType::Ics,
                google_account: None,
                calendar_id: None,
                access_role: None,
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

pub fn load_tokens() -> HashMap<String, GoogleTokens> {
    let path = tokens_path();
    if let Ok(contents) = fs::read_to_string(&path) {
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

pub fn save_tokens(tokens: &HashMap<String, GoogleTokens>) -> Result<(), String> {
    let path = tokens_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let contents = serde_json::to_string_pretty(tokens).map_err(|e| e.to_string())?;
    fs::write(&path, &contents).map_err(|e| e.to_string())?;

    // Set file permissions to 0o600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cal_type_default_is_ics() {
        assert_eq!(CalType::default(), CalType::Ics);
    }

    #[test]
    fn cal_type_serde_roundtrip() {
        let ics: CalType = serde_json::from_str("\"ics\"").unwrap();
        assert_eq!(ics, CalType::Ics);
        let google: CalType = serde_json::from_str("\"google\"").unwrap();
        assert_eq!(google, CalType::Google);
    }

    #[test]
    fn cal_type_caldav_alias() {
        let google: CalType = serde_json::from_str("\"caldav\"").unwrap();
        assert_eq!(google, CalType::Google);
    }

    #[test]
    fn calendar_entry_deserializes_without_new_fields() {
        let toml_str = r#"
            name = "Test"
            url = "https://example.com"
            color = "blue"
            enabled = true
        "#;
        let entry: CalendarEntry = toml::from_str(toml_str).unwrap();
        assert_eq!(entry.cal_type, CalType::Ics);
        assert!(entry.google_account.is_none());
        assert!(entry.calendar_id.is_none());
    }

    #[test]
    fn calendar_entry_deserializes_with_google_fields() {
        let toml_str = r#"
            name = "Google"
            url = ""
            color = "green"
            enabled = true
            cal_type = "google"
            google_account = "user@gmail.com"
            calendar_id = "user@gmail.com"
        "#;
        let entry: CalendarEntry = toml::from_str(toml_str).unwrap();
        assert_eq!(entry.cal_type, CalType::Google);
        assert_eq!(entry.google_account.as_deref(), Some("user@gmail.com"));
        assert_eq!(entry.calendar_id.as_deref(), Some("user@gmail.com"));
    }

    #[test]
    fn is_writable_owner() {
        let entry = CalendarEntry {
            name: "Test".into(), url: String::new(), color: "blue".into(),
            enabled: true, cal_type: CalType::Google,
            google_account: None, calendar_id: None,
            access_role: Some("owner".into()),
        };
        assert!(entry.is_writable());
    }

    #[test]
    fn is_writable_writer() {
        let entry = CalendarEntry {
            name: "Test".into(), url: String::new(), color: "blue".into(),
            enabled: true, cal_type: CalType::Google,
            google_account: None, calendar_id: None,
            access_role: Some("writer".into()),
        };
        assert!(entry.is_writable());
    }

    #[test]
    fn is_writable_reader_returns_false() {
        let entry = CalendarEntry {
            name: "Test".into(), url: String::new(), color: "blue".into(),
            enabled: true, cal_type: CalType::Google,
            google_account: None, calendar_id: None,
            access_role: Some("reader".into()),
        };
        assert!(!entry.is_writable());
    }

    #[test]
    fn is_writable_freebusy_returns_false() {
        let entry = CalendarEntry {
            name: "Test".into(), url: String::new(), color: "blue".into(),
            enabled: true, cal_type: CalType::Google,
            google_account: None, calendar_id: None,
            access_role: Some("freeBusyReader".into()),
        };
        assert!(!entry.is_writable());
    }

    #[test]
    fn is_writable_none_google_assumes_writable() {
        let entry = CalendarEntry {
            name: "Test".into(), url: String::new(), color: "blue".into(),
            enabled: true, cal_type: CalType::Google,
            google_account: None, calendar_id: None,
            access_role: None,
        };
        assert!(entry.is_writable());
    }

    #[test]
    fn is_writable_none_ics_returns_false() {
        let entry = CalendarEntry {
            name: "Test".into(), url: String::new(), color: "blue".into(),
            enabled: true, cal_type: CalType::Ics,
            google_account: None, calendar_id: None,
            access_role: None,
        };
        assert!(!entry.is_writable());
    }

    #[test]
    fn google_tokens_serde_roundtrip() {
        let tokens = GoogleTokens {
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: 12345,
        };
        let json = serde_json::to_string(&tokens).unwrap();
        let parsed: GoogleTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "access");
        assert_eq!(parsed.refresh_token, "refresh");
        assert_eq!(parsed.expires_at, 12345);
    }

    #[test]
    fn tokens_map_serde_roundtrip() {
        let mut map = HashMap::new();
        map.insert("user@gmail.com".to_string(), GoogleTokens {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 999,
        });
        let json = serde_json::to_string(&map).unwrap();
        let parsed: HashMap<String, GoogleTokens> = serde_json::from_str(&json).unwrap();
        assert!(parsed.contains_key("user@gmail.com"));
        assert_eq!(parsed["user@gmail.com"].expires_at, 999);
    }
}
