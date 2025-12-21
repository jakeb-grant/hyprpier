//! Thunderbolt device information commands
//!
//! Provides CLI commands for viewing Thunderbolt device information.

use anyhow::Result;

use crate::dock;

/// List all Thunderbolt devices
pub fn list_devices() -> Result<()> {
    let devices = dock::list_all_devices()?;

    if devices.is_empty() {
        println!("No Thunderbolt devices found");
        return Ok(());
    }

    println!("Thunderbolt devices:");
    for device in devices {
        let vendor = device.vendor.as_deref().unwrap_or("unknown vendor");
        let device_type = if device.is_host { "host" } else { "peripheral" };

        println!();
        println!("  {} ({})", device.name, vendor);
        println!("    UUID: {}", device.uuid);
        println!("    Device ID: {}", device.device_id);
        println!("    Type: {}", device_type);
    }

    Ok(())
}

/// Show Thunderbolt security status
pub fn show_status() -> Result<()> {
    let mode = dock::get_security_mode()?;

    println!("Thunderbolt security mode: {}", mode);
    println!();
    match mode.as_str() {
        "none" => println!("All devices are automatically authorized"),
        "user" => println!("Devices require user authorization"),
        "secure" => println!("Devices require secure key exchange"),
        "dponly" => println!("Only DisplayPort tunneling allowed (no PCIe/USB)"),
        _ => println!("Unknown security mode"),
    }

    Ok(())
}
