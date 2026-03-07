use std::{
    collections::HashMap,
    fs,
    io::{self, stdout},
    path::PathBuf,
};

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike, Weekday};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use icalendar::{Calendar, CalendarComponent, Component, EventLike};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default)]
struct Config {
    ics_url: Option<String>,
}

fn config_path() -> PathBuf {
    let base = config_dir();
    base.join("caltui").join("config.toml")
}

fn config_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        let home = std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config")
    }
}

fn load_config() -> Config {
    let path = config_path();
    if let Ok(contents) = fs::read_to_string(&path) {
        toml::from_str(&contents).unwrap_or_default()
    } else {
        Config::default()
    }
}

fn save_config(config: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let contents = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, contents).map_err(|e| e.to_string())
}

// ── Calendar data ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CalEvent {
    summary: String,
    start: NaiveDate,
    start_time: Option<NaiveTime>,
    end: NaiveDate,
    description: Option<String>,
    location: Option<String>,
}

fn strip_html(s: &str) -> String {
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

fn extract_links(text: &str) -> Vec<String> {
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

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
}

fn fetch_ics(url: &str) -> Result<String, String> {
    reqwest::blocking::get(url)
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())
}

fn parse_ics(raw: &str) -> Vec<CalEvent> {
    let cal: Calendar = raw.parse().unwrap_or_default();
    let mut events = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for component in cal.iter() {
        if let CalendarComponent::Event(ev) = component {
            let summary = ev.get_summary().unwrap_or("(no title)").to_string();

            let start_dt = ev.get_start().and_then(date_time_from_dpt);
            let end_date = ev.get_end().and_then(|dt| date_time_from_dpt(dt).map(|(d, _)| d));

            if let Some((start, start_time)) = start_dt {
                let key = (summary.clone(), start, start_time);
                if !seen.insert(key) {
                    continue;
                }
                let end = end_date.unwrap_or(start + Duration::days(1));
                let end = if end <= start { start + Duration::days(1) } else { end };
                events.push(CalEvent {
                    summary,
                    start,
                    start_time,
                    end,
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

fn events_by_day(events: &[CalEvent]) -> HashMap<NaiveDate, Vec<usize>> {
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

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum ViewMode {
    Month,
    Week,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Normal,
    EnteringUrl,
    Popup,
}

struct PopupState {
    event_idx: usize,
    day_indices: Vec<usize>,
    day_pos: usize,
    scroll: u16,
    links: Vec<String>,
    link_idx: usize,
}

struct App {
    config: Config,
    events: Vec<CalEvent>,
    day_map: HashMap<NaiveDate, Vec<usize>>,
    cursor: NaiveDate,
    view: ViewMode,
    show_events: bool,
    status: String,
    input_mode: InputMode,
    url_input: String,
    event_cursor: usize,
    popup: Option<PopupState>,
}

impl App {
    fn new(config: Config) -> Self {
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

    fn day_event_indices(&self) -> &[usize] {
        self.day_map.get(&self.cursor).map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn open_popup(&mut self) {
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

    fn close_popup(&mut self) {
        self.popup = None;
        self.input_mode = InputMode::Normal;
    }

    fn start_url_input(&mut self) {
        self.url_input = self.config.ics_url.clone().unwrap_or_default();
        self.input_mode = InputMode::EnteringUrl;
    }

    fn confirm_url_input(&mut self) {
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

    fn cancel_url_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.url_input.clear();
        self.status = String::from("Cancelled");
    }

    fn reload(&mut self) {
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

// ── Rendering ─────────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &App) {
    let today = Local::now().date_naive();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let main_area = outer[0];
    let status_area = outer[1];

    let cols = if app.show_events {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
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

    if app.show_events {
        render_event_pane(f, app, cols[1]);
    }

    match app.input_mode {
        InputMode::EnteringUrl => {
            let input_line = Line::from(vec![
                Span::styled("ICS URL: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&app.url_input),
                Span::styled("█", Style::default().fg(Color::DarkGray)),
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
                let status_line = Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray)));
                f.render_widget(Paragraph::new(status_line), status_area);
            }
            // Dim everything rendered so far
            let area = f.area();
            f.buffer_mut().set_style(area, Style::default().add_modifier(Modifier::DIM));
            if let Some(popup) = &app.popup {
                render_popup(f, app, popup);
            }
        }
        InputMode::Normal => {
            let hint = "  [←→↑↓] navigate  [j/k] select event  [o] open  [m/w] month/week  [Tab] events  [r] set URL  [q] quit";
            let status_line = Line::from(vec![
                Span::raw(&app.status),
                Span::styled(hint, Style::default().fg(Color::DarkGray)),
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
        (f.area().width as f32 * 0.75) as u16,
        (f.area().height as f32 * 0.80) as u16,
        f.area(),
    );

    f.render_widget(Clear, area);

    use ratatui::layout::Alignment;
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
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    if let Some(desc) = &ev.description {
        for text_line in desc.lines() {
            // Highlight URLs within the line
            let line = build_line_with_links(text_line, &popup.links, popup.link_idx);
            lines.push(line);
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No description",
            Style::default().fg(Color::DarkGray),
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

    // Find the first link that appears in this line, highlight it if it's the active one
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut remaining = line;

    loop {
        // Find the earliest link occurrence in `remaining`
        let found = links.iter().enumerate().filter_map(|(i, link)| {
            remaining.find(link.as_str()).map(|pos| (pos, i, link.as_str()))
        }).min_by_key(|(pos, _, _)| *pos);

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
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default()
                        .add_modifier(Modifier::UNDERLINED)
                        .fg(Color::DarkGray)
                };
                spans.push(Span::styled(link_str.to_string(), style));
                remaining = &remaining[pos + link_str.len()..];
            }
        }
    }

    Line::from(spans)
}

fn render_month(f: &mut Frame, app: &App, area: Rect, today: NaiveDate) {
    let title = format!(
        " {} {} ",
        month_name(app.cursor.month()),
        app.cursor.year()
    );
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let first_of_month =
        NaiveDate::from_ymd_opt(app.cursor.year(), app.cursor.month(), 1).unwrap();
    let days_in_month = days_in_month(app.cursor.year(), app.cursor.month());
    let start_col = weekday_col(first_of_month.weekday());
    let total_cells = start_col + days_in_month as usize;
    let weeks = (total_cells + 6) / 7;

    let row_constraints: Vec<Constraint> = std::iter::once(Constraint::Length(1))
        .chain((0..weeks).map(|_| Constraint::Min(3)))
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
    for (col, label) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                *label,
                Style::default().add_modifier(Modifier::BOLD),
            )))
            .alignment(ratatui::layout::Alignment::Center),
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
    for (col, label) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                *label,
                Style::default().add_modifier(Modifier::BOLD),
            )))
            .alignment(ratatui::layout::Alignment::Center),
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

fn render_day_cell(f: &mut Frame, app: &App, area: Rect, date: NaiveDate, today: NaiveDate, show_time: bool) {
    let is_cursor = date == app.cursor;
    let is_today = date == today;
    let event_indices = app.day_map.get(&date).map(|v| v.as_slice()).unwrap_or(&[]);

    let number_style = if is_cursor {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else if is_today {
        Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
    };

    let cell_block = if is_cursor {
        Block::default().borders(Borders::ALL)
    } else {
        Block::default().borders(Borders::LEFT | Borders::TOP)
    };

    let cell_inner = cell_block.inner(area);
    f.render_widget(cell_block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" {:2}", date.day()),
        number_style,
    )));

    if show_time {
        // Week view: time prefix + wrapped summary, max 3 lines per event
        // "HH:MM " = 6 chars; all-day uses "      " (6 spaces)
        const TIME_W: usize = 6;
        let text_w = (cell_inner.width as usize).saturating_sub(TIME_W + 1);
        let avail_lines = cell_inner.height.saturating_sub(1) as usize;
        let mut used = 0;

        for &idx in event_indices.iter() {
            if used >= avail_lines {
                break;
            }
            let ev = &app.events[idx];
            let time_str = match ev.start_time {
                Some(t) => format!("{:02}:{:02} ", t.hour(), t.minute()),
                None    => " ".repeat(TIME_W),
            };

            let wrapped = wrap_text(&ev.summary, text_w.max(1));
            let take = wrapped.len().min(3).min(avail_lines - used);

            for (i, chunk) in wrapped.iter().take(take).enumerate() {
                let line = if i == 0 {
                    Line::from(vec![
                        Span::styled(time_str.clone(), Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(chunk.clone(), Style::default().fg(Color::DarkGray)),
                    ])
                } else {
                    Line::from(Span::styled(chunk.clone(), Style::default().fg(Color::DarkGray)))
                };
                lines.push(line);
                used += 1;
            }
        }

        let shown_events = event_indices.iter().scan(0usize, |used, &idx| {
            let ev = &app.events[idx];
            let wrapped = wrap_text(&ev.summary, text_w.max(1));
            let take = wrapped.len().min(3);
            *used += take;
            Some(*used)
        }).take_while(|&u| u <= avail_lines).count();

        if shown_events < event_indices.len() {
            let extra = event_indices.len() - shown_events;
            if let Some(last) = lines.last_mut() {
                *last = Line::from(Span::styled(
                    format!(" +{extra} more"),
                    Style::default().fg(Color::DarkGray),
                ));
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
            lines.push(Line::from(Span::styled(
                display,
                Style::default().fg(Color::DarkGray),
            )));
        }
        if event_indices.len() > avail_lines && avail_lines > 0 {
            let extra = event_indices.len() - avail_lines + 1;
            if let Some(last) = lines.last_mut() {
                *last = Line::from(Span::styled(
                    format!(" +{extra} more"),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
    }

    f.render_widget(Paragraph::new(Text::from(lines)), cell_inner);
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

    let event_indices = app.day_map.get(&app.cursor).map(|v| v.as_slice()).unwrap_or(&[]);

    if event_indices.is_empty() {
        f.render_widget(
            Paragraph::new("No events").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let selected = app.event_cursor.min(event_indices.len().saturating_sub(1));

    let items: Vec<ListItem> = event_indices
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let ev = &app.events[idx];
            let is_selected = i == selected;
            let title_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let mut lines = vec![Line::from(Span::styled(ev.summary.clone(), title_style))];
            if let Some(loc) = &ev.location {
                lines.push(Line::from(Span::styled(
                    format!("  @ {loc}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            if let Some(desc) = &ev.description {
                let trimmed = desc.trim();
                if !trimmed.is_empty() {
                    for line in trimmed.lines().take(3) {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
            lines.push(Line::from(""));
            ListItem::new(Text::from(lines))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January", 2 => "February", 3 => "March",
        4 => "April",   5 => "May",       6 => "June",
        7 => "July",    8 => "August",    9 => "September",
        10 => "October", 11 => "November", 12 => "December",
        _ => "",
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_month, next_year) = if month == 12 { (1, year + 1) } else { (month + 1, year) };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    let first_this = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (first_next - first_this).num_days() as u32
}

fn weekday_col(w: Weekday) -> usize {
    match w {
        Weekday::Mon => 0, Weekday::Tue => 1, Weekday::Wed => 2,
        Weekday::Thu => 3, Weekday::Fri => 4, Weekday::Sat => 5,
        Weekday::Sun => 6,
    }
}

fn week_monday(date: NaiveDate) -> NaiveDate {
    date - Duration::days(weekday_col(date.weekday()) as i64)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let cfg_path = config_path();
    if !cfg_path.exists() {
        if let Some(parent) = cfg_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(
            &cfg_path,
            "# caltui configuration\n# ics_url = \"https://example.com/calendar.ics\"\n",
        )
        .ok();
    }

    let config = load_config();
    let mut app = App::new(config);

    if app.config.ics_url.is_some() {
        app.reload(); // status is already "Loading..." from App::new
    }

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // ── URL input mode ────────────────────────────────────────────
            if app.input_mode == InputMode::EnteringUrl {
                match key.code {
                    KeyCode::Enter => app.confirm_url_input(),
                    KeyCode::Esc => app.cancel_url_input(),
                    KeyCode::Backspace => { app.url_input.pop(); }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.url_input.clear();
                    }
                    KeyCode::Char(c) => app.url_input.push(c),
                    _ => {}
                }
                continue;
            }

            // ── Popup mode ────────────────────────────────────────────────
            if app.input_mode == InputMode::Popup {
                let close = matches!(key.code, KeyCode::Esc | KeyCode::Char('q'));
                if close {
                    app.close_popup();
                    continue;
                }
                if let Some(popup) = app.popup.as_mut() {
                    match key.code {
                        KeyCode::Down => {
                            popup.scroll = popup.scroll.saturating_add(1);
                        }
                        KeyCode::Up => {
                            popup.scroll = popup.scroll.saturating_sub(1);
                        }
                        KeyCode::Char('j') => {
                            if !popup.links.is_empty() {
                                popup.link_idx = (popup.link_idx + 1) % popup.links.len();
                            }
                        }
                        KeyCode::Char('k') => {
                            if !popup.links.is_empty() {
                                popup.link_idx = (popup.link_idx + popup.links.len() - 1) % popup.links.len();
                            }
                        }
                        KeyCode::Char('h') => {
                            let n = popup.day_indices.len();
                            popup.day_pos = (popup.day_pos + n - 1) % n;
                            popup.event_idx = popup.day_indices[popup.day_pos];
                            popup.scroll = 0;
                            popup.link_idx = 0;
                        }
                        KeyCode::Char('l') => {
                            let n = popup.day_indices.len();
                            popup.day_pos = (popup.day_pos + 1) % n;
                            popup.event_idx = popup.day_indices[popup.day_pos];
                            popup.scroll = 0;
                            popup.link_idx = 0;
                        }
                        KeyCode::Char('o') => {
                            if !popup.links.is_empty() {
                                open_url(&popup.links[popup.link_idx].clone());
                            }
                        }
                        _ => {}
                    }
                    // Re-extract links after h/l navigation
                    if matches!(key.code, KeyCode::Char('h') | KeyCode::Char('l')) {
                        let desc = app.events[popup.event_idx].description.as_deref();
                        popup.links = desc.map(extract_links).unwrap_or_default();
                    }
                }
                continue;
            }

            // ── Normal mode ───────────────────────────────────────────────
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('r') => app.start_url_input(),
                KeyCode::Char('o') => {
                    if !app.show_events {
                        app.show_events = true;
                    } else {
                        app.open_popup();
                    }
                }
                KeyCode::Char('j') => {
                    let n = app.day_event_indices().len();
                    if n > 0 {
                        app.event_cursor = (app.event_cursor + 1) % n;
                    }
                }
                KeyCode::Char('k') => {
                    let n = app.day_event_indices().len();
                    if n > 0 {
                        app.event_cursor = (app.event_cursor + n - 1) % n;
                    }
                }
                KeyCode::Char('m') => {
                    app.view = ViewMode::Month;
                    app.status = String::from("Month view");
                }
                KeyCode::Char('w') => {
                    app.view = ViewMode::Week;
                    app.status = String::from("Week view");
                }
                KeyCode::Tab => {
                    app.show_events = !app.show_events;
                }
                KeyCode::Left => {
                    app.cursor -= Duration::days(1);
                    app.event_cursor = 0;
                }
                KeyCode::Right => {
                    app.cursor += Duration::days(1);
                    app.event_cursor = 0;
                }
                KeyCode::Up => {
                    match app.view {
                        ViewMode::Month => app.cursor -= Duration::days(7),
                        ViewMode::Week => app.cursor -= Duration::weeks(1),
                    }
                    app.event_cursor = 0;
                }
                KeyCode::Down => {
                    match app.view {
                        ViewMode::Month => app.cursor += Duration::days(7),
                        ViewMode::Week => app.cursor += Duration::weeks(1),
                    }
                    app.event_cursor = 0;
                }
                _ => {}
            }
        }
    }
    Ok(())
}
