use anyhow::Result;
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::styles;
use crate::dock;
use crate::hyprland;
use crate::metadata::Metadata;
use crate::profile::Profile;

#[derive(Clone)]
pub struct ProfileEditorState {
    pub profile: Profile,
    pub name_input: String,
    pub description_input: String,
    pub focused_field: usize, // 0=name, 1=description, 2+=actions
    pub input_mode: bool,
    pub original_name: Option<String>, // For rename detection
    pub dock_status: Option<String>,   // Current dock link status for display
    pub error_message: Option<String>, // Validation error to display
}

impl ProfileEditorState {
    pub fn new() -> Self {
        Self {
            profile: Profile::new(""),
            name_input: String::new(),
            description_input: String::new(),
            focused_field: 0,
            input_mode: false,
            original_name: None,
            dock_status: None,
            error_message: None,
        }
    }

    pub fn from_profile(profile: Profile) -> Self {
        let name = profile.name.clone();
        let description = profile.description.clone().unwrap_or_default();
        let dock_status = Self::get_dock_status(&name);
        Self {
            original_name: Some(profile.name.clone()),
            profile,
            name_input: name,
            description_input: description,
            focused_field: 0,
            input_mode: false,
            dock_status,
            error_message: None,
        }
    }

    fn get_dock_status(profile_name: &str) -> Option<String> {
        let metadata = Metadata::load().ok()?;
        if let Some(uuid) = metadata.get_profile_dock(profile_name) {
            // Try to get dock name
            if let Ok(docks) = dock::detect_docks() {
                if let Some(d) = docks.iter().find(|d| &d.uuid == uuid) {
                    return Some(format!("Linked: {}", d.name));
                }
            }
            return Some(format!("Linked: {}...", &uuid[..8.min(uuid.len())]));
        }
        None
    }

    pub fn refresh_dock_status(&mut self) {
        self.dock_status = Self::get_dock_status(&self.name_input);
    }

    pub fn next_field(&mut self) {
        self.focused_field = (self.focused_field + 1) % 2;
    }

    pub fn previous_field(&mut self) {
        self.focused_field = if self.focused_field == 0 { 1 } else { 0 };
    }

    pub fn current_input_mut(&mut self) -> &mut String {
        match self.focused_field {
            0 => &mut self.name_input,
            1 => &mut self.description_input,
            _ => &mut self.name_input, // Fallback
        }
    }

    pub fn detect_monitors(&mut self) -> Result<()> {
        let mut monitors = hyprland::detect_monitors()?;
        hyprland::sort_monitors(&mut monitors);
        hyprland::arrange_monitors(&mut monitors);

        let workspaces = hyprland::generate_workspaces(&monitors);
        let lid_switch = hyprland::generate_lid_switch(&monitors);

        self.profile.monitors = monitors;
        self.profile.workspaces = workspaces;
        self.profile.lid_switch = lid_switch;

        Ok(())
    }

    /// Sync name and description inputs to the profile struct
    pub fn sync_inputs_to_profile(&mut self) {
        self.profile.name = self.name_input.clone();
        self.profile.description = if self.description_input.is_empty() {
            None
        } else {
            Some(self.description_input.clone())
        };
    }
}

pub fn render(frame: &mut Frame, state: &mut ProfileEditorState) {
    let has_error = state.error_message.is_some();
    let chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(3), // Name input
        Constraint::Length(3), // Description input
        Constraint::Min(8),    // Monitors list
        Constraint::Length(if has_error { 1 } else { 0 }), // Error message
        Constraint::Length(2), // Help (no box)
    ])
    .split(frame.area());

    // Title
    let title_text = if state.original_name.is_some() {
        "Edit Profile"
    } else {
        "New Profile"
    };
    let title = Paragraph::new(title_text)
        .style(styles::page_title())
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Name input
    let name_style = if state.focused_field == 0 {
        styles::input_focused()
    } else {
        Style::default()
    };
    let name_block = Block::default()
        .borders(Borders::ALL)
        .title(" Name ")
        .border_style(name_style);
    let cursor = if state.input_mode && state.focused_field == 0 { "_" } else { "" };
    let name_para = Paragraph::new(format!("{}{}", state.name_input, cursor)).block(name_block);
    frame.render_widget(name_para, chunks[1]);

    // Description input
    let desc_style = if state.focused_field == 1 {
        styles::input_focused()
    } else {
        Style::default()
    };
    let desc_block = Block::default()
        .borders(Borders::ALL)
        .title(" Description ")
        .border_style(desc_style);
    let cursor = if state.input_mode && state.focused_field == 1 { "_" } else { "" };
    let desc_para =
        Paragraph::new(format!("{}{}", state.description_input, cursor)).block(desc_block);
    frame.render_widget(desc_para, chunks[2]);

    // Monitors list
    let monitor_items: Vec<ListItem> = state
        .profile
        .monitors
        .iter()
        .map(|m| {
            let status = if m.enabled { "" } else { " (disabled)" };
            let desc_info = m.description.as_ref().map(|d| {
                if d.len() > 40 {
                    format!(" ({}...)", &d[..40])
                } else {
                    format!(" ({})", d)
                }
            }).unwrap_or_default();
            let text = format!(
                "{}{}:  {} @ {}x{}{}",
                m.name, desc_info, m.resolution, m.position.x, m.position.y, status
            );
            ListItem::new(text)
        })
        .collect();

    let monitors_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Monitors ({}) ", state.profile.monitors.len()))
        .title_style(styles::title_active())
        .border_style(styles::border_active());
    let monitors_list = List::new(monitor_items).block(monitors_block);
    frame.render_widget(monitors_list, chunks[3]);

    // Error message
    if let Some(error) = &state.error_message {
        let error_para = Paragraph::new(format!(" Error: {}", error)).style(styles::error());
        frame.render_widget(error_para, chunks[4]);
    }

    // Help
    let dock_status = match &state.dock_status {
        Some(status) => format!(" ({})", status),
        None => String::new(),
    };

    let help = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("d", styles::help_key()), Span::styled(" Detect | ", styles::help()),
            Span::styled("a", styles::help_key()), Span::styled(" Arrange | ", styles::help()),
            Span::styled("l", styles::help_key()), Span::styled(format!(" Link/Unlink{dock_status} | "), styles::help()),
            Span::styled("s", styles::help_key()), Span::styled(" Save", styles::help()),
        ]),
        Line::from(vec![
            Span::styled("Tab", styles::help_key()), Span::styled(" Next | ", styles::help()),
            Span::styled("↵", styles::help_key()), Span::styled(" Edit | ", styles::help()),
            Span::styled("Esc", styles::help_key()), Span::styled(" Back", styles::help()),
        ]),
    ])
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(help, chunks[5]);
}
