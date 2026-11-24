//! The XDG base directories (freedesktop basedir spec) that hold `.desktop` and
//! `mimeapps.list` files.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

const APPLICATIONS: &str = "applications";
const LOCAL_DATA: &str = ".local/share";
const SYSTEM_DATA: &str = "/usr/local/share:/usr/share";
const LOCAL_CONFIG: &str = ".config";
const SYSTEM_CONFIG: &str = "/etc/xdg";

/// Where applications are installed, the user's own first so they shadow the system's.
pub fn application_dirs() -> Vec<PathBuf> {
    data_dirs(
        env::var_os("XDG_DATA_HOME"),
        env::var_os("HOME"),
        env::var_os("XDG_DATA_DIRS"),
    )
    .into_iter()
    .map(|dir| dir.join(APPLICATIONS))
    .collect()
}

/// `$XDG_CONFIG_HOME` then `$XDG_CONFIG_DIRS`, the user's own first.
pub fn config_dirs() -> Vec<PathBuf> {
    base_dirs(
        env::var_os("XDG_CONFIG_HOME"),
        env::var_os("HOME").map(|home| Path::new(&home).join(LOCAL_CONFIG).into()),
        env::var_os("XDG_CONFIG_DIRS"),
        SYSTEM_CONFIG,
    )
}

/// `$XDG_DATA_HOME` then `$XDG_DATA_DIRS`, each falling back to its default when unset or empty;
/// relative entries are ignored, as the spec asks.
fn data_dirs(
    data_home: Option<OsString>,
    home: Option<OsString>,
    data_dirs: Option<OsString>,
) -> Vec<PathBuf> {
    let home_default = non_empty(home).map(|home| Path::new(&home).join(LOCAL_DATA).into());
    base_dirs(data_home, home_default, data_dirs, SYSTEM_DATA)
}

fn base_dirs(
    user_dir: Option<OsString>,
    user_default: Option<OsString>,
    system_dirs: Option<OsString>,
    system_default: &str,
) -> Vec<PathBuf> {
    let user = non_empty(user_dir)
        .or(non_empty(user_default))
        .map(PathBuf::from);
    let system = non_empty(system_dirs).unwrap_or_else(|| system_default.into());
    user.into_iter()
        .chain(env::split_paths(&system))
        .filter(|dir| dir.is_absolute())
        .collect()
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dirs: &[&str]) -> Vec<PathBuf> {
        dirs.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_user_data_dir_comes_before_the_system_ones() {
        let dirs = data_dirs(
            Some("/data/home".into()),
            Some("/home/me".into()),
            Some("/opt/share:relative:/usr/share".into()),
        );
        assert_eq!(dirs, paths(&["/data/home", "/opt/share", "/usr/share"]));
    }

    #[test]
    fn unset_dirs_fall_back_to_their_defaults() {
        let dirs = data_dirs(Some("".into()), Some("/home/me".into()), None);
        assert_eq!(
            dirs,
            paths(&["/home/me/.local/share", "/usr/local/share", "/usr/share"])
        );
    }
}
