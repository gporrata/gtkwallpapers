use anyhow::{Context, Result};
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SERVICE_NAMES: &[&str] = &["unsplash", "pexels", "pixabay", "wallhaven"];

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Config {
    /// Seconds between wallpaper rotations (default: 30 minutes)
    #[serde(default = "default_frequency")]
    pub frequency_secs: u64,
    /// Search terms used across all photo providers
    #[serde(default)]
    pub terms: Vec<String>,
    /// Unsplash API key (required)
    pub unsplash_api_key: Option<String>,
    /// Pexels API key (required)
    pub pexels_api_key: Option<String>,
    /// Pixabay API key (required)
    pub pixabay_api_key: Option<String>,
    /// Wallhaven API key (optional; needed for non-SFW content)
    pub wallhaven_api_key: Option<String>,
}

fn default_frequency() -> u64 {
    30 * 60
}

fn base_dir() -> Result<PathBuf> {
    let dir = config_dir()
        .context("could not resolve $HOME/.config")?
        .join("gtkwallpapers");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(base_dir()?.join("config.json"))
}

/// Returns ~/.config/gtkwallpapers/<service>/, creating it if needed.
pub fn service_dir(service: &str) -> Result<PathBuf> {
    let dir = base_dir()?.join(service);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)?;
    serde_json::from_str(&text).context("failed to parse config.json")
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, text)?;
    Ok(())
}
