use anyhow::Result;
use console::{Key, Term};

use crate::config;

pub fn interactive_terms() -> Result<()> {
    let mut cfg = config::load()?;

    if cfg.terms.is_empty() {
        println!("No search terms configured. Use `gtkwallpapers terms <term>` to add one.");
        return Ok(());
    }

    let term = Term::stdout();
    let mut cursor = 0usize;

    loop {
        term.clear_screen()?;
        println!("Search terms  \x1b[2m(↑↓ navigate · Del remove · Esc done)\x1b[0m\n");

        for (i, t) in cfg.terms.iter().enumerate() {
            if i == cursor {
                println!("  \x1b[1;36m> {t}\x1b[0m");
            } else {
                println!("    {t}");
            }
        }

        match term.read_key()? {
            Key::ArrowUp => {
                if cursor > 0 {
                    cursor -= 1;
                }
            }
            Key::ArrowDown => {
                if cursor + 1 < cfg.terms.len() {
                    cursor += 1;
                }
            }
            Key::Del => {
                let removed = cfg.terms.remove(cursor);
                println!("\nRemoved: {removed}");
                if cfg.terms.is_empty() {
                    config::save(&cfg)?;
                    return Ok(());
                }
                if cursor >= cfg.terms.len() {
                    cursor = cfg.terms.len() - 1;
                }
            }
            Key::Escape => {
                config::save(&cfg)?;
                term.clear_screen()?;
                return Ok(());
            }
            _ => {}
        }
    }
}
