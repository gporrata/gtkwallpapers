//! Durable wallpaper history backed by SQLite.
//!
//! Every wallpaper a provider hands us is recorded by `(provider, provider_id)`
//! together with its metadata and local path. The `status` column is the
//! source of truth for whether it may be downloaded again:
//!
//! * `downloaded` – fetched at some point; the file may or may not still be
//!   cached on disk (clearing the cache is fine and loses no history).
//! * `deleted`    – the user deleted it; it is never downloaded again.
//! * `blocked`    – removed because its photographer was blocked; restored to
//!   `downloaded` (i.e. eligible again) if the photographer is unblocked.
//!
//! The database lives in the XDG data dir (`~/.local/share/gtkwallpapers/`),
//! separate from the image cache under `~/.config/gtkwallpapers/`, so wiping
//! the cache never wipes deletion history.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Once;

use crate::config::{self, SERVICE_NAMES};
use crate::metadata::WallpaperMetadata;

pub const STATUS_DOWNLOADED: &str = "downloaded";
pub const STATUS_DELETED: &str = "deleted";
pub const STATUS_BLOCKED: &str = "blocked";

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS wallpapers (
    provider         TEXT NOT NULL,
    provider_id      TEXT NOT NULL,
    search_term      TEXT NOT NULL,
    image_url        TEXT NOT NULL,
    photographer     TEXT,
    local_path       TEXT,
    status           TEXT NOT NULL DEFAULT 'downloaded',
    metadata         TEXT NOT NULL,
    first_seen       INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at       INTEGER NOT NULL DEFAULT (unixepoch()),
    deleted_at       INTEGER,
    PRIMARY KEY (provider, provider_id)
);
CREATE INDEX IF NOT EXISTS wallpapers_local_path ON wallpapers(local_path);
CREATE INDEX IF NOT EXISTS wallpapers_photographer ON wallpapers(photographer COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS blocked_photographers (
    name        TEXT PRIMARY KEY COLLATE NOCASE,
    blocked_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
";

/// A wallpaper row as stored in the database.
pub struct Record {
    pub status: String,
    pub photographer: Option<String>,
    pub metadata: WallpaperMetadata,
}

pub fn db_path() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("could not resolve $HOME/.local/share")?
        .join("gtkwallpapers");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("gtkwallpapers.db"))
}

/// Open (and if needed create/migrate) the database. Cheap enough to call per
/// operation, which keeps connections out of async state.
pub fn open() -> Result<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(SCHEMA)?;

    static IMPORT: Once = Once::new();
    IMPORT.call_once(|| {
        if let Err(e) = import_legacy_sidecars(&conn) {
            eprintln!("Failed to import legacy metadata sidecars: {e}");
        }
    });

    Ok(conn)
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Normalise a photographer name for set lookups.
pub fn photographer_key(name: &str) -> String {
    name.trim().to_lowercase()
}

// ---------------------------------------------------------------------------
// Queries used by providers
// ---------------------------------------------------------------------------

/// Provider ids that must not be downloaded right now: anything deleted or
/// blocked, plus anything already cached on disk (so each fetch brings
/// something new rather than re-selecting a wallpaper we already have).
pub fn skip_ids(provider: &str) -> Result<HashSet<String>> {
    let conn = open()?;
    let mut stmt =
        conn.prepare("SELECT provider_id, status, local_path FROM wallpapers WHERE provider = ?1")?;
    let rows = stmt.query_map(params![provider], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut ids = HashSet::new();
    for row in rows {
        let (id, status, local_path) = row?;
        let cached = local_path.is_some_and(|p| Path::new(&p).exists());
        if status != STATUS_DOWNLOADED || cached {
            ids.insert(id);
        }
    }
    Ok(ids)
}

/// Lower-cased names of all blocked photographers.
pub fn blocked_photographer_keys() -> Result<HashSet<String>> {
    Ok(blocked_photographers()?
        .iter()
        .map(|name| photographer_key(name))
        .collect())
}

/// Record a freshly downloaded wallpaper. Never resurrects a deleted one.
pub fn record_download(metadata: &WallpaperMetadata, local_path: &Path) -> Result<()> {
    let conn = open()?;
    upsert(&conn, metadata, Some(local_path), STATUS_DOWNLOADED)
}

fn upsert(
    conn: &Connection,
    metadata: &WallpaperMetadata,
    local_path: Option<&Path>,
    status: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO wallpapers
             (provider, provider_id, search_term, image_url, photographer, local_path, status, metadata, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CASE WHEN ?7 = 'deleted' THEN unixepoch() END)
         ON CONFLICT (provider, provider_id) DO UPDATE SET
             search_term  = excluded.search_term,
             image_url    = excluded.image_url,
             photographer = excluded.photographer,
             local_path   = COALESCE(excluded.local_path, wallpapers.local_path),
             metadata     = excluded.metadata,
             status       = CASE WHEN wallpapers.status = 'deleted' THEN 'deleted' ELSE excluded.status END,
             deleted_at   = COALESCE(wallpapers.deleted_at, excluded.deleted_at),
             updated_at   = unixepoch()",
        params![
            metadata.provider,
            metadata.provider_id,
            metadata.search_term,
            metadata.image_url,
            metadata.photographer,
            local_path.map(path_str),
            status,
            serde_json::to_string(metadata)?,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queries used by the daemon / tray / CLI
// ---------------------------------------------------------------------------

/// Look up the wallpaper that is cached at `path`.
pub fn find_by_path(path: &Path) -> Result<Option<Record>> {
    let conn = open()?;
    let row = conn
        .query_row(
            "SELECT status, photographer, metadata FROM wallpapers WHERE local_path = ?1",
            params![path_str(path)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;

    row.map(|(status, photographer, metadata)| {
        Ok(Record {
            status,
            photographer,
            metadata: serde_json::from_str(&metadata)?,
        })
    })
    .transpose()
}

/// Local paths whose files must not be shown (deleted, blocked, or by a
/// blocked photographer).
pub fn hidden_paths() -> Result<HashSet<PathBuf>> {
    let conn = open()?;
    let mut stmt = conn.prepare(
        "SELECT local_path FROM wallpapers
         WHERE local_path IS NOT NULL
           AND (status != 'downloaded'
                OR photographer IN (SELECT name FROM blocked_photographers))",
    )?;
    let paths = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Permanently delete the wallpaper at `path`: mark it deleted and remove the
/// cached file. Returns true if the wallpaper was tracked in the database.
pub fn delete_wallpaper(path: &Path) -> Result<bool> {
    let conn = open()?;
    let changed = conn.execute(
        "UPDATE wallpapers
         SET status = 'deleted', deleted_at = unixepoch(), updated_at = unixepoch()
         WHERE local_path = ?1",
        params![path_str(path)],
    )?;
    remove_file_if_exists(path)?;
    Ok(changed > 0)
}

/// Block a photographer and remove all of their cached wallpapers. Returns the
/// number of files removed.
pub fn block_photographer(name: &str) -> Result<usize> {
    let name = name.trim();
    let conn = open()?;
    conn.execute(
        "INSERT OR IGNORE INTO blocked_photographers (name) VALUES (?1)",
        params![name],
    )?;

    let paths = {
        let mut stmt = conn.prepare(
            "SELECT local_path FROM wallpapers
             WHERE photographer = ?1 COLLATE NOCASE AND local_path IS NOT NULL",
        )?;
        stmt.query_map(params![name], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    conn.execute(
        "UPDATE wallpapers SET status = ?2, updated_at = unixepoch()
         WHERE photographer = ?1 COLLATE NOCASE AND status = ?3",
        params![name, STATUS_BLOCKED, STATUS_DOWNLOADED],
    )?;

    let mut removed = 0;
    for path in paths {
        if remove_file_if_exists(Path::new(&path))? {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Unblock a photographer. Their wallpapers become eligible for download again
/// (ones the user deleted individually stay deleted). Returns true if they
/// were blocked.
pub fn unblock_photographer(name: &str) -> Result<bool> {
    let name = name.trim();
    let conn = open()?;
    let removed = conn.execute(
        "DELETE FROM blocked_photographers WHERE name = ?1",
        params![name],
    )?;
    conn.execute(
        "UPDATE wallpapers SET status = 'downloaded', updated_at = unixepoch()
         WHERE photographer = ?1 COLLATE NOCASE AND status = 'blocked'",
        params![name],
    )?;
    Ok(removed > 0)
}

pub fn blocked_photographers() -> Result<Vec<String>> {
    let conn = open()?;
    let mut stmt =
        conn.prepare("SELECT name FROM blocked_photographers ORDER BY name COLLATE NOCASE")?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// Number of wallpapers per status, for display.
pub fn status_counts() -> Result<Vec<(String, i64)>> {
    let conn = open()?;
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*) FROM wallpapers GROUP BY status ORDER BY status")?;
    let counts = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(counts)
}

fn remove_file_if_exists(path: &Path) -> Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("failed to delete {}", path.display())),
    }
}

// ---------------------------------------------------------------------------
// One-time migration from JSON sidecars
// ---------------------------------------------------------------------------

/// Older versions wrote `<image>.json` next to every image and treated a
/// sidecar without its image as "deleted". Fold those into the database
/// (orphans become `deleted`), remove duplicate copies of the same provider
/// image saved under different search terms, and remove the sidecars.
fn import_legacy_sidecars(conn: &Connection) -> Result<()> {
    struct Sidecar {
        json: PathBuf,
        image: Option<PathBuf>,
        metadata: WallpaperMetadata,
    }

    let mut groups: HashMap<(String, String), Vec<Sidecar>> = HashMap::new();

    for name in SERVICE_NAMES {
        let Ok(dir) = config::service_dir(name) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let json = entry.path();
            if json.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(metadata) = std::fs::read_to_string(&json)
                .ok()
                .and_then(|text| serde_json::from_str::<WallpaperMetadata>(&text).ok())
            else {
                continue;
            };
            let image = IMAGE_EXTENSIONS
                .iter()
                .map(|ext| json.with_extension(ext))
                .find(|p| p.exists());
            groups
                .entry((metadata.provider.clone(), metadata.provider_id.clone()))
                .or_default()
                .push(Sidecar {
                    json,
                    image,
                    metadata,
                });
        }
    }

    if groups.is_empty() {
        return Ok(());
    }

    let mut imported = 0;
    let mut deleted = 0;
    for sidecars in groups.values() {
        let orphaned = sidecars.iter().any(|s| s.image.is_none());
        let keep = if orphaned {
            None
        } else {
            sidecars.iter().find_map(|s| s.image.clone())
        };
        let status = if orphaned {
            STATUS_DELETED
        } else {
            STATUS_DOWNLOADED
        };

        upsert(conn, &sidecars[0].metadata, keep.as_deref(), status)?;
        imported += 1;
        if orphaned {
            deleted += 1;
        }

        for sidecar in sidecars {
            if let Some(image) = &sidecar.image
                && Some(image) != keep.as_ref()
            {
                remove_file_if_exists(image)?;
            }
            remove_file_if_exists(&sidecar.json)?;
        }
    }

    println!(
        "Imported {imported} wallpaper(s) from legacy metadata sidecars ({deleted} previously deleted)."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(provider: &str, id: &str, photographer: Option<&str>) -> WallpaperMetadata {
        WallpaperMetadata {
            provider: provider.to_owned(),
            provider_id: id.to_owned(),
            search_term: "mountains".to_owned(),
            image_url: format!("https://example.com/{id}.jpg"),
            source_url: None,
            title: None,
            photographer: photographer.map(ToOwned::to_owned),
            photographer_url: None,
            width: None,
            height: None,
            color: None,
            tags: Vec::new(),
            likes: None,
            views: None,
            downloads: None,
            favorites: None,
            category: None,
            purity: None,
            file_size: None,
        }
    }

    /// One test (not several) because it points XDG dirs at a temp dir for
    /// the whole process.
    #[test]
    fn deletion_and_blocking_survive_cache_clears() {
        let root = std::env::temp_dir().join(format!("gtkwallpapers-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        unsafe {
            std::env::set_var("XDG_DATA_HOME", root.join("data"));
            std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        }
        let dir = config::service_dir("pexels").unwrap();

        // Legacy layout: one kept image, one orphaned sidecar (= deleted), and
        // a duplicate of the deleted image saved under another term.
        let legacy = |stem: &str, id: &str, with_image: bool| {
            std::fs::write(
                dir.join(format!("{stem}.json")),
                serde_json::to_string(&metadata("pexels", id, None)).unwrap(),
            )
            .unwrap();
            if with_image {
                std::fs::write(dir.join(format!("{stem}.jpg")), b"img").unwrap();
            }
        };
        legacy("mountains-1", "1", true);
        legacy("mountains-2", "2", false);
        legacy("ocean-2", "2", true);

        let skip = skip_ids("pexels").unwrap(); // triggers import
        assert!(skip.contains("1"), "cached image is skipped");
        assert!(skip.contains("2"), "orphaned sidecar imported as deleted");
        assert!(
            !dir.join("ocean-2.jpg").exists(),
            "duplicate of deleted image removed"
        );
        assert!(!dir.join("mountains-1.json").exists(), "sidecars removed");

        // New download, then delete it.
        let path = dir.join("3.jpg");
        std::fs::write(&path, b"img").unwrap();
        record_download(&metadata("pexels", "3", Some("Jane Doe")), &path).unwrap();
        assert!(delete_wallpaper(&path).unwrap());
        assert!(!path.exists());

        // Clearing the cache keeps deletions; cached-only ids become eligible.
        std::fs::remove_file(dir.join("mountains-1.jpg")).unwrap();
        let skip = skip_ids("pexels").unwrap();
        assert!(skip.contains("2") && skip.contains("3"));
        assert!(!skip.contains("1"));

        // Re-recording a deleted id never resurrects it.
        record_download(&metadata("pexels", "3", Some("Jane Doe")), &path).unwrap();
        assert!(skip_ids("pexels").unwrap().contains("3"));

        // Blocking removes the photographer's files and hides them.
        let path4 = dir.join("4.jpg");
        std::fs::write(&path4, b"img").unwrap();
        record_download(&metadata("pexels", "4", Some("Jane Doe")), &path4).unwrap();
        assert_eq!(block_photographer("jane doe").unwrap(), 1);
        assert!(!path4.exists());
        assert!(blocked_photographer_keys().unwrap().contains("jane doe"));
        assert!(skip_ids("pexels").unwrap().contains("4"));

        // Unblocking restores eligibility, but individual deletions stick.
        assert!(unblock_photographer("Jane Doe").unwrap());
        let skip = skip_ids("pexels").unwrap();
        assert!(!skip.contains("4"));
        assert!(skip.contains("3"));
        assert!(blocked_photographers().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }
}
