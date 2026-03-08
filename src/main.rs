mod app;
mod calendar;
mod config;
mod ui;

use std::{fs, io::{self, stdout}};

use chrono::Duration;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, InputMode, ViewMode};
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
