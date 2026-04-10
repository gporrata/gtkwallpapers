mod config;
mod daemon;
mod providers;
mod service;
mod terms;
mod wallpaper;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gtkwallpapers", about = "Rotating wallpaper daemon for GNOME/GTK desktops")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Install and start the background wallpaper service
    Start,
    /// Stop the background wallpaper service
    Stop,
    /// Show the current status of the wallpaper service
    Status,
    /// Uninstall the wallpaper service
    Uninstall,
    /// Manage search terms used across all photo providers
    Terms {
        /// One or more search terms to add (omit to open interactive selection)
        terms: Vec<String>,
    },
    /// Set how often the wallpaper rotates (e.g. 30m, 1h)
    Update {
        frequency: String,
    },
    /// Switch to the next wallpaper immediately
    Next,
    /// Print the folder where wallpapers are stored
    Path,
    /// Run the daemon loop (called internally by the systemd service)
    #[command(hide = true)]
    Daemon,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            Cli::parse_from(["gtkwallpapers", "--help"]);
        }
        Some(Command::Start) => service::start()?,
        Some(Command::Stop) => service::stop()?,
        Some(Command::Status) => service::status()?,
        Some(Command::Uninstall) => service::uninstall()?,
        Some(Command::Terms { terms }) if !terms.is_empty() => {
            let mut cfg = config::load()?;
            for t in terms {
                if !cfg.terms.contains(&t) {
                    cfg.terms.push(t.clone());
                    println!("Added search term: {t}");
                } else {
                    println!("Term already exists: {t}");
                }
            }
            config::save(&cfg)?;
        }
        Some(Command::Terms { .. }) => terms::interactive_terms()?,
        Some(Command::Update { frequency }) => {
            let duration = humantime::parse_duration(&frequency)?;
            let mut cfg = config::load()?;
            cfg.frequency_secs = duration.as_secs();
            config::save(&cfg)?;
            println!("Wallpaper rotation set to {frequency}");
        }
        Some(Command::Next) => {
            let cfg = config::load()?;
            if wallpaper::pool_is_empty() && !cfg.terms.is_empty() {
                println!("Wallpaper pool empty — downloading from providers…");
                providers::download_all(&reqwest::Client::new(), &cfg).await?;
            }
            let chosen = wallpaper::pick_random()?;
            wallpaper::set(&chosen)?;
            println!("Wallpaper set: {}", chosen.display());
        }
        Some(Command::Path) => {
            for name in config::SERVICE_NAMES {
                println!("{}", config::service_dir(name)?.display());
            }
        }
        Some(Command::Daemon) => daemon::run().await?,
    }

    Ok(())
}
