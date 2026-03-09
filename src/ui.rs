use chrono::{Datelike, Duration, Local, NaiveDate, Timelike, Weekday};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::{App, InputMode, PopupState, ViewMode};

// ── Theme ─────────────────────────────────────────────────────────────────────
// Edit these constants to change the visual appearance of the TUI.

const SEL_FG: Color = Color::Black; // foreground for selected/cursor items
const SEL_BG: Color = Color::White; // background for selected/cursor items
const DIM_FG: Color = Color::DarkGray; // subdued text (events, hints, descriptions)

// Style is not const-constructible in ratatui 0.29, so use helper fns:
fn style_cursor() -> Style {
    Style::default()
        .fg(SEL_FG)
        .bg(SEL_BG)
        .add_modifier(Modifier::BOLD)
}
fn style_today() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}
fn style_event_text() -> Style {
    Style::default().fg(DIM_FG)
}
fn style_hint() -> Style {
    Style::default().fg(DIM_FG)
}
fn style_selected_item() -> Style {
    Style::default()
        .fg(SEL_FG)
        .bg(SEL_BG)
        .add_modifier(Modifier::BOLD)
}
fn style_link_active() -> Style {
    Style::default()
        .fg(SEL_FG)
        .bg(SEL_BG)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}
fn style_link_inactive() -> Style {
    Style::default()
        .fg(DIM_FG)
        .add_modifier(Modifier::UNDERLINED)
}
fn style_header_label() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
fn style_url_input_label() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

// ── Layout constants ──────────────────────────────────────────────────────────
// Edit these to adjust proportions and sizing.

const CAL_PANE_PCT: u16 = 65;
const EVENT_PANE_PCT: u16 = 35;
const POPUP_WIDTH_RATIO: f32 = 0.75;
const POPUP_HEIGHT_RATIO: f32 = 0.80;
const WEEK_TIME_WIDTH: usize = 6;
const WEEK_MAX_EVENT_LINES: usize = 3;
const CELL_MIN_HEIGHT: u16 = 3;
const PANE_TIME_PREFIX_W: usize = 12; // "HH:MM-HH:MM " — always this width in the event pane

pub fn ui(f: &mut Frame, app: &App) {
    let today = Local::now().date_naive();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let main_area = outer[0];
    let status_area = outer[1];

    let show_sidebar = app.show_events && app.view == ViewMode::Month;

    let cols = if show_sidebar {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(CAL_PANE_PCT),
                Constraint::Percentage(EVENT_PANE_PCT),
            ])
            .split(main_area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(main_area)
    };

    match app.view {
        ViewMode::Month => render_month(f, app, cols[0], today),
        ViewMode::Week => render_week(f, app, cols[0], today),
    }

    if show_sidebar {
        render_event_pane(f, app, cols[1]);
    }

    match app.input_mode {
        InputMode::EnteringUrl => {
            let input_line = Line::from(vec![
                Span::styled("ICS URL: ", style_url_input_label()),
                Span::raw(&app.url_input),
                Span::styled("█", style_hint()),
            ]);
            f.render_widget(Paragraph::new(input_line), status_area);
        }
        InputMode::Popup => {
            if let Some(popup) = &app.popup {
                let hint = if popup.links.is_empty() {
                    "  [↑/↓] scroll  [Esc/q] close".to_string()
                } else {
                    format!(
                        "  [↑/↓] scroll  [j/k] select link ({}/{})  [o] open link  [Esc/q] close",
                        popup.link_idx + 1,
                        popup.links.len()
                    )
                };
                let status_line = Line::from(Span::styled(hint, style_hint()));
                f.render_widget(Paragraph::new(status_line), status_area);
            }
            // Dim everything rendered so far
            let area = f.area();
            f.buffer_mut()
                .set_style(area, Style::default().add_modifier(Modifier::DIM));
            if let Some(popup) = &app.popup {
                render_popup(f, app, popup);
            }
        }
        InputMode::Normal => {
            let hint = match app.view {
                ViewMode::Month => {
                    "  [←→↑↓] navigate  [j/k] select event  [o] open  [m/w] month/week  [Tab] toggle sidebar  [r] set URL  [q] quit"
                }
                ViewMode::Week => {
                    "  [←→↑↓] navigate  [o] open event  [m/w] month/week  [r] set URL  [q] quit"
                }
            };
            let status_line = Line::from(vec![
                Span::raw(&app.status),
                Span::styled(hint, style_hint()),
            ]);
            f.render_widget(Paragraph::new(status_line), status_area);
        }
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn render_popup(f: &mut Frame, app: &App, popup: &PopupState) {
    use ratatui::widgets::Clear;

    let ev = &app.events[popup.event_idx];
    let area = centered_rect(
        (f.area().width as f32 * POPUP_WIDTH_RATIO) as u16,
        (f.area().height as f32 * POPUP_HEIGHT_RATIO) as u16,
        f.area(),
    );

    f.render_widget(Clear, area);

    let title_left = format!(
        " [h←] {} ({}/{}) [→l] ",
        ev.summary,
        popup.day_pos + 1,
        popup.day_indices.len()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title_top(ratatui::text::Line::from(title_left.as_str()).alignment(Alignment::Left));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build text lines
    let mut lines: Vec<Line> = Vec::new();

    if let Some(loc) = &ev.location {
        lines.push(Line::from(Span::styled(
            format!("@ {loc}"),
            style_event_text(),
        )));
        lines.push(Line::from(""));
    }

    if let Some(desc) = &ev.description {
        for text_line in desc.lines() {
            let line = build_line_with_links(text_line, &popup.links, popup.link_idx);
            lines.push(line);
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No description",
            style_event_text(),
        )));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((popup.scroll, 0)),
        inner,
    );
}

fn build_line_with_links<'a>(line: &'a str, links: &[String], active_link_idx: usize) -> Line<'a> {
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

fn render_month(f: &mut Frame, app: &App, area: Rect, today: NaiveDate) {
    let title = format!(" {} {} ", month_name(app.cursor.month()), app.cursor.year());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let first_of_month = NaiveDate::from_ymd_opt(app.cursor.year(), app.cursor.month(), 1).unwrap();
    let days_in_month = days_in_month(app.cursor.year(), app.cursor.month());
    let start_col = weekday_col(first_of_month.weekday());
    let total_cells = start_col + days_in_month as usize;
    let weeks = (total_cells + 6) / 7;

    let row_constraints: Vec<Constraint> = std::iter::once(Constraint::Length(1))
        .chain((0..weeks).map(|_| Constraint::Min(CELL_MIN_HEIGHT)))
        .collect();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    // Header row — use the same Ratio split so labels stay pixel-perfect aligned
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, 7); 7])
        .split(rows[0]);
    for (col, label) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .iter()
        .enumerate()
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(*label, style_header_label())))
                .alignment(Alignment::Center),
            header_cols[col],
        );
    }

    // Week rows
    for week in 0..weeks {
        let Some(&row_area) = rows.get(week + 1) else {
            break;
        };
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Ratio(1, 7); 7])
            .split(row_area);

        for col in 0..7usize {
            let cell_idx = week * 7 + col;
            if cell_idx < start_col || cell_idx >= start_col + days_in_month as usize {
                continue;
            }
            let day_num = (cell_idx - start_col + 1) as u32;
            let date =
                NaiveDate::from_ymd_opt(app.cursor.year(), app.cursor.month(), day_num).unwrap();
            render_day_cell(f, app, cols[col], date, today, false);
        }
    }
}

fn render_week(f: &mut Frame, app: &App, area: Rect, today: NaiveDate) {
    let monday = week_monday(app.cursor);
    let title = format!(
        " Week of {} {} {} ",
        monday.day(),
        month_name(monday.month()),
        monday.year()
    );
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let col_constraints = vec![Constraint::Ratio(1, 7); 7];
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints.clone())
        .split(rows[0]);
    for (col, label) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .iter()
        .enumerate()
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(*label, style_header_label())))
                .alignment(Alignment::Center),
            header_cols[col],
        );
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints)
        .split(rows[1]);

    for col in 0..7usize {
        let date = monday + Duration::days(col as i64);
        render_day_cell(f, app, cols[col], date, today, true);
    }
}

fn render_day_cell(
    f: &mut Frame,
    app: &App,
    area: Rect,
    date: NaiveDate,
    today: NaiveDate,
    show_time: bool,
) {
    let is_cursor = date == app.cursor;
    let is_today = date == today;
    let event_indices = app.day_map.get(&date).map(|v| v.as_slice()).unwrap_or(&[]);

    let number_style = if is_cursor {
        style_cursor()
    } else if is_today {
        style_today()
    } else {
        Style::default()
    };

    let cell_inner = area;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" {:2}", date.day()),
        number_style,
    )));

    if show_time {
        // Week view: time prefix + wrapped summary, max WEEK_MAX_EVENT_LINES lines per event
        // "HH:MM " = WEEK_TIME_WIDTH chars; all-day uses spaces
        let text_w = (cell_inner.width as usize).saturating_sub(WEEK_TIME_WIDTH + 1);
        let avail_lines = cell_inner.height.saturating_sub(1) as usize;
        let mut used = 0;

        for &idx in event_indices.iter() {
            if used >= avail_lines {
                break;
            }
            let ev = &app.events[idx];
            let time_str = match ev.start_time {
                Some(t) => format!("{:02}:{:02} ", t.hour(), t.minute()),
                None => " ".repeat(WEEK_TIME_WIDTH),
            };

            let wrapped = wrap_text(&ev.summary, text_w.max(1));
            let take = wrapped
                .len()
                .min(WEEK_MAX_EVENT_LINES)
                .min(avail_lines - used);

            for (i, chunk) in wrapped.iter().take(take).enumerate() {
                let line = if i == 0 {
                    Line::from(vec![
                        Span::styled(time_str.clone(), style_header_label()),
                        Span::styled(chunk.clone(), style_event_text()),
                    ])
                } else {
                    Line::from(Span::styled(chunk.clone(), style_event_text()))
                };
                lines.push(line);
                used += 1;
            }
        }

        let shown_events = event_indices
            .iter()
            .scan(0usize, |used, &idx| {
                let ev = &app.events[idx];
                let wrapped = wrap_text(&ev.summary, text_w.max(1));
                let take = wrapped.len().min(WEEK_MAX_EVENT_LINES);
                *used += take;
                Some(*used)
            })
            .take_while(|&u| u <= avail_lines)
            .count();

        if shown_events < event_indices.len() {
            let extra = event_indices.len() - shown_events;
            if let Some(last) = lines.last_mut() {
                *last = Line::from(Span::styled(format!(" +{extra} more"), style_event_text()));
            }
        }
    } else {
        // Month view: single truncated line per event
        let avail_lines = cell_inner.height.saturating_sub(1) as usize;
        for &idx in event_indices.iter().take(avail_lines) {
            let summary = &app.events[idx].summary;
            let max_w = cell_inner.width.saturating_sub(2) as usize;
            let display = if summary.len() > max_w {
                format!(" {}…", &summary[..max_w.saturating_sub(1)])
            } else {
                format!(" {}", summary)
            };
            lines.push(Line::from(Span::styled(display, style_event_text())));
        }
        if event_indices.len() > avail_lines && avail_lines > 0 {
            let extra = event_indices.len() - avail_lines + 1;
            if let Some(last) = lines.last_mut() {
                *last = Line::from(Span::styled(format!(" +{extra} more"), style_event_text()));
            }
        }
    }

    let para = Paragraph::new(Text::from(lines));
    if is_cursor {
        f.render_widget(
            para.style(Style::default().bg(SEL_BG).fg(SEL_FG)),
            cell_inner,
        );
    } else {
        f.render_widget(para, cell_inner);
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
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

fn format_time_range(start: Option<chrono::NaiveTime>, end: Option<chrono::NaiveTime>) -> String {
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

fn render_event_pane(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(
        " {} {} {} ",
        app.cursor.day(),
        month_name(app.cursor.month()),
        app.cursor.year()
    );
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let event_indices = app
        .day_map
        .get(&app.cursor)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    if event_indices.is_empty() {
        f.render_widget(Paragraph::new("No events").style(style_event_text()), inner);
        return;
    }

    let selected = app.event_cursor.min(event_indices.len().saturating_sub(1));
    let text_w = (inner.width as usize).saturating_sub(PANE_TIME_PREFIX_W + 1);
    let indent = " ".repeat(PANE_TIME_PREFIX_W);

    let items: Vec<ListItem> = event_indices
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let ev = &app.events[idx];
            let is_selected = i == selected;
            let time_prefix = format_time_range(ev.start_time, ev.end_time);
            let time_style = if is_selected {
                style_selected_item()
            } else {
                style_header_label()
            };
            let text_style = if is_selected {
                style_selected_item()
            } else {
                Style::default()
            };

            let wrapped = wrap_text(&ev.summary, text_w.max(1));
            let mut lines: Vec<Line> = wrapped
                .iter()
                .enumerate()
                .map(|(j, chunk)| {
                    if j == 0 {
                        Line::from(vec![
                            Span::styled(time_prefix.clone(), time_style),
                            Span::styled(chunk.clone(), text_style),
                        ])
                    } else {
                        Line::from(vec![
                            Span::raw(indent.clone()),
                            Span::styled(chunk.clone(), text_style),
                        ])
                    }
                })
                .collect();
            lines.push(Line::from(""));
            ListItem::new(Text::from(lines))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

// ── Date/time helpers ─────────────────────────────────────────────────────────

fn month_name(month: u32) -> &'static str {
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

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_month, next_year) = if month == 12 {
        (1, year + 1)
    } else {
        (month + 1, year)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    let first_this = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (first_next - first_this).num_days() as u32
}

fn weekday_col(w: Weekday) -> usize {
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

fn week_monday(date: NaiveDate) -> NaiveDate {
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
