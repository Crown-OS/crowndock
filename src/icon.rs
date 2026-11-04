mod decode;
mod lookup;

use std::path::{Path, PathBuf};

use crate::desktop_entry::DesktopEntry;

/// A square icon in straight-alpha RGBA8.
#[derive(Clone)]
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
    decode_or_fallback(named, edge)
}

/// The icon theme's icon called `name`, `edge` pixels square, or the generic executable icon.
pub fn load_themed(name: &str, edge: u32) -> Option<Pixels> {
    decode_or_fallback(lookup::find(name, edge), edge)
}

fn decode_or_fallback(found: Option<PathBuf>, edge: u32) -> Option<Pixels> {
    let file = found.or_else(|| lookup::find(lookup::FALLBACK, edge))?;
    match decode::decode(&file, edge) {
        Ok(pixels) => Some(pixels),
        Err(error) => {
            log::warn!("icon {}: {error:#}", file.display());
            None
        }
    }
}
