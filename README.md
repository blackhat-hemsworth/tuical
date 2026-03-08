# caltui

A terminal calendar that renders your ICS/iCal feed in a month or week grid with a side-by-side event pane. Navigate days with the arrow keys, browse events on the selected day, open a scrollable detail popup, and follow URLs directly from the description. All rendering is done with [ratatui](https://github.com/ratatui-org/ratatui); no mouse required.

---

## Installation

**Prerequisites:** Rust toolchain (stable, 1.70+). Install via [rustup.rs](https://rustup.rs).

```sh
git clone <repo-url>
cd caltui
cargo build --release
# binary at ./target/release/caltui
```

---

## Configuration

Config file location (created automatically on first run):

| Platform | Path |
|---|---|
| Linux / macOS (XDG) | `$XDG_CONFIG_HOME/caltui/config.toml` |
| macOS (fallback) | `~/.config/caltui/config.toml` |

**config.toml** fields:

| Field | Type | Description |
|---|---|---|
| `ics_url` | string | URL of an `.ics` calendar feed |

Example:
```toml
ics_url = "https://calendar.example.com/feed.ics"
```

You can also set the URL at runtime with the `r` key — it will be saved to the config file automatically.

---

## Keybindings

### Normal mode

| Key | Action |
|---|---|
| `←` / `→` | Move cursor one day |
| `↑` / `↓` | Move cursor one week (month view) or one week (week view) |
| `j` / `k` | Select next / previous event in the day |
| `o` | Open event detail popup (or show event pane if hidden) |
| `m` | Switch to month view |
| `w` | Switch to week view |
| `Tab` | Toggle event pane |
| `r` | Enter URL input mode |
| `q` | Quit |

### URL input mode

| Key | Action |
|---|---|
| `Enter` | Confirm URL and reload calendar |
| `Esc` | Cancel |
| `Backspace` | Delete last character |
| `Ctrl-u` | Clear input |

### Popup mode

| Key | Action |
|---|---|
| `↑` / `↓` | Scroll description |
| `j` / `k` | Cycle to next / previous link |
| `o` | Open selected link in browser |
| `h` / `←` | Previous event (same day) |
| `l` / `→` | Next event (same day) |
| `Esc` / `q` | Close popup |

---

## Codebase walkthrough

**`src/main.rs`** is the entire application, organized into six clearly marked sections:

**Config** (`// ── Config ──`) — `Config` struct (serde-backed), path resolution respecting `$XDG_CONFIG_HOME`, and `load_config`/`save_config` helpers.

**Calendar data** (`// ── Calendar data ──`) — `CalEvent` struct, HTML stripping, link extraction, URL opening (platform-specific), ICS fetching via `reqwest::blocking`, parsing via `icalendar`, and `events_by_day` which builds a `HashMap<NaiveDate, Vec<usize>>` used throughout rendering.

**App state** (`// ── App state ──`) — `ViewMode`, `InputMode`, `PopupState`, and the central `App` struct with all navigation and mutation methods (`open_popup`, `close_popup`, `start_url_input`, `confirm_url_input`, `reload`, etc.).

**Rendering** (`// ── Rendering ──`) — Theme and layout constants (see below), followed by `ui` (top-level frame composer), `centered_rect`, `render_popup`, `build_line_with_links`, `render_month`, `render_week`, `render_day_cell`, `wrap_text`, and `render_event_pane`.

**Helpers** (`// ── Helpers ──`) — Pure utility functions: `month_name`, `days_in_month`, `weekday_col`, `week_monday`.

**Main + Event loop** (`// ── Main ──`) — Terminal setup/teardown (crossterm alternate screen), the `run_loop` function which calls `terminal.draw` each tick and dispatches key events to the appropriate input-mode handler.

---

## Making aesthetic changes

All visual values are centralized in the **Theme** and **Layout constants** block at the top of the Rendering section (`src/main.rs`, just before `fn ui`). No other code needs to change for purely cosmetic tweaks.

### Color / style constants

| Constant / function | Effect |
|---|---|
| `SEL_FG` | Foreground color of the cursor day and selected event |
| `SEL_BG` | Background color of the cursor day and selected event |
| `DIM_FG` | Color of event text, hints, location, and description previews |
| `style_cursor()` | Style applied to the cursor day number |
| `style_today()` | Style applied to today's day number |
| `style_event_text()` | Style for event titles in cells, location/desc in pane and popup |
| `style_hint()` | Style for the status-bar hint text and URL input cursor glyph |
| `style_selected_item()` | Style for the selected row in the event pane |
| `style_link_active()` | Style for the currently-focused URL in a popup |
| `style_link_inactive()` | Style for non-focused URLs in a popup |
| `style_header_label()` | Style for day-of-week header labels and time prefixes |
| `style_url_input_label()` | Style for the "ICS URL:" label in URL input mode |

### Layout constants

| Constant | Effect |
|---|---|
| `CAL_PANE_PCT` | Width percentage of the calendar pane (default 65) |
| `EVENT_PANE_PCT` | Width percentage of the event pane (default 35) |
| `POPUP_WIDTH_RATIO` | Popup width as a fraction of terminal width (default 0.75) |
| `POPUP_HEIGHT_RATIO` | Popup height as a fraction of terminal height (default 0.80) |
| `WEEK_TIME_WIDTH` | Character width reserved for the time prefix in week view (default 6) |
| `WEEK_MAX_EVENT_LINES` | Max wrapped lines shown per event in week view (default 3) |
| `CELL_MIN_HEIGHT` | Minimum row height for month-view cells in lines (default 3) |
| `PANE_DESC_LINES` | Description preview lines shown in the event pane (default 3) |

---

## Dependencies

| Crate | Role |
|---|---|
| `ratatui` | Terminal UI framework (widgets, layout, styling) |
| `crossterm` | Cross-platform terminal backend and raw-mode control |
| `chrono` | Date/time types and arithmetic |
| `icalendar` | ICS/iCal parsing |
| `reqwest` (blocking) | HTTP fetch of ICS feeds |
| `serde` + `toml` | Config file serialization |
