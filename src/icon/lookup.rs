use std::path::{Path, PathBuf};

pub const FALLBACK: &str = "application-x-executable";

/// The file behind an `Icon=` value: an absolute path as is, a name through the icon theme at
/// the size closest to `edge`.
pub fn find(name: &str, edge: u32) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_owned());
    }
    freedesktop_icons::lookup(name)
        .with_size(u16::try_from(edge).unwrap_or(u16::MAX))
        .find()
}
