//! Hyprpier daemon for handling dock events from udev
//!
//! The daemon listens on a Unix socket for commands from udev rules.
//! Running in the user session gives it access to D-Bus, Hyprland, and notifications.

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use crate::apply;
use crate::metadata::Metadata;

const SOCKET_NAME: &str = "hyprpier.sock";
const SETTLE_DELAY_MS: u64 = 3000;

/// Get the socket path ($XDG_RUNTIME_DIR/hyprpier.sock)
pub fn get_socket_path() -> Result<PathBuf> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .context("XDG_RUNTIME_DIR not set - are you in a user session?")?;
    Ok(PathBuf::from(runtime_dir).join(SOCKET_NAME))
}

/// Find any hyprpier socket (for notify command running as root from udev)
/// Searches /run/user/*/hyprpier.sock
fn find_socket_path() -> Result<PathBuf> {
    // First try the normal path
    if let Ok(path) = get_socket_path() {
        if path.exists() {
            return Ok(path);
        }
    }

    // Search all user runtime dirs (for udev context where XDG_RUNTIME_DIR isn't set)
    let run_user = std::path::Path::new("/run/user");
    if let Ok(entries) = std::fs::read_dir(run_user) {
        for entry in entries.flatten() {
            let socket = entry.path().join(SOCKET_NAME);
            if socket.exists() {
                return Ok(socket);
            }
        }
    }

    anyhow::bail!("No hyprpier daemon socket found - is the daemon running?")
}

/// The daemon that listens for dock events
pub struct Daemon {
    socket_path: PathBuf,
    listener: UnixListener,
}

impl Daemon {
    /// Create a new daemon, binding to the socket
    pub fn new() -> Result<Self> {
        let socket_path = get_socket_path()?;

        // Check if another daemon is already running
        if UnixStream::connect(&socket_path).is_ok() {
            anyhow::bail!("Daemon already running on {}", socket_path.display());
        }

        // Try to bind - this is atomic and handles the race condition
        // If bind fails with AddrInUse, another daemon won the race
        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                // Socket exists - try to remove stale socket and retry once
                let _ = std::fs::remove_file(&socket_path);
                UnixListener::bind(&socket_path)
                    .with_context(|| format!("Failed to bind socket (may be in use): {}", socket_path.display()))?
            }
            Err(e) => {
                return Err(e).with_context(|| format!("Failed to bind socket: {}", socket_path.display()));
            }
        };

        println!("Hyprpier daemon listening on {}", socket_path.display());

        Ok(Self {
            socket_path,
            listener,
        })
    }

    /// Run the main event loop
    pub fn run(&mut self) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Err(e) = self.handle_client(stream) {
                        eprintln!("Error handling client: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handle a single client connection
    fn handle_client(&mut self, mut stream: UnixStream) -> Result<()> {
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf)?;

        if n == 0 {
            return Ok(());
        }

        let cmd = String::from_utf8_lossy(&buf[..n]);
        let response = self.process_command(cmd.trim());

        stream.write_all(response.as_bytes())?;
        Ok(())
    }

    /// Process a command and return a response
    fn process_command(&mut self, cmd: &str) -> String {
        match cmd {
            "refresh" => self.handle_refresh(),
            "status" => self.handle_status(),
            _ => format!("ERROR: Unknown command: {}\n", cmd),
        }
    }

    /// Handle refresh command - wait for dock to settle, then apply
    fn handle_refresh(&mut self) -> String {
        // Simple approach: always wait for devices to settle, then apply
        // Multiple notify calls will each wait and apply, but apply_auto()
        // is idempotent - applying the same profile twice is harmless
        std::thread::sleep(Duration::from_millis(SETTLE_DELAY_MS));

        match apply::apply_auto() {
            Ok(_) => "OK\n".to_string(),
            Err(e) => format!("ERROR: {}\n", e),
        }
    }

    /// Handle status command - return current profile
    fn handle_status(&self) -> String {
        match Metadata::load() {
            Ok(metadata) => {
                let profile = metadata.active_profile.as_deref().unwrap_or("none");
                format!("OK: {}\n", profile)
            }
            Err(e) => format!("ERROR: {}\n", e),
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        // Clean up socket on exit
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Send a command to the running daemon
pub fn notify(cmd: &str) -> Result<()> {
    let socket_path = find_socket_path()?;

    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| "Failed to connect to daemon - is it running?")?;

    stream.write_all(cmd.as_bytes())?;

    // Read response
    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    if response.starts_with("ERROR") {
        anyhow::bail!("{}", response.trim());
    }

    Ok(())
}
