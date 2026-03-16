use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{App, COLOR_PALETTE, GLYPH_PALETTE, CalManagerMode, glyph_for_color};
use crate::config::CalType;

use super::styles::{
    SEL_BG, SEL_FG, calendar_color, style_event_text, style_hint, style_selected_item,
    style_url_input_label,
};
use super::utils::centered_rect;

pub fn render_calendar_manager(f: &mut Frame, app: &App) {
    let width = (f.area().width as f32 * 0.6).max(40.0) as u16;
    let content_lines = match app.cal_manager_mode {
        CalManagerMode::OAuthPickCalendar => app.oauth_calendars.len() + 1, // +1 for "Done"
        _ => app.config.calendars.len(),
    };
    let height = (content_lines as u16 + 8).min(f.area().height.saturating_sub(4));
    let area = centered_rect(width, height, f.area());

    f.render_widget(Clear, area);

    let block = Block::default().borders(Borders::ALL).title(" Calendars ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if app.config.calendars.is_empty() {
        lines.push(Line::from(Span::styled(
            "No calendars configured",
            style_event_text(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press 'a' to add a calendar",
            style_hint(),
        )));
    } else {
        for (i, cal) in app.config.calendars.iter().enumerate() {
            let is_selected = i == app.cal_manager_cursor;
            let checkbox = if cal.enabled { "[x]" } else { "[ ]" };
            let color = calendar_color(&cal.color);
            let dot_style = Style::default().fg(color);

            let row_text = format!(" {} ● {}  {}", checkbox, cal.name, cal.url);

            if is_selected {
                let mut spans = vec![
                    Span::styled(format!(" {} ", checkbox), style_selected_item()),
                    Span::styled(format!("{} ", glyph_for_color(&cal.color)), dot_style.bg(SEL_BG)),
                    Span::styled(format!("{}  ", cal.name), style_selected_item()),
                ];
                // Truncate URL to fit
                let url_max =
                    (inner.width as usize).saturating_sub(row_text.len().min(inner.width as usize));
                let url_display = if cal.url.len() > url_max && url_max > 3 {
                    format!("{}…", &cal.url[..url_max - 1])
                } else {
                    cal.url.clone()
                };
                spans.push(Span::styled(url_display, style_selected_item()));
                lines.push(Line::from(spans));
            } else {
                let mut spans = vec![
                    Span::raw(format!(" {} ", checkbox)),
                    Span::styled(format!("{} ", glyph_for_color(&cal.color)), dot_style),
                    Span::raw(format!("{}  ", cal.name)),
                ];
                let url_max =
                    (inner.width as usize).saturating_sub(row_text.len().min(inner.width as usize));
                let url_display = if cal.url.len() > url_max && url_max > 3 {
                    format!("{}…", &cal.url[..url_max - 1])
                } else {
                    cal.url.clone()
                };
                spans.push(Span::styled(url_display, style_hint()));
                lines.push(Line::from(spans));
            }
        }
    }

    // Show sub-mode input
    match app.cal_manager_mode {
        CalManagerMode::AddingUrl => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("URL: ", style_url_input_label()),
                Span::raw(&app.cal_manager_input),
                Span::styled("█", style_hint()),
            ]));
        }
        CalManagerMode::AddingName => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Name: ", style_url_input_label()),
                Span::raw(&app.cal_manager_input),
                Span::styled("█", style_hint()),
            ]));
        }
        CalManagerMode::EditingName => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Rename: ", style_url_input_label()),
                Span::raw(&app.cal_manager_input),
                Span::styled("█", style_hint()),
            ]));
        }
        CalManagerMode::PickingColor => {
            lines.push(Line::from(""));
            let mut color_spans: Vec<Span> = vec![Span::raw(" ")];
            for (i, &color_name) in COLOR_PALETTE.iter().enumerate() {
                let color = calendar_color(color_name);
                let glyph = GLYPH_PALETTE[i];
                let style = if i == app.cal_manager_color_idx {
                    Style::default().fg(SEL_FG).bg(color).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                };
                let label = if i == app.cal_manager_color_idx {
                    format!("[{}]", glyph)
                } else {
                    format!(" {} ", glyph)
                };
                color_spans.push(Span::styled(label, style));
            }
            lines.push(Line::from(color_spans));
        }
        CalManagerMode::ChoosingType => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Add calendar:",
                style_url_input_label(),
            )));
            lines.push(Line::from(Span::styled(
                "  [1/i] ICS URL (read-only)",
                style_hint(),
            )));
            lines.push(Line::from(Span::styled(
                "  [2/g] Google Calendar",
                style_hint(),
            )));
        }
        CalManagerMode::OAuthShowCode => {
            lines.clear();
            lines.push(Line::from(Span::styled(
                "Google Sign-In",
                style_url_input_label(),
            )));
            lines.push(Line::from(""));
            if let Some(dc) = &app.oauth_device_code {
                lines.push(Line::from(format!("  Go to: {}", dc.verification_url)));
                lines.push(Line::from(format!(
                    "  Enter code: {} (copied to clipboard)",
                    dc.user_code
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] open browser  [Esc] cancel",
                style_hint(),
            )));
        }
        CalManagerMode::OAuthPolling => {
            lines.clear();
            lines.push(Line::from(Span::styled(
                "Google Sign-In",
                style_url_input_label(),
            )));
            lines.push(Line::from(""));
            let spinner = match app.oauth_poll_count % 4 {
                0 => "◐",
                1 => "◓",
                2 => "◑",
                _ => "◒",
            };
            lines.push(Line::from(format!(
                "  Waiting for authorization... {spinner}"
            )));
            if let Some(dc) = &app.oauth_device_code {
                lines.push(Line::from(format!("  Code: {}", dc.user_code)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  [Esc] cancel", style_hint())));
        }
        CalManagerMode::OAuthPickCalendar => {
            lines.clear();
            lines.push(Line::from(Span::styled(
                "Select Google Calendar(s):",
                style_url_input_label(),
            )));
            let email = app.oauth_account_email.as_deref().unwrap_or("");
            for (i, cal_info) in app.oauth_calendars.iter().enumerate() {
                let is_selected = i == app.oauth_cal_cursor;
                let prefix = if is_selected { "  > " } else { "    " };
                let style = if is_selected {
                    style_selected_item()
                } else {
                    Style::default()
                };
                let already_added = app.config.calendars.iter().any(|c| {
                    c.cal_type == CalType::Google
                        && c.google_account.as_deref() == Some(email)
                        && c.calendar_id.as_deref() == Some(&cal_info.id)
                });
                let is_read_only = !matches!(
                    cal_info.access_role.as_deref(),
                    Some("owner") | Some("writer")
                );
                let mut suffix = String::new();
                if is_read_only {
                    suffix.push_str("  (read-only)");
                }
                if already_added {
                    suffix.push_str(" *added*");
                }
                lines.push(Line::from(Span::styled(
                    format!("{}{}{}", prefix, cal_info.display_name, suffix),
                    style,
                )));
            }
            if app.oauth_calendars.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No calendars found",
                    style_event_text(),
                )));
            }
            // "Done" item after all calendars
            let done_selected = app.oauth_cal_cursor == app.oauth_calendars.len();
            let done_prefix = if done_selected { "  > " } else { "    " };
            let mut done_style = style_url_input_label(); // always bold
            if done_selected {
                done_style = done_style.fg(SEL_FG).bg(SEL_BG);
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("{}Done", done_prefix),
                done_style,
            )));
        }
        CalManagerMode::Normal => {}
    }

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
