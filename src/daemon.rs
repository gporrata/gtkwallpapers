use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

use crate::{config, flickr, wallpaper};

pub async fn run() -> Result<()> {
    let client = Client::new();

    loop {
        let cfg = config::load()?;

        // Ensure we have wallpapers; download if the pool is empty.
        let wallpapers_dir = config::wallpapers_dir()?;
        let mut images: Vec<_> = std::fs::read_dir(&wallpapers_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("jpg" | "jpeg" | "png" | "webp")
                )
            })
            .collect();

        if images.is_empty() && !cfg.flickr_terms.is_empty() {
            println!("Wallpaper pool empty — downloading from Flickr…");
            images = flickr::download_batch(&client, &cfg).await?;
        }

        if images.is_empty() {
            eprintln!(
                "No wallpapers available. Add Flickr search terms with `gtkwallpapers flickr <term>`."
            );
        } else {
            // Pick a pseudo-random image.
            let idx = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .subsec_nanos() as usize)
                % images.len();
            let chosen = &images[idx];

            if let Err(e) = wallpaper::set(chosen) {
                eprintln!("Failed to set wallpaper: {e}");
            } else {
                println!("Wallpaper set: {}", chosen.display());
            }

            // Opportunistically download new images in the background so the
            // pool stays fresh without blocking the rotation.
            if !cfg.flickr_terms.is_empty() {
                let client2 = client.clone();
                let cfg2 = cfg.clone();
                tokio::spawn(async move {
                    if let Err(e) = flickr::download_batch(&client2, &cfg2).await {
                        eprintln!("Background download error: {e}");
                    }
                });
            }
        }

        sleep(Duration::from_secs(cfg.frequency_secs)).await;
    }
}
