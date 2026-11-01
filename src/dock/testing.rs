//! A dock row under crowntest, fed the compositor's reports the way the dock is.

use std::path::PathBuf;

use crowntest::TestApp;
use crownui::{
    ext::ListenerKind,
    prelude::{AppWindows, Color, Listeners, Rect, Size, Styled, View, button, component, zstack},
};

use super::{app_icons, glass::Tint, probe, row, running, state::Dock};
use crate::settings::{DockSettings, Metrics};

pub const SURFACE: Size = Size::new(1000.0, 90.0);
pub const UNPIN_FIRST: &str = "unpin first";

pub fn tint() -> Tint {
    Tint::new(&crownconfig::Appearance::default())
}

pub fn pinned(names: &[&str]) -> DockSettings {
    DockSettings {
        items: names
            .iter()
            .map(|name| PathBuf::from(format!("/nonexistent/{name}.desktop")))
            .collect(),
        autohide: false,
        ..DockSettings::default()
    }
}

pub fn windows(app_id: &str, count: u32, focused: bool) -> AppWindows {
    AppWindows {
        app_id: app_id.to_owned(),
        count,
        focused,
    }
}

pub fn dock_row(settings: DockSettings) -> impl View {
    let metrics = Metrics::new(&settings);
    component(move |cx| {
        let dock = Dock::new(cx, settings.clone(), metrics);
        running::track(cx, dock);
        dock.refresh(cx);
        let unpin_first = button(UNPIN_FIRST).on_click(move |cx, _| {
            dock.settings.update(cx, |settings| {
                settings.items.remove(0);
            });
            dock.refresh(cx);
        });
        zstack((
            probe::size_probe(dock.surface).absolute().inset(0.0),
            app_icons::reporter(dock),
            row(dock, tint()),
            unpin_first.absolute(),
        ))
    })
}

/// A settled dock pinning `names`.
pub fn start(names: &[&str]) -> TestApp {
    let mut app = TestApp::with_size(dock_row(pinned(names)), SURFACE.width, SURFACE.height);
    app.settle();
    app
}

fn icon_edge() -> f32 {
    Metrics::new(&DockSettings::default()).icon
}

/// Every app icon in the row, left to right.
pub fn laid_out_icons(app: &mut TestApp) -> Vec<Rect> {
    let edge = icon_edge();
    let mut icons: Vec<Rect> = app
        .ui()
        .cx()
        .tree()
        .iter()
        .filter(|(_, node)| node.listens_to(ListenerKind::Click))
        .map(|(_, node)| node.geometry().window_rect)
        .filter(|rect| rect.size == Size::new(edge, edge))
        .collect();
    icons.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    icons
}

/// How many nodes are filled with `color`.
pub fn painted(app: &mut TestApp, color: Color) -> usize {
    app.ui()
        .cx()
        .tree()
        .iter()
        .filter(|(_, node)| node.visual().background == Some(color))
        .count()
}

/// How many icon-sized nodes are fully transparent.
pub fn hidden_icons(app: &mut TestApp) -> usize {
    let edge = icon_edge();
    app.ui()
        .cx()
        .tree()
        .iter()
        .filter(|(_, node)| {
            node.visual().opacity == 0.0
                && node.geometry().window_rect.size == Size::new(edge, edge)
        })
        .count()
}
