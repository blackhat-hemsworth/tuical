use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::{App, glyph_for_color};

use super::calendar_view::event_color_style;
use super::styles::{
    PANE_TIME_PREFIX_W, style_event_text, style_header_label, style_selected_item,
};
use super::utils::{format_time_range, month_name, wrap_text};

pub fn render_event_pane(f: &mut Frame, app: &App, area: Rect) {
    use chrono::Datelike;

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

            // Build summary with calendar name prefix
            let cal_prefix = app
                .config
                .calendars
                .get(ev.calendar_id)
                .map(|c| format!("[{} {}] ", glyph_for_color(&c.color), c.name))
                .unwrap_or_default();
            let writable_marker = if app
                .config
                .calendars
                .get(ev.calendar_id)
                .map(|c| c.is_writable())
                .unwrap_or(false)
            {
                "✎"
            } else {
                ""
            };
            let rsvp_marker = ev.rsvp_status
                .map(|s| format!("{} ", s.symbol()))
                .unwrap_or_default();
            let full_summary = format!("{}{}{}{}", rsvp_marker, writable_marker, cal_prefix, ev.summary);

            let text_style = if is_selected {
                style_selected_item()
            } else {
                event_color_style(app, idx)
            };

            let wrapped = wrap_text(&full_summary, text_w.max(1));
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
