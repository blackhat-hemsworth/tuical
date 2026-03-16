use chrono::{DateTime, NaiveDate};
use serde::Deserialize;

use crate::calendar::{strip_html, CalEvent, RsvpStatus};

const CALENDAR_LIST_URL: &str = "https://www.googleapis.com/calendar/v3/users/me/calendarList";
const EVENTS_BASE: &str = "https://www.googleapis.com/calendar/v3/calendars";

#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub id: String,
    pub display_name: String,
    pub access_role: Option<String>,
}

#[derive(Deserialize)]
struct CalendarListResponse {
    items: Vec<CalendarListEntry>,
}

#[derive(Deserialize)]
struct CalendarListEntry {
    id: String,
    summary: String,
    #[serde(rename = "accessRole")]
    access_role: Option<String>,
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
        access_role: e.access_role,
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
    id: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    start: Option<EventDateTime>,
    end: Option<EventDateTime>,
    description: Option<String>,
    location: Option<String>,
    #[serde(default)]
    attendees: Vec<GoogleAttendee>,
}

#[derive(Deserialize)]
struct GoogleAttendee {
    #[serde(rename = "responseStatus")]
    response_status: Option<String>,
    #[serde(rename = "self", default)]
    is_self: bool,
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

    let rsvp_status = ge.attendees.iter()
        .find(|a| a.is_self)
        .and_then(|a| a.response_status.as_deref())
        .and_then(RsvpStatus::from_google);

    Some(CalEvent {
        calendar_id: 0,
        summary,
        start: start_date,
        start_time,
        end: end_date,
        end_time,
        description,
        location,
        google_event_id: ge.id.clone(),
        rsvp_status,
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

fn build_event_body(
    summary: &str,
    description: &str,
    start_date: chrono::NaiveDate,
    start_time: Option<chrono::NaiveTime>,
    end_date: chrono::NaiveDate,
    end_time: Option<chrono::NaiveTime>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "summary": summary,
        "description": description,
    });

    if let Some(st) = start_time {
        let offset = chrono::Local::now().offset().clone();
        let start_dt = chrono::NaiveDateTime::new(start_date, st);
        let start_rfc = chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
            start_dt - offset, offset
        ).to_rfc3339();
        body["start"] = serde_json::json!({"dateTime": start_rfc});

        let end_t = end_time.unwrap_or(st + chrono::Duration::hours(1));
        let actual_end_date = if end_time.is_some() && end_t <= st {
            end_date + chrono::Duration::days(1)
        } else {
            end_date
        };
        let end_dt = chrono::NaiveDateTime::new(actual_end_date, end_t);
        let end_rfc = chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
            end_dt - offset, offset
        ).to_rfc3339();
        body["end"] = serde_json::json!({"dateTime": end_rfc});
    } else {
        body["start"] = serde_json::json!({"date": start_date.format("%Y-%m-%d").to_string()});
        let end = end_date + chrono::Duration::days(1);
        body["end"] = serde_json::json!({"date": end.format("%Y-%m-%d").to_string()});
    }

    body
}

pub fn create_google_event(
    access_token: &str,
    calendar_id: &str,
    summary: &str,
    description: &str,
    start_date: chrono::NaiveDate,
    start_time: Option<chrono::NaiveTime>,
    end_date: chrono::NaiveDate,
    end_time: Option<chrono::NaiveTime>,
) -> Result<String, String> {
    let body = build_event_body(summary, description, start_date, start_time, end_date, end_time);
    let url = format!("{}/{}/events", EVENTS_BASE, urlencoded(calendar_id));
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .map_err(|e| format!("Create event request failed: {e}"))?;

    let status = resp.status();
    let resp_body = resp.text().map_err(|e| format!("Failed to read response: {e}"))?;
    if !status.is_success() {
        return Err(format!("Create event failed ({}): {}", status, resp_body));
    }

    let parsed: serde_json::Value = serde_json::from_str(&resp_body)
        .map_err(|e| format!("Failed to parse response: {e}"))?;
    Ok(parsed["id"].as_str().unwrap_or("").to_string())
}

pub fn update_google_event(
    access_token: &str,
    calendar_id: &str,
    event_id: &str,
    summary: &str,
    description: &str,
    start_date: chrono::NaiveDate,
    start_time: Option<chrono::NaiveTime>,
    end_date: chrono::NaiveDate,
    end_time: Option<chrono::NaiveTime>,
) -> Result<(), String> {
    let body = build_event_body(summary, description, start_date, start_time, end_date, end_time);
    let url = format!("{}/{}/events/{}", EVENTS_BASE, urlencoded(calendar_id), urlencoded(event_id));
    let client = reqwest::blocking::Client::new();
    let resp = client
        .put(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .map_err(|e| format!("Update event request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let resp_body = resp.text().unwrap_or_default();
        return Err(format!("Update event failed ({}): {}", status, resp_body));
    }
    Ok(())
}

pub fn delete_google_event(
    access_token: &str,
    calendar_id: &str,
    event_id: &str,
) -> Result<(), String> {
    let url = format!("{}/{}/events/{}", EVENTS_BASE, urlencoded(calendar_id), urlencoded(event_id));
    let client = reqwest::blocking::Client::new();
    let resp = client
        .delete(&url)
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("Delete event request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 204 {
        let resp_body = resp.text().unwrap_or_default();
        return Err(format!("Delete event failed ({}): {}", status, resp_body));
    }
    Ok(())
}

pub fn rsvp_google_event(
    access_token: &str,
    calendar_id: &str,
    event_id: &str,
    status: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/{}/events/{}",
        EVENTS_BASE,
        urlencoded(calendar_id),
        urlencoded(event_id)
    );
    let client = reqwest::blocking::Client::new();

    // Fetch current event to get full attendees list
    let get_resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("Failed to fetch event for RSVP: {e}"))?;

    if !get_resp.status().is_success() {
        let body = get_resp.text().unwrap_or_default();
        return Err(format!("Failed to fetch event: {body}"));
    }

    let event: serde_json::Value = get_resp
        .json()
        .map_err(|e| format!("Failed to parse event: {e}"))?;

    // Update self-attendee's responseStatus
    let mut attendees = event["attendees"].as_array().cloned().unwrap_or_default();
    for att in &mut attendees {
        if att["self"].as_bool() == Some(true) {
            att["responseStatus"] = serde_json::json!(status);
        }
    }

    let patch_body = serde_json::json!({ "attendees": attendees });

    let resp = client
        .patch(&url)
        .bearer_auth(access_token)
        .query(&[("sendUpdates", "none")])
        .json(&patch_body)
        .send()
        .map_err(|e| format!("RSVP request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("RSVP failed: {body}"));
    }

    Ok(())
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
