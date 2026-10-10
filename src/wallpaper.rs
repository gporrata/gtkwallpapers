use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::config::{self, SERVICE_NAMES};
use crate::metadata::WallpaperMetadata;

/// All cached images that may be shown. Files the database says are deleted
/// or blocked are removed from disk on the way.
fn available_images() -> Vec<PathBuf> {
    let hidden = crate::db::hidden_paths().unwrap_or_else(|e| {
        eprintln!("Failed to read wallpaper database: {e}");
        HashSet::new()
    });

    SERVICE_NAMES
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
        .filter(|p| {
            if hidden.contains(p) {
                let _ = std::fs::remove_file(p);
                false
            } else {
                true
            }
        })
        .collect()
}

/// Pick a pseudo-random image from any of the provider directories.
pub fn pick_random() -> Result<PathBuf> {
    let images = available_images();

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

/// Returns true if there are no showable wallpapers cached.
pub fn pool_is_empty() -> bool {
    available_images().is_empty()
}

/// Apply a wallpaper via gsettings (GNOME/GTK).
/// The wallpaper actually applied is a copy with the file name burned into
/// the lower-right corner; if rendering fails the original is used.
pub fn set(path: &Path) -> Result<()> {
    let shown = crate::overlay::render_labelled(path).unwrap_or_else(|e| {
        eprintln!("Failed to label wallpaper: {e}");
        path.to_path_buf()
    });
    let uri = format!("file://{}", shown.display());

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

/// What the user chose in the info dialog.
pub enum InfoAction {
    None,
    Delete,
    BlockPhotographer(String),
}

const DELETE_BUTTON: &str = "Delete wallpaper";
const BLOCK_BUTTON: &str = "Block photographer";

/// Show wallpaper info. The dialog offers "Delete wallpaper" and, when the
/// photographer is known, "Block photographer" buttons.
pub fn show_info_dialog(path: &Path) -> Result<InfoAction> {
    let record = crate::db::find_by_path(path).ok().flatten();
    let photographer = record
        .as_ref()
        .and_then(|r| r.photographer.clone())
        .filter(|name| !name.trim().is_empty());
    let message = info_text(path, record.as_ref())?;

    let mut buttons = vec![DELETE_BUTTON];
    if photographer.is_some() {
        buttons.push(BLOCK_BUTTON);
    }

    let choice = match zenity_info("Wallpaper Info", &message, &buttons)? {
        Some(choice) => choice,
        None => {
            if !show_kdialog("Wallpaper Info", &message)? {
                println!("{message}");
            }
            return Ok(InfoAction::None);
        }
    };

    Ok(match (choice.as_str(), photographer) {
        (DELETE_BUTTON, _) => InfoAction::Delete,
        (BLOCK_BUTTON, Some(name)) => InfoAction::BlockPhotographer(name),
        _ => InfoAction::None,
    })
}

/// Show the configured search terms and blocked photographers.
pub fn show_terms_dialog(terms: &[String]) -> Result<()> {
    let blocked = crate::db::blocked_photographers().unwrap_or_default();
    let mut lines = vec!["Search terms".to_owned()];
    if terms.is_empty() {
        lines.push("  (none)".to_owned());
    }
    lines.extend(terms.iter().map(|t| format!("  • {t}")));
    lines.push(String::new());
    lines.push("Blocked photographers".to_owned());
    if blocked.is_empty() {
        lines.push("  (none)".to_owned());
    }
    lines.extend(blocked.iter().map(|b| format!("  • {b}")));
    lines.push(String::new());
    lines.push("Unblock from the tray menu or with `gtkwallpapers unblock <name>`.".to_owned());
    let message = lines.join("\n");

    if zenity_info("Search Terms & Blocked Photographers", &message, &[])?.is_none()
        && !show_kdialog("Search Terms & Blocked Photographers", &message)?
    {
        println!("{message}");
    }
    Ok(())
}

fn info_text(path: &Path, record: Option<&crate::db::Record>) -> Result<String> {
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

    if let Some(record) = record {
        lines.push(format!("Status: {}", record.status));
        append_metadata_lines(&mut lines, &record.metadata);
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

/// Show a zenity info dialog with optional extra buttons. Returns `None` if
/// zenity is not installed, otherwise the label of the extra button pressed
/// (empty string for OK / close).
fn zenity_info(title: &str, message: &str, buttons: &[&str]) -> Result<Option<String>> {
    let mut cmd = Command::new("zenity");
    cmd.args([
        "--info",
        "--title",
        title,
        "--no-markup",
        "--width",
        "420",
        "--text",
        message,
    ]);
    for button in buttons {
        cmd.args(["--extra-button", button]);
    }

    match cmd.output() {
        Ok(out) => Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn show_kdialog(title: &str, message: &str) -> Result<bool> {
    match Command::new("kdialog")
        .args(["--title", title, "--msgbox", message])
        .status()
    {
        Ok(status) => Ok(status.success()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
