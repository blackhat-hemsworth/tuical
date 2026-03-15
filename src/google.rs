use chrono::{DateTime, NaiveDate};
use serde::Deserialize;

use crate::calendar::{strip_html, CalEvent};

const CALENDAR_LIST_URL: &str = "https://www.googleapis.com/calendar/v3/users/me/calendarList";
const EVENTS_BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";

#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub id: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct CalendarListResponse {
    items: Vec<CalendarListEntry>,
}

#[derive(Deserialize)]
struct CalendarListEntry {
    id: String,
    summary: String,
}

/// Discover calendars via the Google Calendar REST API (calendarList).
pub fn discover_calendars(access_token: &str) -> Result<Vec<CalendarInfo>, String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(CALENDAR_LIST_URL)
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("Calendar list request failed: {e}"))?;

    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Failed to read response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Calendar list failed ({}): {}", status, body));
    }

    let list: CalendarListResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse calendar list: {e}"))?;

    Ok(list.items.into_iter().map(|e| CalendarInfo {
        id: e.id,
        display_name: e.summary,
    }).collect())
}

#[derive(Deserialize)]
struct EventsResponse {
    #[serde(default)]
    items: Vec<GoogleEvent>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct GoogleEvent {
    #[serde(default)]
    summary: Option<String>,
    start: Option<EventDateTime>,
    end: Option<EventDateTime>,
    description: Option<String>,
    location: Option<String>,
}

#[derive(Deserialize)]
struct EventDateTime {
    date: Option<String>,
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
}

/// Fetch events from a Google Calendar via the REST API.
pub fn fetch_google_events(access_token: &str, calendar_id: &str) -> Result<Vec<CalEvent>, String> {
    let client = reqwest::blocking::Client::new();
    let mut all_events = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let url = format!("{}/{}/events", EVENTS_BASE, urlencoded(calendar_id));
        let mut req = client
            .get(&url)
            .bearer_auth(access_token)
            .query(&[
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
                ("maxResults", "2500"),
            ]);

        if let Some(token) = &page_token {
            req = req.query(&[("pageToken", token.as_str())]);
        }

        let resp = req
            .send()
            .map_err(|e| format!("Google events request failed: {e}"))?;

        let status = resp.status();
        let body = resp.text().map_err(|e| format!("Failed to read events response: {e}"))?;

        if !status.is_success() {
            return Err(format!("Google events failed ({}): {}", status, body));
        }

        let events_resp: EventsResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse events: {e}"))?;

        for ge in &events_resp.items {
            if let Some(ev) = convert_google_event(ge) {
                all_events.push(ev);
            }
        }

        match events_resp.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }

    Ok(all_events)
}

fn convert_google_event(ge: &GoogleEvent) -> Option<CalEvent> {
    let start_dt = ge.start.as_ref()?;
    let end_dt = ge.end.as_ref()?;

    let (start_date, start_time) = parse_event_datetime(start_dt)?;
    let (end_date, end_time) = parse_event_datetime(end_dt)?;

    // Ensure end > start so events_by_day includes this event
    let end_date = if end_date <= start_date { start_date + chrono::Duration::days(1) } else { end_date };

    let summary = ge.summary.clone().unwrap_or_default();
    let description = ge.description.as_deref().map(strip_html);
    let location = ge.location.clone();

    Some(CalEvent {
        calendar_id: 0,
        summary,
        start: start_date,
        start_time,
        end: end_date,
        end_time,
        description,
        location,
    })
}

fn parse_event_datetime(dt: &EventDateTime) -> Option<(NaiveDate, Option<chrono::NaiveTime>)> {
    if let Some(date_str) = &dt.date {
        // All-day event
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
        Some((date, None))
    } else if let Some(dt_str) = &dt.date_time {
        // Timed event — parse RFC3339
        let parsed = DateTime::parse_from_rfc3339(dt_str).ok()?;
        let local = parsed.naive_local();
        Some((local.date(), Some(local.time())))
    } else {
        None
    }
}

fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace('@', "%40")
        .replace('#', "%23")
        .replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_list_response_parses() {
        let json = r#"{
            "kind": "calendar#calendarList",
            "items": [
                {"id": "user@gmail.com", "summary": "My Calendar"},
                {"id": "en.usa#holiday@group.v.calendar.google.com", "summary": "Holidays in US"}
            ]
        }"#;
        let resp: CalendarListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.items.len(), 2);
        assert_eq!(resp.items[0].id, "user@gmail.com");
        assert_eq!(resp.items[0].summary, "My Calendar");
        assert_eq!(resp.items[1].summary, "Holidays in US");
    }

    #[test]
    fn calendar_list_response_empty() {
        let json = r#"{"kind": "calendar#calendarList", "items": []}"#;
        let resp: CalendarListResponse = serde_json::from_str(json).unwrap();
        assert!(resp.items.is_empty());
    }

    #[test]
    fn urlencoded_escapes_special_chars() {
        assert_eq!(urlencoded("user@gmail.com"), "user%40gmail.com");
        assert_eq!(urlencoded("noemail"), "noemail");
        assert_eq!(urlencoded("a#b"), "a%23b");
    }

    #[test]
    fn events_response_parses_timed_event() {
        let json = r#"{
            "items": [
                {
                    "summary": "Meeting",
                    "start": {"dateTime": "2025-03-15T10:00:00-04:00"},
                    "end": {"dateTime": "2025-03-15T11:00:00-04:00"},
                    "description": "<b>Notes</b>",
                    "location": "Room 1"
                }
            ]
        }"#;
        let resp: EventsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.items.len(), 1);
        let events: Vec<CalEvent> = resp.items.iter().filter_map(convert_google_event).collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Meeting");
        assert!(events[0].start_time.is_some());
        assert_eq!(events[0].start, NaiveDate::from_ymd_opt(2025, 3, 15).unwrap());
        // end must be > start so events_by_day includes it
        assert!(events[0].end > events[0].start);
        assert_eq!(events[0].description.as_deref(), Some("Notes"));
        assert_eq!(events[0].location.as_deref(), Some("Room 1"));
    }

    #[test]
    fn events_response_parses_allday_event() {
        let json = r#"{
            "items": [
                {
                    "summary": "All Day Event",
                    "start": {"date": "2025-03-16"},
                    "end": {"date": "2025-03-17"}
                }
            ]
        }"#;
        let resp: EventsResponse = serde_json::from_str(json).unwrap();
        let events: Vec<CalEvent> = resp.items.iter().filter_map(convert_google_event).collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "All Day Event");
        assert!(events[0].start_time.is_none());
        assert!(events[0].end_time.is_none());
        assert_eq!(events[0].start, NaiveDate::from_ymd_opt(2025, 3, 16).unwrap());
        assert_eq!(events[0].end, NaiveDate::from_ymd_opt(2025, 3, 17).unwrap());
    }

    #[test]
    fn events_response_empty() {
        let json = r#"{"items": []}"#;
        let resp: EventsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.items.is_empty());
        assert!(resp.next_page_token.is_none());
    }

    #[test]
    fn events_response_with_pagination() {
        let json = r#"{"items": [], "nextPageToken": "abc123"}"#;
        let resp: EventsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.next_page_token.as_deref(), Some("abc123"));
    }

    #[test]
    fn events_response_skips_missing_fields() {
        let json = r#"{
            "items": [
                {
                    "start": {"date": "2025-03-16"},
                    "end": {"date": "2025-03-17"}
                }
            ]
        }"#;
        let resp: EventsResponse = serde_json::from_str(json).unwrap();
        let events: Vec<CalEvent> = resp.items.iter().filter_map(convert_google_event).collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "");
        assert!(events[0].description.is_none());
        assert!(events[0].location.is_none());
    }

    #[test]
    fn events_response_strips_html_from_description() {
        let json = r#"{
            "items": [
                {
                    "summary": "Test",
                    "start": {"date": "2025-03-16"},
                    "end": {"date": "2025-03-17"},
                    "description": "<p>Hello &amp; world</p>"
                }
            ]
        }"#;
        let resp: EventsResponse = serde_json::from_str(json).unwrap();
        let events: Vec<CalEvent> = resp.items.iter().filter_map(convert_google_event).collect();
        assert_eq!(events[0].description.as_deref(), Some("Hello & world"));
    }
}
