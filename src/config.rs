use anyhow::{Context, Result};
use std::path::PathBuf;

/// Get the config directory, respecting SUDO_USER when running with sudo
fn config_dir() -> Result<PathBuf> {
    // If running with sudo (e.g., hyprpier setup), use the original user's config
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        // Validate username: alphanumeric, underscore, hyphen only (no path traversal)
        let is_valid = !sudo_user.is_empty()
            && sudo_user != "root"
            && sudo_user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

        if is_valid {
            let path = PathBuf::from(format!("/home/{}/.config", sudo_user));
            // Extra safety: ensure the resolved path is under /home/
            if path.starts_with("/home/") {
                return Ok(path);
            }
        }
    }

    dirs::config_dir().context("Could not find config directory")
}

/// Get the profile directory (~/.config/hyprpier/)
pub fn profile_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("hyprpier"))
}

/// Get the metadata file path (~/.config/hyprpier/.metadata.json)
pub fn metadata_path() -> Result<PathBuf> {
    Ok(profile_dir()?.join(".metadata.json"))
}

/// Get the Hyprland monitors.conf output path (~/.config/hypr/monitors.conf)
pub fn hyprland_monitors_conf() -> Result<PathBuf> {
    Ok(config_dir()?.join("hypr").join("monitors.conf"))
}

/// Ensure the profile directory exists
pub fn ensure_profile_dir() -> Result<()> {
    let dir = profile_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create profile directory: {}", dir.display()))?;
    }
    Ok(())
}

/// Get the path for a specific profile
pub fn profile_path(name: &str) -> Result<PathBuf> {
    Ok(profile_dir()?.join(format!("{}.json", name)))
}
