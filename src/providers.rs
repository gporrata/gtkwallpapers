use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::{self, Config};
use crate::db;
use crate::metadata::WallpaperMetadata;

// ---------------------------------------------------------------------------
// Top-level entry point
// ---------------------------------------------------------------------------

/// Download one photo per term from every configured provider.
/// Skips providers whose API key is not set (except Wallhaven, which is
/// usable without a key for SFW content).
pub async fn download_all(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let mut saved = Vec::new();

    macro_rules! run {
        ($fut:expr) => {
            match $fut.await {
                Ok(mut paths) => saved.append(&mut paths),
                Err(e) => eprintln!("Download error: {e}"),
            }
        };
    }

    if cfg.unsplash_api_key.is_some() {
        run!(unsplash(client, cfg));
    }
    if cfg.pexels_api_key.is_some() {
        run!(pexels(client, cfg));
    }
    if cfg.pixabay_api_key.is_some() {
        run!(pixabay(client, cfg));
    }
    // Wallhaven works without a key (SFW content only).
    run!(wallhaven(client, cfg));

    Ok(saved)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn random_index(len: usize) -> usize {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize)
        % len
}

async fn save_image(client: &Client, url: &str, dest: &Path) -> Result<()> {
    let resp = client.get(url).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    std::fs::write(dest, &bytes)?;
    Ok(())
}

/// What must not be downloaded, loaded from the wallpaper database. This is
/// independent of what happens to be on disk, so clearing the cache never
/// brings back deleted wallpapers or blocked photographers.
struct Exclusions {
    ids: HashSet<String>,
    photographers: HashSet<String>,
}

impl Exclusions {
    fn load(provider: &str) -> Result<Self> {
        Ok(Self {
            ids: db::skip_ids(provider)?,
            photographers: db::blocked_photographer_keys()?,
        })
    }

    fn allows(&self, id: &str, photographer: Option<&str>) -> bool {
        !self.ids.contains(id)
            && !photographer.is_some_and(|p| self.photographers.contains(&db::photographer_key(p)))
    }
}

/// Download `metadata.image_url` into `dir` as `<provider_id>.<ext>` and record
/// it in the database. Naming by provider id (not search term) means the same
/// image found via different terms is only ever stored once.
async fn download(
    client: &Client,
    dir: &Path,
    ext: &str,
    label: &str,
    metadata: &WallpaperMetadata,
) -> Result<PathBuf> {
    let dest = dir.join(format!("{}.{ext}", metadata.provider_id));
    if !dest.exists() {
        save_image(client, &metadata.image_url, &dest).await?;
        println!(
            "{label}: downloaded {}",
            dest.file_name().unwrap().to_string_lossy()
        );
    }
    db::record_download(metadata, &dest)?;
    Ok(dest)
}

fn split_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------
// Unsplash
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct UnsplashResponse {
    results: Vec<UnsplashPhoto>,
}

#[derive(Deserialize)]
struct UnsplashPhoto {
    id: String,
    width: Option<u32>,
    height: Option<u32>,
    color: Option<String>,
    description: Option<String>,
    alt_description: Option<String>,
    likes: Option<u64>,
    urls: UnsplashUrls,
    links: Option<UnsplashLinks>,
    user: Option<UnsplashUser>,
}

#[derive(Deserialize)]
struct UnsplashUrls {
    regular: String,
}

#[derive(Deserialize)]
struct UnsplashLinks {
    html: Option<String>,
}

#[derive(Deserialize)]
struct UnsplashUser {
    name: Option<String>,
    links: Option<UnsplashUserLinks>,
}

#[derive(Deserialize)]
struct UnsplashUserLinks {
    html: Option<String>,
}

async fn unsplash(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let key = cfg.unsplash_api_key.as_deref().unwrap();
    const PROVIDER: &str = "unsplash";
    let dir = config::service_dir(PROVIDER)?;
    let mut saved = Vec::new();

    for term in &cfg.terms {
        let resp: UnsplashResponse = client
            .get("https://api.unsplash.com/search/photos")
            .header("Authorization", format!("Client-ID {key}"))
            .query(&[
                ("query", term.as_str()),
                ("per_page", "30"),
                ("orientation", "landscape"),
            ])
            .send()
            .await?
            .json()
            .await
            .context("failed to parse Unsplash response")?;

        let excluded = Exclusions::load(PROVIDER)?;
        let candidates: Vec<_> = resp
            .results
            .iter()
            .filter(|photo| {
                excluded.allows(
                    &photo.id,
                    photo.user.as_ref().and_then(|user| user.name.as_deref()),
                )
            })
            .collect();

        if candidates.is_empty() {
            eprintln!("Unsplash: no new results for '{term}'");
            continue;
        }

        let photo = candidates[random_index(candidates.len())];
        let metadata = WallpaperMetadata {
            provider: PROVIDER.to_owned(),
            provider_id: photo.id.clone(),
            search_term: term.clone(),
            image_url: photo.urls.regular.clone(),
            source_url: photo.links.as_ref().and_then(|links| links.html.clone()),
            title: photo
                .description
                .clone()
                .or_else(|| photo.alt_description.clone()),
            photographer: photo.user.as_ref().and_then(|user| user.name.clone()),
            photographer_url: photo
                .user
                .as_ref()
                .and_then(|user| user.links.as_ref())
                .and_then(|links| links.html.clone()),
            width: photo.width,
            height: photo.height,
            color: photo.color.clone(),
            tags: Vec::new(),
            likes: photo.likes,
            views: None,
            downloads: None,
            favorites: None,
            category: None,
            purity: None,
            file_size: None,
        };
        saved.push(download(client, &dir, "jpg", "Unsplash", &metadata).await?);
    }

    Ok(saved)
}

// ---------------------------------------------------------------------------
// Pexels
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PexelsResponse {
    photos: Vec<PexelsPhoto>,
}

#[derive(Deserialize)]
struct PexelsPhoto {
    id: u64,
    width: Option<u32>,
    height: Option<u32>,
    url: Option<String>,
    photographer: Option<String>,
    photographer_url: Option<String>,
    avg_color: Option<String>,
    alt: Option<String>,
    src: PexelsSrc,
}

#[derive(Deserialize)]
struct PexelsSrc {
    large2x: String,
}

async fn pexels(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let key = cfg.pexels_api_key.as_deref().unwrap();
    const PROVIDER: &str = "pexels";
    let dir = config::service_dir(PROVIDER)?;
    let mut saved = Vec::new();

    for term in &cfg.terms {
        let resp: PexelsResponse = client
            .get("https://api.pexels.com/v1/search")
            .header("Authorization", key)
            .query(&[
                ("query", term.as_str()),
                ("per_page", "30"),
                ("orientation", "landscape"),
            ])
            .send()
            .await?
            .json()
            .await
            .context("failed to parse Pexels response")?;

        let excluded = Exclusions::load(PROVIDER)?;
        let candidates: Vec<_> = resp
            .photos
            .iter()
            .filter(|photo| excluded.allows(&photo.id.to_string(), photo.photographer.as_deref()))
            .collect();

        if candidates.is_empty() {
            eprintln!("Pexels: no new results for '{term}'");
            continue;
        }

        let photo = candidates[random_index(candidates.len())];
        let metadata = WallpaperMetadata {
            provider: PROVIDER.to_owned(),
            provider_id: photo.id.to_string(),
            search_term: term.clone(),
            image_url: photo.src.large2x.clone(),
            source_url: photo.url.clone(),
            title: photo.alt.clone(),
            photographer: photo.photographer.clone(),
            photographer_url: photo.photographer_url.clone(),
            width: photo.width,
            height: photo.height,
            color: photo.avg_color.clone(),
            tags: Vec::new(),
            likes: None,
            views: None,
            downloads: None,
            favorites: None,
            category: None,
            purity: None,
            file_size: None,
        };
        saved.push(download(client, &dir, "jpg", "Pexels", &metadata).await?);
    }

    Ok(saved)
}

// ---------------------------------------------------------------------------
// Pixabay
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PixabayResponse {
    hits: Vec<PixabayHit>,
}

#[derive(Deserialize)]
struct PixabayHit {
    id: u64,
    #[serde(rename = "pageURL")]
    page_url: Option<String>,
    tags: Option<String>,
    user: Option<String>,
    #[serde(rename = "imageWidth")]
    image_width: Option<u32>,
    #[serde(rename = "imageHeight")]
    image_height: Option<u32>,
    likes: Option<u64>,
    views: Option<u64>,
    downloads: Option<u64>,
    #[serde(rename = "largeImageURL")]
    large_image_url: String,
}

async fn pixabay(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let key = cfg.pixabay_api_key.as_deref().unwrap();
    const PROVIDER: &str = "pixabay";
    let dir = config::service_dir(PROVIDER)?;
    let mut saved = Vec::new();

    for term in &cfg.terms {
        let resp: PixabayResponse = client
            .get("https://pixabay.com/api/")
            .query(&[
                ("key", key),
                ("q", term.as_str()),
                ("per_page", "100"),
                ("image_type", "photo"),
                ("orientation", "horizontal"),
                ("safesearch", "true"),
            ])
            .send()
            .await?
            .json()
            .await
            .context("failed to parse Pixabay response")?;

        let excluded = Exclusions::load(PROVIDER)?;
        let candidates: Vec<_> = resp
            .hits
            .iter()
            .filter(|hit| excluded.allows(&hit.id.to_string(), hit.user.as_deref()))
            .collect();

        if candidates.is_empty() {
            eprintln!("Pixabay: no new results for '{term}'");
            continue;
        }

        let hit = candidates[random_index(candidates.len())];
        let metadata = WallpaperMetadata {
            provider: PROVIDER.to_owned(),
            provider_id: hit.id.to_string(),
            search_term: term.clone(),
            image_url: hit.large_image_url.clone(),
            source_url: hit.page_url.clone(),
            title: None,
            photographer: hit.user.clone(),
            photographer_url: None,
            width: hit.image_width,
            height: hit.image_height,
            color: None,
            tags: hit.tags.as_deref().map(split_tags).unwrap_or_default(),
            likes: hit.likes,
            views: hit.views,
            downloads: hit.downloads,
            favorites: None,
            category: None,
            purity: None,
            file_size: None,
        };
        saved.push(download(client, &dir, "jpg", "Pixabay", &metadata).await?);
    }

    Ok(saved)
}

// ---------------------------------------------------------------------------
// Wallhaven
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WallhavenResponse {
    data: Vec<WallhavenWallpaper>,
}

#[derive(Deserialize)]
struct WallhavenWallpaper {
    id: String,
    path: String,
    url: Option<String>,
    short_url: Option<String>,
    views: Option<u64>,
    favorites: Option<u64>,
    source: Option<String>,
    purity: Option<String>,
    category: Option<String>,
    dimension_x: Option<u32>,
    dimension_y: Option<u32>,
    file_size: Option<u64>,
    colors: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct WallhavenDetailResponse {
    data: WallhavenDetail,
}

#[derive(Deserialize)]
struct WallhavenDetail {
    uploader: Option<WallhavenUploader>,
    #[serde(default)]
    tags: Vec<WallhavenTag>,
}

#[derive(Deserialize)]
struct WallhavenUploader {
    username: Option<String>,
}

#[derive(Deserialize)]
struct WallhavenTag {
    name: String,
}

/// Search results carry no uploader, so look it up (plus tags) per wallpaper.
/// Failures are non-fatal: the wallpaper is simply recorded without them.
async fn wallhaven_detail(client: &Client, id: &str, key: Option<&str>) -> Option<WallhavenDetail> {
    let mut req = client.get(format!("https://wallhaven.cc/api/v1/w/{id}"));
    if let Some(key) = key {
        req = req.query(&[("apikey", key)]);
    }
    let resp = req.send().await.ok()?.error_for_status().ok()?;
    resp.json::<WallhavenDetailResponse>()
        .await
        .ok()
        .map(|detail| detail.data)
}

async fn wallhaven(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    const PROVIDER: &str = "wallhaven";
    let dir = config::service_dir(PROVIDER)?;
    let mut saved = Vec::new();

    for term in &cfg.terms {
        let mut query = vec![
            ("q", term.as_str()),
            ("atleast", "1920x1080"),
            ("sorting", "relevance"),
            ("categories", "110"),
            ("purity", "100"),
        ];

        let key_owned;
        if let Some(key) = &cfg.wallhaven_api_key {
            key_owned = key.clone();
            query.push(("apikey", &key_owned));
        }

        let resp: WallhavenResponse = client
            .get("https://wallhaven.cc/api/v1/search")
            .query(&query)
            .send()
            .await?
            .json()
            .await
            .context("failed to parse Wallhaven response")?;

        let excluded = Exclusions::load(PROVIDER)?;
        let candidates: Vec<_> = resp
            .data
            .iter()
            .filter(|wallpaper| excluded.allows(&wallpaper.id, None))
            .collect();

        if candidates.is_empty() {
            eprintln!("Wallhaven: no new results for '{term}'");
            continue;
        }

        // Try a few candidates so a blocked uploader doesn't end the round.
        let start = random_index(candidates.len());
        let mut chosen = None;
        for offset in 0..candidates.len().min(5) {
            let wallpaper = candidates[(start + offset) % candidates.len()];
            let detail =
                wallhaven_detail(client, &wallpaper.id, cfg.wallhaven_api_key.as_deref()).await;
            let uploader = detail
                .as_ref()
                .and_then(|d| d.uploader.as_ref())
                .and_then(|u| u.username.clone());
            if excluded.allows(&wallpaper.id, uploader.as_deref()) {
                chosen = Some((wallpaper, uploader, detail));
                break;
            }
        }
        let Some((wallpaper, uploader, detail)) = chosen else {
            eprintln!("Wallhaven: no new results for '{term}'");
            continue;
        };

        let ext = wallpaper.path.rsplit('.').next().unwrap_or("jpg");
        let metadata = WallpaperMetadata {
            provider: PROVIDER.to_owned(),
            provider_id: wallpaper.id.clone(),
            search_term: term.clone(),
            image_url: wallpaper.path.clone(),
            source_url: wallpaper
                .short_url
                .clone()
                .or_else(|| wallpaper.url.clone())
                .or_else(|| wallpaper.source.clone().filter(|source| !source.is_empty())),
            title: None,
            photographer_url: uploader
                .as_ref()
                .map(|name| format!("https://wallhaven.cc/user/{name}")),
            photographer: uploader,
            width: wallpaper.dimension_x,
            height: wallpaper.dimension_y,
            color: wallpaper
                .colors
                .as_ref()
                .and_then(|colors| colors.first().cloned()),
            tags: detail
                .map(|d| d.tags.into_iter().map(|tag| tag.name).collect())
                .unwrap_or_default(),
            likes: None,
            views: wallpaper.views,
            downloads: None,
            favorites: wallpaper.favorites,
            category: wallpaper.category.clone(),
            purity: wallpaper.purity.clone(),
            file_size: wallpaper.file_size,
        };
        saved.push(download(client, &dir, ext, "Wallhaven", &metadata).await?);
    }

    Ok(saved)
}
