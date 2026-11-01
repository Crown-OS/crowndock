//! Which installed `.desktop` file describes the app whose windows set a given app_id.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use super::{DesktopEntry, EXTENSION};
use crate::util::xdg;

/// The `.desktop` file of the application `app_id`, among those installed: the one named after
/// it, or else the one whose `StartupWMClass` it is, both without regard to ASCII case.
pub fn find(app_id: &str) -> Option<PathBuf> {
    find_in(&xdg::application_dirs(), app_id)
}

fn find_in(dirs: &[PathBuf], app_id: &str) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = dirs.iter().flat_map(|dir| desktop_files(dir)).collect();
    let named = entries.iter().find(|path| {
        path.file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| stem.eq_ignore_ascii_case(app_id))
    });
    named
        .or_else(|| {
            entries.iter().find(|path| {
                DesktopEntry::read(path)
                    .ok()
                    .and_then(|entry| entry.startup_wm_class)
                    .is_some_and(|class| class.eq_ignore_ascii_case(app_id))
            })
        })
        .cloned()
}

fn desktop_files(dir: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == EXTENSION)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Installed(PathBuf);

    impl Installed {
        fn new(name: &str, entries: &[(&str, &str)]) -> Self {
            let dir = std::env::temp_dir().join(format!("crowndock-{name}-{}", std::process::id()));
            let written = std::fs::create_dir_all(&dir).and_then(|()| {
                entries
                    .iter()
                    .try_for_each(|(file, text)| std::fs::write(dir.join(file), text))
            });
            if let Err(error) = written {
                panic!("write the test entries: {error}");
            }
            Self(dir)
        }

        fn dirs(&self) -> &[PathBuf] {
            std::slice::from_ref(&self.0)
        }
    }

    impl Drop for Installed {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const PLAIN: &str = "[Desktop Entry]\nName=App\nExec=app\n";

    #[test]
    fn an_app_is_found_by_its_file_name_in_any_case() {
        let installed = Installed::new("named", &[("org.example.Files.desktop", PLAIN)]);
        assert_eq!(
            find_in(installed.dirs(), "ORG.EXAMPLE.files"),
            Some(installed.0.join("org.example.Files.desktop"))
        );
    }

    #[test]
    fn an_app_is_found_by_its_startup_wm_class_when_no_file_is_named_after_it() {
        let classed = "[Desktop Entry]\nName=Code\nExec=code\nStartupWMClass=Code\n";
        let installed = Installed::new(
            "classed",
            &[
                ("visual-studio-code.desktop", classed),
                ("other.desktop", PLAIN),
            ],
        );
        assert_eq!(
            find_in(installed.dirs(), "code"),
            Some(installed.0.join("visual-studio-code.desktop"))
        );
    }

    #[test]
    fn the_first_directory_shadows_the_later_ones() {
        let user = Installed::new("user", &[("app.desktop", PLAIN)]);
        let system = Installed::new("system", &[("app.desktop", PLAIN)]);
        assert_eq!(
            find_in(&[user.0.clone(), system.0.clone()], "app"),
            Some(user.0.join("app.desktop"))
        );
    }

    #[test]
    fn an_app_nothing_describes_is_not_found() {
        let installed = Installed::new("unlisted", &[("app.desktop", PLAIN)]);
        assert_eq!(find_in(installed.dirs(), "org.example.Unlisted"), None);
    }
}
