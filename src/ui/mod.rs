mod cal_manager;
mod calendar_view;
mod event_pane;
mod modals;
mod styles;
mod utils;

use chrono::Local;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, CalManagerMode, InputMode, ViewMode};
use crate::config::EventFormMode;

use cal_manager::render_calendar_manager;
use calendar_view::{render_month, render_week};
use event_pane::render_event_pane;
use modals::{render_delete_confirm, render_error_popup, render_event_form, render_popup};
use styles::{
    CAL_PANE_PCT, EVENT_PANE_PCT, style_hint,
};

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

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
        InputMode::EventForm => {
            let hint = if let Some(form) = &app.event_form {
                if form.is_edit {
                    "  [↑/↓] fields  [Enter] save  [Esc] cancel"
                } else {
                    match form.mode {
                        EventFormMode::Title => "  Enter title  [Esc] cancel",
                        EventFormMode::Date => "  Enter date (YYYY-MM-DD)  [Esc] cancel",
                        EventFormMode::StartTime => "  Enter start time (HH:MM) or empty for all-day  [Esc] cancel",
                        EventFormMode::EndTime => "  Enter end time (HH:MM) or empty  [Esc] cancel",
                        EventFormMode::Description => "  Enter description  [Esc] cancel",
                        EventFormMode::Confirm => "  [Enter] save  [Esc] cancel",
                    }
                }
            } else {
                ""
            };
            let status_line = Line::from(Span::styled(hint.trim_start(), style_hint()));
            f.render_widget(Paragraph::new(status_line), status_area);

            // Dim background
            let area = f.area();
            f.buffer_mut()
                .set_style(area, Style::default().add_modifier(Modifier::DIM));
            if let Some(form) = &app.event_form {
                render_event_form(f, app, form);
            }
        }
        InputMode::Popup => {
            if let Some(popup) = &app.popup {
                let hint = if app.confirm_delete.is_some() {
                    "  [y/Enter] confirm delete  [any key] cancel".to_string()
                } else if popup.links.is_empty() {
                    "  [e] edit  [d] delete  [y] accept  [n] decline  [m] maybe  [↑/↓] scroll  [Esc/q] close".to_string()
                } else {
                    format!(
                        "  [j/k] link ({}/{})  [o] open  [e] edit  [d] delete  [y/n/m] RSVP  [↑/↓] scroll  [Esc/q] close",
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
            // Delete confirmation overlay
            if let Some(event_idx) = app.confirm_delete {
                render_delete_confirm(f, app, event_idx);
            }
        }
        InputMode::CalendarManager => {
            let hint = match app.cal_manager_mode {
                CalManagerMode::Normal => {
                    "  [Space] toggle  [a] add  [d] delete  [r] rename  [c] color/glyph  [Esc/q] close"
                }
                CalManagerMode::AddingUrl => "  Type URL, then press Enter  [Esc] cancel",
                CalManagerMode::AddingName => "  Type calendar name, then press Enter  [Esc] cancel",
                CalManagerMode::EditingName => "  Type new name, then press Enter  [Esc] cancel",
                CalManagerMode::PickingColor => "  [←/→] pick color  [Enter] confirm  [Esc] cancel",
                CalManagerMode::ChoosingType => "  [1/i] ICS URL  [2/g] Google Calendar  [Esc] cancel",
                CalManagerMode::OAuthShowCode => "  [Enter] open browser  [Esc] cancel",
                CalManagerMode::OAuthPolling => "  Waiting for authorization...  [Esc] cancel",
                CalManagerMode::OAuthPickCalendar => "  [Enter/Space] add  [Esc] done",
            };
            let status_line = if app.loading {
                let frame = SPINNER[app.loading_tick as usize % SPINNER.len()];
                Line::from(vec![
                    Span::styled(format!("{} Loading… ", frame), style_hint()),
                    Span::styled(hint, style_hint()),
                ])
            } else {
                Line::from(vec![
                    Span::raw(&app.status),
                    Span::styled(hint, style_hint()),
                ])
            };
            f.render_widget(Paragraph::new(status_line), status_area);

            // Dim background
            let area = f.area();
            f.buffer_mut()
                .set_style(area, Style::default().add_modifier(Modifier::DIM));
            render_calendar_manager(f, app);
        }
        InputMode::Normal => {
            let hint = match app.view {
                ViewMode::Month => {
                    "  [j/k] select event  [o] open  [a] new event  [m/w] month/week  [Tab] toggle sidebar  [c] calendars  [q] quit"
                }
                ViewMode::Week => {
                    "  [o] open event  [a] new event  [m/w] month/week  [c] calendars  [q] quit"
                }
            };
            let status_line = if app.loading {
                let frame = SPINNER[app.loading_tick as usize % SPINNER.len()];
                Line::from(vec![
                    Span::styled(format!("{} Loading… ", frame), style_hint()),
                ])
            } else if app.status.is_empty() {
                Line::from(Span::styled(hint.trim_start(), style_hint()))
            } else {
                Line::from(vec![
                    Span::raw(&app.status),
                    Span::styled(hint, style_hint()),
                ])
            };
            f.render_widget(Paragraph::new(status_line), status_area);
        }
    }

    // ── Error popup overlay (rendered last, on top of everything) ─────
    if let Some(err_msg) = &app.error {
        render_error_popup(f, err_msg);
    }
}
