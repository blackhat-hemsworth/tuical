use std::collections::HashMap;

use chrono::Local;
use chrono::NaiveDate;

use crate::google::{self, CalendarInfo};
use crate::calendar::{events_by_day, extract_links, fetch_ics, parse_ics, CalEvent};
use crate::config::{
    save_config, save_tokens, CalType, CalendarEntry, Config, GoogleTokens, load_tokens,
};
use crate::oauth::{self, DeviceCodeResponse};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Month,
    Week,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    EnteringUrl,
    Popup,
    CalendarManager,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalManagerMode {
    Normal,
    AddingUrl,
    AddingName,
    EditingName,
    PickingColor,
    ChoosingType,
    OAuthShowCode,
    OAuthPolling,
    OAuthPickCalendar,
}

pub struct PopupState {
    pub event_idx: usize,
    pub day_indices: Vec<usize>,
    pub day_pos: usize,
    pub scroll: u16,
    pub links: Vec<String>,
    pub link_idx: usize,
}

pub const COLOR_PALETTE: &[&str] = &[
    "red", "green", "blue", "yellow", "magenta", "cyan",
    "light_red", "light_green", "light_blue", "light_yellow", "light_magenta", "light_cyan",
];

pub struct App {
    pub config: Config,
    pub events: Vec<CalEvent>,
    pub day_map: HashMap<NaiveDate, Vec<usize>>,
    pub cursor: NaiveDate,
    pub view: ViewMode,
    pub show_events: bool,
    pub status: String,
    pub input_mode: InputMode,
    pub url_input: String,
    pub event_cursor: usize,
    pub popup: Option<PopupState>,
    // Calendar manager state
    pub cal_manager_cursor: usize,
    pub cal_manager_mode: CalManagerMode,
    pub cal_manager_input: String,
    pub cal_manager_pending_url: String,
    pub cal_manager_color_idx: usize,
    // OAuth state
    pub oauth_device_code: Option<DeviceCodeResponse>,
    pub oauth_poll_count: u32,
    pub oauth_account_email: Option<String>,
    pub oauth_calendars: Vec<CalendarInfo>,
    pub oauth_cal_cursor: usize,
    pub google_tokens: HashMap<String, GoogleTokens>,
    pub error: Option<String>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let today = Local::now().date_naive();
        let has_calendars = !config.calendars.is_empty();
        let google_tokens = load_tokens();
        App {
            config,
            events: Vec::new(),
            day_map: HashMap::new(),
            cursor: today,
            view: ViewMode::Month,
            show_events: true,
            status: if has_calendars {
                String::from("Loading...")
            } else {
                String::from("Press c to manage calendars")
            },
            input_mode: InputMode::Normal,
            url_input: String::new(),
            event_cursor: 0,
            popup: None,
            cal_manager_cursor: 0,
            cal_manager_mode: CalManagerMode::Normal,
            cal_manager_input: String::new(),
            cal_manager_pending_url: String::new(),
            cal_manager_color_idx: 0,
            oauth_device_code: None,
            oauth_poll_count: 0,
            oauth_account_email: None,
            oauth_calendars: Vec::new(),
            oauth_cal_cursor: 0,
            google_tokens,
            error: None,
        }
    }

    pub fn clear_status(&mut self) {
        self.status = String::new();
    }

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn day_event_indices(&self) -> &[usize] {
        self.day_map.get(&self.cursor).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn open_popup(&mut self) {
        let indices = self.day_event_indices().to_vec();
        if indices.is_empty() {
            return;
        }
        let day_pos = self.event_cursor.min(indices.len().saturating_sub(1));
        let event_idx = indices[day_pos];
        let links = self.events[event_idx]
            .description
            .as_deref()
            .map(extract_links)
            .unwrap_or_default();
        self.popup = Some(PopupState {
            event_idx,
            day_indices: indices,
            day_pos,
            scroll: 0,
            links,
            link_idx: 0,
        });
        self.input_mode = InputMode::Popup;
    }

    pub fn close_popup(&mut self) {
        self.popup = None;
        self.input_mode = InputMode::Normal;
    }

    pub fn start_url_input(&mut self) {
        // Legacy: pre-fill from first calendar URL if any
        self.url_input = self.config.calendars.first().map(|c| c.url.clone()).unwrap_or_default();
        self.input_mode = InputMode::EnteringUrl;
    }

    pub fn confirm_url_input(&mut self) {
        self.input_mode = InputMode::Normal;
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            self.config.calendars.clear();
            self.status = String::from("URL cleared");
        } else {
            if let Err(e) = Self::validate_calendar_url(&url) {
                self.set_error(format!("Invalid URL: {e}"));
                return;
            }
            if self.config.calendars.is_empty() {
                self.config.calendars.push(CalendarEntry {
                    name: "Calendar".into(),
                    url,
                    color: "blue".into(),
                    enabled: true,
                    cal_type: CalType::Ics,
                    google_account: None,
                    calendar_id: None,
                });
            } else {
                self.config.calendars[0].url = url;
            }
            if let Err(e) = save_config(&self.config) {
                self.set_error(format!("Failed to save config: {e}"));
                return;
            }
            self.reload();
        }
    }

    pub fn cancel_url_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.url_input.clear();
        self.status = String::from("Cancelled");
    }

    pub fn reload(&mut self) {
        // Collect enabled calendar info into owned data to avoid borrow issues
        let enabled: Vec<(usize, CalendarEntry)> = self.config.calendars.iter().enumerate()
            .filter(|(_, c)| c.enabled)
            .map(|(i, c)| (i, c.clone()))
            .collect();

        if enabled.is_empty() {
            self.events.clear();
            self.day_map.clear();
            self.status = String::from("No calendars enabled — press c to manage");
            return;
        }

        self.status = String::from("Loading...");
        let mut all_events = Vec::new();
        let mut loaded = 0usize;
        let mut errors = Vec::new();

        for (cal_idx, cal) in &enabled {
            let events_result = match cal.cal_type {
                CalType::Ics => {
                    fetch_ics(&cal.url).map(|raw| parse_ics(&raw))
                }
                CalType::Google => self.fetch_google_calendar(cal),
            };

            match events_result {
                Ok(mut events) => {
                    for ev in &mut events {
                        ev.calendar_id = *cal_idx;
                    }
                    loaded += events.len();
                    all_events.extend(events);
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", cal.name));
                }
            }
        }

        self.events = all_events;
        self.day_map = events_by_day(&self.events);

        if errors.is_empty() {
            self.status = format!("Loaded {} events from {} calendars", loaded, enabled.len());
        } else {
            self.set_error(format!("Loaded {} events; errors: {}", loaded, errors.join(", ")));
        }
    }

    fn fetch_google_calendar(&mut self, cal: &CalendarEntry) -> Result<Vec<CalEvent>, String> {
        let account = cal.google_account.as_deref()
            .ok_or("Google calendar missing google_account")?;
        let cal_id = cal.calendar_id.as_deref()
            .ok_or("Google calendar missing calendar_id")?;

        let tokens = self.google_tokens.get(account)
            .ok_or(format!("No tokens for account {account} — re-authenticate via calendar manager"))?
            .clone();

        // Refresh token if expired
        let now = chrono::Utc::now().timestamp();
        let access_token = if now >= tokens.expires_at - 60 {
            match oauth::refresh_access_token(&tokens.refresh_token) {
                Ok(new_token) => {
                    let expires_at = chrono::Utc::now().timestamp() + new_token.expires_in as i64;
                    let updated = GoogleTokens {
                        access_token: new_token.access_token.clone(),
                        refresh_token: new_token.refresh_token
                            .unwrap_or(tokens.refresh_token.clone()),
                        expires_at,
                    };
                    self.google_tokens.insert(account.to_string(), updated);
                    let _ = save_tokens(&self.google_tokens);
                    new_token.access_token
                }
                Err(e) => return Err(format!("Token refresh failed: {e}")),
            }
        } else {
            tokens.access_token.clone()
        };

        google::fetch_google_events(&access_token, cal_id)
    }

    // ── Calendar manager methods ─────────────────────────────────────────

    pub fn open_calendar_manager(&mut self) {
        self.input_mode = InputMode::CalendarManager;
        self.cal_manager_mode = CalManagerMode::Normal;
        self.cal_manager_cursor = 0;
        self.cal_manager_input.clear();
    }

    pub fn close_calendar_manager(&mut self) {
        self.input_mode = InputMode::Normal;
        self.cal_manager_mode = CalManagerMode::Normal;
        self.cal_manager_input.clear();
    }

    pub fn toggle_calendar(&mut self, idx: usize) {
        if let Some(cal) = self.config.calendars.get_mut(idx) {
            cal.enabled = !cal.enabled;
            let _ = save_config(&self.config);
            self.reload();
        }
    }

    pub fn remove_calendar(&mut self, idx: usize) {
        if idx < self.config.calendars.len() {
            self.config.calendars.remove(idx);
            if self.cal_manager_cursor > 0 && self.cal_manager_cursor >= self.config.calendars.len() {
                self.cal_manager_cursor = self.config.calendars.len().saturating_sub(1);
            }
            let _ = save_config(&self.config);
            self.reload();
        }
    }

    pub fn validate_calendar_url(url: &str) -> Result<(), String> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("URL must start with http:// or https://".into());
        }
        // Must have a host after the scheme
        let after_scheme = if url.starts_with("https://") {
            &url[8..]
        } else {
            &url[7..]
        };
        if after_scheme.is_empty() || after_scheme.starts_with('/') {
            return Err("URL must include a hostname".into());
        }
        // Try fetching and parsing to verify it serves valid ICS data
        let raw = fetch_ics(url)?;
        let events = parse_ics(&raw);
        if events.is_empty() && !raw.contains("VCALENDAR") {
            return Err("URL does not appear to serve a valid ICS calendar".into());
        }
        Ok(())
    }

    pub fn add_calendar(&mut self, name: String, url: String, color: String) {
        self.config.calendars.push(CalendarEntry {
            name,
            url,
            color,
            enabled: true,
            cal_type: CalType::Ics,
            google_account: None,
            calendar_id: None,
        });
        let _ = save_config(&self.config);
        self.reload();
    }

    pub fn rename_calendar(&mut self, idx: usize, name: String) {
        if let Some(cal) = self.config.calendars.get_mut(idx) {
            cal.name = name;
            let _ = save_config(&self.config);
        }
    }

    pub fn set_calendar_color(&mut self, idx: usize, color: String) {
        if let Some(cal) = self.config.calendars.get_mut(idx) {
            cal.color = color;
            let _ = save_config(&self.config);
        }
    }

    // ── OAuth / CalDAV methods ───────────────────────────────────────────

    pub fn start_add_calendar(&mut self) {
        self.cal_manager_mode = CalManagerMode::ChoosingType;
    }

    pub fn choose_ics_type(&mut self) {
        self.cal_manager_mode = CalManagerMode::AddingUrl;
        self.cal_manager_input.clear();
    }

    pub fn choose_google_type(&mut self) {
        if !oauth::credentials_configured() {
            self.set_error(String::from("Google Calendar not configured — build with GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET"));
            self.cal_manager_mode = CalManagerMode::Normal;
            return;
        }

        match oauth::request_device_code() {
            Ok(resp) => {
                crate::calendar::copy_to_clipboard(&resp.user_code);
                self.oauth_device_code = Some(resp);
                self.oauth_poll_count = 0;
                self.cal_manager_mode = CalManagerMode::OAuthShowCode;
            }
            Err(e) => {
                self.set_error(format!("OAuth error: {e}"));
                self.cal_manager_mode = CalManagerMode::Normal;
            }
        }
    }

    pub fn start_oauth_polling(&mut self) {
        // Open browser with verification URL
        if let Some(dc) = &self.oauth_device_code {
            crate::calendar::open_url(&dc.verification_url);
        }
        self.cal_manager_mode = CalManagerMode::OAuthPolling;
    }

    pub fn poll_oauth_token(&mut self) {
        let device_code = match &self.oauth_device_code {
            Some(dc) => dc.device_code.clone(),
            None => {
                self.set_error(String::from("No device code — try again"));
                self.cal_manager_mode = CalManagerMode::Normal;
                return;
            }
        };

        self.oauth_poll_count += 1;

        match oauth::poll_token(&device_code) {
            Ok(Some(token_resp)) => {
                // Got tokens — fetch user email and discover calendars
                let expires_at = chrono::Utc::now().timestamp() + token_resp.expires_in as i64;
                let refresh_token = token_resp.refresh_token.clone().unwrap_or_default();

                match oauth::fetch_user_email(&token_resp.access_token) {
                    Ok(email) => {
                        self.google_tokens.insert(email.clone(), GoogleTokens {
                            access_token: token_resp.access_token.clone(),
                            refresh_token,
                            expires_at,
                        });
                        let _ = save_tokens(&self.google_tokens);
                        self.oauth_account_email = Some(email);

                        // Discover calendars
                        match google::discover_calendars(&token_resp.access_token) {
                            Ok(cals) => {
                                self.oauth_calendars = cals;
                                self.oauth_cal_cursor = 0;
                                self.cal_manager_mode = CalManagerMode::OAuthPickCalendar;
                                self.status = String::from("Authorized — select calendars to add");
                            }
                            Err(e) => {
                                self.set_error(format!("Calendar discovery failed: {e}"));
                                self.cal_manager_mode = CalManagerMode::Normal;
                            }
                        }
                    }
                    Err(e) => {
                        self.set_error(format!("Failed to get user info: {e}"));
                        self.cal_manager_mode = CalManagerMode::Normal;
                    }
                }
            }
            Ok(None) => {
                // Still pending — stay in polling mode
            }
            Err(e) => {
                self.set_error(format!("OAuth failed: {e}"));
                self.cal_manager_mode = CalManagerMode::Normal;
                self.oauth_device_code = None;
            }
        }
    }

    pub fn add_google_calendar(&mut self) {
        if self.oauth_calendars.is_empty() {
            return;
        }
        let cal_info = &self.oauth_calendars[self.oauth_cal_cursor];
        let email = self.oauth_account_email.clone().unwrap_or_default();

        // Check if already added
        let already_exists = self.config.calendars.iter().any(|c| {
            c.cal_type == CalType::Google
                && c.google_account.as_deref() == Some(&email)
                && c.calendar_id.as_deref() == Some(&cal_info.id)
        });

        if already_exists {
            self.status = format!("'{}' is already added", cal_info.display_name);
            return;
        }

        let color_idx = self.config.calendars.len() % COLOR_PALETTE.len();
        self.config.calendars.push(CalendarEntry {
            name: cal_info.display_name.clone(),
            url: String::new(),
            color: COLOR_PALETTE[color_idx].to_string(),
            enabled: true,
            cal_type: CalType::Google,
            google_account: Some(email),
            calendar_id: Some(cal_info.id.clone()),
        });
        let _ = save_config(&self.config);
        self.status = format!("Added '{}'", cal_info.display_name);
        self.reload();
    }

    pub fn cancel_oauth(&mut self) {
        self.oauth_device_code = None;
        self.oauth_poll_count = 0;
        self.oauth_account_email = None;
        self.oauth_calendars.clear();
        self.oauth_cal_cursor = 0;
        self.cal_manager_mode = CalManagerMode::Normal;
        self.status = String::from("OAuth cancelled");
    }

    pub fn finish_oauth_pick(&mut self) {
        self.oauth_device_code = None;
        self.oauth_poll_count = 0;
        self.oauth_account_email = None;
        self.oauth_calendars.clear();
        self.oauth_cal_cursor = 0;
        self.cal_manager_mode = CalManagerMode::Normal;
        self.reload();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveTime};
    use crate::calendar::CalEvent;

    fn make_app() -> App {
        App::new(Config::default())
    }

    fn make_app_with_events() -> App {
        let mut app = make_app();
        let date = app.cursor;
        app.events = vec![
            CalEvent {
                calendar_id: 0,
                summary: "Event A".into(),
                start: date,
                start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
                end: date + Duration::days(1),
                end_time: Some(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                description: Some("Details at https://example.com".into()),
                location: Some("Room 1".into()),
            },
            CalEvent {
                calendar_id: 0,
                summary: "Event B".into(),
                start: date,
                start_time: None,
                end: date + Duration::days(1),
                end_time: None,
                description: None,
                location: None,
            },
        ];
        app.day_map = events_by_day(&app.events);
        app
    }

    // ── App::new ─────────────────────────────────────────────────────────

    #[test]
    fn new_defaults() {
        let app = make_app();
        assert_eq!(app.view, ViewMode::Month);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.show_events);
        assert!(app.events.is_empty());
        assert_eq!(app.event_cursor, 0);
        assert!(app.popup.is_none());
    }

    #[test]
    fn new_with_calendars_shows_loading() {
        let config = Config {
            ics_url: None,
            calendars: vec![CalendarEntry {
                name: "Test".into(),
                url: "https://example.com".into(),
                color: "blue".into(),
                enabled: true,
                cal_type: CalType::Ics,
                google_account: None,
                calendar_id: None,
            }],
        };
        let app = App::new(config);
        assert_eq!(app.status, "Loading...");
    }

    #[test]
    fn new_without_calendars_shows_prompt() {
        let app = make_app();
        assert!(app.status.contains("calendars"));
    }

    // ── day_event_indices ────────────────────────────────────────────────

    #[test]
    fn day_event_indices_empty() {
        let app = make_app();
        assert!(app.day_event_indices().is_empty());
    }

    #[test]
    fn day_event_indices_with_events() {
        let app = make_app_with_events();
        assert_eq!(app.day_event_indices().len(), 2);
    }

    // ── open_popup / close_popup ─────────────────────────────────────────

    #[test]
    fn open_popup_with_events() {
        let mut app = make_app_with_events();
        app.open_popup();
        assert_eq!(app.input_mode, InputMode::Popup);
        assert!(app.popup.is_some());
        let popup = app.popup.as_ref().unwrap();
        assert_eq!(popup.scroll, 0);
        assert_eq!(popup.day_pos, 0);
    }

    #[test]
    fn open_popup_without_events_is_noop() {
        let mut app = make_app();
        app.open_popup();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.popup.is_none());
    }

    #[test]
    fn open_popup_extracts_links() {
        let mut app = make_app_with_events();
        app.event_cursor = 0;
        app.open_popup();
        let popup = app.popup.as_ref().unwrap();
        assert!(popup.links.is_empty());
    }

    #[test]
    fn close_popup_resets_state() {
        let mut app = make_app_with_events();
        app.open_popup();
        app.close_popup();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.popup.is_none());
    }

    // ── URL input ────────────────────────────────────────────────────────

    #[test]
    fn start_url_input_enters_mode() {
        let mut app = make_app();
        app.start_url_input();
        assert_eq!(app.input_mode, InputMode::EnteringUrl);
    }

    #[test]
    fn start_url_input_prefills_existing() {
        let config = Config {
            ics_url: None,
            calendars: vec![CalendarEntry {
                name: "Test".into(),
                url: "https://cal.test".into(),
                color: "blue".into(),
                enabled: true,
                cal_type: CalType::Ics,
                google_account: None,
                calendar_id: None,
            }],
        };
        let mut app = App::new(config);
        app.start_url_input();
        assert_eq!(app.url_input, "https://cal.test");
    }

    #[test]
    fn cancel_url_input_returns_to_normal() {
        let mut app = make_app();
        app.start_url_input();
        app.url_input.push_str("https://test.com");
        app.cancel_url_input();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.url_input.is_empty());
        assert_eq!(app.status, "Cancelled");
    }

    // ── Calendar manager ─────────────────────────────────────────────────

    #[test]
    fn open_calendar_manager_sets_mode() {
        let mut app = make_app();
        app.open_calendar_manager();
        assert_eq!(app.input_mode, InputMode::CalendarManager);
        assert_eq!(app.cal_manager_mode, CalManagerMode::Normal);
    }

    #[test]
    fn close_calendar_manager_returns_to_normal() {
        let mut app = make_app();
        app.open_calendar_manager();
        app.close_calendar_manager();
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn toggle_calendar_flips_enabled() {
        let mut app = make_app();
        app.config.calendars.push(CalendarEntry {
            name: "Test".into(),
            url: "https://example.com".into(),
            color: "blue".into(),
            enabled: true,
            cal_type: CalType::Ics,
            google_account: None,
            calendar_id: None,
        });
        app.toggle_calendar(0);
        assert!(!app.config.calendars[0].enabled);
        app.toggle_calendar(0);
        assert!(app.config.calendars[0].enabled);
    }

    #[test]
    fn remove_calendar_removes_entry() {
        let mut app = make_app();
        app.config.calendars.push(CalendarEntry {
            name: "A".into(),
            url: "https://a.com".into(),
            color: "red".into(),
            enabled: true,
            cal_type: CalType::Ics,
            google_account: None,
            calendar_id: None,
        });
        app.config.calendars.push(CalendarEntry {
            name: "B".into(),
            url: "https://b.com".into(),
            color: "green".into(),
            enabled: true,
            cal_type: CalType::Ics,
            google_account: None,
            calendar_id: None,
        });
        app.remove_calendar(0);
        assert_eq!(app.config.calendars.len(), 1);
        assert_eq!(app.config.calendars[0].name, "B");
    }

    #[test]
    fn rename_calendar_updates_name() {
        let mut app = make_app();
        app.config.calendars.push(CalendarEntry {
            name: "Old".into(),
            url: "https://example.com".into(),
            color: "blue".into(),
            enabled: true,
            cal_type: CalType::Ics,
            google_account: None,
            calendar_id: None,
        });
        app.rename_calendar(0, "New".into());
        assert_eq!(app.config.calendars[0].name, "New");
    }

    #[test]
    fn set_calendar_color_updates_color() {
        let mut app = make_app();
        app.config.calendars.push(CalendarEntry {
            name: "Test".into(),
            url: "https://example.com".into(),
            color: "blue".into(),
            enabled: true,
            cal_type: CalType::Ics,
            google_account: None,
            calendar_id: None,
        });
        app.set_calendar_color(0, "red".into());
        assert_eq!(app.config.calendars[0].color, "red");
    }

    // ── URL validation ──────────────────────────────────────────────────

    #[test]
    fn validate_rejects_empty_url() {
        let result = App::validate_calendar_url("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("http"));
    }

    #[test]
    fn validate_rejects_non_http_scheme() {
        assert!(App::validate_calendar_url("ftp://example.com").is_err());
        assert!(App::validate_calendar_url("file:///etc/passwd").is_err());
        assert!(App::validate_calendar_url("not-a-url").is_err());
    }

    #[test]
    fn validate_rejects_scheme_only() {
        assert!(App::validate_calendar_url("http://").is_err());
        assert!(App::validate_calendar_url("https://").is_err());
    }

    #[test]
    fn validate_rejects_scheme_with_path_only() {
        assert!(App::validate_calendar_url("http:///path").is_err());
        assert!(App::validate_calendar_url("https:///path").is_err());
    }

    #[test]
    fn validate_rejects_invalid_url_before_add() {
        let result = App::validate_calendar_url("not-a-url");
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_unreachable_url() {
        let result = App::validate_calendar_url(
            "https://this-domain-does-not-exist-999.invalid/cal.ics",
        );
        assert!(result.is_err());
    }

    // ── OAuth flow states ────────────────────────────────────────────────

    #[test]
    fn start_add_calendar_enters_choosing_type() {
        let mut app = make_app();
        app.open_calendar_manager();
        app.start_add_calendar();
        assert_eq!(app.cal_manager_mode, CalManagerMode::ChoosingType);
    }

    #[test]
    fn choose_ics_type_enters_adding_url() {
        let mut app = make_app();
        app.open_calendar_manager();
        app.start_add_calendar();
        app.choose_ics_type();
        assert_eq!(app.cal_manager_mode, CalManagerMode::AddingUrl);
        assert!(app.cal_manager_input.is_empty());
    }

    #[test]
    fn cancel_oauth_resets_state() {
        let mut app = make_app();
        app.oauth_device_code = Some(DeviceCodeResponse {
            device_code: "test".into(),
            user_code: "TEST-CODE".into(),
            verification_url: "https://example.com".into(),
            expires_in: 300,
            interval: 5,
        });
        app.cal_manager_mode = CalManagerMode::OAuthPolling;
        app.cancel_oauth();
        assert!(app.oauth_device_code.is_none());
        assert_eq!(app.cal_manager_mode, CalManagerMode::Normal);
        assert_eq!(app.status, "OAuth cancelled");
    }

    #[test]
    fn add_google_calendar_prevents_duplicates() {
        let mut app = make_app();
        app.oauth_account_email = Some("user@gmail.com".into());
        app.oauth_calendars = vec![CalendarInfo {
            id: "user@gmail.com".into(),
            display_name: "My Calendar".into(),
        }];
        app.oauth_cal_cursor = 0;

        // Add once
        app.add_google_calendar();
        assert_eq!(app.config.calendars.len(), 1);
        assert_eq!(app.config.calendars[0].cal_type, CalType::Google);

        // Try to add again — should be prevented
        app.add_google_calendar();
        assert_eq!(app.config.calendars.len(), 1);
        assert!(app.status.contains("already added"));
    }

    #[test]
    fn finish_oauth_pick_resets_state() {
        let mut app = make_app();
        app.oauth_calendars = vec![CalendarInfo {
            id: "test".into(),
            display_name: "Test".into(),
        }];
        app.cal_manager_mode = CalManagerMode::OAuthPickCalendar;
        app.finish_oauth_pick();
        assert!(app.oauth_calendars.is_empty());
        assert_eq!(app.cal_manager_mode, CalManagerMode::Normal);
    }
}
