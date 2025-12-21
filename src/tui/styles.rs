//! Shared styles for the TUI components.

use ratatui::style::{Color, Modifier, Style};

/// Style for help bar text (blue)
pub fn help() -> Style {
    Style::default().fg(Color::Blue)
}

/// Style for help bar keys (blue + bold)
pub fn help_key() -> Style {
    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
}

/// Style for active table headers (yellow + bold)
pub fn header_active() -> Style {
    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

/// Style for inactive table headers (normal)
pub fn header_inactive() -> Style {
    Style::default()
}

/// Style for active section borders (green)
pub fn border_active() -> Style {
    Style::default().fg(Color::Green)
}

/// Style for inactive section borders (normal)
pub fn border_inactive() -> Style {
    Style::default()
}

/// Style for active section titles (bold)
pub fn title_active() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// Style for inactive section titles (normal)
pub fn title_inactive() -> Style {
    Style::default()
}

/// Style for highlighted table rows (white on dark gray)
pub fn row_highlight() -> Style {
    Style::default().bg(Color::DarkGray).fg(Color::White)
}

/// Style for page titles (bold, centered)
pub fn page_title() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// Style for focused input fields (yellow border)
pub fn input_focused() -> Style {
    Style::default().fg(Color::Yellow)
}

/// Style for error messages (red)
pub fn error() -> Style {
    Style::default().fg(Color::Red)
}

/// Style for warning/status messages (yellow)
pub fn warning() -> Style {
    Style::default().fg(Color::Yellow)
}

/// Style for selected list items (yellow + bold)
pub fn list_selected() -> Style {
    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

/// Style for disabled items (dimmed)
pub fn disabled() -> Style {
    Style::default().fg(Color::DarkGray)
}
