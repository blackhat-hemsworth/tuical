use std::collections::HashMap;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};
use icalendar::{Calendar, CalendarComponent, Component, EventLike};

#[derive(Debug, Clone)]
pub struct CalEvent {
    pub summary: String,
    pub start: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end: NaiveDate,
    pub end_time: Option<NaiveTime>,
    pub description: Option<String>,
    pub location: Option<String>,
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
                events.push(CalEvent {
                    summary,
                    start,
                    start_time,
                    end,
                    end_time,
                    description: ev.get_description().map(|d| strip_html(d)),
                    location: ev.get_location().map(str::to_string),
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
