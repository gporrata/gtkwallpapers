use anyhow::{Context, Result};
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// Seconds between wallpaper rotations (default: 30 minutes)
    pub frequency_secs: u64,
    /// Flickr search terms used to download wallpapers
    pub flickr_terms: Vec<String>,
    /// Flickr API key (optional; enables higher rate limits)
    pub flickr_api_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frequency_secs: 30 * 60,
            flickr_terms: Vec::new(),
            flickr_api_key: None,
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dir = config_dir()
        .context("could not resolve $HOME/.config")?
        .join("gtkwallpapers");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn wallpapers_dir() -> Result<PathBuf> {
    let dir = config_dir()
        .context("could not resolve $HOME/.config")?
        .join("gtkwallpapers")
        .join("wallpapers");
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
