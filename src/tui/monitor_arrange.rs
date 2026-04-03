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

pub struct MonitorArrangeState {
    pub monitors: Vec<Monitor>,
    pub workspaces: Vec<Workspace>,
    pub selected: usize,
    pub editor_state: ProfileEditorState,
    /// Row assignment per monitor (index-aligned with monitors vec)
    pub rows: Vec<i32>,
    /// Vertical offset from row baseline per monitor (for alignment within multi-monitor rows)
    pub y_offsets: Vec<i32>,
}

impl MonitorArrangeState {
    pub fn new(editor_state: ProfileEditorState) -> Self {
        let monitors = editor_state.profile.monitors.clone();
        let mut workspaces = editor_state.profile.workspaces.clone();
        workspaces.sort_by_key(|w| w.id);

        // Derive row assignments from existing Y positions
        let mut unique_ys: Vec<i32> = monitors
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.position.y)
            .collect();
        unique_ys.sort();
        unique_ys.dedup();

        let rows: Vec<i32> = monitors
            .iter()
            .map(|m| {
                unique_ys
                    .iter()
                    .position(|&y| y == m.position.y)
                    .unwrap_or(0) as i32
            })
            .collect();

        // Derive y_offsets from existing positions (offset from row's min y)
        let y_offsets: Vec<i32> = monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let row = rows[i];
                let row_min_y = monitors
                    .iter()
                    .enumerate()
                    .filter(|(j, other)| rows[*j] == row && other.enabled)
                    .map(|(_, other)| other.position.y)
                    .min()
                    .unwrap_or(m.position.y);
                m.position.y - row_min_y
            })
            .collect();

        Self {
            monitors,
            workspaces,
            selected: 0,
            editor_state,
            rows,
            y_offsets,
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
        let my_row = self.rows[self.selected];
        let row_mates: Vec<usize> = self.monitors.iter().enumerate()
            .filter(|(i, m)| *i != self.selected && self.rows[*i] == my_row && m.enabled)
            .map(|(i, _)| i)
            .collect();

        if !row_mates.is_empty() {
            // Swap with nearest left neighbor in same row
            if let Some(&swap_idx) = row_mates.iter().filter(|&&i| i < self.selected).last() {
                self.monitors.swap(self.selected, swap_idx);
                self.rows.swap(self.selected, swap_idx);
                self.selected = swap_idx;
                self.recalculate_positions();
            }
        } else {
            // Alone in row: snap to previous alignment point from other rows
            let snap_points = self.collect_x_snap_points();
            let cur_x = self.monitors[self.selected].position.x;
            if let Some(&new_x) = snap_points.iter().rev().find(|&&x| x < cur_x) {
                self.monitors[self.selected].position.x = new_x;
            }
        }
    }

    pub fn move_right(&mut self) {
        let my_row = self.rows[self.selected];
        let row_mates: Vec<usize> = self.monitors.iter().enumerate()
            .filter(|(i, m)| *i != self.selected && self.rows[*i] == my_row && m.enabled)
            .map(|(i, _)| i)
            .collect();

        if !row_mates.is_empty() {
            // Swap with nearest right neighbor in same row
            if let Some(&swap_idx) = row_mates.iter().find(|&&i| i > self.selected) {
                self.monitors.swap(self.selected, swap_idx);
                self.rows.swap(self.selected, swap_idx);
                self.selected = swap_idx;
                self.recalculate_positions();
            }
        } else {
            // Alone in row: snap to next alignment point from other rows
            let snap_points = self.collect_x_snap_points();
            let cur_x = self.monitors[self.selected].position.x;
            if let Some(&new_x) = snap_points.iter().find(|&&x| x > cur_x) {
                self.monitors[self.selected].position.x = new_x;
            }
        }
    }

    /// Collect x-position snap points from all monitors in other rows
    /// Includes left-align, center-align, right-align, and right-edge positions
    fn collect_x_snap_points(&self) -> Vec<i32> {
        let my_row = self.rows[self.selected];
        let (my_w, _) = self.monitors[self.selected].effective_resolution();
        let mut points = Vec::new();
        points.push(0);
        for (i, monitor) in self.monitors.iter().enumerate() {
            if self.rows[i] != my_row && monitor.enabled {
                let (w, _) = monitor.effective_resolution();
                let x = monitor.position.x;
                points.push(x);                       // Left-align
                points.push(x + (w - my_w) / 2);     // Center-align
                points.push(x + w - my_w);            // Right-align
                points.push(x + w);                   // Right edge
            }
        }
        points.sort();
        points.dedup();
        points
    }

    pub fn move_up(&mut self) {
        self.rows[self.selected] -= 1;
        self.y_offsets[self.selected] = 0;
        self.recalculate_positions();
    }

    pub fn move_down(&mut self) {
        self.rows[self.selected] += 1;
        self.y_offsets[self.selected] = 0;
        self.recalculate_positions();
    }

    /// Context-sensitive upward action: align within row or move to row above
    pub fn align_up(&mut self) {
        let my_row = self.rows[self.selected];
        let has_row_mates = self.monitors.iter().enumerate()
            .any(|(i, m)| i != self.selected && self.rows[i] == my_row && m.enabled);

        if has_row_mates {
            let snaps = self.collect_y_snap_points_in_row();
            let cur = self.y_offsets[self.selected];
            if let Some(&new_offset) = snaps.iter().rev().find(|&&y| y < cur) {
                self.y_offsets[self.selected] = new_offset;
                self.recalculate_positions();
            } else {
                // Already at top alignment — move to row above
                self.move_up();
            }
        } else {
            self.move_up();
        }
    }

    /// Context-sensitive downward action: align within row or move to row below
    pub fn align_down(&mut self) {
        let my_row = self.rows[self.selected];
        let has_row_mates = self.monitors.iter().enumerate()
            .any(|(i, m)| i != self.selected && self.rows[i] == my_row && m.enabled);

        if has_row_mates {
            let snaps = self.collect_y_snap_points_in_row();
            let cur = self.y_offsets[self.selected];
            if let Some(&new_offset) = snaps.iter().find(|&&y| y > cur) {
                self.y_offsets[self.selected] = new_offset;
                self.recalculate_positions();
            } else {
                // Already at bottom alignment — move to row below
                self.move_down();
            }
        } else {
            self.move_down();
        }
    }

    /// Collect vertical alignment snap points within the current row
    fn collect_y_snap_points_in_row(&self) -> Vec<i32> {
        let my_row = self.rows[self.selected];
        let (_, my_h) = self.monitors[self.selected].effective_resolution();

        let row_height = self.monitors.iter().enumerate()
            .filter(|(i, m)| self.rows[*i] == my_row && m.enabled)
            .map(|(_, m)| m.effective_resolution().1)
            .max()
            .unwrap_or(my_h);

        let max_offset = (row_height - my_h).max(0);
        let mut points = vec![0];
        if max_offset > 0 {
            points.push(max_offset / 2); // Center
            points.push(max_offset);     // Bottom
        }
        points.sort();
        points.dedup();
        points
    }

    pub fn rotate(&mut self) {
        if let Some(monitor) = self.monitors.get_mut(self.selected) {
            monitor.transform = (monitor.transform + 1) % 4;
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
            self.rows.remove(self.selected);
            self.y_offsets.remove(self.selected);
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
        // Normalize rows: remap to 0..N with no gaps
        let mut unique_rows: Vec<i32> = self.rows.iter().copied().collect();
        unique_rows.sort();
        unique_rows.dedup();
        for row in &mut self.rows {
            *row = unique_rows.iter().position(|&r| r == *row).unwrap_or(0) as i32;
        }

        // Calculate row heights (max effective height per row)
        let num_rows = unique_rows.len().max(1);
        let mut row_heights = vec![0i32; num_rows];
        for (i, monitor) in self.monitors.iter().enumerate() {
            if monitor.enabled {
                let (_, h) = monitor.effective_resolution();
                let r = self.rows[i] as usize;
                if r < row_heights.len() {
                    row_heights[r] = row_heights[r].max(h);
                }
            }
        }

        // Calculate row Y offsets (cumulative heights)
        let mut row_y_offsets = vec![0i32; num_rows];
        for r in 1..num_rows {
            row_y_offsets[r] = row_y_offsets[r - 1] + row_heights[r - 1];
        }

        // Lay out each row left-to-right
        // First, collect indices per row in list order
        let mut row_indices: Vec<Vec<usize>> = vec![Vec::new(); num_rows];
        for (i, &r) in self.rows.iter().enumerate() {
            if self.monitors[i].enabled {
                row_indices[r as usize].push(i);
            }
        }

        for (r, indices) in row_indices.iter().enumerate() {
            if indices.len() > 1 {
                // Multi-monitor row: auto-layout left-to-right with y_offsets
                let mut x_offset = 0;
                for &i in indices {
                    let (w, mh) = self.monitors[i].effective_resolution();
                    let max_offset = (row_heights[r] - mh).max(0);
                    self.y_offsets[i] = self.y_offsets[i].clamp(0, max_offset);
                    self.monitors[i].position.x = x_offset;
                    self.monitors[i].position.y = row_y_offsets[r] + self.y_offsets[i];
                    x_offset += w;
                }
            } else if indices.len() == 1 {
                let i = indices[0];
                let (mw, _) = self.monitors[i].effective_resolution();
                let mx = self.monitors[i].position.x;
                self.y_offsets[i] = 0; // Single-monitor rows: y from overlap, not offset

                // Find the monitor in the row above with the most x-overlap
                // and set y to its bottom edge (y + height)
                let y = if r > 0 {
                    self.best_overlap_bottom(r - 1, &row_indices, mx, mw)
                        .unwrap_or(row_y_offsets[r])
                } else if r + 1 < row_indices.len() {
                    // Row 0 with single monitor: find overlap in row below
                    // and set y so our bottom edge meets their top
                    let below_top = self.best_overlap_top(r + 1, &row_indices, mx, mw);
                    let (_, mh) = self.monitors[i].effective_resolution();
                    below_top.map(|t| t - mh).unwrap_or(0)
                } else {
                    row_y_offsets[r]
                };

                self.monitors[i].position.y = y;
            }
        }
    }

    /// Find the bottom edge (y + height) of the monitor in the given row
    /// that has the most x-overlap with a monitor at (mx, mw)
    fn best_overlap_bottom(&self, row: usize, row_indices: &[Vec<usize>], mx: i32, mw: i32) -> Option<i32> {
        let mut best_overlap = 0;
        let mut best_bottom = None;
        for &j in &row_indices[row] {
            let (jw, jh) = self.monitors[j].effective_resolution();
            let jx = self.monitors[j].position.x;
            let jy = self.monitors[j].position.y;
            let overlap = (mx + mw).min(jx + jw) - mx.max(jx);
            if overlap > best_overlap {
                best_overlap = overlap;
                best_bottom = Some(jy + jh);
            }
        }
        best_bottom
    }

    /// Find the top edge (y) of the monitor in the given row
    /// that has the most x-overlap with a monitor at (mx, mw)
    fn best_overlap_top(&self, row: usize, row_indices: &[Vec<usize>], mx: i32, mw: i32) -> Option<i32> {
        let mut best_overlap = 0;
        let mut best_top = None;
        for &j in &row_indices[row] {
            let (jw, _) = self.monitors[j].effective_resolution();
            let jx = self.monitors[j].position.x;
            let jy = self.monitors[j].position.y;
            let overlap = (mx + mw).min(jx + jw) - mx.max(jx);
            if overlap > best_overlap {
                best_overlap = overlap;
                best_top = Some(jy);
            }
        }
        best_top
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
        let (ew, eh) = monitor.effective_resolution();
        let (w, h) = (ew as f64, eh as f64);
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

    // Inset for selected monitor border (proportional to scale)
    let inset = (100.0 * scale).max(0.5).min(3.0);

    // Pre-calculate monitor positions for the closure (avoids lifetime issues)
    let preview_monitors: Vec<PreviewMonitor> = enabled_monitors
        .iter()
        .map(|(idx, monitor)| {
            let (ew, eh) = monitor.effective_resolution();
            let (w, h) = (ew as f64, eh as f64);
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
        Constraint::Min(10),    // Preview (takes remaining space)
        Constraint::Length(6),  // Monitor list
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
            let rotation = if m.transform != 0 {
                format!(" R:{}°", m.transform as u16 * 90)
            } else {
                String::new()
            };
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
                    "{}{}{}:  {} @ {}x{}{}{}",
                    prefix, m.name, desc_info, m.resolution, m.position.x, m.position.y, rotation, status
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
            Span::styled("J,K", styles::help_key()), Span::styled(" Stack/Align | ", styles::help()),
            Span::styled("r", styles::help_key()), Span::styled(" Rotate", styles::help()),
        ]),
        Line::from(vec![
            Span::styled("d", styles::help_key()), Span::styled(" Disable | ", styles::help()),
            Span::styled("x", styles::help_key()), Span::styled(" Remove | ", styles::help()),
            Span::styled("1-0", styles::help_key()), Span::styled(" Workspace | ", styles::help()),
            Span::styled("s", styles::help_key()), Span::styled(" Save | ", styles::help()),
            Span::styled("Esc", styles::help_key()), Span::styled(" Cancel", styles::help()),
        ]),
    ])
    .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(help, chunks[4]);
}
