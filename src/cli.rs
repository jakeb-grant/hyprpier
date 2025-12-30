use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hyprpier")]
#[command(about = "Hyprland monitor profile manager with dock detection")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Apply a monitor profile
    Apply {
        /// Profile name (required unless --auto is used)
        profile: Option<String>,

        /// Auto-detect dock and apply linked profile
        #[arg(long)]
        auto: bool,

        /// Generate config only, don't apply via hyprctl
        #[arg(long)]
        no_runtime: bool,
    },

    /// Launch the TUI manager
    Mgr,

    /// List all profiles
    List,

    /// Show currently active profile
    Current,

    /// Show Thunderbolt device information
    Thunderbolt {
        /// List all Thunderbolt devices
        #[arg(long)]
        list: bool,

        /// Show Thunderbolt security status
        #[arg(long)]
        status: bool,
    },

    /// Install/uninstall udev rules for auto-switching
    Setup {
        /// Remove instead of installing
        #[arg(long)]
        uninstall: bool,

        /// Install/uninstall resume service (resets Thunderbolt controller on wake)
        #[arg(long)]
        resume: bool,
    },

    /// Start the background daemon
    Daemon,

    /// Notify the daemon of a dock event (used by udev)
    #[command(hide = true)]
    Notify,
}
