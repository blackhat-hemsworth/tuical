use chrono::{Datelike, Duration, NaiveDate, Timelike};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, glyph_for_color};

use super::styles::{
    CELL_MIN_HEIGHT, DIM_FG, SEL_BG, SEL_FG, WEEK_MAX_EVENT_LINES, WEEK_TIME_WIDTH, calendar_color,
    style_cursor, style_event_text, style_header_label, style_section_separator, style_today,
};
use super::utils::{days_in_month, month_name, week_monday, weekday_col, wrap_text};

pub fn event_color_style(app: &App, event_idx: usize) -> Style {
    let cal_id = app.events[event_idx].calendar_id;
    let color = app
        .config
        .calendars
        .get(cal_id)
        .map(|c| calendar_color(&c.color))
        .unwrap_or(DIM_FG);
    Style::default().fg(color)
}

pub fn render_month(f: &mut Frame, app: &App, area: Rect, today: NaiveDate) {
    let title = format!(" {} {} ", month_name(app.cursor.month()), app.cursor.year());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let first_of_month = NaiveDate::from_ymd_opt(app.cursor.year(), app.cursor.month(), 1).unwrap();
    let days_in_month = days_in_month(app.cursor.year(), app.cursor.month());
    let start_col = weekday_col(first_of_month.weekday());
    let total_cells = start_col + days_in_month as usize;
    let weeks = total_cells.div_ceil(7);

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

/// Classify an event into a time section by its start_time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeSection {
    AllDay,
    Morning,
    Midday,
    Afternoon,
    Night,
}

fn classify_event_section(start_time: Option<chrono::NaiveTime>) -> TimeSection {
    match start_time {
        None => TimeSection::AllDay,
        Some(t) => {
            let hour = t.hour();
            if hour < 10 {
                TimeSection::Morning
            } else if hour < 14 {
                TimeSection::Midday
            } else if hour < 18 {
                TimeSection::Afternoon
            } else {
                TimeSection::Night
            }
        }
    }
}

pub fn render_week(f: &mut Frame, app: &App, area: Rect, today: NaiveDate) {
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

    // Collect dates and per-day event indices classified by section
    let dates: Vec<NaiveDate> = (0..7).map(|i| monday + Duration::days(i)).collect();

    // Build per-day, per-section event index lists
    let mut day_sections: Vec<[Vec<usize>; 5]> = Vec::with_capacity(7);
    for date in &dates {
        let mut sections: [Vec<usize>; 5] = Default::default();
        if let Some(indices) = app.day_map.get(date) {
            for &idx in indices {
                let section = classify_event_section(app.events[idx].start_time);
                let s = match section {
                    TimeSection::AllDay => 0,
                    TimeSection::Morning => 1,
                    TimeSection::Midday => 2,
                    TimeSection::Afternoon => 3,
                    TimeSection::Night => 4,
                };
                sections[s].push(idx);
            }
        }
        day_sections.push(sections);
    }

    // Compute all-day section height; omit entirely when no all-day events
    let max_allday = day_sections.iter().map(|s| s[0].len()).max().unwrap_or(0);
    let has_allday = max_allday > 0;
    let allday_height = max_allday as u16;

    // Compute available height for the 4 timed sections.
    // Fixed overhead: header(1) + date(1) + 3 separators between timed sections(3)
    let mut overhead: u16 = 1 + 1 + 3;
    if has_allday {
        // allday separator + allday rows + separator after allday
        overhead += 1 + allday_height + 1;
    } else {
        // just the separator after the date row
        overhead += 1;
    }
    let remaining = inner.height.saturating_sub(overhead);
    let section_height = remaining / 4;

    // Build layout constraints dynamically
    let mut vert_constraints: Vec<Constraint> = Vec::new();
    vert_constraints.push(Constraint::Length(1)); // weekday header
    vert_constraints.push(Constraint::Length(1)); // date number row

    if has_allday {
        vert_constraints.push(Constraint::Length(1));            // separator
        vert_constraints.push(Constraint::Length(allday_height)); // all-day section
    }

    // separator + morning
    vert_constraints.push(Constraint::Length(1));
    vert_constraints.push(Constraint::Length(section_height));
    // separator + midday
    vert_constraints.push(Constraint::Length(1));
    vert_constraints.push(Constraint::Length(section_height));
    // separator + afternoon
    vert_constraints.push(Constraint::Length(1));
    vert_constraints.push(Constraint::Length(section_height));
    // separator + night
    vert_constraints.push(Constraint::Length(1));
    vert_constraints.push(Constraint::Length(section_height));

    let vert_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vert_constraints)
        .split(inner);

    let col_constraints = vec![Constraint::Ratio(1, 7); 7];

    // Row 0: weekday headers (Mon, Tue, ...)
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints.clone())
        .split(vert_areas[0]);
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

    // Row 1: date numbers
    let date_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints.clone())
        .split(vert_areas[1]);
    for (col, date) in dates.iter().enumerate() {
        let is_cursor = *date == app.cursor;
        let is_today = *date == today;
        let number_style = if is_cursor {
            style_cursor()
        } else if is_today {
            style_today()
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {:2}", date.day()),
                number_style,
            ))),
            date_cols[col],
        );
    }

    // Build indices into vert_areas for separators and section content.
    // After header(0) and date(1), the next index depends on whether allday is present.
    let mut separator_indices: Vec<usize> = Vec::new();
    let mut section_indices: Vec<usize> = Vec::new(); // pairs with day_sections index
    let mut section_kinds: Vec<usize> = Vec::new();   // 0=allday, 1..4=timed

    let mut idx = 2;
    if has_allday {
        separator_indices.push(idx); idx += 1; // separator before allday
        section_indices.push(idx); section_kinds.push(0); idx += 1; // allday content
    }
    for kind in 1..=4u8 {
        separator_indices.push(idx); idx += 1; // separator
        section_indices.push(idx); section_kinds.push(kind as usize); idx += 1; // section content
    }

    // Draw separators
    for &sep_idx in &separator_indices {
        let sep_area = vert_areas[sep_idx];
        if sep_area.width > 0 && sep_area.height > 0 {
            let sep_str: String = "─".repeat(sep_area.width as usize);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(sep_str, style_section_separator()))),
                sep_area,
            );
        }
    }

    // Render each section
    for (i, &area_idx) in section_indices.iter().enumerate() {
        let section_kind = section_kinds[i];
        let section_area = vert_areas[area_idx];
        let section_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints.clone())
            .split(section_area);

        for col in 0..7usize {
            let date = dates[col];
            let is_cursor = date == app.cursor;
            let event_indices = &day_sections[col][section_kind];
            let show_time = section_kind != 0;

            render_section_cell(f, app, section_cols[col], event_indices, show_time, is_cursor);
        }
    }
}

fn render_section_cell(
    f: &mut Frame,
    app: &App,
    area: Rect,
    event_indices: &[usize],
    show_time: bool,
    is_cursor: bool,
) {
    let text_w = if show_time {
        (area.width as usize).saturating_sub(WEEK_TIME_WIDTH + 1)
    } else {
        (area.width as usize).saturating_sub(1)
    };
    let avail_lines = area.height as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut used = 0;

    for &idx in event_indices.iter() {
        if used >= avail_lines {
            break;
        }
        let ev = &app.events[idx];
        let ev_style = event_color_style(app, idx);

        if show_time {
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
                        Span::styled(chunk.clone(), ev_style),
                    ])
                } else {
                    Line::from(Span::styled(chunk.clone(), ev_style))
                };
                lines.push(line);
                used += 1;
            }
        } else {
            // All-day: just summary, no time
            let wrapped = wrap_text(&ev.summary, text_w.max(1));
            let take = wrapped.len().min(1).min(avail_lines - used);
            for chunk in wrapped.iter().take(take) {
                lines.push(Line::from(Span::styled(
                    format!(" {chunk}"),
                    ev_style,
                )));
                used += 1;
            }
        }
    }

    // Compute how many events were fully shown
    let shown_events = event_indices
        .iter()
        .scan(0usize, |used_lines, &idx| {
            let ev = &app.events[idx];
            let take = if show_time {
                let wrapped = wrap_text(&ev.summary, text_w.max(1));
                wrapped.len().min(WEEK_MAX_EVENT_LINES)
            } else {
                1
            };
            *used_lines += take;
            Some(*used_lines)
        })
        .take_while(|&u| u <= avail_lines)
        .count();

    if shown_events < event_indices.len() {
        let extra = event_indices.len() - shown_events;
        if let Some(last) = lines.last_mut() {
            *last = Line::from(Span::styled(format!(" +{extra} more"), style_event_text()));
        } else if avail_lines > 0 {
            lines.push(Line::from(Span::styled(
                format!(" +{} more", event_indices.len()),
                style_event_text(),
            )));
        }
    }

    let para = Paragraph::new(Text::from(lines));
    if is_cursor {
        f.render_widget(
            para.style(Style::default().bg(SEL_BG).fg(SEL_FG)),
            area,
        );
    } else {
        f.render_widget(para, area);
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
        let text_w = (cell_inner.width as usize).saturating_sub(WEEK_TIME_WIDTH + 1);
        let avail_lines = cell_inner.height.saturating_sub(1) as usize;
        let mut used = 0;

        for &idx in event_indices.iter() {
            if used >= avail_lines {
                break;
            }
            let ev = &app.events[idx];
            let ev_style = event_color_style(app, idx);
            let time_str = match ev.start_time {
                Some(t) => format!("{:02}:{:02} ", t.hour(), t.minute()),
                None => "".repeat(WEEK_TIME_WIDTH),
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
                        Span::styled(chunk.clone(), ev_style),
                    ])
                } else {
                    Line::from(Span::styled(chunk.clone(), ev_style))
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
        // Month view
        let avail_lines = cell_inner.height.saturating_sub(1) as usize;
        if app.compact_month {
            // Compact mode: one line of glyphs, each colored by calendar
            if !event_indices.is_empty() {
                let glyph_spans: Vec<Span> = event_indices
                    .iter()
                    .map(|&idx| {
                        let cal = app.config.calendars.get(app.events[idx].calendar_id);
                        let color = cal.map(|c| calendar_color(&c.color)).unwrap_or(DIM_FG);
                        let glyph = cal.map(|c| glyph_for_color(&c.color)).unwrap_or("●");
                        Span::styled(glyph.to_string(), Style::default().fg(color))
                    })
                    .collect();
                lines.push(Line::from(glyph_spans));
            }
        } else {
            // Normal mode: one truncated title line per event, colored by calendar
            for &idx in event_indices.iter().take(avail_lines) {
                let summary = &app.events[idx].summary;
                let ev_style = event_color_style(app, idx);
                let is_writable = app
                    .config
                    .calendars
                    .get(app.events[idx].calendar_id)
                    .map(|c| c.is_writable())
                    .unwrap_or(false);
                let max_w = cell_inner.width.saturating_sub(4) as usize; // 2 prefix + 1 space + 1 pad
                let truncated = if summary.len() > max_w {
                    format!("{}…", &summary[..max_w.saturating_sub(1)])
                } else {
                    summary.clone()
                };
                let prefix_span = if is_writable {
                    Span::styled("✎", ev_style)
                } else {
                    Span::raw("")
                };
                lines.push(Line::from(vec![
                    prefix_span,
                    Span::styled(truncated, ev_style),
                ]));
            }
            if event_indices.len() > avail_lines && avail_lines > 0 {
                let extra = event_indices.len() - avail_lines + 1;
                if let Some(last) = lines.last_mut() {
                    *last = Line::from(Span::styled(format!(" +{extra} more"), style_event_text()));
                }
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
