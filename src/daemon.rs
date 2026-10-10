use anyhow::Result;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::config::Config;
use crate::{config, db, providers, tray, wallpaper};

/// Rebuild the dynamic parts of the tray menu.
fn refresh_tray(handle: &tray::TrayHandle, cfg: &Config, current: Option<&Path>) {
    handle.set_state(tray::State {
        photographer: current_photographer(current),
        terms: cfg.terms.clone(),
        blocked: db::blocked_photographers().unwrap_or_default(),
    });
}

fn delete_wallpaper(path: &Path) {
    match db::delete_wallpaper(path) {
        Ok(_) => println!("Deleted wallpaper: {}", path.display()),
        Err(e) => eprintln!("Failed to delete wallpaper: {e}"),
    }
}

fn block_photographer(name: &str) {
    match db::block_photographer(name) {
        Ok(removed) => println!("Blocked photographer {name}; removed {removed} wallpaper(s)."),
        Err(e) => eprintln!("Failed to block photographer {name}: {e}"),
    }
}

fn current_photographer(path: Option<&Path>) -> Option<String> {
    path.and_then(|p| db::find_by_path(p).ok().flatten())
        .and_then(|record| record.photographer)
        .filter(|name| !name.trim().is_empty())
}

pub async fn run() -> Result<()> {
    let client = Client::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<tray::Event>();
    let tray = tray::spawn(tx);

    // Opening the database once up front also migrates legacy sidecars.
    db::open()?;

    let mut current_wallpaper: Option<PathBuf> = None;

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

                current_wallpaper = Some(chosen);

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

        // Wait for the rotation timer or a tray action that intentionally rotates.
        loop {
            let cfg = config::load()?;
            refresh_tray(&tray, &cfg, current_wallpaper.as_deref());

            tokio::select! {
                _ = sleep(Duration::from_secs(cfg.frequency_secs)) => break,
                Some(event) = rx.recv() => {
                    match event {
                        tray::Event::Next => break,
                        tray::Event::DeleteNext => {
                            if let Some(path) = current_wallpaper.take() {
                                delete_wallpaper(&path);
                            }
                            break;
                        }
                        tray::Event::BlockPhotographer => {
                            if let Some(name) = current_photographer(current_wallpaper.as_deref()) {
                                block_photographer(&name);
                                current_wallpaper = None;
                                break;
                            }
                            eprintln!("Current wallpaper has no known photographer.");
                        }
                        tray::Event::UnblockPhotographer(name) => {
                            match db::unblock_photographer(&name) {
                                Ok(_) => println!("Unblocked photographer {name}."),
                                Err(e) => eprintln!("Failed to unblock photographer {name}: {e}"),
                            }
                        }
                        tray::Event::Info => {
                            let Some(path) = current_wallpaper.clone() else {
                                eprintln!("No current wallpaper to show info for.");
                                continue;
                            };
                            match wallpaper::show_info_dialog(&path) {
                                Ok(wallpaper::InfoAction::None) => {}
                                Ok(wallpaper::InfoAction::Delete) => {
                                    delete_wallpaper(&path);
                                    current_wallpaper = None;
                                    break;
                                }
                                Ok(wallpaper::InfoAction::BlockPhotographer(name)) => {
                                    block_photographer(&name);
                                    current_wallpaper = None;
                                    break;
                                }
                                Err(e) => eprintln!("Failed to show wallpaper info: {e}"),
                            }
                        }
                        tray::Event::ShowTerms => {
                            if let Err(e) = wallpaper::show_terms_dialog(&cfg.terms) {
                                eprintln!("Failed to show search terms: {e}");
                            }
                        }
                        tray::Event::Quit => std::process::exit(0),
                    }
                }
            }
        }
    }
}
