mod config;
mod daemon;
mod db;
mod metadata;
mod overlay;
mod providers;
mod service;
mod terms;
mod tray;
mod wallpaper;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, ValueEnum)]
enum Provider {
    Unsplash,
    Pexels,
    Pixabay,
    Wallhaven,
}

#[derive(Parser)]
#[command(
    name = "gtkwallpapers",
    about = "Rotating wallpaper daemon for GNOME/GTK desktops"
)]
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
    /// Restart the background wallpaper service
    Restart,
    /// Show the current status of the wallpaper service
    Status,
    /// Uninstall the wallpaper service
    Uninstall,
    /// Manage search terms used across all photo providers
    Terms {
        /// One or more search terms to add (omit to open interactive selection)
        terms: Vec<String>,
    },
    /// Set or clear an API key for a photo provider (omit to list providers)
    Key {
        /// Photo provider: unsplash, pexels, pixabay, wallhaven
        provider: Option<Provider>,
        /// API key to store (omit to clear)
        key: Option<String>,
    },
    /// Set how often the wallpaper rotates (e.g. 30m, 1h)
    Update { frequency: String },
    /// Download wallpapers from all configured photo providers
    Init,
    /// Switch to the next wallpaper immediately
    Next,
    /// Print the folders where wallpapers are stored and the database path
    Path,
    /// List blocked photographers and wallpaper history counts
    Blocked,
    /// Block a photographer: never download their work and remove cached wallpapers
    Block { name: String },
    /// Unblock a previously blocked photographer
    Unblock { name: String },
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
        Some(Command::Restart) => service::restart()?,
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
        Some(Command::Key { provider: None, .. }) => {
            let cfg = config::load()?;
            println!("{:<12} {}", "PROVIDER", "KEY");
            println!("{:<12} {}", "unsplash", key_status(&cfg.unsplash_api_key));
            println!("{:<12} {}", "pexels", key_status(&cfg.pexels_api_key));
            println!("{:<12} {}", "pixabay", key_status(&cfg.pixabay_api_key));
            println!("{:<12} {}", "wallhaven", key_status(&cfg.wallhaven_api_key));
        }
        Some(Command::Key {
            provider: Some(provider),
            key,
        }) => {
            let mut cfg = config::load()?;
            let (slot, name) = match provider {
                Provider::Unsplash => (&mut cfg.unsplash_api_key, "unsplash"),
                Provider::Pexels => (&mut cfg.pexels_api_key, "pexels"),
                Provider::Pixabay => (&mut cfg.pixabay_api_key, "pixabay"),
                Provider::Wallhaven => (&mut cfg.wallhaven_api_key, "wallhaven"),
            };
            match key {
                Some(k) => {
                    *slot = Some(k);
                    config::save(&cfg)?;
                    println!("{name} API key saved.");
                }
                None => {
                    if slot.take().is_some() {
                        config::save(&cfg)?;
                        println!("{name} API key cleared.");
                    } else {
                        println!("No {name} API key was set.");
                    }
                }
            }
        }
        Some(Command::Update { frequency }) => {
            let duration = humantime::parse_duration(&frequency)?;
            let mut cfg = config::load()?;
            cfg.frequency_secs = duration.as_secs();
            config::save(&cfg)?;
            println!("Wallpaper rotation set to {frequency}");
        }
        Some(Command::Init) => {
            let cfg = config::load()?;
            if cfg.terms.is_empty() {
                anyhow::bail!(
                    "No search terms configured. Add some with `gtkwallpapers terms <term>`."
                );
            }
            println!("Downloading wallpapers from all configured providers…");
            let saved = providers::download_all(&reqwest::Client::new(), &cfg).await?;
            println!("{} wallpaper(s) downloaded.", saved.len());
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
            println!("{}", db::db_path()?.display());
        }
        Some(Command::Blocked) => {
            let blocked = db::blocked_photographers()?;
            if blocked.is_empty() {
                println!("No blocked photographers.");
            } else {
                println!("Blocked photographers:");
                for name in blocked {
                    println!("  {name}");
                }
            }
            let counts = db::status_counts()?;
            if !counts.is_empty() {
                println!("\nWallpaper history:");
                for (status, count) in counts {
                    println!("  {status:<12} {count}");
                }
            }
        }
        Some(Command::Block { name }) => {
            let removed = db::block_photographer(&name)?;
            println!("Blocked photographer {name}; removed {removed} cached wallpaper(s).");
        }
        Some(Command::Unblock { name }) => {
            if db::unblock_photographer(&name)? {
                println!("Unblocked photographer {name}.");
            } else {
                println!("{name} was not blocked.");
            }
        }
        Some(Command::Daemon) => daemon::run().await?,
    }

    Ok(())
}

fn key_status(key: &Option<String>) -> &str {
    if key.is_some() { "set" } else { "not set" }
}
