mod decode;
mod lookup;

use std::path::Path;

use crate::desktop_entry::DesktopEntry;

/// A square icon in straight-alpha RGBA8.
pub struct Pixels {
    pub edge: u32,
    pub rgba: Vec<u8>,
}

/// The icon of the application `desktop_file` describes, `edge` pixels square, or the generic
/// executable icon when it names none that can be found.
pub fn load(desktop_file: &Path, edge: u32) -> Option<Pixels> {
    let named = DesktopEntry::read(desktop_file)
        .ok()
        .and_then(|entry| entry.icon)
        .and_then(|name| lookup::find(&name, edge));
    let file = named.or_else(|| lookup::find(lookup::FALLBACK, edge))?;
    match decode::decode(&file, edge) {
        Ok(pixels) => Some(pixels),
        Err(error) => {
            log::warn!("icon {}: {error:#}", file.display());
            None
        }
    }
}
