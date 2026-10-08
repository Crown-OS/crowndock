//! The pinned items' icons on the GPU, decoded off the UI thread at exactly the size they are
//! shown. The decoded pixels are handed to the renderer and not kept; an unpinned item's
//! icon is freed.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crownui::prelude::Cx;

use super::state::Dock;
use crate::icon;

/// Icons being decoded, and the pixel edge they are decoded at.
#[derive(Default)]
pub struct IconLoads {
    edge: u32,
    pending: HashSet<PathBuf>,
}

/// Starts over at `scale`: every icon is freed and decoded again at the new size.
pub fn reload(cx: &mut Cx, dock: Dock, scale: f32) {
    let edge = (dock.metrics.icon * scale).round().max(1.0) as u32;
    let released: Vec<_> = dock
        .icons
        .with(cx, |icons| icons.values().copied().collect());
    dock.icons.set(cx, HashMap::new());
    for image in released {
        cx.release_image(image);
    }
    dock.loads.update(cx, |loads| {
        loads.edge = edge;
        loads.pending.clear();
    });
    sync(cx, dock);
    dock.refresh(cx);
}

/// Frees the icons of items no longer pinned and starts decoding those not loaded yet.
pub fn sync(cx: &mut Cx, dock: Dock) {
    let items = dock.settings.with(cx, |settings| settings.items.clone());
    let mut unpinned = Vec::new();
    dock.icons.update(cx, |icons| {
        icons.retain(|path, image| {
            let pinned = items.contains(path);
            if !pinned {
                unpinned.push(*image);
            }
            pinned
        });
    });
    for image in unpinned {
        cx.release_image(image);
    }
    let edge = dock.loads.with(cx, |loads| loads.edge);
    if edge == 0 {
        return;
    }
    let missing: Vec<PathBuf> = dock.icons.with(cx, |icons| {
        items
            .into_iter()
            .filter(|path| !icons.contains_key(path))
            .collect()
    });
    for path in missing {
        if dock
            .loads
            .update(cx, |loads| loads.pending.insert(path.clone()))
        {
            load(cx, dock, path, edge);
        }
    }
}

fn load(cx: &mut Cx, dock: Dock, path: PathBuf, edge: u32) {
    let source = path.clone();
    let decode = async move {
        tokio::task::spawn_blocking(move || icon::load(&source, edge))
            .await
            .ok()
            .flatten()
    };
    cx.spawn(decode, move |cx, pixels| {
        let current = dock.loads.update(cx, |loads| {
            loads.edge == edge && loads.pending.remove(&path)
        });
        let pinned = dock
            .settings
            .with(cx, |settings| settings.items.contains(&path));
        let (Some(pixels), true, true) = (pixels, current, pinned) else {
            return;
        };
        let image = cx.register_image(pixels.edge, pixels.edge, pixels.rgba);
        dock.icons.update(cx, |icons| {
            icons.insert(path, image);
        });
        dock.refresh(cx);
    });
}
