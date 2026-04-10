mod config;
mod daemon;
mod flickr;
mod service;
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
    /// Manage Flickr search terms used for downloading wallpapers
    Flickr {
        /// One or more search terms to add (omit to open interactive selection)
        terms: Vec<String>,
    },
    /// Set or clear the Flickr API key
    Flickrkey {
        /// API key to store (omit to clear the existing key)
        key: Option<String>,
    },
    /// Set how often the wallpaper rotates (e.g. 30m, 1h)
    Update {
        frequency: String,
    },
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
        Some(Command::Flickr { terms }) if !terms.is_empty() => {
            let mut cfg = config::load()?;
            for t in terms {
                if !cfg.flickr_terms.contains(&t) {
                    cfg.flickr_terms.push(t.clone());
                    println!("Added search term: {t}");
                } else {
                    println!("Term already exists: {t}");
                }
            }
            config::save(&cfg)?;
        }
        Some(Command::Flickr { .. }) => flickr::interactive_terms()?,
        Some(Command::Flickrkey { key: Some(k) }) => {
            let mut cfg = config::load()?;
            cfg.flickr_api_key = Some(k);
            config::save(&cfg)?;
            println!("Flickr API key saved.");
        }
        Some(Command::Flickrkey { key: None }) => {
            let mut cfg = config::load()?;
            if cfg.flickr_api_key.take().is_some() {
                config::save(&cfg)?;
                println!("Flickr API key cleared.");
            } else {
                println!("No Flickr API key was set.");
            }
        }
        Some(Command::Update { frequency }) => {
            let duration = humantime::parse_duration(&frequency)?;
            let mut cfg = config::load()?;
            cfg.frequency_secs = duration.as_secs();
            config::save(&cfg)?;
            println!("Wallpaper rotation set to {frequency}");
        }
        Some(Command::Path) => {
            println!("{}", config::wallpapers_dir()?.display());
        }
        Some(Command::Daemon) => daemon::run().await?,
    }

    Ok(())
}
