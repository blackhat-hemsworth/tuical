use std::collections::HashMap;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};
use icalendar::{Calendar, CalendarComponent, Component, EventLike};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RsvpStatus {
    Accepted,
    Declined,
    Tentative,
    NeedsAction,
}

impl RsvpStatus {
    pub fn from_google(s: &str) -> Option<Self> {
        match s {
            "accepted" => Some(Self::Accepted),
            "declined" => Some(Self::Declined),
            "tentative" => Some(Self::Tentative),
            "needsAction" => Some(Self::NeedsAction),
            _ => None,
        }
    }

    pub fn from_ics(s: &str) -> Option<Self> {
        match s {
            "ACCEPTED" => Some(Self::Accepted),
            "DECLINED" => Some(Self::Declined),
            "TENTATIVE" => Some(Self::Tentative),
            "NEEDS-ACTION" => Some(Self::NeedsAction),
            _ => None,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Accepted => "✓",
            Self::Declined => "✗",
            Self::Tentative => "?",
            Self::NeedsAction => "·",
        }
    }

    pub fn as_google(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Tentative => "tentative",
            Self::NeedsAction => "needsAction",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CalEvent {
    pub calendar_id: usize,
    pub summary: String,
    pub start: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end: NaiveDate,
    pub end_time: Option<NaiveTime>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub google_event_id: Option<String>,
    pub rsvp_status: Option<RsvpStatus>,
}

pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    // Collapse runs of blank lines down to one
    let mut result = String::new();
    let mut prev_blank = false;
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
            }
            prev_blank = true;
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }
    result.trim().to_string()
}

pub fn extract_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with("http://") || text[i..].starts_with("https://") {
            let rest = &text[i..];
            let len = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']'))
                .unwrap_or(rest.len());
            let url = rest[..len].trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
            if !url.is_empty() {
                links.push(url.to_string());
            }
            i += len;
        } else {
            i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
    }
    links.dedup();
    links
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
}

pub fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", &format!("echo {} | clip", text)])
        .spawn();
}

pub fn fetch_ics(url: &str) -> Result<String, String> {
    reqwest::blocking::get(url)
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())
}

pub fn parse_ics(raw: &str) -> Vec<CalEvent> {
    let cal: Calendar = raw.parse().unwrap_or_default();
    let mut events = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for component in cal.iter() {
        if let CalendarComponent::Event(ev) = component {
            let summary = ev.get_summary().unwrap_or("(no title)").to_string();

            let start_dt = ev.get_start().and_then(date_time_from_dpt);
            let end_dt = ev.get_end().and_then(date_time_from_dpt);

            if let Some((start, start_time)) = start_dt {
                let key = (summary.clone(), start, start_time);
                if !seen.insert(key) {
                    continue;
                }
                let end = end_dt.map(|(d, _)| d).unwrap_or(start + Duration::days(1));
                let end = if end <= start { start + Duration::days(1) } else { end };
                let end_time = end_dt.and_then(|(_, t)| t);
                // Extract RSVP status from ATTENDEE properties
                let rsvp_status = ev.multi_properties()
                    .get("ATTENDEE")
                    .and_then(|attendees| {
                        attendees.iter().find_map(|prop| {
                            prop.params()
                                .get("PARTSTAT")
                                .and_then(|p| RsvpStatus::from_ics(p.value()))
                        })
                    });

                events.push(CalEvent {
                    calendar_id: 0,
                    summary,
                    start,
                    start_time,
                    end,
                    end_time,
                    description: ev.get_description().map(|d| strip_html(d)),
                    location: ev.get_location().map(str::to_string),
                    google_event_id: None,
                    rsvp_status,
                });
            }
        }
    }

    events
}

fn date_time_from_dpt(dt: icalendar::DatePerhapsTime) -> Option<(NaiveDate, Option<NaiveTime>)> {
    use icalendar::{CalendarDateTime, DatePerhapsTime};
    match dt {
        DatePerhapsTime::Date(d) => {
            let date = NaiveDate::from_ymd_opt(d.year().into(), d.month().into(), d.day().into())?;
            Some((date, None))
        }
        DatePerhapsTime::DateTime(cdt) => match cdt {
            CalendarDateTime::Floating(ndt) => Some((ndt.date(), Some(ndt.time()))),
            CalendarDateTime::Utc(utc) => {
                let local = utc.with_timezone(&Local);
                Some((local.date_naive(), Some(local.time())))
            }
            CalendarDateTime::WithTimezone { date_time, .. } => {
                Some((date_time.date(), Some(date_time.time())))
            }
        },
    }
}

pub fn events_by_day(events: &[CalEvent]) -> HashMap<NaiveDate, Vec<usize>> {
    let mut map: HashMap<NaiveDate, Vec<usize>> = HashMap::new();
    for (i, ev) in events.iter().enumerate() {
        let mut day = ev.start;
        while day < ev.end {
            map.entry(day).or_default().push(i);
            day += Duration::days(1);
        }
    }
    // Sort each day's events: all-day (None) first, then by start time ascending
    for indices in map.values_mut() {
        indices.sort_by_key(|&i| events[i].start_time);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_html ───────────────────────────────────────────────────────

    #[test]
    fn strip_html_removes_tags() {
        assert_eq!(strip_html("<b>bold</b> text"), "bold text");
    }

    #[test]
    fn strip_html_decodes_entities() {
        assert_eq!(strip_html("a &amp; b &lt; c &gt; d"), "a & b < c > d");
        assert_eq!(strip_html("&quot;hi&quot;"), "\"hi\"");
        assert_eq!(strip_html("it&#39;s &apos;fine&apos;"), "it's 'fine'");
        assert_eq!(strip_html("no&nbsp;break"), "no break");
    }

    #[test]
    fn strip_html_collapses_blank_lines() {
        let input = "line1\n\n\n\nline2\n\n\nline3";
        let result = strip_html(input);
        assert_eq!(result, "line1\n\nline2\n\nline3");
    }

    #[test]
    fn strip_html_trims_result() {
        assert_eq!(strip_html("  hello  "), "hello");
    }

    #[test]
    fn strip_html_nested_tags() {
        assert_eq!(strip_html("<div><p>nested</p></div>"), "nested");
    }

    // ── extract_links ────────────────────────────────────────────────────

    #[test]
    fn extract_links_finds_urls() {
        let text = "Visit https://example.com for info";
        assert_eq!(extract_links(text), vec!["https://example.com"]);
    }

    #[test]
    fn extract_links_http_and_https() {
        let text = "http://a.com and https://b.com";
        assert_eq!(
            extract_links(text),
            vec!["http://a.com", "https://b.com"]
        );
    }

    #[test]
    fn extract_links_trims_trailing_punctuation() {
        let text = "See https://example.com/page.";
        assert_eq!(extract_links(text), vec!["https://example.com/page"]);
    }

    #[test]
    fn extract_links_stops_at_delimiters() {
        let text = "link: <https://example.com> done";
        assert_eq!(extract_links(text), vec!["https://example.com"]);
    }

    #[test]
    fn extract_links_deduplicates() {
        let text = "https://a.com https://a.com";
        assert_eq!(extract_links(text), vec!["https://a.com"]);
    }

    #[test]
    fn extract_links_empty_input() {
        assert!(extract_links("").is_empty());
        assert!(extract_links("no links here").is_empty());
    }

    // ── parse_ics ────────────────────────────────────────────────────────

    #[test]
    fn parse_ics_basic_event() {
        let ics = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
SUMMARY:Test Event\r
DTSTART;VALUE=DATE:20250315\r
DTEND;VALUE=DATE:20250316\r
END:VEVENT\r
END:VCALENDAR";
        let events = parse_ics(ics);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Test Event");
        assert_eq!(events[0].start, NaiveDate::from_ymd_opt(2025, 3, 15).unwrap());
        assert!(events[0].start_time.is_none());
    }

    #[test]
    fn parse_ics_with_description_and_location() {
        let ics = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
SUMMARY:Meeting\r
DTSTART;VALUE=DATE:20250401\r
DTEND;VALUE=DATE:20250402\r
DESCRIPTION:<b>Important</b> meeting &amp; notes\r
LOCATION:Room 42\r
END:VEVENT\r
END:VCALENDAR";
        let events = parse_ics(ics);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].description.as_deref(), Some("Important meeting & notes"));
        assert_eq!(events[0].location.as_deref(), Some("Room 42"));
    }

    #[test]
    fn parse_ics_deduplicates() {
        let ics = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
SUMMARY:Dup\r
DTSTART;VALUE=DATE:20250401\r
DTEND;VALUE=DATE:20250402\r
END:VEVENT\r
BEGIN:VEVENT\r
SUMMARY:Dup\r
DTSTART;VALUE=DATE:20250401\r
DTEND;VALUE=DATE:20250402\r
END:VEVENT\r
END:VCALENDAR";
        assert_eq!(parse_ics(ics).len(), 1);
    }

    #[test]
    fn parse_ics_empty_input() {
        assert!(parse_ics("").is_empty());
    }

    #[test]
    fn parse_ics_no_title() {
        let ics = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
DTSTART;VALUE=DATE:20250501\r
DTEND;VALUE=DATE:20250502\r
END:VEVENT\r
END:VCALENDAR";
        let events = parse_ics(ics);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "(no title)");
    }

    // ── events_by_day ────────────────────────────────────────────────────

    #[test]
    fn events_by_day_single_day() {
        let events = vec![CalEvent {
            calendar_id: 0,
            summary: "A".into(),
            start: NaiveDate::from_ymd_opt(2025, 3, 10).unwrap(),
            start_time: None,
            end: NaiveDate::from_ymd_opt(2025, 3, 11).unwrap(),
            end_time: None,
            description: None,
            location: None,
            google_event_id: None,
            rsvp_status: None,
        }];
        let map = events_by_day(&events);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&NaiveDate::from_ymd_opt(2025, 3, 10).unwrap()));
    }

    #[test]
    fn events_by_day_multi_day_span() {
        let events = vec![CalEvent {
            calendar_id: 0,
            summary: "Trip".into(),
            start: NaiveDate::from_ymd_opt(2025, 3, 10).unwrap(),
            start_time: None,
            end: NaiveDate::from_ymd_opt(2025, 3, 13).unwrap(),
            end_time: None,
            description: None,
            location: None,
            google_event_id: None,
            rsvp_status: None,
        }];
        let map = events_by_day(&events);
        assert_eq!(map.len(), 3); // 10, 11, 12 (end is exclusive)
        for day in 10..=12 {
            assert!(map.contains_key(&NaiveDate::from_ymd_opt(2025, 3, day).unwrap()));
        }
    }

    #[test]
    fn events_by_day_sorts_allday_before_timed() {
        let t = NaiveTime::from_hms_opt(10, 0, 0).unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 3, 10).unwrap();
        let events = vec![
            CalEvent {
                calendar_id: 0,
                summary: "Timed".into(),
                start: date,
                start_time: Some(t),
                end: date + Duration::days(1),
                end_time: None,
                description: None,
                location: None,
                google_event_id: None,
                rsvp_status: None,
            },
            CalEvent {
                calendar_id: 0,
                summary: "AllDay".into(),
                start: date,
                start_time: None,
                end: date + Duration::days(1),
                end_time: None,
                description: None,
                location: None,
                google_event_id: None,
                rsvp_status: None,
            },
        ];
        let map = events_by_day(&events);
        let indices = &map[&date];
        // All-day (index 1) should come before timed (index 0)
        assert_eq!(indices, &[1, 0]);
    }
}
