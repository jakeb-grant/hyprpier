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
            // Resolve the real home from /etc/passwd; fall back to
            // /home/{user} for users resolved via NSS (LDAP etc.) that
            // aren't listed there.
            let home = home_from_passwd(&sudo_user)
                .unwrap_or_else(|| PathBuf::from(format!("/home/{}", sudo_user)));
            return Ok(home.join(".config"));
        }
    }

    dirs::config_dir().context("Could not find config directory")
}

/// Look up a user's home directory in /etc/passwd
/// (format: name:passwd:uid:gid:gecos:home:shell)
fn home_from_passwd(user: &str) -> Option<PathBuf> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? != user {
            return None;
        }
        let home = fields.nth(4)?;
        if home.is_empty() {
            None
        } else {
            Some(PathBuf::from(home))
        }
    })
}

/// Get the profile directory (~/.config/hyprpier/)
pub fn profile_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("hyprpier"))
}

/// Get the metadata file path (~/.config/hyprpier/.metadata.json)
pub fn metadata_path() -> Result<PathBuf> {
    Ok(profile_dir()?.join(".metadata.json"))
}

/// Get the Hyprland monitors.lua output path (~/.config/hypr/monitors.lua)
pub fn hyprland_monitors_lua() -> Result<PathBuf> {
    Ok(config_dir()?.join("hypr").join("monitors.lua"))
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
