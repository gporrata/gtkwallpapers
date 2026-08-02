use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::config::{self, SERVICE_NAMES};
use crate::metadata::WallpaperMetadata;

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
        .args([
            "set",
            "org.gnome.desktop.background",
            "picture-uri-dark",
            &uri,
        ])
        .status();

    Ok(())
}

pub fn show_info_dialog(path: &Path) -> Result<()> {
    let message = info_text(path)?;

    if show_zenity_dialog(&message)? || show_kdialog(&message)? {
        return Ok(());
    }

    println!("{message}");
    Ok(())
}

fn info_text(path: &Path) -> Result<String> {
    let base_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown");
    let provider = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown");
    let (width, height) = image::image_dimensions(path)
        .with_context(|| format!("failed to read image dimensions for {}", path.display()))?;
    let downloaded = downloaded_at(path)?;

    let mut lines = vec![
        format!("Base name: {base_name}"),
        format!("Image size: {width} x {height}"),
        format!("API: {provider}"),
        format!("Downloaded: {downloaded}"),
    ];

    if let Ok(Some(metadata)) = crate::metadata::load(path) {
        append_metadata_lines(&mut lines, &metadata);
    }

    Ok(lines.join("\n"))
}

fn append_metadata_lines(lines: &mut Vec<String>, metadata: &WallpaperMetadata) {
    lines.push(String::new());
    lines.push("Metadata".to_owned());
    lines.push(format!("Provider ID: {}", metadata.provider_id));
    lines.push(format!("Search term: {}", metadata.search_term));

    push_optional_line(lines, "Title", metadata.title.as_deref());
    push_optional_line(lines, "Photographer", metadata.photographer.as_deref());
    push_optional_line(
        lines,
        "Photographer URL",
        metadata.photographer_url.as_deref(),
    );
    push_optional_line(lines, "Source URL", metadata.source_url.as_deref());
    push_optional_line(lines, "Image URL", Some(&metadata.image_url));

    if let (Some(width), Some(height)) = (metadata.width, metadata.height) {
        lines.push(format!("Provider size: {width} x {height}"));
    }

    push_optional_line(lines, "Color", metadata.color.as_deref());

    if !metadata.tags.is_empty() {
        lines.push(format!("Tags: {}", metadata.tags.join(", ")));
    }

    push_optional_u64_line(lines, "Likes", metadata.likes);
    push_optional_u64_line(lines, "Views", metadata.views);
    push_optional_u64_line(lines, "Downloads", metadata.downloads);
    push_optional_u64_line(lines, "Favorites", metadata.favorites);
    push_optional_line(lines, "Category", metadata.category.as_deref());
    push_optional_line(lines, "Purity", metadata.purity.as_deref());

    if let Some(file_size) = metadata.file_size {
        lines.push(format!("File size: {}", format_bytes(file_size)));
    }
}

fn push_optional_line(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        lines.push(format!("{label}: {value}"));
    }
}

fn push_optional_u64_line(lines: &mut Vec<String>, label: &str, value: Option<u64>) {
    if let Some(value) = value {
        lines.push(format!("{label}: {value}"));
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn downloaded_at(path: &Path) -> Result<String> {
    let metadata = std::fs::metadata(path)?;
    let time = metadata
        .created()
        .or_else(|_| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    Ok(humantime::format_rfc3339_seconds(time).to_string())
}

fn show_zenity_dialog(message: &str) -> Result<bool> {
    match Command::new("zenity")
        .args([
            "--info",
            "--title",
            "Wallpaper Info",
            "--no-markup",
            "--width",
            "420",
            "--text",
            message,
        ])
        .status()
    {
        Ok(status) => Ok(status.success()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn show_kdialog(message: &str) -> Result<bool> {
    match Command::new("kdialog")
        .args(["--title", "Wallpaper Info", "--msgbox", message])
        .status()
    {
        Ok(status) => Ok(status.success()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
