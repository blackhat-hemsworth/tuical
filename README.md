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
