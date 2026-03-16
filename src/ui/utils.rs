use chrono::{Datelike, Duration, NaiveDate, Timelike, Weekday};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
};

use super::styles::{style_link_active, style_link_inactive, PANE_TIME_PREFIX_W};

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn build_line_with_links<'a>(line: &'a str, links: &[String], active_link_idx: usize) -> Line<'a> {
    if links.is_empty() {
        return Line::from(line.to_string());
    }

    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut remaining = line;

    loop {
        let found = links
            .iter()
            .enumerate()
            .filter_map(|(i, link)| {
                remaining
                    .find(link.as_str())
                    .map(|pos| (pos, i, link.as_str()))
            })
            .min_by_key(|(pos, _, _)| *pos);

        match found {
            None => {
                spans.push(Span::raw(remaining.to_string()));
                break;
            }
            Some((pos, link_i, link_str)) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let style = if link_i == active_link_idx {
                    style_link_active()
                } else {
                    style_link_inactive()
                };
                spans.push(Span::styled(link_str.to_string(), style));
                remaining = &remaining[pos + link_str.len()..];
            }
        }
    }

    Line::from(spans)
}

pub fn format_time_range(start: Option<chrono::NaiveTime>, end: Option<chrono::NaiveTime>) -> String {
    // Always returns exactly PANE_TIME_PREFIX_W characters.
    match (start, end) {
        (Some(s), Some(e)) => format!(
            "{:02}:{:02}-{:02}:{:02} ",
            s.hour(),
            s.minute(),
            e.hour(),
            e.minute()
        ),
        (Some(s), None) => format!("{:02}:{:02}       ", s.hour(), s.minute()),
        (None, _) => " ".repeat(PANE_TIME_PREFIX_W),
    }
}

pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_month, next_year) = if month == 12 {
        (1, year + 1)
    } else {
        (month + 1, year)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    let first_this = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (first_next - first_this).num_days() as u32
}

pub fn weekday_col(w: Weekday) -> usize {
    match w {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    }
}

pub fn week_monday(date: NaiveDate) -> NaiveDate {
    date - Duration::days(weekday_col(date.weekday()) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use ratatui::layout::Rect;

    // ── month_name ───────────────────────────────────────────────────────

    #[test]
    fn month_name_all_months() {
        let names = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December",
        ];
        for (i, expected) in names.iter().enumerate() {
            assert_eq!(month_name(i as u32 + 1), *expected);
        }
    }

    #[test]
    fn month_name_invalid() {
        assert_eq!(month_name(0), "");
        assert_eq!(month_name(13), "");
    }

    // ── days_in_month ────────────────────────────────────────────────────

    #[test]
    fn days_in_month_regular() {
        assert_eq!(days_in_month(2025, 1), 31);
        assert_eq!(days_in_month(2025, 2), 28);
        assert_eq!(days_in_month(2025, 4), 30);
        assert_eq!(days_in_month(2025, 12), 31);
    }

    #[test]
    fn days_in_month_leap_year() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
    }

    // ── weekday_col ──────────────────────────────────────────────────────

    #[test]
    fn weekday_col_monday_is_zero() {
        assert_eq!(weekday_col(Weekday::Mon), 0);
    }

    #[test]
    fn weekday_col_sunday_is_six() {
        assert_eq!(weekday_col(Weekday::Sun), 6);
    }

    // ── week_monday ──────────────────────────────────────────────────────

    #[test]
    fn week_monday_from_monday() {
        let mon = NaiveDate::from_ymd_opt(2025, 3, 10).unwrap(); // Monday
        assert_eq!(week_monday(mon), mon);
    }

    #[test]
    fn week_monday_from_wednesday() {
        let wed = NaiveDate::from_ymd_opt(2025, 3, 12).unwrap(); // Wednesday
        let mon = NaiveDate::from_ymd_opt(2025, 3, 10).unwrap();
        assert_eq!(week_monday(wed), mon);
    }

    #[test]
    fn week_monday_from_sunday() {
        let sun = NaiveDate::from_ymd_opt(2025, 3, 16).unwrap(); // Sunday
        let mon = NaiveDate::from_ymd_opt(2025, 3, 10).unwrap();
        assert_eq!(week_monday(sun), mon);
    }

    // ── wrap_text ────────────────────────────────────────────────────────

    #[test]
    fn wrap_text_fits_in_one_line() {
        assert_eq!(wrap_text("hello world", 20), vec!["hello world"]);
    }

    #[test]
    fn wrap_text_wraps_at_width() {
        assert_eq!(wrap_text("hello world", 5), vec!["hello", "world"]);
    }

    #[test]
    fn wrap_text_empty_input() {
        assert_eq!(wrap_text("", 10), vec![""]);
    }

    #[test]
    fn wrap_text_zero_width() {
        assert_eq!(wrap_text("hello", 0), vec!["hello"]);
    }

    #[test]
    fn wrap_text_long_word() {
        assert_eq!(wrap_text("superlongword", 5), vec!["superlongword"]);
    }

    #[test]
    fn wrap_text_multiple_wraps() {
        let result = wrap_text("a b c d e f", 3);
        assert_eq!(result, vec!["a b", "c d", "e f"]);
    }

    // ── format_time_range ────────────────────────────────────────────────

    #[test]
    fn format_time_range_both_times() {
        let start = NaiveTime::from_hms_opt(9, 30, 0);
        let end = NaiveTime::from_hms_opt(10, 45, 0);
        let result = format_time_range(start, end);
        assert_eq!(result, "09:30-10:45 ");
        assert_eq!(result.len(), PANE_TIME_PREFIX_W);
    }

    #[test]
    fn format_time_range_start_only() {
        let start = NaiveTime::from_hms_opt(14, 0, 0);
        let result = format_time_range(start, None);
        assert_eq!(result.len(), PANE_TIME_PREFIX_W);
        assert!(result.starts_with("14:00"));
    }

    #[test]
    fn format_time_range_all_day() {
        let result = format_time_range(None, None);
        assert_eq!(result.len(), PANE_TIME_PREFIX_W);
        assert!(result.trim().is_empty());
    }

    // ── centered_rect ────────────────────────────────────────────────────

    #[test]
    fn centered_rect_centers_correctly() {
        let area = Rect::new(0, 0, 100, 50);
        let r = centered_rect(40, 20, area);
        assert_eq!(r.x, 30);
        assert_eq!(r.y, 15);
        assert_eq!(r.width, 40);
        assert_eq!(r.height, 20);
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 10, 10);
        let r = centered_rect(20, 20, area);
        assert_eq!(r.width, 10);
        assert_eq!(r.height, 10);
    }

    #[test]
    fn centered_rect_with_offset() {
        let area = Rect::new(5, 5, 100, 50);
        let r = centered_rect(40, 20, area);
        assert_eq!(r.x, 35);
        assert_eq!(r.y, 20);
    }
}
