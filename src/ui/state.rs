use std::path::PathBuf;

use vello::peniko::ImageData;

use crate::{persistence, ui::icon};

pub struct Icon {
    pub path: PathBuf,
    pub image: Option<ImageData>,
}

impl Icon {
    pub fn new(path: PathBuf) -> Self {
        let image = match icon::load_from_desktop(&path) {
            Ok(img) => Some(img),
            Err(e) => {
                log::warn!("icon load failed for {}: {e}", path.display());
                None
            }
        };
        Self { path, image }
    }

    /// Deterministic fallback color used when the .desktop file has no
    /// resolvable icon — keeps the slot visible instead of going blank.
    pub fn fallback_color(&self) -> [f32; 4] {
        let bytes = self
            .path
            .as_os_str()
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        let mut hash: u32 = 0x811c_9dc5;
        for b in bytes {
            hash ^= b as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        let h = (hash & 0xFF) as f32 / 255.0;
        let (r, g, b) = hsv_to_rgb(h, 0.55, 0.95);
        [r, g, b, 1.0]
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Snapshot describing the icon currently being dragged, in render-ready
/// form so the UI layer doesn't need to know about timing or input state.
#[derive(Clone)]
pub struct DragRender {
    pub path: PathBuf,
    pub image: Option<ImageData>,
    pub cx: f32,
    pub cy: f32,
    pub scale: f32,
    pub alpha: f32,
}

/// An icon that is animating out (was dragged off the dock and released).
/// Owns its own image data because the corresponding `Icon` is already gone
/// from `icons` by the time we draw the animation.
#[derive(Clone)]
pub struct Vanishing {
    pub path: PathBuf,
    pub image: Option<ImageData>,
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub scale: f32,
    pub alpha: f32,
}

#[derive(Default)]
pub struct State {
    pub icons: Vec<Icon>,
    pub drag_skip_idx: Option<usize>,
    pub drag_render: Option<DragRender>,
    pub vanishing: Vec<Vanishing>,
}

impl State {
    /// Build a fresh `State` populated with whatever pinned items are
    /// persisted under the user's XDG config dir, in saved order.
    pub fn load() -> Self {
        let icons = persistence::load_items()
            .into_iter()
            .map(Icon::new)
            .collect();
        Self {
            icons,
            ..Self::default()
        }
    }

    pub fn add_icon(&mut self, path: PathBuf) {
        if self.icons.iter().any(|i| i.path == path) {
            return;
        }
        self.icons.push(Icon::new(path));
        self.persist();
    }

    /// Remove the icon at `idx`, returning it. Persists the new order.
    pub fn remove_icon(&mut self, idx: usize) -> Option<Icon> {
        if idx >= self.icons.len() {
            return None;
        }
        let icon = self.icons.remove(idx);
        self.persist();
        Some(icon)
    }

    fn persist(&self) {
        let paths: Vec<PathBuf> = self.icons.iter().map(|i| i.path.clone()).collect();
        if let Err(e) = persistence::save_items(&paths) {
            log::warn!("persist dock items: {e:#}");
        }
    }
}
