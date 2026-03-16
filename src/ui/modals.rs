use chrono::Timelike;
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{App, EventFormState, PopupState, glyph_for_color};
use crate::config::EventFormMode;

use super::styles::{
    calendar_color, style_event_text, style_hint, style_url_input_label,
    POPUP_HEIGHT_RATIO, POPUP_WIDTH_RATIO,
};
use super::utils::{build_line_with_links, centered_rect, wrap_text};

pub fn render_error_popup(f: &mut Frame, err_msg: &str) {
    // Dim background
    let area = f.area();
    f.buffer_mut()
        .set_style(area, Style::default().add_modifier(Modifier::DIM));

    let max_width = ((area.width as f32 * 0.6) as u16).max(40).min(area.width);
    let text_width = max_width.saturating_sub(4); // borders + padding

    // Word-wrap the error message
    let wrapped = wrap_text(err_msg, text_width as usize);
    let text_lines = wrapped.len() as u16;
    // +4: 2 for borders, 1 blank line before hint, 1 hint line
    let height = (text_lines + 4).max(5).min(area.height);

    let popup_area = centered_rect(max_width, height, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Error ");
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let mut lines: Vec<Line> = wrapped.into_iter().map(Line::from).collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("[any key] dismiss", style_hint())));

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(ratatui::widgets::Wrap { trim: false }),
        inner,
    );
}

pub fn render_popup(f: &mut Frame, app: &App, popup: &PopupState) {
    let ev = &app.events[popup.event_idx];
    let area = centered_rect(
        (f.area().width as f32 * POPUP_WIDTH_RATIO) as u16,
        (f.area().height as f32 * POPUP_HEIGHT_RATIO) as u16,
        f.area(),
    );

    f.render_widget(Clear, area);

    let cal_label = app.config.calendars.get(ev.calendar_id)
        .map(|c| format!(" [{}]", c.name))
        .unwrap_or_default();

    let title_left = format!(
        " [h←] {}{} ({}/{}) [→l] ",
        ev.summary,
        cal_label,
        popup.day_pos + 1,
        popup.day_indices.len()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title_top(ratatui::text::Line::from(title_left.as_str()).alignment(ratatui::layout::Alignment::Left));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build text lines
    let mut lines: Vec<Line> = Vec::new();

    // Time / date line
    let time_str = match (ev.start_time, ev.end_time) {
        (Some(s), Some(e)) => format!(
            "{:02}:{:02}–{:02}:{:02}",
            s.hour(), s.minute(), e.hour(), e.minute()
        ),
        (Some(s), None) => format!("{:02}:{:02}", s.hour(), s.minute()),
        (None, _) => "All day".to_string(),
    };
    lines.push(Line::from(Span::styled(time_str, style_event_text())));
    lines.push(Line::from(""));

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

pub fn render_event_form(f: &mut Frame, app: &App, form: &EventFormState) {
    let area = f.area();
    let width = (area.width / 2).max(40).min(area.width);
    let height = 14u16.min(area.height);
    let popup_area = centered_rect(width, height, area);
    f.render_widget(Clear, popup_area);

    let cal_name = app.config.calendars.get(form.calendar_idx)
        .map(|c| c.name.as_str())
        .unwrap_or("Calendar");
    let cal_color = app.config.calendars.get(form.calendar_idx)
        .map(|c| calendar_color(&c.color))
        .unwrap_or(Color::White);
    let title = if form.is_edit {
        format!(" Edit Event — {} ", cal_name)
    } else {
        format!(" New Event — {} ", cal_name)
    };
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let cal_glyph = app.config.calendars.get(form.calendar_idx)
        .map(|c| glyph_for_color(&c.color))
        .unwrap_or("●");
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(format!("  {} ", cal_glyph), Style::default().fg(cal_color)),
        Span::styled(cal_name, style_url_input_label()),
    ]));

    let fields: &[(&str, &str, EventFormMode)] = &[
        ("Title", &form.title, EventFormMode::Title),
        ("Date", &form.date, EventFormMode::Date),
        ("Start", &form.start_time, EventFormMode::StartTime),
        ("End", &form.end_time, EventFormMode::EndTime),
        ("Desc", &form.description, EventFormMode::Description),
    ];

    if form.is_edit {
        // Edit mode: show all fields, active one is editable
        for &(label, value, field_mode) in fields {
            if form.mode == field_mode {
                lines.push(Line::from(vec![
                    Span::styled(format!("▸ {}: ", label), style_url_input_label()),
                    Span::raw(&app.event_form_input),
                    Span::styled("█", style_hint()),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", label), style_hint()),
                    Span::raw(value.to_string()),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  [↑/↓] fields  [Enter] save  [Esc] cancel", style_hint())));
    } else {
        for &(label, value, field_mode) in fields {
            if form.mode == field_mode {
                // Active input field
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", label), style_url_input_label()),
                    Span::raw(&app.event_form_input),
                    Span::styled("█", style_hint()),
                ]));
            } else if form.mode > field_mode {
                // Completed field
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", label), style_hint()),
                    Span::styled(value.to_string(), style_hint()),
                ]));
            }
            // Future fields: don't show
        }

        if form.mode == EventFormMode::Confirm {
            // Show calendar + all fields + confirm hint
            lines.push(Line::from(vec![
                Span::styled("  Calendar: ", style_hint()),
                Span::styled(format!("{} ", cal_glyph), Style::default().fg(cal_color)),
                Span::raw(cal_name.to_string()),
            ]));
            for &(label, value, _) in fields {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", label), style_hint()),
                    Span::raw(value.to_string()),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  [Enter] save  [Esc] cancel", style_hint())));
        }
    }

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

pub fn render_delete_confirm(f: &mut Frame, app: &App, event_idx: usize) {
    let area = f.area();
    let width = 45u16.min(area.width);
    let height = 5u16.min(area.height);
    let popup_area = centered_rect(width, height, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default().borders(Borders::ALL).title(" Confirm Delete ");
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let event_title = app.events.get(event_idx)
        .map(|e| e.summary.as_str())
        .unwrap_or("event");

    let lines = vec![
        Line::from(format!("  Delete \"{}\"?", event_title)),
        Line::from(""),
        Line::from(Span::styled("  [y/Enter] confirm  [any key] cancel", style_hint())),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
