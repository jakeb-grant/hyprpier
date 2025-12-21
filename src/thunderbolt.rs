//! Low-level Thunderbolt sysfs operations
//!
//! This module provides direct access to Thunderbolt device information via sysfs,
//! without requiring boltd/boltctl.

use anyhow::Result;
use std::fs;
use std::path::Path;

const THUNDERBOLT_PATH: &str = "/sys/bus/thunderbolt/devices";

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
