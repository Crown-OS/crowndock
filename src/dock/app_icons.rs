//! The compositor opens each pinned app's windows out of its icon and minimizes them back into
//! it, so it is told where every icon rests with the dock fully up. Clicks go through it too:
//! it brings up the app's windows, or asks the dock to start an app that has none.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crownui::prelude::{Cx, Size, Styled, View, component, keyed, vstack};

use super::{state::Dock, tile::PLACEHOLDER_CORNER};
use crate::{desktop_entry, launcher};

/// Reports the icons whenever the pinned items, their order or the surface's size change, and
/// starts the apps the compositor asks for.
pub fn reporter(dock: Dock) -> impl View {
    component(move |cx| {
        cx.on_launch_app(move |cx, app_id| launch(cx, dock, app_id));
        keyed(
            move |runtime| {
                let surface = dock.surface.get(runtime);
                let items = dock
                    .settings
                    .with(runtime, |settings| settings.items.clone());
                (surface.width > 0.0).then_some((items, surface))
            },
            move |cx, layout| {
                if let Some((items, surface)) = layout {
                    report(cx, dock, &items, surface);
                }
                vstack(()).display_none()
            },
        )
    })
}

/// Opens the app `path` describes: through the compositor when it knows the icon, otherwise by
/// starting it.
pub fn open(cx: &mut Cx, dock: Dock, path: &Path) {
    let app_id = dock.app_ids.with(cx, |ids| ids.get(path).cloned());
    let activated = app_id.is_some_and(|app_id| cx.activate_app_icon(&app_id));
    if !activated {
        launcher::launch(path.to_owned());
    }
}

fn report(cx: &mut Cx, dock: Dock, items: &[PathBuf], surface: Size) {
    let mut previous = dock.app_ids.update(cx, std::mem::take);
    let ids: HashMap<PathBuf, String> = items
        .iter()
        .filter_map(|item| {
            let app_id = previous
                .remove(item)
                .or_else(|| desktop_entry::app_id(item))?;
            Some((item.clone(), app_id))
        })
        .collect();
    for unpinned in previous.values() {
        if !ids.values().any(|app_id| app_id == unpinned) {
            cx.remove_app_icon(unpinned);
        }
    }
    let metrics = dock.metrics;
    let corner_radius = metrics.icon * PLACEHOLDER_CORNER;
    for (index, item) in items.iter().enumerate() {
        if let Some(app_id) = ids.get(item) {
            let rect = metrics.resting_icon(index, items.len(), surface);
            cx.set_app_icon(app_id, rect, corner_radius);
        }
    }
    dock.app_ids.update(cx, |current| *current = ids);
}

fn launch(cx: &mut Cx, dock: Dock, app_id: &str) {
    let path = dock.app_ids.with(cx, |ids| {
        ids.iter()
            .find(|(_, id)| *id == app_id)
            .map(|(path, _)| path.clone())
    });
    match path {
        Some(path) => launcher::launch(path),
        None => log::warn!("the compositor asked to start {app_id}, which is not pinned"),
    }
}

#[cfg(test)]
mod tests {
    use crowntest::{TestApp, role};
    use crownui::{
        ext::{AppIconRequest, ListenerKind},
        prelude::{Listeners, Rect, Role, button, zstack},
    };

    use super::*;
    use crate::{
        dock::{probe, row},
        settings::{DockSettings, Metrics},
    };

    const SURFACE: Size = Size::new(1000.0, 90.0);
    const UNPIN_FIRST: &str = "unpin first";

    fn pinned(names: &[&str]) -> DockSettings {
        DockSettings {
            items: names
                .iter()
                .map(|name| PathBuf::from(format!("/nonexistent/{name}.desktop")))
                .collect(),
            autohide: false,
            ..DockSettings::default()
        }
    }

    fn dock_row(settings: DockSettings) -> impl View {
        let metrics = Metrics::new(&settings);
        component(move |cx| {
            let dock = Dock::new(cx, settings.clone(), metrics);
            dock.refresh(cx);
            let unpin_first = button(UNPIN_FIRST).on_click(move |cx, _| {
                dock.settings.update(cx, |settings| {
                    settings.items.remove(0);
                });
                dock.refresh(cx);
            });
            zstack((
                probe::size_probe(dock.surface).absolute().inset(0.0),
                reporter(dock),
                row(dock),
                unpin_first.absolute(),
            ))
        })
    }

    fn reported(app: &mut TestApp) -> Vec<(String, Rect)> {
        app.ui()
            .take_app_icon_requests()
            .into_iter()
            .filter_map(|request| match request {
                AppIconRequest::Set(app_id, placement) => Some((app_id, placement.rect)),
                _ => None,
            })
            .collect()
    }

    fn laid_out_icons(app: &mut TestApp) -> Vec<Rect> {
        let mut icons: Vec<Rect> = app
            .ui()
            .cx()
            .tree()
            .iter()
            .filter(|(_, node)| node.listens_to(ListenerKind::DragStart))
            .map(|(_, node)| node.geometry().window_rect)
            .collect();
        icons.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
        icons
    }

    #[test]
    fn each_pinned_app_is_reported_where_the_row_lays_its_icon_out() {
        let mut app = TestApp::with_size(
            dock_row(pinned(&["org.example.A", "org.example.B", "org.example.C"])),
            SURFACE.width,
            SURFACE.height,
        );
        app.settle();
        let reported = reported(&mut app);
        let ids: Vec<&str> = reported.iter().map(|(app_id, _)| app_id.as_str()).collect();
        assert_eq!(ids, ["org.example.A", "org.example.B", "org.example.C"]);
        let rects: Vec<Rect> = reported.iter().map(|(_, rect)| *rect).collect();
        assert_eq!(rects, laid_out_icons(&mut app));
    }

    #[test]
    fn a_resize_moves_the_icons_and_sends_nothing_when_settled() {
        let mut app = TestApp::with_size(
            dock_row(pinned(&["org.example.A", "org.example.B"])),
            SURFACE.width,
            SURFACE.height,
        );
        app.settle();
        reported(&mut app);
        app.resize(1400, 90).settle();
        let rects: Vec<Rect> = reported(&mut app)
            .into_iter()
            .map(|(_, rect)| rect)
            .collect();
        assert_eq!(rects, laid_out_icons(&mut app));
        app.settle();
        assert!(app.ui().take_app_icon_requests().is_empty());
    }

    #[test]
    fn an_unpinned_app_is_removed_and_the_rest_close_up() {
        let mut app = TestApp::with_size(
            dock_row(pinned(&["org.example.A", "org.example.B"])),
            SURFACE.width,
            SURFACE.height,
        );
        app.settle();
        reported(&mut app);
        app.click(role(Role::Button).name(UNPIN_FIRST)).settle();
        let requests = app.ui().take_app_icon_requests();
        assert_eq!(requests[0], AppIconRequest::Remove("org.example.A".into()));
        let moved: Vec<Rect> = requests
            .iter()
            .filter_map(|request| match request {
                AppIconRequest::Set(_, placement) => Some(placement.rect),
                _ => None,
            })
            .collect();
        assert_eq!(moved, laid_out_icons(&mut app));
    }
}
