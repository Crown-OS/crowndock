use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::{self, Config};
use crate::desktop::{self, DesktopEntry};
use crate::icon;
use crate::item::{DockItem, ItemId};

/// A source of dock items. Implementors enumerate items they want to appear on
/// the dock. Sources are queried each time the dock reloads; ordering between
/// sources is controlled by [`SourceRegistry::add`] insertion order.
///
/// New sources (recents, mounted volumes, running apps, …) are added by
/// implementing this trait and registering an instance with
/// [`SourceRegistry::add`].
pub trait ItemSource {
    fn name(&self) -> &str;
    fn collect(&self, icon_size: u32) -> Vec<DockItem>;
}

/// Aggregates [`ItemSource`]s into a single deduplicated item list. The first
/// source to emit a given [`ItemId`] wins, so earlier-registered sources have
/// priority — used by the default registry to make the configured pin list
/// override the auto-discovered drop folder.
#[derive(Default)]
pub struct SourceRegistry {
    sources: Vec<Box<dyn ItemSource>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<S: ItemSource + 'static>(&mut self, source: S) -> &mut Self {
        log::debug!("registering item source: {}", source.name());
        self.sources.push(Box::new(source));
        self
    }

    pub fn collect(&self, icon_size: u32) -> Vec<DockItem> {
        let mut out = Vec::new();
        let mut seen: HashSet<ItemId> = HashSet::new();
        for source in &self.sources {
            let mut count = 0usize;
            for item in source.collect(icon_size) {
                if seen.insert(item.id.clone()) {
                    out.push(item);
                    count += 1;
                }
            }
            log::trace!("source {} contributed {count} items", source.name());
        }
        out
    }
}

/// Build the default registry: explicit `config.pinned` paths first, then any
/// `.desktop` files dropped into `~/.config/crowndock/pinned/`.
pub fn default_registry(cfg: &Config) -> SourceRegistry {
    let mut reg = SourceRegistry::new();
    reg.add(PinnedConfigSource {
        paths: cfg.pinned.clone(),
    })
    .add(DropFolderSource {
        dir: config::pinned_dir(),
    });
    reg
}

/// Items declared explicitly in the user's `config.toml` under `pinned = [..]`.
pub struct PinnedConfigSource {
    pub paths: Vec<String>,
}

impl ItemSource for PinnedConfigSource {
    fn name(&self) -> &str {
        "pinned-config"
    }

    fn collect(&self, icon_size: u32) -> Vec<DockItem> {
        self.paths
            .iter()
            .map(|raw| config::shell_expand(raw))
            .filter_map(|path| load_desktop_item(&path, icon_size))
            .collect()
    }
}

/// `.desktop` files dropped (or pinned via CLI) into the watched folder.
pub struct DropFolderSource {
    pub dir: PathBuf,
}

impl ItemSource for DropFolderSource {
    fn name(&self) -> &str {
        "drop-folder"
    }

    fn collect(&self, icon_size: u32) -> Vec<DockItem> {
        desktop::scan_dir(&self.dir)
            .into_iter()
            .map(|entry| build_item(entry, icon_size))
            .collect()
    }
}

fn load_desktop_item(path: &std::path::Path, icon_size: u32) -> Option<DockItem> {
    DesktopEntry::load(path).map(|entry| build_item(entry, icon_size))
}

fn build_item(entry: DesktopEntry, icon_size: u32) -> DockItem {
    let icon = entry
        .icon_path(icon_size as u16)
        .and_then(|p| icon::load(&p, icon_size));
    DockItem::from_desktop(entry, icon)
}
