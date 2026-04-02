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
const USB4_PCI_CLASS: &str = "0x0c0340";

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

/// Detect all Thunderbolt/USB4 controller PCI addresses
///
/// Scans /sys/bus/pci/devices/ for devices with:
/// 1. class == 0x088000 (Thunderbolt) or 0x0c0340 (USB4)
/// 2. Has a domainN/ subdirectory (marks a controller)
///
/// Returns PCI addresses (e.g., ["0000:00:0d.2", "0000:00:0d.3"])
pub fn get_controller_pci_addresses() -> Vec<String> {
    let pci_path = Path::new(PCI_DEVICES_PATH);

    if !pci_path.exists() {
        return Vec::new();
    }

    let entries = match fs::read_dir(pci_path) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut addresses = Vec::new();

    for entry in entries.flatten() {
        let device_path = entry.path();

        // Check if this is a Thunderbolt (0x088000) or USB4 (0x0c0340) device
        let class_path = device_path.join("class");
        let class = match fs::read_to_string(&class_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let class = class.trim();
        if !class.starts_with(THUNDERBOLT_PCI_CLASS) && !class.starts_with(USB4_PCI_CLASS) {
            continue;
        }

        // Check if this is a controller (has any domainN subdirectory)
        let has_domain = fs::read_dir(&device_path)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("domain")
            });

        if has_domain {
            if let Some(name) = device_path.file_name() {
                addresses.push(name.to_string_lossy().to_string());
            }
        }
    }

    addresses
}
