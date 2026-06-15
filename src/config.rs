use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::theme::Theme;

/// User-facing configuration. Theme parameters are flattened so existing
/// `config.toml` files (`height = 72`, `background = [...]`) keep working.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(flatten)]
    pub theme: Theme,

    /// Ordered, explicit pins. Paths are shell-expanded (`~/` -> $HOME).
    pub pinned: Vec<String>,
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                log::warn!("failed to parse {}: {err}", path.display());
                Self::default()
            }),
            Err(_) => {
                let cfg = Self::default();
                if let Err(err) = cfg.save() {
                    log::warn!("failed to write default config: {err}");
                }
                cfg
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        ensure_dirs()?;
        let text = toml::to_string_pretty(self)?;
        fs::write(config_path(), text)?;
        Ok(())
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crowndock")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn pinned_dir() -> PathBuf {
    config_dir().join("pinned")
}

pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::create_dir_all(pinned_dir())?;
    Ok(())
}

/// Expand a leading `~/` to the user's home directory.
pub fn shell_expand(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(s)
}
