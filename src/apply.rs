use anyhow::Result;
use notify_rust::Notification;

use crate::dock;
use crate::hyprland;
use crate::metadata::Metadata;
use crate::profile::Profile;

/// Apply a profile by name
pub fn apply_profile(name: &str, no_runtime: bool) -> Result<()> {
    apply_profile_inner(name, no_runtime, false)
}

/// Apply a profile without printing (for TUI use)
pub fn apply_profile_quiet(name: &str, no_runtime: bool) -> Result<()> {
    apply_profile_inner(name, no_runtime, true)
}

fn apply_profile_inner(name: &str, no_runtime: bool, quiet: bool) -> Result<()> {
    let mut profile = Profile::load(name)?;

    // Resolve stored monitor descriptions to current port names
    // This handles dock reconnections that assign different port names
    if let Err(e) = hyprland::resolve_monitor_names(&mut profile) {
        if !quiet {
            eprintln!("Warning: Could not resolve monitor names: {}", e);
        }
        // Continue anyway - will use stored names as fallback
    }

    // Write config file
    hyprland::write_config(&profile)?;

    // Apply at runtime if Hyprland is running and not disabled
    if !no_runtime && hyprland::is_running() {
        hyprland::apply_runtime(&profile)?;
    }

    // Update metadata
    let mut metadata = Metadata::load()?;
    metadata.set_active(Some(name.to_string()));
    metadata.save()?;

    if !quiet {
        println!("Applied profile: {}", name);
    }
    Ok(())
}

/// Auto-detect dock and apply appropriate profile
///
/// Note: Only supports one dock at a time. If multiple docks are connected,
/// the first one with a linked profile wins.
///
/// Skips applying if the target profile is already active (no duplicate notifications).
pub fn apply_auto() -> Result<()> {
    let metadata = Metadata::load()?;
    let docks = dock::detect_docks()?;
    let current_profile = metadata.active_profile.as_deref();

    // Check if any connected dock has a linked profile
    for d in &docks {
        if let Some(profile_name) = metadata.get_dock_profile(&d.uuid) {
            // Skip if already on this profile
            if current_profile == Some(profile_name) {
                return Ok(());
            }
            println!("Detected dock: {} ({})", d.name, d.uuid);
            send_notification(
                "Dock Connected",
                &format!("Applying profile: {}", profile_name),
            );
            return apply_profile(profile_name, false);
        }
    }

    // No dock found or no linked profile - use undocked profile
    if let Some(ref undocked) = metadata.undocked_profile {
        // Skip if already on this profile
        if current_profile == Some(undocked.as_str()) {
            return Ok(());
        }
        if docks.is_empty() {
            println!("No dock detected, applying undocked profile: {}", undocked);
        } else {
            println!(
                "Dock detected but not linked, applying undocked profile: {}",
                undocked
            );
        }
        send_notification("Undocked", &format!("Applying profile: {}", undocked));
        return apply_profile(undocked, false);
    }

    // No undocked profile configured
    if docks.is_empty() {
        println!("No dock detected and no undocked profile configured");
    } else {
        println!("Dock detected but not linked, and no undocked profile configured");
        for d in &docks {
            println!("  - {} ({})", d.name, d.uuid);
        }
    }

    Ok(())
}

/// Send a desktop notification
fn send_notification(summary: &str, body: &str) {
    let _ = Notification::new()
        .summary(summary)
        .body(body)
        .appname("hyprpier")
        .timeout(3000)
        .show();
}

/// Show the currently active profile
pub fn show_current() -> Result<()> {
    let metadata = Metadata::load()?;

    match metadata.active_profile {
        Some(name) => println!("Active profile: {}", name),
        None => println!("No active profile"),
    }

    Ok(())
}

/// List all available profiles
pub fn list_profiles() -> Result<()> {
    let profiles = crate::profile::list_profiles()?;
    let metadata = Metadata::load()?;

    if profiles.is_empty() {
        println!("No profiles found");
        println!("Create profiles with: hyprpier mgr");
        return Ok(());
    }

    println!("Available profiles:");
    for name in profiles {
        let marker = if metadata.active_profile.as_ref() == Some(&name) {
            " (active)"
        } else {
            ""
        };

        // Check if linked to a dock
        let dock_info = if let Some(uuid) = metadata.get_profile_dock(&name) {
            format!(" [dock: {}]", &uuid[..8.min(uuid.len())])
        } else {
            String::new()
        };

        // Check if it's the undocked profile
        let undocked_info = if metadata.undocked_profile.as_ref() == Some(&name) {
            " [undocked]"
        } else {
            ""
        };

        println!("  {}{}{}{}", name, marker, dock_info, undocked_info);
    }

    Ok(())
}
