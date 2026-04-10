use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::{config, providers, tray, wallpaper};

pub async fn run() -> Result<()> {
    let client = Client::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<tray::Event>();
    tray::spawn(tx);

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

        // Wait for the rotation timer or a tray event, whichever comes first.
        let cfg = config::load()?;
        tokio::select! {
            _ = sleep(Duration::from_secs(cfg.frequency_secs)) => {}
            Some(event) = rx.recv() => {
                match event {
                    tray::Event::Next => {} // falls through to top of loop
                    tray::Event::Quit => std::process::exit(0),
                }
            }
        }
    }
}
