use ratatui::style::{Color, Modifier, Style};

// ── Theme constants ───────────────────────────────────────────────────────────

pub const SEL_FG: Color = Color::Black;
pub const SEL_BG: Color = Color::White;
pub const DIM_FG: Color = Color::DarkGray;

// ── Layout constants ──────────────────────────────────────────────────────────

pub const CAL_PANE_PCT: u16 = 65;
pub const EVENT_PANE_PCT: u16 = 35;
pub const POPUP_WIDTH_RATIO: f32 = 0.75;
pub const POPUP_HEIGHT_RATIO: f32 = 0.80;
pub const WEEK_TIME_WIDTH: usize = 6;
pub const WEEK_MAX_EVENT_LINES: usize = 3;
pub const CELL_MIN_HEIGHT: u16 = 3;
pub const PANE_TIME_PREFIX_W: usize = 12; // "HH:MM-HH:MM " — always this width in the event pane

// ── Style helpers ─────────────────────────────────────────────────────────────
// Style is not const-constructible in ratatui 0.29, so we use helper functions.

pub fn style_cursor() -> Style {
    Style::default()
        .fg(SEL_FG)
        .bg(SEL_BG)
        .add_modifier(Modifier::BOLD)
}

pub fn style_today() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn style_event_text() -> Style {
    Style::default().fg(DIM_FG)
}

pub fn style_hint() -> Style {
    Style::default().fg(DIM_FG)
}

pub fn style_selected_item() -> Style {
    Style::default()
        .fg(SEL_FG)
        .bg(SEL_BG)
        .add_modifier(Modifier::BOLD)
}

pub fn style_link_active() -> Style {
    Style::default()
        .fg(SEL_FG)
        .bg(SEL_BG)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

pub fn style_link_inactive() -> Style {
    Style::default()
        .fg(DIM_FG)
        .add_modifier(Modifier::UNDERLINED)
}

pub fn style_header_label() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn style_section_separator() -> Style {
    Style::default().fg(DIM_FG).add_modifier(Modifier::DIM)
}

pub fn style_url_input_label() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

// ── Color mapping ─────────────────────────────────────────────────────────────

pub fn calendar_color(color_str: &str) -> Color {
    match color_str {
        "red" => Color::Red,
        "green" => Color::Green,
        "blue" => Color::Blue,
        "yellow" => Color::Yellow,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_blue" => Color::LightBlue,
        "light_yellow" => Color::LightYellow,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_color_maps_known_colors() {
        assert_eq!(calendar_color("red"), Color::Red);
        assert_eq!(calendar_color("green"), Color::Green);
        assert_eq!(calendar_color("blue"), Color::Blue);
        assert_eq!(calendar_color("yellow"), Color::Yellow);
        assert_eq!(calendar_color("magenta"), Color::Magenta);
        assert_eq!(calendar_color("cyan"), Color::Cyan);
        assert_eq!(calendar_color("light_red"), Color::LightRed);
        assert_eq!(calendar_color("light_green"), Color::LightGreen);
        assert_eq!(calendar_color("light_blue"), Color::LightBlue);
        assert_eq!(calendar_color("light_yellow"), Color::LightYellow);
        assert_eq!(calendar_color("light_magenta"), Color::LightMagenta);
        assert_eq!(calendar_color("light_cyan"), Color::LightCyan);
    }

    #[test]
    fn calendar_color_unknown_returns_white() {
        assert_eq!(calendar_color("unknown"), Color::White);
        assert_eq!(calendar_color(""), Color::White);
    }
}
