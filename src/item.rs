use std::path::PathBuf;

use tiny_skia::Pixmap;

use crate::desktop::DesktopEntry;

/// What happens when a dock item is activated. The enum is the extension
/// point for new sources: add a variant here and handle it in
/// [`crate::launcher::activate`].
#[derive(Debug, Clone)]
#[allow(dead_code)] // `Command` is part of the public surface for new sources.
pub enum Action {
    /// Launch a freedesktop `.desktop` entry.
    LaunchDesktop(DesktopEntry),

    /// Run a raw command. `args` is forwarded verbatim; no shell is involved.
    Command {
        program: String,
        args: Vec<String>,
        terminal: bool,
    },
}

/// A single thing the dock can display. Identity is the [`id`] field, which
/// sources use to dedupe (typically the absolute path of the source file).
/// `label` is held for accessibility hooks even though the current renderer
/// doesn't draw it.
#[allow(dead_code)]
pub struct DockItem {
    pub id: ItemId,
    pub label: String,
    pub icon: Option<Pixmap>,
    pub action: Action,
}

/// Stable identifier for an item. A path covers all current use cases; if a
/// future source needs synthetic IDs, add a variant here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemId {
    Path(PathBuf),
}

impl DockItem {
    pub fn from_desktop(entry: DesktopEntry, icon: Option<Pixmap>) -> Self {
        Self {
            id: ItemId::Path(entry.path.clone()),
            label: entry.name.clone(),
            icon,
            action: Action::LaunchDesktop(entry),
        }
    }
}
