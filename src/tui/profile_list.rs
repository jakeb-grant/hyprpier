use anyhow::Result;
use ratatui::{
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use super::styles;
use crate::dock;
use crate::metadata::Metadata;
use crate::profile::{list_profiles, Profile};

#[derive(Clone)]
pub struct ProfileListState {
    pub profiles: Vec<ProfileInfo>,
    pub table_state: TableState,
}

#[derive(Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub description: String,
    pub monitor_count: usize,
    pub is_active: bool,
    pub dock_uuid: Option<String>,
    pub is_undocked: bool,
    pub load_error: bool,
}

impl ProfileListState {
    pub fn new() -> Result<Self> {
        let profile_names = list_profiles()?;
        let metadata = Metadata::load()?;

        let profiles: Vec<ProfileInfo> = profile_names
            .iter()
            .map(|name| {
                let (profile, load_error) = match Profile::load(name) {
                    Ok(p) => (Some(p), false),
                    Err(e) => {
                        eprintln!("Warning: Failed to load profile '{}': {}", name, e);
                        (None, true)
                    }
                };
                let description = profile
                    .as_ref()
                    .and_then(|p| p.description.clone())
                    .unwrap_or_default();
                let monitor_count = profile.as_ref().map(|p| p.monitors.len()).unwrap_or(0);
                let is_active = metadata.active_profile.as_ref() == Some(name);
                let dock_uuid = metadata.get_profile_dock(name).cloned();
                let is_undocked = metadata.undocked_profile.as_ref() == Some(name);

                ProfileInfo {
                    name: name.clone(),
                    description,
                    monitor_count,
                    is_active,
                    dock_uuid,
                    is_undocked,
                    load_error,
                }
            })
            .collect();

        let mut table_state = TableState::default();
        if !profiles.is_empty() {
            table_state.select(Some(0));
        }

        Ok(Self {
            profiles,
            table_state,
        })
    }

    pub fn selected_profile(&self) -> Option<String> {
        self.table_state
            .selected()
            .and_then(|i| self.profiles.get(i))
            .map(|p| p.name.clone())
    }

    pub fn next(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1) % self.profiles.len(),
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.profiles.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    /// Refresh profile list from disk (preserves selection)
    pub fn refresh(&mut self) {
        let selected_name = self.selected_profile();
        if let Ok(new_state) = Self::new() {
            self.profiles = new_state.profiles;
            self.table_state = new_state.table_state;

            // Try to restore selection by name
            if let Some(name) = selected_name {
                if let Some(idx) = self.profiles.iter().position(|p| p.name == name) {
                    self.table_state.select(Some(idx));
                }
            }
        }
    }
}

pub fn render(frame: &mut Frame, state: &mut ProfileListState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Status bar
        Constraint::Min(5),    // Table
        Constraint::Length(2), // Help (no box)
    ])
    .split(frame.area());

    // Title
    let title = Paragraph::new("Hyprpier - Monitor Profile Manager")
        .style(styles::page_title())
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Status bar (centered)
    let status = build_status_line();
    let status_bar = Paragraph::new(status)
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(status_bar, chunks[1]);

    // Table
    let header = Row::new(vec![
        Cell::from("Name").style(styles::header_active()),
        Cell::from("Description").style(styles::header_active()),
        Cell::from("Monitors").style(styles::header_active()),
        Cell::from("Status").style(styles::header_active()),
    ])
    .height(1);

    let rows: Vec<Row> = state
        .profiles
        .iter()
        .map(|p| {
            let status_cell = if p.load_error {
                Cell::from("error").style(styles::error())
            } else {
                let mut status_parts = Vec::new();
                if p.is_active {
                    status_parts.push("active");
                }
                if p.dock_uuid.is_some() {
                    status_parts.push("docked");
                }
                if p.is_undocked {
                    status_parts.push("undocked");
                }
                Cell::from(status_parts.join(", "))
            };

            Row::new(vec![
                Cell::from(p.name.clone()),
                Cell::from(p.description.clone()),
                Cell::from(p.monitor_count.to_string()),
                status_cell,
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Profiles ")
            .title_style(styles::title_active())
            .border_style(styles::border_active()),
    )
    .row_highlight_style(styles::row_highlight())
    .highlight_symbol(">> ");

    frame.render_stateful_widget(table, chunks[2], &mut state.table_state);

    // Help
    let help = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("n", styles::help_key()), Span::styled(" New | ", styles::help()),
            Span::styled("e", styles::help_key()), Span::styled(" Edit | ", styles::help()),
            Span::styled("d", styles::help_key()), Span::styled(" Delete | ", styles::help()),
            Span::styled("a", styles::help_key()), Span::styled(" Apply | ", styles::help()),
            Span::styled("u", styles::help_key()), Span::styled(" Undocked | ", styles::help()),
            Span::styled("t", styles::help_key()), Span::styled(" Thunderbolt", styles::help()),
        ]),
        Line::from(vec![
            Span::styled("j,↓", styles::help_key()), Span::styled(" Down | ", styles::help()),
            Span::styled("k,↑", styles::help_key()), Span::styled(" Up | ", styles::help()),
            Span::styled("q", styles::help_key()), Span::styled(" Quit", styles::help()),
        ]),
    ])
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(help, chunks[3]);
}

fn build_status_line() -> String {
    let mut parts = Vec::new();

    // Current active profile
    if let Ok(metadata) = Metadata::load() {
        if let Some(ref active) = metadata.active_profile {
            parts.push(format!("Active: {}", active));
        }
    }

    // Connected dock
    if let Ok(docks) = dock::detect_docks() {
        if let Some(d) = docks.first() {
            parts.push(format!("Dock: {}", d.name));
        }
    }

    if parts.is_empty() {
        "No active profile".to_string()
    } else {
        parts.join("  |  ")
    }
}
