//! Helix-inspired theme and layout helpers for the Kataba TUI.
//!
//! The palette mirrors Helix's built-in default theme (`theme.toml` in the
//! Helix repo): a purple `midnight` background, `revolver` popups/statusline,
//! `lilac` accents, `lavender` body text and `comet` line numbers. Keeping the
//! names close to Helix's scopes makes it obvious which element a color is for.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders},
};

use crate::app::Mode;

// ---------------------------------------------------------------------------
// Helix default palette
// ---------------------------------------------------------------------------

/// `ui.background` — midnight.
pub const BG: Color = Color::Rgb(0x3b, 0x22, 0x4c);
/// `ui.popup` — revolver. Also used for the statusline.
pub const POPUP_BG: Color = Color::Rgb(0x28, 0x17, 0x33);
/// `ui.statusline` background.
pub const STATUSLINE_BG: Color = POPUP_BG;
/// `ui.statusline` / `ui.linenr.selected` — lilac.
pub const STATUSLINE_FG: Color = Color::Rgb(0xdb, 0xbf, 0xef);
/// `ui.text` — lavender.
pub const TEXT: Color = Color::Rgb(0xa4, 0xa0, 0xe8);
/// `ui.text.focus` — white.
pub const TEXT_FOCUS: Color = Color::Rgb(0xff, 0xff, 0xff);
/// `ui.linenr` — comet.
pub const MUTED: Color = Color::Rgb(0x5a, 0x59, 0x77);
/// `ui.selection` — deep purple.
pub const SELECTION: Color = Color::Rgb(0x54, 0x00, 0x99);
/// `ui.cursorline.primary` — bossanova.
pub const CURSORLINE: Color = Color::Rgb(0x45, 0x28, 0x59);
/// `diff.plus` / `diff.delta` / `diff.minus`.
pub const DIFF_PLUS: Color = Color::Rgb(0x35, 0xbf, 0x86);
pub const DIFF_DELTA: Color = Color::Rgb(0x6f, 0x44, 0xf0);
pub const DIFF_MINUS: Color = Color::Rgb(0xf2, 0x2c, 0x86);
/// Diagnostic severities.
pub const ERROR: Color = Color::Rgb(0xf4, 0x78, 0x68);
pub const WARNING: Color = Color::Rgb(0xff, 0xcd, 0x1c);

// Mode colors (Helix `color-modes` look, adapted to the default palette).
const MODE_NORMAL_BG: Color = Color::Rgb(0xdb, 0xbf, 0xef); // lilac
const MODE_INSERT_BG: Color = Color::Rgb(0x9f, 0xf2, 0x8f); // mint

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

/// Default editor background + body text.
pub fn base_style() -> Style {
    Style::default().fg(TEXT).bg(BG)
}

/// Statusline base style.
pub fn statusline_style() -> Style {
    Style::default().fg(STATUSLINE_FG).bg(STATUSLINE_BG)
}

/// Short mode label, as Helix shows it (`NOR` / `INS`).
pub fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NOR",
        Mode::Insert => "INS",
    }
}

/// Coloured mode badge used at the start of the statusline.
pub fn mode_style(mode: Mode) -> Style {
    let bg = match mode {
        Mode::Normal => MODE_NORMAL_BG,
        Mode::Insert => MODE_INSERT_BG,
    };
    Style::default()
        .fg(POPUP_BG)
        .bg(bg)
        .add_modifier(Modifier::BOLD)
}

/// Gutter line numbers: muted, except the cursor line which is lilac.
pub fn line_number_style(current: bool) -> Style {
    if current {
        Style::default().fg(STATUSLINE_FG)
    } else {
        Style::default().fg(MUTED)
    }
}

/// Selected text in the editor.
pub fn selected_style() -> Style {
    Style::default().bg(SELECTION).fg(TEXT_FOCUS)
}

/// Selected row in a menu/picker (`ui.menu.selected`).
pub fn menu_selected_style() -> Style {
    Style::default().bg(STATUSLINE_FG).fg(POPUP_BG)
}

/// Default block title style.
pub fn title_style() -> Style {
    Style::default()
        .fg(STATUSLINE_FG)
        .add_modifier(Modifier::BOLD)
}

/// A plain-bordered popup block, matching Helix's picker frame.
pub fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(MUTED))
        .title(Span::styled(format!(" {title} "), title_style()))
        .style(Style::default().bg(POPUP_BG).fg(TEXT))
}

/// A rectangle centred in `area`, sized as a percentage of it.
pub fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
