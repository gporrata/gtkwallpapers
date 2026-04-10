use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;

use crate::config::{self, Config};

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

async fn save_image(client: &Client, url: &str, dest: &PathBuf) -> Result<()> {
    let bytes = client.get(url).send().await?.bytes().await?;
    std::fs::write(dest, &bytes)?;
    Ok(())
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
    urls: UnsplashUrls,
}

#[derive(Deserialize)]
struct UnsplashUrls {
    regular: String,
}

async fn unsplash(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let key = cfg.unsplash_api_key.as_deref().unwrap();
    let dir = config::service_dir("unsplash")?;
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

        if resp.results.is_empty() {
            eprintln!("Unsplash: no results for '{term}'");
            continue;
        }

        let photo = &resp.results[random_index(resp.results.len())];
        let dest = dir.join(format!("{}-{}.jpg", term.replace(' ', "_"), photo.id));

        if !dest.exists() {
            save_image(client, &photo.urls.regular, &dest).await?;
            println!("Unsplash: downloaded {}", dest.file_name().unwrap().to_string_lossy());
        }
        saved.push(dest);
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
    src: PexelsSrc,
}

#[derive(Deserialize)]
struct PexelsSrc {
    large2x: String,
}

async fn pexels(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let key = cfg.pexels_api_key.as_deref().unwrap();
    let dir = config::service_dir("pexels")?;
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

        if resp.photos.is_empty() {
            eprintln!("Pexels: no results for '{term}'");
            continue;
        }

        let photo = &resp.photos[random_index(resp.photos.len())];
        let dest = dir.join(format!("{}-{}.jpg", term.replace(' ', "_"), photo.id));

        if !dest.exists() {
            save_image(client, &photo.src.large2x, &dest).await?;
            println!("Pexels: downloaded {}", dest.file_name().unwrap().to_string_lossy());
        }
        saved.push(dest);
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
    #[serde(rename = "largeImageURL")]
    large_image_url: String,
}

async fn pixabay(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let key = cfg.pixabay_api_key.as_deref().unwrap();
    let dir = config::service_dir("pixabay")?;
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

        if resp.hits.is_empty() {
            eprintln!("Pixabay: no results for '{term}'");
            continue;
        }

        let hit = &resp.hits[random_index(resp.hits.len())];
        let dest = dir.join(format!("{}-{}.jpg", term.replace(' ', "_"), hit.id));

        if !dest.exists() {
            save_image(client, &hit.large_image_url, &dest).await?;
            println!("Pixabay: downloaded {}", dest.file_name().unwrap().to_string_lossy());
        }
        saved.push(dest);
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
}

async fn wallhaven(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let dir = config::service_dir("wallhaven")?;
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

        if resp.data.is_empty() {
            eprintln!("Wallhaven: no results for '{term}'");
            continue;
        }

        let wallpaper = &resp.data[random_index(resp.data.len())];
        let ext = wallpaper.path.rsplit('.').next().unwrap_or("jpg");
        let dest = dir.join(format!("{}-{}.{ext}", term.replace(' ', "_"), wallpaper.id));

        if !dest.exists() {
            save_image(client, &wallpaper.path, &dest).await?;
            println!("Wallhaven: downloaded {}", dest.file_name().unwrap().to_string_lossy());
        }
        saved.push(dest);
    }

    Ok(saved)
}
