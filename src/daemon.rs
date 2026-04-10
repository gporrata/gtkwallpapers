use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

use crate::{config, providers, wallpaper};

pub async fn run() -> Result<()> {
    let client = Client::new();

    loop {
        let cfg = config::load()?;

        if !cfg.terms.is_empty() && wallpaper::pool_is_empty() {
            println!("Wallpaper pool empty — downloading from providers…");
            providers::download_all(&client, &cfg).await?;
        }

        match wallpaper::pick_random() {
            Err(e) => eprintln!("{e}"),
            Ok(chosen) => {
                if let Err(e) = wallpaper::set(&chosen) {
                    eprintln!("Failed to set wallpaper: {e}");
                } else {
                    println!("Wallpaper set: {}", chosen.display());
                }

                // Opportunistically fetch new images in the background.
                if !cfg.terms.is_empty() {
                    let client2 = client.clone();
                    let cfg2 = cfg.clone();
                    tokio::spawn(async move {
                        if let Err(e) = providers::download_all(&client2, &cfg2).await {
                            eprintln!("Background download error: {e}");
                        }
                    });
                }
            }
        }

        sleep(Duration::from_secs(cfg.frequency_secs)).await;
    }
}
