mod apply;
mod cli;
mod config;
mod daemon;
mod dock;
mod hyprland;
mod metadata;
mod profile;
mod setup;
mod thunderbolt;
mod thunderbolt_cli;
mod tui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Apply {
            profile,
            auto,
            no_runtime,
        } => {
            if auto {
                apply::apply_auto()?;
            } else if let Some(name) = profile {
                apply::apply_profile(&name, no_runtime)?;
            } else {
                eprintln!("Error: Either specify a profile name or use --auto");
                std::process::exit(1);
            }
        }

        Commands::Mgr => {
            let mut app = tui::App::new()?;
            app.run()?;
        }

        Commands::List => {
            apply::list_profiles()?;
        }

        Commands::Current => {
            apply::show_current()?;
        }

        Commands::Thunderbolt { list, status } => {
            if status {
                thunderbolt_cli::show_status()?;
            } else if list {
                thunderbolt_cli::list_devices()?;
            } else {
                // Default to showing status if no flags provided
                thunderbolt_cli::show_status()?;
            }
        }

        Commands::Setup { uninstall, resume } => {
            if resume {
                // Resume service management
                if uninstall {
                    setup::uninstall_resume_service()?;
                } else {
                    setup::install_resume_service()?;
                }
            } else {
                // udev rules management
                if uninstall {
                    setup::uninstall()?;
                } else {
                    setup::install()?;
                }
            }
        }

        Commands::Daemon => {
            daemon::Daemon::new()?.run()?;
        }

        Commands::Notify => {
            daemon::notify("refresh")?;
        }
    }

    Ok(())
}
