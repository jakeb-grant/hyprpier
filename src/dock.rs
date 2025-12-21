//! Thunderbolt dock detection and management
//!
//! This module provides high-level dock operations built on top of the
//! thunderbolt sysfs module.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::thunderbolt;

const THUNDERBOLT_PATH: &str = "/sys/bus/thunderbolt/devices";

#[derive(Debug, Clone)]
pub struct ThunderboltDevice {
    pub name: String,
    pub uuid: String,
    pub vendor: Option<String>,
    pub is_host: bool,
    pub device_id: String, // e.g., "0-0", "0-1"
}

impl ThunderboltDevice {
    /// Check if this is a dock/peripheral (not the host controller)
    pub fn is_dock(&self) -> bool {
        !self.is_host
    }
}

/// Detect all Thunderbolt devices from sysfs
pub fn list_all_devices() -> Result<Vec<ThunderboltDevice>> {
    let tb_path = Path::new(THUNDERBOLT_PATH);

    if !tb_path.exists() {
        return Ok(Vec::new());
    }

    let mut devices = Vec::new();

    let entries = fs::read_dir(tb_path)
        .context("Failed to read Thunderbolt sysfs")?;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only process device entries (e.g., "0-0", "0-1", "1-0")
        // Skip "domain0", "domain1", etc.
        if !name_str.contains('-') {
            continue;
        }

        let device_path = entry.path();

        // Read device attributes
        let device_name = thunderbolt::read_attr(&device_path, "device_name")
            .unwrap_or_else(|| "Unknown".to_string());
        let vendor = thunderbolt::read_attr(&device_path, "vendor_name");
        let uuid = thunderbolt::read_attr(&device_path, "unique_id")
            .unwrap_or_default();

        // Host controller is typically "X-0" (e.g., "0-0", "1-0")
        let is_host = name_str.ends_with("-0");

        devices.push(ThunderboltDevice {
            name: device_name,
            uuid,
            vendor,
            is_host,
            device_id: name_str.to_string(),
        });
    }

    // Sort by device_id for consistent ordering
    devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));

    Ok(devices)
}

/// Detect connected Thunderbolt docks (peripherals only, not host)
pub fn detect_docks() -> Result<Vec<ThunderboltDevice>> {
    let devices = list_all_devices()?;
    Ok(devices.into_iter().filter(|d| d.is_dock()).collect())
}

/// Get Thunderbolt security mode
pub fn get_security_mode() -> Result<String> {
    thunderbolt::get_security_mode()
}
