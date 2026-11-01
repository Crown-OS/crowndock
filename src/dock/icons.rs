//! The row's icons on the GPU, decoded off the UI thread at exactly the size they are shown.
//! The decoded pixels are handed to the renderer and not kept; an icon the row no longer shows
//! is freed.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
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
    for source in missing {
        load(cx, dock, source, edge);
    }
}

fn load(cx: &mut Cx, dock: Dock, source: IconSource, edge: u32) {
    let decoded = source.clone();
    let decode = async move {
        tokio::task::spawn_blocking(move || decoded.load(edge))
            .await
            .ok()
            .flatten()
    };
    cx.spawn(decode, move |cx, pixels| {
        let current = dock.loads.with(cx, |loads| {
            loads.edge == edge && loads.attempted.contains(&source)
        });
        let (Some(pixels), true) = (pixels, current) else {
            return;
        };
        let image = cx.register_image(pixels.edge, pixels.edge, pixels.rgba);
        dock.icons.update(cx, |icons| {
            icons.insert(source, image);
        });
        dock.refresh(cx);
    });
}
