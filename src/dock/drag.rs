//! Dragging with the system's drag and drop, so the compositor draws the icon under the
//! pointer anywhere on screen and any app can be a source or a target.
//!
//! The dock takes `text/uri-list` drags of `.desktop` files: from the launcher, a file manager
//! or itself. An icon dragged out of the dock offers the same, so it can be dropped back to
//! move it, or let go anywhere else to unpin it.

use std::path::{Path, PathBuf};

use crownui::{
    ext::{DragOverEvent, DropEvent},
    prelude::{Cx, DragIcon, DragOutcome, DragSource, Point},
};

use super::{slots, state::Dock};
use crate::{
    icon,
    util::uri::{self, URI_LIST},
};

const DESKTOP_EXTENSION: &str = "desktop";

/// Hands `path`'s icon to the compositor to carry, held at `grip`, and opens a gap where it was.
pub fn lift(cx: &mut Cx, dock: Dock, path: &Path, grip: Point) {
    let size = dock.metrics.icon;
    let scale = cx.scale_factor().with(cx, |scale| *scale);
    let edge = (size * scale).round().max(1.0) as u32;
    let icon = icon::load(path, edge).map(|pixels| DragIcon {
        width: pixels.edge,
        height: pixels.edge,
        rgba: pixels.rgba,
        logical_width: size as u32,
        logical_height: size as u32,
        hotspot: grip,
    });
    let source = DragSource {
        items: vec![(URI_LIST.to_owned(), uri::uri_list(path))],
        icon,
    };
    if !cx.start_drag(source, move |cx, outcome| land(cx, dock, outcome)) {
        return;
    }
    let index = dock.settings.with(cx, |settings| {
        settings.items.iter().position(|item| item == path)
    });
    dock.update_drag(cx, |drag| {
        drag.lifted = Some(path.to_owned());
        drag.gap = index;
    });
    dock.update_presence(cx, |presence| presence.lifting = true);
}

/// A lifted icon that was not dropped back on the dock is unpinned.
fn land(cx: &mut Cx, dock: Dock, outcome: DragOutcome) {
    let lifted = dock.drag.with(cx, |drag| drag.lifted.clone());
    dock.drag.update(cx, |drag| *drag = Default::default());
    match lifted {
        Some(lifted) => {
            log::debug!("{} dragged off the dock ({outcome:?})", lifted.display());
            dock.update_items(cx, |items| items.retain(|item| *item != lifted));
        }
        None => dock.refresh(cx),
    }
    dock.update_presence(cx, |presence| {
        presence.lifting = false;
        presence.drag = false;
    });
}

pub fn over(cx: &mut Cx, dock: Dock, event: &mut DragOverEvent) {
    if !event.offers(URI_LIST) {
        return;
    }
    event.accept(URI_LIST);
    let center = dock.surface.with(cx, |surface| surface.width / 2.0);
    let shown = dock.shown_count(cx);
    let gap = slots::gap_index(event.position.x, center, shown, &dock.metrics);
    if dock.drag.with(cx, |drag| drag.gap != Some(gap)) {
        dock.update_drag(cx, |drag| drag.gap = Some(gap));
    }
    dock.update_presence(cx, |presence| presence.drag = true);
}

pub fn leave(cx: &mut Cx, dock: Dock) {
    dock.update_drag(cx, |drag| drag.gap = None);
    dock.update_presence(cx, |presence| presence.drag = false);
}

pub fn drop(cx: &mut Cx, dock: Dock, event: &DropEvent) {
    let text = String::from_utf8_lossy(&event.data);
    let dropped: Vec<PathBuf> = uri::file_paths(&text)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == DESKTOP_EXTENSION)
        })
        .collect();
    let shown = dock.shown_count(cx);
    let drag = dock.drag.with(cx, Clone::clone);
    let landed = drag
        .lifted
        .as_ref()
        .is_some_and(|lifted| dropped.contains(lifted));
    dock.drag.update(cx, |drag| {
        drag.gap = None;
        if landed {
            drag.lifted = None;
        }
    });
    let index = drag.gap.unwrap_or(shown);
    let lifted = drag.lifted.as_deref().filter(|_| landed);
    if dropped.is_empty() {
        dock.refresh(cx);
    } else {
        dock.update_items(cx, |items| slots::place(items, lifted, &dropped, index));
    }
    dock.update_presence(cx, |presence| presence.drag = false);
}
