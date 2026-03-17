# TUIcal

A terminal calendar that displays your ICS/iCal feeds and Google Calendar events in a month or week view. Navigate with the keyboard, manage multiple calendars, create and edit events, RSVP to invitations, and open links from event descriptions — all without leaving the terminal.

Built with [ratatui](https://github.com/ratatui-org/ratatui). No mouse required.

---

## Installation

**Prerequisites:** Rust toolchain (stable, 1.70+). Install via [rustup.rs](https://rustup.rs).

```sh
git clone <repo-url>
cd TUIcal
cargo build --release
# binary at ./target/release/TUIcal
```

To enable Google Calendar support, provide OAuth credentials at build time:

```sh
GOOGLE_CLIENT_ID="..." GOOGLE_CLIENT_SECRET="..." cargo build --release
```

---

## Getting started

Run `TUIcal`. On first launch it creates a config file and prompts you to add a calendar.

Press `c` to open the calendar manager, then `a` to add a calendar — choose between an ICS URL or a Google Calendar account. You can add as many calendars as you like.

---

## Calendar sources

### ICS feeds

Any public `.ics` URL (Outlook, iCloud, Fastmail, etc.). When adding, TUIcal validates the URL and parses the feed before saving.

### Google Calendar

Uses the OAuth device code flow — TUIcal displays a code, you authorize in your browser, and it imports your calendars. Tokens are stored locally at `~/.config/TUIcal/tokens.json` and refresh automatically.

---

## Views

| Key | Action |
|-----|--------|
| `m` | Switch to month view |
| `w` | Switch to week view |
| `g` | Toggle compact month grid |
| `Tab` | Toggle the event sidebar (month view) |

---

## Navigation

| Key | Action |
|-----|--------|
| `Left` / `Right` | Move one day |
| `Up` / `Down` | Move one week (month view) or one week (week view) |
| `j` / `k` | Select next / previous event on the current day |
| `o` | Open event detail popup |

---

## Managing calendars

Press `c` to open the calendar manager.

| Key | Action |
|-----|--------|
| `a` | Add a new calendar (ICS or Google) |
| `d` | Remove the selected calendar |
| `r` | Rename the selected calendar |
| `c` | Change color and glyph |
| `Space` / `Enter` | Toggle calendar visibility |
| `j` / `k` or `Up` / `Down` | Navigate the list |
| `Esc` / `q` | Close |

Each calendar is assigned a color and a unique glyph (e.g. `♥`, `♣`, `♠`, `♦`). The glyph appears next to the calendar name throughout the UI so you can tell calendars apart at a glance.

---

## Creating and editing events

Events can be created and edited on writable Google Calendar calendars (marked with `✎` in the event list). ICS feeds are read-only.

| Key | Context | Action |
|-----|---------|--------|
| `a` | Normal mode | Create a new event on the selected day |
| `e` | Event popup | Edit the current event |
| `d` | Event popup | Delete the current event (asks for confirmation) |

When creating an event, TUIcal walks you through each field: title, date, start time, end time, and description. When editing, use `Up` / `Down` to move between fields.

---

## RSVP

When viewing a Google Calendar event in the popup, you can change your RSVP status:

| Key | Action |
|-----|--------|
| `y` | Accept |
| `n` | Decline |
| `m` | Maybe |

Your RSVP status is shown in both the event list and the detail popup:

| Symbol | Meaning |
|--------|---------|
| `✓` | Accepted |
| `✗` | Declined |
| `?` | Tentative |
| `·` | No response yet |

RSVP is only available for Google Calendar events. ICS events display their status if present in the feed but cannot be changed.

---

## Event popup

Press `o` on a selected event to open the detail popup.

| Key | Action |
|-----|--------|
| `Up` / `Down` | Scroll the description |
| `h` / `l` | Previous / next event on the same day |
| `j` / `k` | Cycle through links in the description |
| `o` | Open the selected link in your browser |
| `e` | Edit event |
| `d` | Delete event |
| `y` / `n` / `m` | RSVP (accept / decline / maybe) |
| `Esc` / `q` | Close |

---

## Configuration

Config file location (created automatically on first run):

| Platform | Path |
|----------|------|
| Linux / macOS | `$XDG_CONFIG_HOME/TUIcal/config.toml` (default: `~/.config/TUIcal/config.toml`) |

Calendars are managed through the in-app calendar manager (`c` key) and saved automatically. You can also set a legacy ICS URL with the `r` key.

---

## Keyboard reference

| Key | Mode | Action |
|-----|------|--------|
| `q` | Normal | Quit |
| `m` / `w` | Normal | Month / week view |
| `g` | Normal | Toggle compact month |
| `Tab` | Normal | Toggle event sidebar |
| `Arrow keys` | Normal | Navigate days / weeks |
| `j` / `k` | Normal | Select event |
| `o` | Normal | Open event popup |
| `a` | Normal | New event |
| `c` | Normal | Calendar manager |
| `r` | Normal | Set ICS URL |
| `e` | Popup | Edit event |
| `d` | Popup | Delete event |
| `y` / `n` / `m` | Popup | RSVP accept / decline / maybe |
| `h` / `l` | Popup | Prev / next event |
| `j` / `k` | Popup | Cycle links |
| `o` | Popup | Open link |
| `Up` / `Down` | Popup | Scroll |
| `Esc` | Any | Cancel / close |
