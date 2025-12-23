use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Rectangle},
        Block, Borders, Paragraph,
    },
    Frame,
};

use super::profile_editor::ProfileEditorState;
use super::styles;
use crate::profile::{Monitor, Workspace};

/// Parse resolution string "WxH" into (width, height)
fn parse_resolution(resolution: &str) -> (f64, f64) {
    let parts: Vec<&str> = resolution.split('x').collect();
    let width = parts.first().and_then(|w| w.parse().ok()).unwrap_or(1920.0);
    let height = parts.get(1).and_then(|h| h.parse().ok()).unwrap_or(1080.0);
    (width, height)
}

pub struct MonitorArrangeState {
    pub monitors: Vec<Monitor>,
    pub workspaces: Vec<Workspace>,
    pub selected: usize,
    pub editor_state: ProfileEditorState,
}

impl MonitorArrangeState {
    pub fn new(editor_state: ProfileEditorState) -> Self {
        let monitors = editor_state.profile.monitors.clone();
        let mut workspaces = editor_state.profile.workspaces.clone();
        workspaces.sort_by_key(|w| w.id);
        Self {
            monitors,
            workspaces,
            selected: 0,
            editor_state,
        }
    }

    /// Return to editor with updated monitors/workspaces
    pub fn apply_to_editor(mut self) -> ProfileEditorState {
        self.editor_state.profile.monitors = self.monitors;
        self.editor_state.profile.workspaces = self.workspaces;
        self.editor_state
    }

    /// Return to editor discarding changes
    pub fn cancel(self) -> ProfileEditorState {
        self.editor_state
    }

    pub fn next(&mut self) {
        if !self.monitors.is_empty() {
            self.selected = (self.selected + 1) % self.monitors.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.monitors.is_empty() {
            self.selected = if self.selected == 0 {
                self.monitors.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn move_left(&mut self) {
        if self.selected > 0 {
            self.monitors.swap(self.selected, self.selected - 1);
            self.selected -= 1;
            self.recalculate_positions();
        }
    }

    pub fn move_right(&mut self) {
        if self.selected < self.monitors.len() - 1 {
            self.monitors.swap(self.selected, self.selected + 1);
            self.selected += 1;
            self.recalculate_positions();
        }
    }

    pub fn toggle_disable(&mut self) {
        if let Some(monitor) = self.monitors.get_mut(self.selected) {
            monitor.enabled = !monitor.enabled;
            self.recalculate_positions();
        }
    }

    pub fn remove_selected(&mut self) {
        if !self.monitors.is_empty() {
            self.monitors.remove(self.selected);
            if self.selected >= self.monitors.len() && self.selected > 0 {
                self.selected -= 1;
            }
            self.recalculate_positions();
        }
    }

    pub fn toggle_workspace(&mut self, ws_id: u8) {
        let Some(monitor) = self.monitors.get(self.selected) else {
            return;
        };
        let monitor_name = monitor.name.clone();

        // Check if workspace is already on this monitor
        let existing_idx = self
            .workspaces
            .iter()
            .position(|w| w.id == ws_id && w.monitor == monitor_name);

        if let Some(idx) = existing_idx {
            // Remove workspace from this monitor
            self.workspaces.remove(idx);
        } else {
            // Remove from any other monitor and add to this one
            self.workspaces.retain(|w| w.id != ws_id);
            self.workspaces.push(Workspace {
                id: ws_id,
                monitor: monitor_name,
                default: false,
            });
        }

        // Keep workspaces sorted by ID
        self.workspaces.sort_by_key(|w| w.id);

        // Update default flags (lowest workspace number on each monitor is default)
        self.update_defaults();
    }

    fn recalculate_positions(&mut self) {
        let mut x_offset = 0;
        for monitor in &mut self.monitors {
            if monitor.enabled {
                monitor.position.x = x_offset;
                monitor.position.y = 0;
                if let Some(width_str) = monitor.resolution.split('x').next() {
                    if let Ok(width) = width_str.parse::<i32>() {
                        x_offset += width;
                    }
                }
            }
        }
    }

    fn update_defaults(&mut self) {
        // Find lowest workspace for each monitor
        let mut lowest_per_monitor: std::collections::HashMap<String, u8> =
            std::collections::HashMap::new();

        for ws in &self.workspaces {
            lowest_per_monitor
                .entry(ws.monitor.clone())
                .and_modify(|min| {
                    if ws.id < *min {
                        *min = ws.id
                    }
                })
                .or_insert(ws.id);
        }

        // Update default flags
        for ws in &mut self.workspaces {
            ws.default = lowest_per_monitor.get(&ws.monitor) == Some(&ws.id);
        }
    }
}

/// Data needed for rendering a monitor in the preview
struct PreviewMonitor {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    is_selected: bool,
}

/// Render the visual monitor preview using Canvas
fn render_preview(frame: &mut Frame, area: Rect, state: &MonitorArrangeState) {
    // Only show enabled monitors in preview
    let enabled_monitors: Vec<(usize, &Monitor)> = state
        .monitors
        .iter()
        .enumerate()
        .filter(|(_, m)| m.enabled)
        .collect();

    if enabled_monitors.is_empty() {
        let empty = Paragraph::new("No enabled monitors")
            .style(styles::disabled())
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Preview ")
                    .title_style(styles::title_active())
                    .border_style(styles::border_active()),
            );
        frame.render_widget(empty, area);
        return;
    }

    // Calculate bounding box of all monitors
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for (_, monitor) in &enabled_monitors {
        let (w, h) = parse_resolution(&monitor.resolution);
        let x = monitor.position.x as f64;
        let y = monitor.position.y as f64;

        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }

    let total_width = max_x - min_x;
    let total_height = max_y - min_y;

    // Canvas area (account for block borders)
    let canvas_width = (area.width.saturating_sub(2)) as f64;
    let canvas_height = (area.height.saturating_sub(2)) as f64;

    // Terminal cells are roughly 2:1 (height:width in pixels)
    // So we need to scale Y differently to maintain aspect ratio
    let cell_aspect = 2.0;

    // Calculate scale to fit, accounting for cell aspect ratio
    let scale_x = canvas_width / total_width;
    let scale_y = (canvas_height * cell_aspect) / total_height;
    let scale = scale_x.min(scale_y);

    // Pre-calculate monitor positions for the closure (avoids lifetime issues)
    let preview_monitors: Vec<PreviewMonitor> = enabled_monitors
        .iter()
        .map(|(idx, monitor)| {
            let (w, h) = parse_resolution(&monitor.resolution);
            let x = (monitor.position.x as f64 - min_x) * scale;
            // Flip Y: canvas Y=0 is bottom, but we want monitors at top
            let y = (canvas_height * cell_aspect)
                - ((monitor.position.y as f64 - min_y) * scale)
                - (h * scale);

            PreviewMonitor {
                name: monitor.name.clone(),
                x,
                y,
                width: w * scale,
                height: h * scale,
                is_selected: *idx == state.selected,
            }
        })
        .collect();

    // Use coordinate system where Y increases upward (canvas default)
    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Preview ")
                .title_style(styles::title_active())
                .border_style(styles::border_active()),
        )
        .x_bounds([0.0, canvas_width])
        .y_bounds([0.0, canvas_height * cell_aspect])
        .paint(move |ctx| {
            // Draw non-selected monitors first
            for pm in preview_monitors.iter().filter(|pm| !pm.is_selected) {
                ctx.draw(&Rectangle {
                    x: pm.x,
                    y: pm.y,
                    width: pm.width,
                    height: pm.height,
                    color: Color::Green,
                });

                let label_x = pm.x + pm.width / 2.0;
                let label_y = pm.y + pm.height / 2.0;
                ctx.print(label_x, label_y, Line::styled(pm.name.clone(), Style::default().fg(Color::Green)));
            }

            // Draw selected monitor last so its borders appear on top
            // Inset slightly to avoid corner overlap with adjacent monitors
            for pm in preview_monitors.iter().filter(|pm| pm.is_selected) {
                let inset = 1.0;
                ctx.draw(&Rectangle {
                    x: pm.x + inset,
                    y: pm.y + inset,
                    width: pm.width - (inset * 2.0),
                    height: pm.height - (inset * 2.0),
                    color: Color::Yellow,
                });

                let label_x = pm.x + pm.width / 2.0;
                let label_y = pm.y + pm.height / 2.0;
                ctx.print(label_x, label_y, Line::styled(pm.name.clone(), Style::default().fg(Color::Yellow)));
            }
        });

    frame.render_widget(canvas, area);
}

pub fn render(frame: &mut Frame, state: &mut MonitorArrangeState) {
    let chunks = Layout::vertical([
        Constraint::Length(1),  // Title
        Constraint::Length(10), // Preview
        Constraint::Min(6),     // Monitor list
        Constraint::Length(5),  // Workspaces
        Constraint::Length(2),  // Help (no box)
    ])
    .split(frame.area());

    // Title
    let title = Paragraph::new("Monitor Arrangement")
        .style(styles::page_title())
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Preview
    render_preview(frame, chunks[1], state);

    // Monitor list
    let monitor_lines: Vec<Line> = state
        .monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let selected = i == state.selected;
            let prefix = if selected { ">> " } else { "   " };
            let status = if m.enabled { "" } else { " [DISABLED]" };
            // Show truncated description if available
            let desc_info = m.description.as_ref().map(|d| {
                if d.len() > 35 {
                    format!(" ({}...)", &d[..35])
                } else {
                    format!(" ({})", d)
                }
            }).unwrap_or_default();

            let style = if selected {
                styles::list_selected()
            } else if !m.enabled {
                styles::disabled()
            } else {
                Style::default()
            };

            Line::from(vec![Span::styled(
                format!(
                    "{}{}{}:  {} @ {}x{}{}",
                    prefix, m.name, desc_info, m.resolution, m.position.x, m.position.y, status
                ),
                style,
            )])
        })
        .collect();

    let monitors_para = Paragraph::new(monitor_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Monitors ")
                .title_style(styles::title_active())
                .border_style(styles::border_active()),
        );
    frame.render_widget(monitors_para, chunks[2]);

    // Workspaces for selected monitor
    let selected_monitor = state.monitors.get(state.selected).map(|m| &m.name);
    let ws_text = if let Some(monitor_name) = selected_monitor {
        let ws_ids: Vec<String> = state
            .workspaces
            .iter()
            .filter(|w| &w.monitor == monitor_name)
            .map(|w| {
                if w.default {
                    format!("[{}]", w.id)
                } else {
                    w.id.to_string()
                }
            })
            .collect();

        if ws_ids.is_empty() {
            format!("Workspaces on {}: (none)", monitor_name)
        } else {
            format!("Workspaces on {}: {}", monitor_name, ws_ids.join(", "))
        }
    } else {
        "No monitor selected".to_string()
    };

    let ws_para = Paragraph::new(ws_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Workspaces ")
                .title_style(styles::title_active())
                .border_style(styles::border_active()),
        );
    frame.render_widget(ws_para, chunks[3]);

    // Help
    let help = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("j,↓", styles::help_key()), Span::styled(" / ", styles::help()),
            Span::styled("k,↑", styles::help_key()), Span::styled(" Select | ", styles::help()),
            Span::styled("h,←", styles::help_key()), Span::styled(" / ", styles::help()),
            Span::styled("l,→", styles::help_key()), Span::styled(" Move | ", styles::help()),
            Span::styled("d", styles::help_key()), Span::styled(" Disable | ", styles::help()),
            Span::styled("x", styles::help_key()), Span::styled(" Remove | ", styles::help()),
            Span::styled("1-0", styles::help_key()), Span::styled(" Workspace", styles::help()),
        ]),
        Line::from(vec![
            Span::styled("s", styles::help_key()), Span::styled(" Save | ", styles::help()),
            Span::styled("Esc", styles::help_key()), Span::styled(" Cancel", styles::help()),
        ]),
    ])
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(help, chunks[4]);
}
