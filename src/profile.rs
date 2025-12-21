use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::config;

const MAX_PROFILE_NAME_LENGTH: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub monitors: Vec<Monitor>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub lid_switch: Option<LidSwitch>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    pub name: String,
    /// Stable hardware identifier (e.g., "Ancor Communications Inc ASUS VS239 L3LMTF263862")
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub resolution: String,
    pub refresh_rate: f64,
    pub position: Position,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub transform: u8,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: u8,
    pub monitor: String,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LidSwitch {
    pub enabled: bool,
    pub monitor: String,
    pub on_close: String,
    pub on_open: String,
}

fn default_true() -> bool {
    true
}

fn default_scale() -> f64 {
    1.0
}

/// Validate a profile name for use as a filename
/// Returns Ok(()) if valid, Err with message if invalid
pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Profile name cannot be empty");
    }

    if name.len() > MAX_PROFILE_NAME_LENGTH {
        anyhow::bail!("Profile name too long (max {} characters)", MAX_PROFILE_NAME_LENGTH);
    }

    if name.starts_with('.') {
        anyhow::bail!("Profile name cannot start with '.'");
    }

    if name.contains('/') || name.contains('\\') {
        anyhow::bail!("Profile name cannot contain path separators");
    }

    if name.contains("..") {
        anyhow::bail!("Profile name cannot contain '..'");
    }

    // Check for other problematic characters
    let invalid_chars = ['<', '>', ':', '"', '|', '?', '*', '\0'];
    for c in invalid_chars {
        if name.contains(c) {
            anyhow::bail!("Profile name contains invalid character: '{}'", c);
        }
    }

    Ok(())
}

impl Profile {
    /// Create a new empty profile with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            monitors: Vec::new(),
            workspaces: Vec::new(),
            lid_switch: None,
        }
    }

    /// Load a profile from disk by name
    pub fn load(name: &str) -> Result<Self> {
        let path = config::profile_path(name)?;
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read profile: {}", name))?;
        let profile: Profile = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse profile: {}", name))?;
        Ok(profile)
    }

    /// Save this profile to disk (atomic write via temp file + rename)
    pub fn save(&self) -> Result<()> {
        config::ensure_profile_dir()?;
        let path = config::profile_path(&self.name)?;
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize profile")?;

        // Write to temp file, then rename for atomic save
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, content)
            .with_context(|| format!("Failed to write profile: {}", self.name))?;
        fs::rename(&temp_path, &path)
            .with_context(|| format!("Failed to save profile: {}", self.name))?;
        Ok(())
    }

    /// Delete this profile from disk
    pub fn delete(name: &str) -> Result<()> {
        let path = config::profile_path(name)?;
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to delete profile: {}", name))?;
        }
        Ok(())
    }
}

/// List all available profile names
pub fn list_profiles() -> Result<Vec<String>> {
    let dir = config::profile_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Some(name) = path.file_stem() {
                let name = name.to_string_lossy().to_string();
                // Skip metadata file
                if !name.starts_with('.') {
                    profiles.push(name);
                }
            }
        }
    }

    profiles.sort();
    Ok(profiles)
}
