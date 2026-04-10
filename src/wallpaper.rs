use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, SERVICE_NAMES};

/// Pick a pseudo-random image from any of the provider directories.
pub fn pick_random() -> Result<PathBuf> {
    let images: Vec<PathBuf> = SERVICE_NAMES
        .iter()
        .filter_map(|name| config::service_dir(name).ok())
        .flat_map(|dir| {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("jpg" | "jpeg" | "png" | "webp")
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    if images.is_empty() {
        bail!("No wallpapers available. Add search terms with `gtkwallpapers terms <term>`.");
    }

    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system time error")?
        .subsec_nanos() as usize)
        % images.len();

    Ok(images[idx].clone())
}

/// Returns true if all provider directories are empty (or don't exist yet).
pub fn pool_is_empty() -> bool {
    SERVICE_NAMES.iter().all(|name| {
        config::service_dir(name)
            .ok()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .map(|mut d| d.next().is_none())
            .unwrap_or(true)
    })
}

/// Apply a wallpaper via gsettings (GNOME/GTK).
pub fn set(path: &Path) -> Result<()> {
    let uri = format!("file://{}", path.display());

    let status = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.background", "picture-uri", &uri])
        .status()?;

    if !status.success() {
        bail!("gsettings failed for {uri}");
    }

    // Also update the dark-mode variant so both themes rotate.
    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.background", "picture-uri-dark", &uri])
        .status();

    Ok(())
}
