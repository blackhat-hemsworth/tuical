use chrono::{Datelike, Duration, NaiveDate, Timelike};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::App;

use super::styles::{
    calendar_color, style_cursor, style_event_text, style_header_label, style_today,
    CELL_MIN_HEIGHT, DIM_FG, SEL_BG, SEL_FG, WEEK_MAX_EVENT_LINES, WEEK_TIME_WIDTH,
};
use super::utils::{days_in_month, month_name, week_monday, weekday_col, wrap_text};

pub fn event_color_style(app: &App, event_idx: usize) -> Style {
    let cal_id = app.events[event_idx].calendar_id;
    let color = app.config.calendars.get(cal_id)
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
        // Month view: single truncated line per event, colored by calendar
        let avail_lines = cell_inner.height.saturating_sub(1) as usize;
        for &idx in event_indices.iter().take(avail_lines) {
            let summary = &app.events[idx].summary;
            let ev_style = event_color_style(app, idx);
            let max_w = cell_inner.width.saturating_sub(2) as usize;
            let display = if summary.len() > max_w {
                format!(" {}…", &summary[..max_w.saturating_sub(1)])
            } else {
                format!(" {}", summary)
            };
            lines.push(Line::from(Span::styled(display, ev_style)));
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
