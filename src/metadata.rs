use serde::{Deserialize, Serialize};

/// Provider metadata for one wallpaper. Stored as JSON in the `metadata`
/// column of the wallpaper database (see `db.rs`).
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
    #[serde(default)]
    pub tags: Vec<String>,
    pub likes: Option<u64>,
    pub views: Option<u64>,
    pub downloads: Option<u64>,
    pub favorites: Option<u64>,
    pub category: Option<String>,
    pub purity: Option<String>,
    pub file_size: Option<u64>,
}
