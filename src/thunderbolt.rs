//! Low-level Thunderbolt sysfs operations
//!
//! This module provides direct access to Thunderbolt device information via sysfs,
//! without requiring boltd/boltctl.

use anyhow::Result;
use std::fs;
use std::path::Path;

const THUNDERBOLT_PATH: &str = "/sys/bus/thunderbolt/devices";
const PCI_DEVICES_PATH: &str = "/sys/bus/pci/devices";
const THUNDERBOLT_PCI_CLASS: &str = "0x088000";

/// Read a sysfs attribute, returning None if it doesn't exist or can't be read
pub fn read_attr(device_path: &Path, attr: &str) -> Option<String> {
    let path = device_path.join(attr);
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Get Thunderbolt security mode from sysfs
pub fn get_security_mode() -> Result<String> {
    let sys_path = Path::new(THUNDERBOLT_PATH).join("domain0/security");

    if sys_path.exists() {
        let mode = fs::read_to_string(&sys_path)
            .unwrap_or_else(|_| "unknown".to_string())
            .trim()
            .to_string();
        return Ok(mode);
    }

    Ok("unknown".to_string())
}

/// Detect the main Thunderbolt controller PCI address
///
/// Scans /sys/bus/pci/devices/ for devices with:
/// 1. class == 0x088000 (Thunderbolt class code)
/// 2. Has a domain0/ subdirectory (marks the main controller)
///
/// Returns the PCI address (e.g., "0000:04:00.0") or None if not found
pub fn get_controller_pci_address() -> Option<String> {
    let pci_path = Path::new(PCI_DEVICES_PATH);

    if !pci_path.exists() {
        return None;
    }

    let entries = fs::read_dir(pci_path).ok()?;

    for entry in entries.flatten() {
        let device_path = entry.path();

        // Check if this is a Thunderbolt device (class 0x088000)
        let class_path = device_path.join("class");
        if let Ok(class) = fs::read_to_string(&class_path) {
            if !class.trim().starts_with(THUNDERBOLT_PCI_CLASS) {
                continue;
            }
        } else {
            continue;
        }

        // Check if this is the main controller (has domain0 subdirectory)
        let domain0_path = device_path.join("domain0");
        if domain0_path.exists() {
            // Return the PCI address (directory name)
            if let Some(name) = device_path.file_name() {
                return Some(name.to_string_lossy().to_string());
            }
        }
    }

    None
}
