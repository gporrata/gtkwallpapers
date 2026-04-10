use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

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
