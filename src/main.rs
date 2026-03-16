mod app;
mod google;
mod calendar;
mod config;
mod oauth;
mod ui;

use std::{fs, io::{self, stdout}, time::Duration};

use chrono::Duration as ChronoDuration;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, CalManagerMode, InputMode, ViewMode, COLOR_PALETTE};
use calendar::{extract_links, open_url};
use config::{config_path, load_config};
use ui::ui;

fn main() -> io::Result<()> {
    let cfg_path = config_path();
    if !cfg_path.exists() {
        if let Some(parent) = cfg_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(
            &cfg_path,
            "# caltui configuration\n# Add calendars with the 'c' key in the app\n",
        )
        .ok();
    }

    let config = load_config();
    let mut app = App::new(config);

    app.start_reload();

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

/// Handle a key event for a plain text input field.
/// Returns true if the key was consumed, false if the caller should handle it.
fn handle_text_input(field: &mut String, key: &event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Backspace => { field.pop(); true }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            field.clear();
            true
        }
        KeyCode::Char(c) => { field.push(c); true }
        _ => false,
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        app.handle_messages();
        terminal.draw(|f| ui(f, app))?;

        let timeout = if app.loading {
            Duration::from_millis(150)
        } else if app.cal_manager_mode == CalManagerMode::OAuthPolling {
            let interval = app.oauth_device_code.as_ref().map(|dc| dc.interval).unwrap_or(5);
            Duration::from_secs(interval)
        } else {
            Duration::from_secs(60)
        };

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // ── Dismiss error popup on any keypress ──────────────────────
                if app.error.is_some() {
                    app.clear_error();
                    continue;
                }

                // ── URL input mode ────────────────────────────────────────────
                if app.input_mode == InputMode::EnteringUrl {
                    match key.code {
                        KeyCode::Enter => app.confirm_url_input(),
                        KeyCode::Esc => app.cancel_url_input(),
                        _ => { handle_text_input(&mut app.url_input, &key); }
                    }
                    continue;
                }

                // ── Event Form mode ────────────────────────────────────────────
                if app.input_mode == InputMode::EventForm {
                    match key.code {
                        KeyCode::Enter => app.advance_event_form(),
                        KeyCode::Esc => app.cancel_event_form(),
                        KeyCode::Up if app.event_form.as_ref().is_some_and(|f| f.is_edit) => {
                            app.event_form_prev_field();
                        }
                        KeyCode::Down if app.event_form.as_ref().is_some_and(|f| f.is_edit) => {
                            app.event_form_next_field();
                        }
                        _ => { handle_text_input(&mut app.event_form_input, &key); }
                    }
                    continue;
                }

                // ── Popup mode ────────────────────────────────────────────────
                if app.input_mode == InputMode::Popup {
                    // Delete confirmation takes priority
                    if app.confirm_delete.is_some() {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => app.confirm_delete_event(),
                            _ => app.cancel_delete(),
                        }
                        continue;
                    }
                    let close = matches!(key.code, KeyCode::Esc | KeyCode::Char('q'));
                    if close {
                        app.close_popup();
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('e') => { app.start_edit_event(); continue; }
                        KeyCode::Char('d') => { app.start_delete_event(); continue; }
                        _ => {}
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

                // ── Calendar Manager mode ─────────────────────────────────────
                if app.input_mode == InputMode::CalendarManager {
                    match app.cal_manager_mode {
                        CalManagerMode::Normal => {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('q') => {
                                    app.close_calendar_manager();
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if !app.config.calendars.is_empty() {
                                        app.cal_manager_cursor = (app.cal_manager_cursor + 1) % app.config.calendars.len();
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if !app.config.calendars.is_empty() {
                                        let n = app.config.calendars.len();
                                        app.cal_manager_cursor = (app.cal_manager_cursor + n - 1) % n;
                                    }
                                }
                                KeyCode::Char(' ') | KeyCode::Enter => {
                                    let idx = app.cal_manager_cursor;
                                    app.toggle_calendar(idx);
                                }
                                KeyCode::Char('a') => {
                                    app.start_add_calendar();
                                }
                                KeyCode::Char('d') => {
                                    let idx = app.cal_manager_cursor;
                                    app.remove_calendar(idx);
                                }
                                KeyCode::Char('r') => {
                                    if !app.config.calendars.is_empty() {
                                        let idx = app.cal_manager_cursor;
                                        app.cal_manager_input = app.config.calendars[idx].name.clone();
                                        app.cal_manager_mode = CalManagerMode::EditingName;
                                    }
                                }
                                KeyCode::Char('c') => {
                                    if !app.config.calendars.is_empty() {
                                        let idx = app.cal_manager_cursor;
                                        let current_color = &app.config.calendars[idx].color;
                                        app.cal_manager_color_idx = COLOR_PALETTE.iter()
                                            .position(|&c| c == current_color)
                                            .unwrap_or(0);
                                        app.cal_manager_mode = CalManagerMode::PickingColor;
                                    }
                                }
                                _ => {}
                            }
                        }
                        CalManagerMode::ChoosingType => {
                            match key.code {
                                KeyCode::Char('1') | KeyCode::Char('i') => {
                                    app.choose_ics_type();
                                }
                                KeyCode::Char('2') | KeyCode::Char('g') => {
                                    app.choose_google_type();
                                }
                                KeyCode::Esc => {
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                _ => {}
                            }
                        }
                        CalManagerMode::OAuthShowCode => {
                            match key.code {
                                KeyCode::Enter => {
                                    app.start_oauth_polling();
                                }
                                KeyCode::Esc => {
                                    app.cancel_oauth();
                                }
                                _ => {}
                            }
                        }
                        CalManagerMode::OAuthPolling => {
                            match key.code {
                                KeyCode::Esc => {
                                    app.cancel_oauth();
                                }
                                _ => {}
                            }
                        }
                        CalManagerMode::OAuthPickCalendar => {
                            let total = app.oauth_calendars.len() + 1; // +1 for "Done" item
                            match key.code {
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if total > 0 {
                                        app.oauth_cal_cursor = (app.oauth_cal_cursor + 1) % total;
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if total > 0 {
                                        app.oauth_cal_cursor = (app.oauth_cal_cursor + total - 1) % total;
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    if app.oauth_cal_cursor == app.oauth_calendars.len() {
                                        app.finish_oauth_pick();
                                    } else {
                                        app.add_google_calendar();
                                    }
                                }
                                KeyCode::Esc => {
                                    app.finish_oauth_pick();
                                }
                                _ => {}
                            }
                        }
                        CalManagerMode::AddingUrl => {
                            match key.code {
                                KeyCode::Enter => {
                                    let url = app.cal_manager_input.trim().to_string();
                                    if !url.is_empty() {
                                        // Quick format check only — full validation happens async in add_calendar()
                                        let after = if url.starts_with("https://") { &url[8..] } else { &url[7..] };
                                        let format_ok = (url.starts_with("http://") || url.starts_with("https://"))
                                            && !after.is_empty() && !after.starts_with('/');
                                        if format_ok {
                                            app.cal_manager_pending_url = url;
                                            app.cal_manager_input.clear();
                                            app.cal_manager_mode = CalManagerMode::AddingName;
                                            app.status = String::from("Enter a name for this calendar");
                                        } else {
                                            app.set_error("URL must start with http:// or https:// and include a hostname".into());
                                        }
                                    }
                                }
                                KeyCode::Esc => {
                                    app.cal_manager_input.clear();
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                _ => { handle_text_input(&mut app.cal_manager_input, &key); }
                            }
                        }
                        CalManagerMode::AddingName => {
                            match key.code {
                                KeyCode::Enter => {
                                    let name = app.cal_manager_input.trim().to_string();
                                    let name = if name.is_empty() { "Calendar".to_string() } else { name };
                                    let url = app.cal_manager_pending_url.clone();
                                    // Pick a default color based on position
                                    let color_idx = app.config.calendars.len() % COLOR_PALETTE.len();
                                    let color = COLOR_PALETTE[color_idx].to_string();
                                    app.add_calendar(name, url, color);
                                    app.cal_manager_cursor = app.config.calendars.len().saturating_sub(1);
                                    app.cal_manager_input.clear();
                                    app.cal_manager_pending_url.clear();
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                KeyCode::Esc => {
                                    app.cal_manager_input.clear();
                                    app.cal_manager_pending_url.clear();
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                _ => { handle_text_input(&mut app.cal_manager_input, &key); }
                            }
                        }
                        CalManagerMode::EditingName => {
                            match key.code {
                                KeyCode::Enter => {
                                    let name = app.cal_manager_input.trim().to_string();
                                    if !name.is_empty() {
                                        let idx = app.cal_manager_cursor;
                                        app.rename_calendar(idx, name);
                                    }
                                    app.cal_manager_input.clear();
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                KeyCode::Esc => {
                                    app.cal_manager_input.clear();
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                _ => { handle_text_input(&mut app.cal_manager_input, &key); }
                            }
                        }
                        CalManagerMode::PickingColor => {
                            match key.code {
                                KeyCode::Left => {
                                    let n = COLOR_PALETTE.len();
                                    app.cal_manager_color_idx = (app.cal_manager_color_idx + n - 1) % n;
                                }
                                KeyCode::Right => {
                                    app.cal_manager_color_idx = (app.cal_manager_color_idx + 1) % COLOR_PALETTE.len();
                                }
                                KeyCode::Enter => {
                                    let idx = app.cal_manager_cursor;
                                    let color = COLOR_PALETTE[app.cal_manager_color_idx].to_string();
                                    app.set_calendar_color(idx, color);
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                KeyCode::Esc => {
                                    app.cal_manager_mode = CalManagerMode::Normal;
                                }
                                _ => {}
                            }
                        }
                    }
                    continue;
                }

                // ── Normal mode ───────────────────────────────────────────────
                app.clear_status();
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('a') => app.start_create_event(),
                    KeyCode::Char('r') => app.start_url_input(),
                    KeyCode::Char('c') => app.open_calendar_manager(),
                    KeyCode::Char('o') => {
                        match app.view {
                            ViewMode::Week => app.open_popup(),
                            ViewMode::Month => {
                                if !app.show_events {
                                    app.show_events = true;
                                } else {
                                    app.open_popup();
                                }
                            }
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
                        app.event_cursor = 0;
                        app.status = String::from("Week view");
                    }
                    KeyCode::Tab => {
                        app.show_events = !app.show_events;
                    }
                    KeyCode::Left => {
                        app.cursor -= ChronoDuration::days(1);
                        app.event_cursor = 0;
                    }
                    KeyCode::Right => {
                        app.cursor += ChronoDuration::days(1);
                        app.event_cursor = 0;
                    }
                    KeyCode::Up => {
                        match app.view {
                            ViewMode::Month => app.cursor -= ChronoDuration::days(7),
                            ViewMode::Week => app.cursor -= ChronoDuration::weeks(1),
                        }
                        app.event_cursor = 0;
                    }
                    KeyCode::Down => {
                        match app.view {
                            ViewMode::Month => app.cursor += ChronoDuration::days(7),
                            ViewMode::Week => app.cursor += ChronoDuration::weeks(1),
                        }
                        app.event_cursor = 0;
                    }
                    _ => {}
                }
            }
        } else {
            if app.loading {
                app.loading_tick = app.loading_tick.wrapping_add(1);
            }
            if app.cal_manager_mode == CalManagerMode::OAuthPolling {
                app.poll_oauth_token();
            }
        }
    }
    Ok(())
}
