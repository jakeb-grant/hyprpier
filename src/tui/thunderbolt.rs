use anyhow::Result;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use super::styles;
use crate::dock::{self, ThunderboltDevice};
use crate::metadata::Metadata;

#[derive(Clone, PartialEq)]
pub enum Section {
    Connected,
    Disconnected,
}

#[derive(Clone)]
pub struct ThunderboltState {
    pub devices: Vec<DeviceInfo>,
    pub disconnected: Vec<DisconnectedDock>,
    pub connected_table: TableState,
    pub disconnected_table: TableState,
    pub section: Section,
    pub security_mode: String,
    pub error_message: Option<String>,
    pub auto_switch_enabled: bool,
}

#[derive(Clone)]
pub struct DeviceInfo {
    pub device: ThunderboltDevice,
    pub linked_profile: Option<String>,
}

#[derive(Clone)]
pub struct DisconnectedDock {
    pub uuid: String,
    pub profile: String,
}

impl ThunderboltState {
    pub fn new() -> Result<Self> {
        let devices = dock::list_all_devices()?;
        let metadata = Metadata::load()?;
        let security_mode = dock::get_security_mode().unwrap_or_else(|_| "unknown".to_string());

        // Collect connected UUIDs as owned Strings first
        let connected_uuids: Vec<String> = devices.iter().map(|d| d.uuid.clone()).collect();

        // Connected devices with their linked profiles
        let device_infos: Vec<DeviceInfo> = devices
            .into_iter()
            .map(|device| {
                let linked_profile = metadata
                    .dock_profiles
                    .iter()
                    .find(|(uuid, _)| *uuid == &device.uuid)
                    .map(|(_, profile)| profile.clone());

                DeviceInfo {
                    device,
                    linked_profile,
                }
            })
            .collect();

        // Disconnected docks (in metadata but not connected)
        let disconnected: Vec<DisconnectedDock> = metadata
            .dock_profiles
            .iter()
            .filter(|(uuid, _)| !connected_uuids.contains(uuid))
            .map(|(uuid, profile)| DisconnectedDock {
                uuid: uuid.clone(),
                profile: profile.clone(),
            })
            .collect();

        let mut connected_table = TableState::default();
        let mut disconnected_table = TableState::default();

        if !device_infos.is_empty() {
            connected_table.select(Some(0));
        }
        if !disconnected.is_empty() {
            disconnected_table.select(Some(0));
        }

        Ok(Self {
            devices: device_infos,
            disconnected,
            connected_table,
            disconnected_table,
            section: Section::Connected,
            security_mode,
            error_message: None,
            auto_switch_enabled: crate::setup::is_installed(),
        })
    }

    pub fn selected_device(&self) -> Option<&DeviceInfo> {
        self.connected_table
            .selected()
            .and_then(|i| self.devices.get(i))
    }

    pub fn selected_disconnected(&self) -> Option<&DisconnectedDock> {
        self.disconnected_table
            .selected()
            .and_then(|i| self.disconnected.get(i))
    }

    pub fn next(&mut self) {
        match self.section {
            Section::Connected => {
                if self.devices.is_empty() {
                    return;
                }
                let i = match self.connected_table.selected() {
                    Some(i) => (i + 1) % self.devices.len(),
                    None => 0,
                };
                self.connected_table.select(Some(i));
            }
            Section::Disconnected => {
                if self.disconnected.is_empty() {
                    return;
                }
                let i = match self.disconnected_table.selected() {
                    Some(i) => (i + 1) % self.disconnected.len(),
                    None => 0,
                };
                self.disconnected_table.select(Some(i));
            }
        }
    }

    pub fn previous(&mut self) {
        match self.section {
            Section::Connected => {
                if self.devices.is_empty() {
                    return;
                }
                let i = match self.connected_table.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.devices.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.connected_table.select(Some(i));
            }
            Section::Disconnected => {
                if self.disconnected.is_empty() {
                    return;
                }
                let i = match self.disconnected_table.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.disconnected.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.disconnected_table.select(Some(i));
            }
        }
    }

    pub fn switch_section(&mut self) {
        match self.section {
            Section::Connected => {
                if !self.disconnected.is_empty() {
                    self.section = Section::Disconnected;
                }
            }
            Section::Disconnected => {
                self.section = Section::Connected;
            }
        }
    }

    /// Refresh device list (preserves section and selection)
    pub fn refresh(&mut self) {
        if let Ok(new_state) = Self::new() {
            let old_section = self.section.clone();
            let old_connected_idx = self.connected_table.selected();
            let old_disconnected_idx = self.disconnected_table.selected();

            self.devices = new_state.devices;
            self.disconnected = new_state.disconnected;
            self.connected_table = new_state.connected_table;
            self.disconnected_table = new_state.disconnected_table;
            self.security_mode = new_state.security_mode;
            self.auto_switch_enabled = new_state.auto_switch_enabled;

            // Restore section, but switch if current section is now empty
            self.section = old_section;
            if self.section == Section::Disconnected && self.disconnected.is_empty() {
                self.section = Section::Connected;
            }

            // Restore selection indices (clamped to new lengths)
            if let Some(idx) = old_connected_idx {
                if !self.devices.is_empty() {
                    self.connected_table.select(Some(idx.min(self.devices.len() - 1)));
                }
            }
            if let Some(idx) = old_disconnected_idx {
                if !self.disconnected.is_empty() {
                    self.disconnected_table.select(Some(idx.min(self.disconnected.len() - 1)));
                }
            }
        }
    }
}

pub fn render(frame: &mut Frame, state: &mut ThunderboltState) {
    let has_disconnected = !state.disconnected.is_empty();
    let has_error = state.error_message.is_some();

    let chunks = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Security mode
        Constraint::Min(6),    // Connected devices table
        Constraint::Length(if has_disconnected { 6 } else { 0 }), // Disconnected table
        Constraint::Length(if has_error { 1 } else { 0 }), // Error message
        Constraint::Length(2), // Help (no box)
    ])
    .split(frame.area());

    // Title
    let title = Paragraph::new("Thunderbolt Manager")
        .style(styles::page_title())
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Security mode and auto-switch status (centered)
    let security_color = match state.security_mode.as_str() {
        "none" => Color::Green,
        "user" => Color::Yellow,
        "secure" => Color::Red,
        _ => Color::Gray,
    };
    let (auto_switch_word, auto_switch_color) = if state.auto_switch_enabled {
        ("enabled", Color::Green)
    } else {
        ("disabled", Color::DarkGray)
    };
    let security = Paragraph::new(Line::from(vec![
        Span::raw("Security: "),
        Span::styled(&state.security_mode, Style::default().fg(security_color)),
        Span::raw(" | Auto-switch: "),
        Span::styled(auto_switch_word, Style::default().fg(auto_switch_color)),
    ]))
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(security, chunks[1]);

    // Connected devices table
    let connected_active = state.section == Section::Connected;
    let connected_header_style = if connected_active {
        styles::header_active()
    } else {
        styles::header_inactive()
    };
    let connected_header = Row::new(vec![
        Cell::from("Device").style(connected_header_style),
        Cell::from("Vendor").style(connected_header_style),
        Cell::from("Type").style(connected_header_style),
        Cell::from("Profile").style(connected_header_style),
    ])
    .height(1);

    let connected_rows: Vec<Row> = state
        .devices
        .iter()
        .map(|info| {
            let device = &info.device;
            let vendor = device.vendor.as_deref().unwrap_or("-");
            let device_type = if device.is_host { "host" } else { "dock" };
            let profile = info.linked_profile.as_deref().unwrap_or("-");

            Row::new(vec![
                Cell::from(device.name.clone()),
                Cell::from(vendor.to_string()),
                Cell::from(device_type),
                Cell::from(profile.to_string()),
            ])
        })
        .collect();

    let connected_table = Table::new(
        connected_rows,
        [
            Constraint::Percentage(35),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(30),
        ],
    )
    .header(connected_header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Connected ")
            .title_style(if connected_active { styles::title_active() } else { styles::title_inactive() })
            .border_style(if connected_active { styles::border_active() } else { styles::border_inactive() }),
    )
    .row_highlight_style(if connected_active { styles::row_highlight() } else { Style::default() })
    .highlight_symbol(if connected_active { ">> " } else { "   " });

    frame.render_stateful_widget(connected_table, chunks[2], &mut state.connected_table);

    // Disconnected docks table
    if has_disconnected {
        let disconnected_active = state.section == Section::Disconnected;
        let disconnected_header_style = if disconnected_active {
            styles::header_active()
        } else {
            styles::header_inactive()
        };
        let disconnected_header = Row::new(vec![
            Cell::from("UUID").style(disconnected_header_style),
            Cell::from("Linked Profile").style(disconnected_header_style),
        ])
        .height(1);

        let disconnected_rows: Vec<Row> = state
            .disconnected
            .iter()
            .map(|dock| {
                let short_uuid = if dock.uuid.len() > 20 {
                    format!("{}...", &dock.uuid[..20])
                } else {
                    dock.uuid.clone()
                };
                Row::new(vec![
                    Cell::from(short_uuid),
                    Cell::from(dock.profile.clone()),
                ])
            })
            .collect();

        let disconnected_table = Table::new(
            disconnected_rows,
            [Constraint::Percentage(50), Constraint::Percentage(50)],
        )
        .header(disconnected_header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Linked (disconnected) ")
                .title_style(if disconnected_active { styles::title_active() } else { styles::title_inactive() })
                .border_style(if disconnected_active { styles::border_active() } else { styles::border_inactive() }),
        )
        .row_highlight_style(if disconnected_active { styles::row_highlight() } else { Style::default() })
        .highlight_symbol(if disconnected_active { ">> " } else { "   " });

        frame.render_stateful_widget(disconnected_table, chunks[3], &mut state.disconnected_table);
    }

    // Status message (if any)
    if let Some(msg) = &state.error_message {
        let msg_para = Paragraph::new(format!(" {}", msg)).style(styles::warning());
        frame.render_widget(msg_para, chunks[4]);
    }

    // Help
    let setup_action = if state.auto_switch_enabled {
        " Disable Auto-switch"
    } else {
        " Enable Auto-switch"
    };

    let mut line1_spans = vec![
        Span::styled("x", styles::help_key()), Span::styled(" Unlink | ", styles::help()),
        Span::styled("s", styles::help_key()), Span::styled(setup_action, styles::help()),
    ];
    if has_disconnected {
        line1_spans.push(Span::styled(" | ", styles::help()));
        line1_spans.push(Span::styled("Tab", styles::help_key()));
        line1_spans.push(Span::styled(" Switch", styles::help()));
    }

    let help = Paragraph::new(vec![
        Line::from(line1_spans),
        Line::from(vec![
            Span::styled("j,↓", styles::help_key()), Span::styled(" Down | ", styles::help()),
            Span::styled("k,↑", styles::help_key()), Span::styled(" Up | ", styles::help()),
            Span::styled("Esc", styles::help_key()), Span::styled(" Back", styles::help()),
        ]),
    ])
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(help, chunks[5]);
}
