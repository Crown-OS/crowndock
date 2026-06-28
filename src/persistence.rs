use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const APP_DIR: &str = "crowndock";
const ITEMS_FILE: &str = "items.toml";

#[derive(Default, Serialize, Deserialize)]
struct StoredItems {
    #[serde(default)]
    items: Vec<PathBuf>,
}

/// On-disk path used to persist the dock's pinned items, per the XDG Base
/// Directory spec — `$XDG_CONFIG_HOME/crowndock/items.toml`, falling back to
/// `~/.config/crowndock/items.toml`.
pub fn items_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_DIR).join(ITEMS_FILE))
}

/// Load the persisted list of `.desktop` paths, in order. Missing or
/// unreadable files yield an empty list — the dock simply starts blank.
pub fn load_items() -> Vec<PathBuf> {
    let Some(path) = items_path() else {
        return Vec::new();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            log::warn!("read {}: {e}", path.display());
            return Vec::new();
        }
    };
    match toml::from_str::<StoredItems>(&text) {
        Ok(stored) => stored.items,
        Err(e) => {
            log::warn!("parse {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Persist `items` to disk in order. Writes atomically via a temp file so a
/// crash mid-write cannot truncate the user's pinned list.
pub fn save_items(items: &[PathBuf]) -> Result<()> {
    let path = items_path().context("no XDG config dir available")?;
    let parent = path
        .parent()
        .context("config path has no parent directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create {}", parent.display()))?;

    let stored = StoredItems {
        items: items.to_vec(),
    };
    let text = toml::to_string_pretty(&stored)?;
    write_atomic(&path, text.as_bytes())
        .with_context(|| format!("write {}", path.display()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir")
    })?;
    let file_name = path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name")
        })?
        .to_string_lossy();
    let tmp = parent.join(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
