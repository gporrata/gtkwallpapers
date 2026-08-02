use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperMetadata {
    pub provider: String,
    pub provider_id: String,
    pub search_term: String,
    pub image_url: String,
    pub source_url: Option<String>,
    pub title: Option<String>,
    pub photographer: Option<String>,
    pub photographer_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub likes: Option<u64>,
    pub views: Option<u64>,
    pub downloads: Option<u64>,
    pub favorites: Option<u64>,
    pub category: Option<String>,
    pub purity: Option<String>,
    pub file_size: Option<u64>,
}

pub fn metadata_path(image_path: &Path) -> PathBuf {
    image_path.with_extension("json")
}

pub fn save(image_path: &Path, metadata: &WallpaperMetadata) -> Result<()> {
    let path = metadata_path(image_path);
    let text = serde_json::to_string_pretty(metadata)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn load(image_path: &Path) -> Result<Option<WallpaperMetadata>> {
    let path = metadata_path(image_path);
    if !path.exists() {
        return Ok(None);
    }

    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}
