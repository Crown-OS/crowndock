//! `dock.ron` as it is edited: by the settings app, by hand, or by another dock.

use crownconfig::SectionWatch;
use crownui::prelude::Cx;

use super::{icons, presence, state::Dock};
use crate::settings::{DockSettings, Metrics};

pub fn follow(cx: &mut Cx, dock: Dock, initial: DockSettings) {
    next(
        cx,
        dock,
        crownconfig::watch_section(DockSettings::SECTION, initial),
    );
}

fn next(cx: &mut Cx, dock: Dock, mut watch: SectionWatch<DockSettings>) {
    let edited = async move {
        let settings = watch.changed().await;
        (watch, settings)
    };
    cx.spawn(edited, move |cx, (watch, settings)| {
        apply(cx, dock, settings);
        next(cx, dock, watch);
    });
}

fn apply(cx: &mut Cx, dock: Dock, settings: DockSettings) {
    let reshaped = dock.settings.with(cx, |current| {
        Metrics::new(&settings) != dock.metrics
            || settings.layer != current.layer
            || settings.autohide != current.autohide
    });
    if reshaped {
        log::info!("dock.ron reshapes the dock; restart crowndock to apply it fully");
    }
    dock.settings.set(cx, settings);
    icons::sync(cx, dock);
    dock.refresh(cx);
    presence::changed(cx, dock);
}
