use anyhow::{Context, Result};
use inquire::MultiSelect;
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;

use crate::config::{self, Config};

// ---------------------------------------------------------------------------
// Interactive term management
// ---------------------------------------------------------------------------

pub fn interactive_terms() -> Result<()> {
    let mut cfg = config::load()?;

    if cfg.flickr_terms.is_empty() {
        println!("No search terms configured. Use `gtkwallpapers flickr <term>` to add one.");
        return Ok(());
    }

    let selected = MultiSelect::new(
        "Select terms to KEEP (space to toggle, enter to confirm, esc to cancel):",
        cfg.flickr_terms.clone(),
    )
    .prompt_skippable()?;

    if let Some(kept) = selected {
        let removed: Vec<_> = cfg
            .flickr_terms
            .iter()
            .filter(|t| !kept.contains(t))
            .cloned()
            .collect();

        cfg.flickr_terms = kept;
        config::save(&cfg)?;

        if removed.is_empty() {
            println!("No changes made.");
        } else {
            for t in &removed {
                println!("Removed: {t}");
            }
        }
    } else {
        println!("Cancelled.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Flickr photo download
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchResponse {
    photos: Photos,
}

#[derive(Deserialize)]
struct Photos {
    photo: Vec<Photo>,
}

#[derive(Deserialize)]
struct Photo {
    id: String,
    server: String,
    secret: String,
}

impl Photo {
    /// Constructs the "Large" (1024px) image URL using the current Flickr CDN.
    fn url_large(&self) -> String {
        format!(
            "https://live.staticflickr.com/{}/{}_{}_b.jpg",
            self.server, self.id, self.secret
        )
    }
}

/// Fetch and save one random photo per search term in `cfg`.
/// Returns the list of saved file paths.
pub async fn download_batch(client: &Client, cfg: &Config) -> Result<Vec<PathBuf>> {
    let mut saved = Vec::new();

    for term in &cfg.flickr_terms {
        match fetch_random_photo(client, cfg, term).await {
            Ok(path) => saved.push(path),
            Err(e) => eprintln!("Warning: could not download photo for '{term}': {e}"),
        }
    }

    Ok(saved)
}

async fn fetch_random_photo(client: &Client, cfg: &Config, term: &str) -> Result<PathBuf> {
    let dir = config::wallpapers_dir()?;

    // Flickr public API (no key) or authenticated.
    let mut params = vec![
        ("method", "flickr.photos.search"),
        ("format", "json"),
        ("nojsoncallback", "1"),
        ("text", term),
        ("sort", "relevance"),
        ("content_type", "1"),   // photos only
        ("media", "photos"),
        ("per_page", "100"),
        ("page", "1"),
        ("safe_search", "1"),
    ];

    let api_key_owned = cfg
        .flickr_api_key
        .clone()
        .context("No Flickr API key set. Run `gtkwallpapers flickrkey <key>` first.")?;
    params.push(("api_key", &api_key_owned));

    let resp: SearchResponse = client
        .get("https://api.flickr.com/services/rest/")
        .query(&params)
        .send()
        .await?
        .json()
        .await
        .context("failed to parse Flickr search response")?;

    let photos = resp.photos.photo;
    if photos.is_empty() {
        anyhow::bail!("no photos returned for term '{term}'");
    }

    // Pick a random photo from the results.
    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos() as usize)
        % photos.len();
    let photo = &photos[idx];

    let url = photo.url_large();
    let filename = format!("{}-{}.jpg", term.replace(' ', "_"), photo.id);
    let dest = dir.join(&filename);

    if !dest.exists() {
        let bytes = client.get(&url).send().await?.bytes().await?;
        std::fs::write(&dest, &bytes)?;
        println!("Downloaded: {filename}");
    }

    Ok(dest)
}
