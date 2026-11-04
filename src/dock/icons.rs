//! The row's icons on the GPU, decoded off the UI thread at exactly the size they are shown.
//! The pixels of `.desktop` icons are also kept, so dragging one hands the compositor its icon
//! without decoding it again on the UI thread; an icon the row no longer shows is freed.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crownui::prelude::Cx;

use super::state::Dock;
use crate::icon::{self, Pixels};

/// Where an icon's pixels come from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconSource {
    /// The icon a `.desktop` file names.
    Entry(PathBuf),
    /// The icon theme's icon of this name, for an app no `.desktop` file describes.
    Themed(String),
}

impl IconSource {
    fn load(&self, edge: u32) -> Option<Pixels> {
        match self {
            Self::Entry(desktop_file) => icon::load(desktop_file, edge),
            Self::Themed(name) => icon::load_themed(name, edge),
        }
    }
}

#[derive(Default)]
pub struct IconLoads {
    /// The pixel edge icons are decoded at.
    edge: u32,
    /// Icons decoded, being decoded or that failed to decode at this edge, so none is decoded
    /// twice.
    attempted: HashSet<IconSource>,
    /// The decoded pixels of the `.desktop` files' icons, for drags.
    draggable: HashMap<IconSource, Pixels>,
}

/// The pixels of the icon `desktop_file` names, as the row shows it.
pub fn draggable(cx: &mut Cx, dock: Dock, desktop_file: &Path) -> Option<Pixels> {
    dock.loads.with(cx, |loads| {
        let source = IconSource::Entry(desktop_file.to_owned());
        loads.draggable.get(&source).cloned()
    })
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
        loads.attempted.clear();
        loads.draggable.clear();
    });
    dock.refresh(cx);
}

/// Frees the icons the row no longer shows and starts decoding those not loaded yet.
pub fn sync(cx: &mut Cx, dock: Dock) {
    let wanted: HashSet<IconSource> = dock
        .apps
        .with(cx, |apps| apps.iter().map(|app| app.icon.clone()).collect());
    let mut unwanted = Vec::new();
    dock.icons.update(cx, |icons| {
        icons.retain(|source, image| {
            let shown = wanted.contains(source);
            if !shown {
                unwanted.push(*image);
            }
            shown
        });
    });
    for image in unwanted {
        cx.release_image(image);
    }
    let edge = dock.loads.update(cx, |loads| {
        loads.attempted.retain(|source| wanted.contains(source));
        loads.draggable.retain(|source, _| wanted.contains(source));
        loads.edge
    });
    if edge == 0 {
        return;
    }
    let missing: Vec<IconSource> = dock.loads.update(cx, |loads| {
        wanted
            .into_iter()
            .filter(|source| loads.attempted.insert(source.clone()))
            .collect()
    });
    if !missing.is_empty() {
        load(cx, dock, missing, edge);
    }
}

/// Decodes `sources` one after another on a single blocking thread, so a dock full of icons
/// does not spread its decoding buffers over as many threads and allocator arenas, and shows
/// them all in one refresh.
fn load(cx: &mut Cx, dock: Dock, sources: Vec<IconSource>, edge: u32) {
    let decode = async move {
        tokio::task::spawn_blocking(move || {
            sources
                .into_iter()
                .filter_map(|source| source.load(edge).map(|pixels| (source, pixels)))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default()
    };
    cx.spawn(decode, move |cx, decoded| {
        for (source, pixels) in decoded {
            show(cx, dock, source, pixels, edge);
        }
        dock.refresh(cx);
    });
}

fn show(cx: &mut Cx, dock: Dock, source: IconSource, pixels: Pixels, edge: u32) {
    let current = dock.loads.with(cx, |loads| {
        loads.edge == edge && loads.attempted.contains(&source)
    });
    if !current {
        return;
    }
    if matches!(source, IconSource::Entry(_)) {
        let kept = (source.clone(), pixels.clone());
        dock.loads.update(cx, |loads| {
            loads.draggable.insert(kept.0, kept.1);
        });
    }
    let image = cx.register_image(pixels.edge, pixels.edge, pixels.rgba);
    dock.icons.update(cx, |icons| {
        icons.insert(source, image);
    });
}
