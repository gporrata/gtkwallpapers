use anyhow::Result;
use inquire::MultiSelect;

use crate::config;

pub fn interactive_terms() -> Result<()> {
    let mut cfg = config::load()?;

    if cfg.terms.is_empty() {
        println!("No search terms configured. Use `gtkwallpapers terms <term>` to add one.");
        return Ok(());
    }

    let selected = MultiSelect::new(
        "Select terms to KEEP (space to toggle, enter to confirm, esc to cancel):",
        cfg.terms.clone(),
    )
    .prompt_skippable()?;

    if let Some(kept) = selected {
        let removed: Vec<_> = cfg
            .terms
            .iter()
            .filter(|t| !kept.contains(t))
            .cloned()
            .collect();

        cfg.terms = kept;
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
