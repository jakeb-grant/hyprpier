use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub active_profile: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub dock_profiles: HashMap<String, String>, // uuid -> profile name
    #[serde(default)]
    pub undocked_profile: Option<String>,
}

impl Metadata {
    /// Load metadata from disk, or return default if not exists
    pub fn load() -> Result<Self> {
        let path = config::metadata_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).context("Failed to read metadata")?;
        let metadata: Metadata =
            serde_json::from_str(&content).context("Failed to parse metadata")?;
        Ok(metadata)
    }

    /// Save metadata to disk (atomic write via temp file + rename)
    pub fn save(&self) -> Result<()> {
        config::ensure_profile_dir()?;
        let path = config::metadata_path()?;
        let content = serde_json::to_string_pretty(self).context("Failed to serialize metadata")?;

        // Write to temp file, then rename for atomic save
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, &content).context("Failed to write metadata")?;
        fs::rename(&temp_path, &path).context("Failed to save metadata")?;
        Ok(())
    }

    /// Update the last_modified timestamp to now
    pub fn touch(&mut self) {
        self.last_modified = Some(unix_timestamp());
    }

    /// Set the active profile
    pub fn set_active(&mut self, profile: Option<String>) {
        self.active_profile = profile;
        self.touch();
    }

    /// Link a dock UUID to a profile name
    pub fn link_dock(&mut self, uuid: &str, profile: &str) {
        self.dock_profiles.insert(uuid.to_string(), profile.to_string());
        self.touch();
    }

    /// Unlink a dock UUID
    pub fn unlink_dock(&mut self, uuid: &str) {
        self.dock_profiles.remove(uuid);
        self.touch();
    }

    /// Get the profile linked to a dock UUID
    pub fn get_dock_profile(&self, uuid: &str) -> Option<&String> {
        self.dock_profiles.get(uuid)
    }

    /// Find which dock UUID is linked to a profile (reverse lookup)
    pub fn get_profile_dock(&self, profile: &str) -> Option<&String> {
        self.dock_profiles
            .iter()
            .find(|(_, p)| *p == profile)
            .map(|(uuid, _)| uuid)
    }
}

/// Get current Unix timestamp as a string
fn unix_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
