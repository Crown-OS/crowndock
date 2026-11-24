//! The `web-browser` dock item: the application that opens `https` links, as
//! the XDG `mimeapps.list` files say. The distro dock pins this item and not a
//! fixed browser, so the dock follows the browser that the user chose during
//! setup or later, and the distro dock does not change per machine.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use super::xdg;

pub const WEB_BROWSER_ITEM: &str = "web-browser";

const MIMEAPPS_FILE: &str = "mimeapps.list";
const DEFAULTS_SECTION: &str = "Default Applications";
const HTTPS_HANDLER: &str = "x-scheme-handler/https";

/// The `.desktop` file of the default web browser for the `web-browser` item,
/// and every other item unchanged. The item stays unchanged when no installed
/// browser is the default.
pub fn resolve_pinned_item(item: &Path) -> Cow<'_, Path> {
    if item != Path::new(WEB_BROWSER_ITEM) {
        return Cow::Borrowed(item);
    }
    let application_dirs = xdg::application_dirs();
    let mimeapps_dirs: Vec<PathBuf> = xdg::config_dirs()
        .into_iter()
        .chain(application_dirs.iter().cloned())
        .collect();
    default_browser(&mimeapps_dirs, &application_dirs).map_or(Cow::Borrowed(item), Cow::Owned)
}

/// The first installed `https` handler, from the `mimeapps.list` files in
/// `mimeapps_dirs`, highest priority first.
fn default_browser(mimeapps_dirs: &[PathBuf], application_dirs: &[PathBuf]) -> Option<PathBuf> {
    mimeapps_dirs
        .iter()
        .flat_map(|dir| https_handlers(&dir.join(MIMEAPPS_FILE)))
        .find_map(|desktop_id| installed_desktop_file(application_dirs, &desktop_id))
}

fn https_handlers(mimeapps_list: &Path) -> Vec<String> {
    let Ok(entry) = freedesktop_entry_parser::parse_entry(mimeapps_list) else {
        return Vec::new();
    };
    entry
        .section(DEFAULTS_SECTION)
        .attr(HTTPS_HANDLER)
        .map(|desktop_ids| {
            desktop_ids
                .split(';')
                .map(str::trim)
                .filter(|desktop_id| !desktop_id.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn installed_desktop_file(application_dirs: &[PathBuf], desktop_id: &str) -> Option<PathBuf> {
    application_dirs
        .iter()
        .map(|dir| dir.join(desktop_id))
        .find(|desktop_file| desktop_file.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TemporaryRoot(PathBuf);

    impl TemporaryRoot {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "crowndock-web-browser-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            Self(root)
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap_or_default();
            }
            std::fs::write(&path, text).unwrap_or_default();
            path
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn defaults(handlers: &str) -> String {
        format!("[Default Applications]\nx-scheme-handler/https={handlers}\n")
    }

    #[test]
    fn other_items_stay_unchanged() {
        let item = Path::new("/usr/share/applications/kitty.desktop");
        assert_eq!(resolve_pinned_item(item), Cow::Borrowed(item));
    }

    #[test]
    fn the_user_default_wins_over_the_system_default() {
        let root = TemporaryRoot::new("user-wins");
        root.write("user/mimeapps.list", &defaults("firefox.desktop;"));
        root.write("system/mimeapps.list", &defaults("chromium.desktop;"));
        let firefox = root.write("applications/firefox.desktop", "[Desktop Entry]\n");
        root.write("applications/chromium.desktop", "[Desktop Entry]\n");

        let found = default_browser(
            &[root.0.join("user"), root.0.join("system")],
            &[root.0.join("applications")],
        );
        assert_eq!(found, Some(firefox));
    }

    #[test]
    fn a_default_that_is_not_installed_gives_the_next_one() {
        let root = TemporaryRoot::new("next");
        root.write("user/mimeapps.list", &defaults("brave-browser.desktop;"));
        root.write("system/mimeapps.list", &defaults("chromium.desktop"));
        let chromium = root.write("applications/chromium.desktop", "[Desktop Entry]\n");

        let found = default_browser(
            &[root.0.join("user"), root.0.join("system")],
            &[root.0.join("applications")],
        );
        assert_eq!(found, Some(chromium));
    }

    #[test]
    fn no_installed_default_gives_nothing() {
        let root = TemporaryRoot::new("none");
        root.write("system/mimeapps.list", &defaults("firefox.desktop;"));

        assert_eq!(
            default_browser(&[root.0.join("system")], &[root.0.join("applications")]),
            None
        );
    }
}
