use std::collections::HashMap;

use chrono::{Local, NaiveDate};

use crate::calendar::{extract_links, events_by_day, fetch_ics, parse_ics, CalEvent};
use crate::config::{save_config, Config};

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
}

pub struct PopupState {
    pub event_idx: usize,
    pub day_indices: Vec<usize>,
    pub day_pos: usize,
    pub scroll: u16,
    pub links: Vec<String>,
    pub link_idx: usize,
}

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
}

impl App {
    pub fn new(config: Config) -> Self {
        let today = Local::now().date_naive();
        let has_url = config.ics_url.is_some();
        App {
            config,
            events: Vec::new(),
            day_map: HashMap::new(),
            cursor: today,
            view: ViewMode::Month,
            show_events: true,
            status: if has_url {
                String::from("Loading...")
            } else {
                String::from("Press r to set ICS URL")
            },
            input_mode: InputMode::Normal,
            url_input: String::new(),
            event_cursor: 0,
            popup: None,
        }
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
        self.url_input = self.config.ics_url.clone().unwrap_or_default();
        self.input_mode = InputMode::EnteringUrl;
    }

    pub fn confirm_url_input(&mut self) {
        self.input_mode = InputMode::Normal;
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            self.config.ics_url = None;
            self.status = String::from("URL cleared");
        } else {
            self.config.ics_url = Some(url);
            if let Err(e) = save_config(&self.config) {
                self.status = format!("Failed to save config: {e}");
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
        if let Some(url) = self.config.ics_url.clone() {
            self.status = String::from("Loading...");
            match fetch_ics(&url) {
                Ok(raw) => {
                    self.events = parse_ics(&raw);
                    self.day_map = events_by_day(&self.events);
                    self.status = format!("Loaded {} events", self.events.len());
                }
                Err(e) => {
                    self.status = format!("Error: {e}");
                }
            }
        } else {
            self.status = String::from("No ICS URL set — press r to add one");
        }
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
                summary: "Event A".into(),
                start: date,
                start_time: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
                end: date + Duration::days(1),
                end_time: Some(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                description: Some("Details at https://example.com".into()),
                location: Some("Room 1".into()),
            },
            CalEvent {
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
    fn new_with_url_shows_loading() {
        let config = Config { ics_url: Some("https://example.com".into()) };
        let app = App::new(config);
        assert_eq!(app.status, "Loading...");
    }

    #[test]
    fn new_without_url_shows_prompt() {
        let app = make_app();
        assert!(app.status.contains("Press r"));
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
        // Event A (timed) has a description with a link; Event B (all-day) is first
        // All-day sorts first, so day_pos=0 -> Event B (no links)
        app.event_cursor = 0;
        app.open_popup();
        let popup = app.popup.as_ref().unwrap();
        // Event B (all-day, no description) comes first
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
        let config = Config { ics_url: Some("https://cal.test".into()) };
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
}
