use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::profile::{LidSwitch, Monitor, Position, Profile, Workspace};

/// Get the Hyprland instance signature, with fallback discovery
/// Usually set in the environment, but we can discover it if needed
fn get_hyprland_instance_signature() -> Option<String> {
    // First check if it's already set
    if let Ok(sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        return Some(sig);
    }

    // Try to discover from XDG_RUNTIME_DIR/hypr/
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let hypr_dir = std::path::Path::new(&runtime_dir).join("hypr");

    if let Ok(entries) = std::fs::read_dir(&hypr_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Instance signatures look like: 386376400119dd46a767c9f8c8791fd22c7b6e61_1766260165_608814011
            if name_str.contains('_') && entry.path().is_dir() {
                return Some(name_str.to_string());
            }
        }
    }

    None
}

/// Create a hyprctl Command with the instance signature set
fn hyprctl_command() -> Command {
    let mut cmd = Command::new("hyprctl");
    if let Some(sig) = get_hyprland_instance_signature() {
        cmd.env("HYPRLAND_INSTANCE_SIGNATURE", sig);
    }
    cmd
}

// Timing constants for runtime apply
const MONITOR_APPLY_DELAY_MS: u64 = 1000;
const WORKSPACE_MOVE_RETRY_DELAY_MS: u64 = 500;
const WORKSPACE_MOVE_MAX_RETRIES: u8 = 3;

/// Raw monitor info from hyprctl monitors -j
#[derive(Debug, Deserialize)]
struct HyprMonitor {
    name: String,
    description: String,
    width: i32,
    height: i32,
    #[serde(rename = "refreshRate")]
    refresh_rate: f64,
    x: i32,
    y: i32,
    scale: f64,
    transform: u8,
}

/// Check if Hyprland is currently running
pub fn is_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "Hyprland"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect currently connected monitors using hyprctl
pub fn detect_monitors() -> Result<Vec<Monitor>> {
    let output = hyprctl_command()
        .args(["monitors", "-j"])
        .output()
        .context("Failed to run hyprctl monitors")?;

    if !output.status.success() {
        anyhow::bail!("hyprctl monitors failed");
    }

    let hypr_monitors: Vec<HyprMonitor> = serde_json::from_slice(&output.stdout)
        .context("Failed to parse hyprctl output")?;

    let monitors = hypr_monitors
        .into_iter()
        .map(|m| {
            let resolution = format!("{}x{}", m.width, m.height);
            let mode = format!("{}x{}@{:.0}", m.width, m.height, m.refresh_rate);
            Monitor {
                name: m.name,
                description: Some(m.description),
                enabled: true,
                resolution,
                refresh_rate: m.refresh_rate,
                position: Position { x: m.x, y: m.y },
                scale: m.scale,
                transform: m.transform,
                mode,
            }
        })
        .collect();

    Ok(monitors)
}

/// Resolve stored monitor descriptions to current port names
/// This allows profiles to work even when dock assigns different port names
pub fn resolve_monitor_names(profile: &mut Profile) -> Result<()> {
    let current_monitors = detect_monitors()?;
    resolve_monitor_names_with(profile, &current_monitors);
    Ok(())
}

/// Apply description-based port-name remapping using a pre-fetched current
/// monitor list. Split out for testability.
///
/// Computes the full `old_name -> new_name` map in one pass before applying
/// any renames. A per-monitor rename-and-walk approach miscompiles port
/// swaps (e.g. profile DP-10/DP-8 ↔ live DP-8/DP-10): renaming monitor A's
/// workspaces from DP-10 to DP-8 collides with monitor B's still-unrenamed
/// label DP-8, so the next pass re-renames A's workspaces a second time.
fn resolve_monitor_names_with(profile: &mut Profile, current: &[Monitor]) {
    let mut renames: HashMap<String, String> = HashMap::new();
    for monitor in &profile.monitors {
        let Some(desc) = monitor.description.as_ref() else { continue };
        let Some(live) = current
            .iter()
            .find(|m| m.description.as_ref() == Some(desc))
        else {
            continue;
        };
        if live.name != monitor.name {
            renames.insert(monitor.name.clone(), live.name.clone());
        }
    }

    if renames.is_empty() {
        return;
    }

    for monitor in &mut profile.monitors {
        if let Some(new_name) = renames.get(&monitor.name) {
            monitor.name = new_name.clone();
        }
    }
    for ws in &mut profile.workspaces {
        if let Some(new_name) = renames.get(&ws.monitor) {
            ws.monitor = new_name.clone();
        }
    }
    if let Some(ref mut lid) = profile.lid_switch {
        if let Some(new_name) = renames.get(&lid.monitor) {
            lid.monitor = new_name.clone();
        }
    }
}

/// Generate Hyprland Lua config content from a profile
///
/// Emits `hl.monitor`, `hl.workspace_rule`, and `hl.bind` calls for Hyprland 0.55+.
/// Loaded from `hyprland.lua` via `pcall(require, "monitors")`.
pub fn generate_config(profile: &Profile) -> String {
    let mut lines = vec![format!(
        "-- Generated by hyprpier from profile: {}",
        profile.name
    )];
    lines.push(format!(
        "-- {}",
        profile.description.as_deref().unwrap_or("No description")
    ));
    lines.push(String::new());

    // Monitor entries
    for monitor in &profile.monitors {
        lines.push(format!("hl.monitor({{ {} }})", lua_monitor_fields(monitor)));
    }

    if !profile.workspaces.is_empty() {
        lines.push(String::new());
        for ws in &profile.workspaces {
            let default_part = if ws.default { ", default = true" } else { "" };
            lines.push(format!(
                "hl.workspace_rule({{ workspace = {}, monitor = {}{} }})",
                ws.id,
                lua_str(&ws.monitor),
                default_part
            ));
        }
    }

    // Lid switch bindings.
    //
    // On lid close: `hl.monitor({ output, disabled = true })` works fine via
    // the runtime bind context, so the close handler disables natively.
    //
    // On lid open: Hyprland 0.55's runtime `hl.monitor({...})` does NOT
    // re-enable a monitor that was previously disabled by a runtime call
    // (returns ok, monitor stays disabled). The reliable way to bring it back
    // is `hyprctl reload`, which re-evaluates the top-level `hl.monitor({...})`
    // lines from this very file. After reload the DRM panel needs a moment
    // before DPMS can be re-asserted, so we defer the dpms-on with hl.timer
    // per Hyprland's reference doc recommendation (§15: "Recommended for
    // delayed DPMS/idle dispatch").
    //
    // Reload also re-applies workspace_rules — addresses the "workspace
    // re-homing on lid open" follow-up from the original bug report.
    if let Some(ref lid) = profile.lid_switch {
        if lid.enabled {
            if profile.monitors.iter().any(|m| m.name == lid.monitor) {
                lines.push(String::new());
                lines.push("-- Lid switch handling".to_string());
                lines.push(format!(
                    "hl.bind(\"switch:on:Lid Switch\", function() hl.monitor({{ output = {}, disabled = true }}) end, {{ locked = true }})",
                    lua_str(&lid.monitor)
                ));
                lines.push("hl.bind(\"switch:off:Lid Switch\", function()".to_string());
                lines.push("  hl.dispatch(hl.dsp.exec_cmd(\"hyprctl reload\"))".to_string());
                lines.push("  hl.timer(function()".to_string());
                lines.push(format!(
                    "    hl.dispatch(hl.dsp.dpms({{ action = \"on\", monitor = {} }}))",
                    lua_str(&lid.monitor)
                ));
                lines.push("  end, { timeout = 500, type = \"oneshot\" })".to_string());
                lines.push("end, { locked = true })".to_string());
            }
        }
    }

    lines.push(String::new());
    lines.join("\n")
}

/// Format the inner fields of an `hl.monitor({...})` call for a monitor.
///
/// Disabled monitors emit `output = "X", disabled = true`. Enabled monitors
/// emit `output`, `mode`, `position`, `scale`, and `transform` (when nonzero).
fn lua_monitor_fields(monitor: &Monitor) -> String {
    if !monitor.enabled {
        return format!("output = {}, disabled = true", lua_str(&monitor.name));
    }
    let mut fields = vec![
        format!("output = {}", lua_str(&monitor.name)),
        format!("mode = {}", lua_str(&monitor.mode)),
        format!(
            "position = \"{}x{}\"",
            monitor.position.x, monitor.position.y
        ),
        format!("scale = {}", lua_num(monitor.scale)),
    ];
    if monitor.transform != 0 {
        fields.push(format!("transform = {}", monitor.transform));
    }
    fields.join(", ")
}

/// Format a value as a Lua double-quoted string with escapes for `\` and `"`.
fn lua_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Format a finite f64 for Lua, preserving fractional part (1.0 stays "1.0").
fn lua_num(n: f64) -> String {
    if !n.is_finite() {
        return "1.0".to_string();
    }
    if n.fract() == 0.0 {
        format!("{:.1}", n)
    } else {
        format!("{}", n)
    }
}

/// Write the config to ~/.config/hypr/monitors.lua
pub fn write_config(profile: &Profile) -> Result<()> {
    let config = generate_config(profile);
    let path = crate::config::hyprland_monitors_lua()?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Temp + rename: Hyprland reads this file on `hyprctl reload` (including
    // from the lid-open bind), so it must never observe a partial write.
    let temp_path = path.with_extension("lua.tmp");
    std::fs::write(&temp_path, config)
        .with_context(|| format!("Failed to write {}", temp_path.display()))?;
    std::fs::rename(&temp_path, &path)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Apply a single monitor configuration via hyprctl eval.
///
/// Hyprland 0.55's non-legacy (Lua) parser rejects `hyprctl keyword monitor`,
/// so we invoke the native `hl.monitor({...})` API through `hyprctl eval`.
pub fn apply_monitor(monitor: &Monitor) -> Result<()> {
    let expr = format!("hl.monitor({{ {} }})", lua_monitor_fields(monitor));
    hyprctl_eval(&expr).with_context(|| format!("Failed to apply monitor {}", monitor.name))
}

/// Move a workspace to a monitor via hyprctl eval.
///
/// Replaces `hyprctl dispatch moveworkspacetomonitor`, which is rejected by
/// Hyprland 0.55's non-legacy parser.
pub fn move_workspace(workspace_id: u8, monitor: &str) -> Result<()> {
    let expr = format!(
        "hl.dispatch(hl.dsp.workspace.move({{ workspace = {}, monitor = {} }}))",
        workspace_id,
        lua_str(monitor)
    );
    hyprctl_eval(&expr).with_context(|| format!("Failed to move workspace {}", workspace_id))
}

/// Run `hyprctl eval <expr>` and surface non-zero exit or in-band Lua errors.
fn hyprctl_eval(expr: &str) -> Result<()> {
    let output = hyprctl_command()
        .args(["eval", expr])
        .output()
        .context("Failed to run hyprctl eval")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("hyprctl eval failed: {}", stderr.trim());
    }

    // hyprctl eval surfaces Lua errors as stdout starting with "error:" but
    // exits 0. Treat that as a failure so callers (and retries) notice.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.starts_with("error:") {
        anyhow::bail!("hyprctl eval failed: {}", trimmed);
    }

    Ok(())
}

/// Apply all monitors from a profile at runtime
pub fn apply_runtime(profile: &Profile) -> Result<()> {
    if !is_running() {
        return Ok(());
    }

    // Apply monitors
    for monitor in &profile.monitors {
        apply_monitor(monitor)?;
    }

    // Wait for monitor changes to take effect
    thread::sleep(Duration::from_millis(MONITOR_APPLY_DELAY_MS));

    // Move existing workspaces to correct monitors
    for ws in &profile.workspaces {
        for attempt in 0..WORKSPACE_MOVE_MAX_RETRIES {
            match move_workspace(ws.id, &ws.monitor) {
                Ok(_) => break,
                Err(_) if attempt < WORKSPACE_MOVE_MAX_RETRIES - 1 => {
                    thread::sleep(Duration::from_millis(WORKSPACE_MOVE_RETRY_DELAY_MS));
                }
                Err(e) => {
                    eprintln!("Warning: Failed to move workspace {}: {}", ws.id, e);
                }
            }
        }
    }

    // Reload so the freshly-written hl.workspace_rule lines take effect in
    // Hyprland's in-memory state, and `configreloaded` fires for bars/tools
    // that cache workspace rules. Eval-style apply above can't re-evaluate
    // workspace_rule lines, so a reload is the only way to refresh them.
    if let Err(e) = hyprctl_reload() {
        eprintln!("Warning: hyprctl reload failed after apply: {}", e);
    }

    Ok(())
}

/// Run `hyprctl reload` to re-evaluate hyprland.lua (and the monitors.lua
/// it sources via pcall) so workspace_rule lines and other top-level state
/// pick up the freshly-written profile.
fn hyprctl_reload() -> Result<()> {
    let output = hyprctl_command()
        .arg("reload")
        .output()
        .context("Failed to run hyprctl reload")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("hyprctl reload failed: {}", stderr.trim());
    }
    Ok(())
}

/// Sort monitors: external first, laptop display (eDP) last
pub fn sort_monitors(monitors: &mut [Monitor]) {
    monitors.sort_by(|a, b| {
        let a_is_edp = a.name.to_lowercase().starts_with("edp");
        let b_is_edp = b.name.to_lowercase().starts_with("edp");
        a_is_edp.cmp(&b_is_edp)
    });
}

/// Auto-arrange monitors left-to-right
pub fn arrange_monitors(monitors: &mut [Monitor]) {
    let mut x_offset = 0;
    for monitor in monitors.iter_mut() {
        if monitor.enabled {
            monitor.position.x = x_offset;
            monitor.position.y = 0;
            let (w, _) = monitor.logical_size();
            x_offset += w;
        }
    }
}

/// Fix y-gaps for stacked monitors by snapping to the nearest overlapping neighbor
///
/// Groups monitors into rows by horizontal adjacency (tiling x-ranges within a
/// similar y-band). For each row group below the top, snaps its minimum y to the
/// bottom edge of the best x-overlapping monitor above, preserving internal y-offsets.
pub fn fix_stacking_gaps(monitors: &mut [Monitor]) {
    let enabled: Vec<usize> = monitors
        .iter()
        .enumerate()
        .filter(|(_, m)| m.enabled)
        .map(|(i, _)| i)
        .collect();

    if enabled.len() <= 1 {
        return;
    }

    // Group into rows by horizontal adjacency:
    // Monitors that tile horizontally (touching/overlapping x-ranges)
    // within a similar y-band belong to the same row.
    let row_groups = group_into_rows(&monitors, &enabled);

    if row_groups.len() <= 1 {
        return;
    }

    // Sort row groups by their minimum y
    let mut sorted_groups = row_groups;
    sorted_groups.sort_by_key(|group| {
        group.iter().map(|&i| monitors[i].position.y).min().unwrap_or(0)
    });

    // For each row group below the first, snap to the bottom edge of the
    // best overlapping monitor/group above, preserving internal y-offsets
    for g in 1..sorted_groups.len() {
        let group = &sorted_groups[g];
        let group_min_y = group.iter().map(|&i| monitors[i].position.y).min().unwrap_or(0);

        // Find the group x-range
        let group_x_min = group.iter()
            .map(|&i| monitors[i].position.x)
            .min().unwrap_or(0);
        let group_x_max = group.iter()
            .map(|&i| {
                let (w, _) = monitors[i].logical_size();
                monitors[i].position.x + w
            })
            .max().unwrap_or(0);
        let group_w = group_x_max - group_x_min;

        // Find best overlap in any row above
        let above_indices: Vec<usize> = sorted_groups[..g].iter().flatten().copied().collect();
        if let Some(target_y) = best_overlap_edge(&monitors, &above_indices, group_x_min, group_w, true) {
            let dy = target_y - group_min_y;
            if dy != 0 {
                for &i in group {
                    monitors[i].position.y += dy;
                }
            }
        }
    }
}

/// Group monitors into rows by horizontal adjacency
/// Monitors whose x-ranges touch or overlap and are within the same y-band
/// are considered part of the same row
fn group_into_rows(monitors: &[Monitor], enabled: &[usize]) -> Vec<Vec<usize>> {
    if enabled.is_empty() {
        return Vec::new();
    }

    // Sort by y then x
    let mut sorted: Vec<usize> = enabled.to_vec();
    sorted.sort_by(|&a, &b| {
        monitors[a].position.y.cmp(&monitors[b].position.y)
            .then(monitors[a].position.x.cmp(&monitors[b].position.x))
    });

    let mut groups: Vec<Vec<usize>> = Vec::new();

    for &idx in &sorted {
        let my = monitors[idx].position.y;
        let mx = monitors[idx].position.x;
        let (mw, mh) = monitors[idx].logical_size();

        // Try to join an existing group where:
        // 1. Y-bands overlap (this monitor's y-range intersects the group's y-range)
        // 2. X-ranges touch (this monitor is horizontally adjacent to some group member)
        let mut joined = false;
        for group in &mut groups {
            let group_y_min = group.iter().map(|&i| monitors[i].position.y).min().unwrap_or(0);
            let group_y_max = group.iter()
                .map(|&i| monitors[i].position.y + monitors[i].logical_size().1)
                .max().unwrap_or(0);

            // Check y-band overlap
            let y_overlaps = my < group_y_max && (my + mh) > group_y_min;
            if !y_overlaps {
                continue;
            }

            // Check x-adjacency with any group member
            let x_adjacent = group.iter().any(|&i| {
                let (iw, _) = monitors[i].logical_size();
                let ix = monitors[i].position.x;
                // Touching or overlapping in x
                mx < (ix + iw) && (mx + mw) > ix
            });

            if x_adjacent {
                group.push(idx);
                joined = true;
                break;
            }
        }

        if !joined {
            groups.push(vec![idx]);
        }
    }

    groups
}

/// Find the edge (bottom or top) of the monitor with the most x-overlap
fn best_overlap_edge(monitors: &[Monitor], indices: &[usize], mx: i32, mw: i32, bottom: bool) -> Option<i32> {
    let mut best_overlap = 0;
    let mut best_edge = None;
    for &j in indices {
        let (jw, jh) = monitors[j].logical_size();
        let jx = monitors[j].position.x;
        let jy = monitors[j].position.y;
        let overlap = (mx + mw).min(jx + jw) - mx.max(jx);
        if overlap > best_overlap {
            best_overlap = overlap;
            best_edge = Some(if bottom { jy + jh } else { jy });
        }
    }
    best_edge
}

/// Generate default workspaces for monitors
pub fn generate_workspaces(monitors: &[Monitor]) -> Vec<Workspace> {
    let mut workspaces = Vec::new();
    let mut ws_id: u8 = 1;

    let enabled_monitors: Vec<_> = monitors.iter().filter(|m| m.enabled).collect();
    let count = enabled_monitors.len();

    for (i, monitor) in enabled_monitors.iter().enumerate() {
        // Distribute the 10 workspaces evenly; earlier monitors absorb the
        // remainder (e.g. 3 monitors -> 4/3/3, not 5/5/0).
        let ws_count = if count == 0 {
            0
        } else {
            10 / count + usize::from(i < 10 % count)
        };

        for j in 0..ws_count {
            if ws_id > 10 {
                break;
            }
            workspaces.push(Workspace {
                id: ws_id,
                monitor: monitor.name.clone(),
                default: j == 0, // First workspace on each monitor is default
            });
            ws_id += 1;
        }
    }

    workspaces
}

/// Generate default lid switch config for laptop display
pub fn generate_lid_switch(monitors: &[Monitor]) -> Option<LidSwitch> {
    // Find laptop display (eDP)
    let edp = monitors
        .iter()
        .find(|m| m.name.to_lowercase().starts_with("edp"))?;

    Some(LidSwitch {
        enabled: true,
        monitor: edp.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrange_monitors_no_gaps_with_mixed_scales() {
        let mut monitors = vec![
            Monitor::test_fixture("DP-10", "3840x2160", 1.5, 0),
            Monitor::test_fixture("DP-11", "3840x2160", 1.5, 0),
            Monitor::test_fixture("DP-6", "1920x1080", 1.0, 3),
            Monitor::test_fixture("eDP-1", "1920x1200", 1.0, 0),
        ];
        arrange_monitors(&mut monitors);

        assert_eq!(monitors[0].position.x, 0);
        assert_eq!(monitors[1].position.x, 2560);
        assert_eq!(monitors[2].position.x, 5120);
        assert_eq!(monitors[3].position.x, 6200);

        for i in 1..monitors.len() {
            let (prev_w, _) = monitors[i - 1].logical_size();
            let expected = monitors[i - 1].position.x + prev_w;
            assert_eq!(
                monitors[i].position.x, expected,
                "gap between monitor {} and {}", i - 1, i
            );
        }
    }

    fn monitor_with_desc(name: &str, desc: &str) -> Monitor {
        let mut m = Monitor::test_fixture(name, "1920x1080", 1.0, 0);
        m.description = Some(desc.to_string());
        m
    }

    #[test]
    fn resolve_monitor_names_handles_port_swap() {
        // Profile labels two identical-model monitors as DP-10 / DP-8 keyed
        // by description; this dock session reshuffles them so the
        // descriptions land on swapped ports DP-8 / DP-10. A per-monitor
        // rename-and-walk approach would collide on the shared old/new
        // label and double-rename one set of workspaces. The two-phase
        // resolver must land each monitor's workspaces on its own screen.
        let mut profile = Profile {
            name: "swap".to_string(),
            description: None,
            monitors: vec![
                monitor_with_desc("DP-10", "Display A"),
                monitor_with_desc("DP-8", "Display B"),
                monitor_with_desc("DP-6", "Display C"),
            ],
            workspaces: vec![
                Workspace { id: 1, monitor: "DP-10".to_string(), default: true },
                Workspace { id: 2, monitor: "DP-10".to_string(), default: false },
                Workspace { id: 5, monitor: "DP-8".to_string(), default: true },
                Workspace { id: 6, monitor: "DP-8".to_string(), default: false },
                Workspace { id: 9, monitor: "DP-6".to_string(), default: true },
            ],
            lid_switch: None,
        };
        let current = vec![
            monitor_with_desc("DP-8", "Display A"),  // was DP-10 in profile
            monitor_with_desc("DP-10", "Display B"), // was DP-8 in profile
            monitor_with_desc("DP-9", "Display C"),  // was DP-6 in profile
        ];

        resolve_monitor_names_with(&mut profile, &current);

        assert_eq!(profile.monitors[0].name, "DP-8");
        assert_eq!(profile.monitors[1].name, "DP-10");
        assert_eq!(profile.monitors[2].name, "DP-9");

        // Workspaces from profile DP-10 (Display A) must end up on live DP-8.
        assert_eq!(profile.workspaces[0].monitor, "DP-8");
        assert_eq!(profile.workspaces[1].monitor, "DP-8");
        // Workspaces from profile DP-8 (Display B) must end up on live DP-10
        // and NOT also get caught by the DP-10 → DP-8 rename above.
        assert_eq!(profile.workspaces[2].monitor, "DP-10");
        assert_eq!(profile.workspaces[3].monitor, "DP-10");
        // Workspaces from profile DP-6 (Display C) must end up on live DP-9.
        assert_eq!(profile.workspaces[4].monitor, "DP-9");
    }

    #[test]
    fn resolve_monitor_names_preserves_unchanged_names() {
        let mut profile = Profile {
            name: "stable".to_string(),
            description: None,
            monitors: vec![monitor_with_desc("eDP-1", "Laptop")],
            workspaces: vec![Workspace {
                id: 1,
                monitor: "eDP-1".to_string(),
                default: true,
            }],
            lid_switch: Some(LidSwitch {
                enabled: true,
                monitor: "eDP-1".to_string(),
            }),
        };
        let current = vec![monitor_with_desc("eDP-1", "Laptop")];
        resolve_monitor_names_with(&mut profile, &current);
        assert_eq!(profile.monitors[0].name, "eDP-1");
        assert_eq!(profile.workspaces[0].monitor, "eDP-1");
        assert_eq!(profile.lid_switch.as_ref().unwrap().monitor, "eDP-1");
    }

    fn make_profile() -> Profile {
        Profile {
            name: "test".to_string(),
            description: Some("smoke".to_string()),
            monitors: vec![
                Monitor::test_fixture("eDP-1", "1920x1200", 1.0, 0),
                Monitor::test_fixture("DP-2", "3840x2160", 1.5, 1),
            ],
            workspaces: vec![
                Workspace { id: 1, monitor: "eDP-1".to_string(), default: true },
                Workspace { id: 2, monitor: "DP-2".to_string(), default: false },
            ],
            lid_switch: Some(LidSwitch {
                enabled: true,
                monitor: "eDP-1".to_string(),
            }),
        }
    }

    #[test]
    fn generate_config_emits_lua_monitor_calls() {
        let p = make_profile();
        let out = generate_config(&p);
        assert!(out.contains("hl.monitor({ output = \"eDP-1\", mode = \"1920x1200@60\", position = \"0x0\", scale = 1.0 })"), "got:\n{}", out);
        assert!(out.contains("hl.monitor({ output = \"DP-2\", mode = \"3840x2160@60\", position = \"0x0\", scale = 1.5, transform = 1 })"), "got:\n{}", out);
    }

    #[test]
    fn generate_config_emits_disabled_monitor() {
        let mut p = make_profile();
        p.monitors[1].enabled = false;
        let out = generate_config(&p);
        assert!(out.contains("hl.monitor({ output = \"DP-2\", disabled = true })"), "got:\n{}", out);
    }

    #[test]
    fn generate_config_emits_workspace_rules() {
        let out = generate_config(&make_profile());
        assert!(out.contains("hl.workspace_rule({ workspace = 1, monitor = \"eDP-1\", default = true })"), "got:\n{}", out);
        assert!(out.contains("hl.workspace_rule({ workspace = 2, monitor = \"DP-2\" })"), "got:\n{}", out);
    }

    #[test]
    fn generate_config_emits_native_lid_close_bind() {
        let out = generate_config(&make_profile());
        assert!(
            out.contains("hl.bind(\"switch:on:Lid Switch\", function() hl.monitor({ output = \"eDP-1\", disabled = true }) end, { locked = true })"),
            "got:\n{}", out
        );
        assert!(
            !out.contains("hyprctl keyword"),
            "generator must not shell to `hyprctl keyword` from inside Lua binds:\n{}", out
        );
    }

    #[test]
    fn generate_config_emits_reload_plus_dpms_on_open() {
        // Runtime `hl.monitor({...})` cannot re-enable a previously-disabled
        // monitor on Hyprland 0.55. Open handler must reload + dpms-on.
        let out = generate_config(&make_profile());
        assert!(
            out.contains("hl.bind(\"switch:off:Lid Switch\", function()"),
            "got:\n{}", out
        );
        assert!(
            out.contains("hl.dispatch(hl.dsp.exec_cmd(\"hyprctl reload\"))"),
            "got:\n{}", out
        );
        assert!(
            out.contains("hl.dispatch(hl.dsp.dpms({ action = \"on\", monitor = \"eDP-1\" }))"),
            "got:\n{}", out
        );
        assert!(
            out.contains("hl.timer(function()"),
            "dpms-on must be deferred via hl.timer per Hyprland's docs:\n{}", out
        );
        assert!(
            out.contains("{ timeout = 500, type = \"oneshot\" }"),
            "got:\n{}", out
        );
    }

    #[test]
    fn generate_config_skips_lid_binds_when_monitor_missing() {
        let mut p = make_profile();
        // Point lid_switch at an output that isn't in the profile
        p.lid_switch.as_mut().unwrap().monitor = "eDP-99".to_string();
        let out = generate_config(&p);
        assert!(
            !out.contains("Lid Switch"),
            "lid binds must be skipped when the referenced output isn't in the profile:\n{}",
            out
        );
    }

    #[test]
    fn generate_config_uses_lua_comment_header() {
        let out = generate_config(&make_profile());
        assert!(out.starts_with("-- Generated by hyprpier from profile: test\n-- smoke"), "got:\n{}", out);
    }

    #[test]
    fn generate_workspaces_covers_every_monitor() {
        let monitors = vec![
            Monitor::test_fixture("DP-1", "1920x1080", 1.0, 0),
            Monitor::test_fixture("DP-2", "1920x1080", 1.0, 0),
            Monitor::test_fixture("eDP-1", "1920x1200", 1.0, 0),
        ];
        let ws = generate_workspaces(&monitors);
        assert_eq!(ws.len(), 10);
        let per = |name: &str| ws.iter().filter(|w| w.monitor == name).count();
        assert_eq!(per("DP-1"), 4);
        assert_eq!(per("DP-2"), 3);
        assert_eq!(per("eDP-1"), 3);
        // First workspace on each monitor is the default
        assert!(ws.iter().filter(|w| w.default).count() == 3);
    }

    #[test]
    fn generate_workspaces_single_monitor_gets_all_ten() {
        let monitors = vec![Monitor::test_fixture("eDP-1", "1920x1200", 1.0, 0)];
        let ws = generate_workspaces(&monitors);
        assert_eq!(ws.len(), 10);
    }

    #[test]
    fn arrange_monitors_unscaled_unchanged_behavior() {
        let mut monitors = vec![
            Monitor::test_fixture("eDP-1", "1920x1200", 1.0, 0),
            Monitor::test_fixture("DP-2", "1920x1080", 1.0, 0),
            Monitor::test_fixture("DP-6", "1920x1080", 1.0, 0),
        ];
        arrange_monitors(&mut monitors);
        assert_eq!(monitors[0].position.x, 0);
        assert_eq!(monitors[1].position.x, 1920);
        assert_eq!(monitors[2].position.x, 3840);
    }
}
