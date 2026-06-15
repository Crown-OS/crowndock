use std::fs;
use std::path::{Path, PathBuf};

use freedesktop_entry_parser::parse_entry;

/// Parsed `.desktop` file. Only the fields the dock actually consumes are kept;
/// downstream code should treat this as immutable.
#[derive(Debug, Clone)]
pub struct DesktopEntry {
    pub path: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub terminal: bool,
}

impl DesktopEntry {
    /// Parse `path` and return `None` if it isn't a displayable Application.
    pub fn load(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let entry = parse_entry(path).ok()?;
        let section = entry.section("Desktop Entry");

        if section.attr("Type").unwrap_or("Application") != "Application"
            || section.attr("NoDisplay") == Some("true")
            || section.attr("Hidden") == Some("true")
        {
            return None;
        }

        Some(Self {
            path: path.to_path_buf(),
            name: section.attr("Name")?.to_string(),
            exec: section.attr("Exec")?.to_string(),
            icon: section.attr("Icon").map(str::to_string),
            terminal: section.attr("Terminal") == Some("true"),
        })
    }

    /// Resolve an absolute path to the entry's icon at `size`, if one is found.
    pub fn icon_path(&self, size: u16) -> Option<PathBuf> {
        let icon = self.icon.as_deref()?;

        let direct = Path::new(icon);
        if direct.is_absolute() && direct.exists() {
            return Some(direct.to_path_buf());
        }

        freedesktop_icons::lookup(icon)
            .with_size(size)
            .with_cache()
            .find()
            .or_else(|| {
                freedesktop_icons::lookup(icon)
                    .with_size(size)
                    .force_svg()
                    .with_cache()
                    .find()
            })
    }

    /// Argv with freedesktop field codes stripped (`%f`, `%u`, `%F`, …).
    pub fn launch_argv(&self) -> Vec<String> {
        self.exec
            .split_whitespace()
            .filter(|tok| !tok.starts_with('%'))
            .map(str::to_string)
            .collect()
    }
}

/// Read every `.desktop` in `dir` (non-recursive), sorted case-insensitively
/// by `Name`.
pub fn scan_dir(dir: &Path) -> Vec<DesktopEntry> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<_> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("desktop"))
        .filter_map(DesktopEntry::load)
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}
