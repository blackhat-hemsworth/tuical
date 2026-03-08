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
