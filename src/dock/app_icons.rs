//! The compositor opens each app's windows out of its icon and minimizes them back into it, so
//! it is told where every icon rests with the dock fully up, under every app id the icon stands
//! for. Clicks go through it too: it brings up, minimizes or cycles the app's windows, or asks
//! the dock to start an app that has none.

use crownui::prelude::{Cx, Size, Styled, View, component, keyed, vstack};

use super::{apps::AppKey, state::Dock, tile::PLACEHOLDER_CORNER};
use crate::launcher;

/// Reports the icons whenever the row's apps, their order or the surface's size change, and
/// starts the apps the compositor asks for.
pub fn reporter(dock: Dock) -> impl View {
    component(move |cx| {
        cx.on_launch_app(move |cx, app_id| launch(cx, dock, app_id));
        keyed(
            move |runtime| {
                let surface = dock.surface.get(runtime);
                let icons: Vec<Vec<String>> = dock.apps.with(runtime, |apps| {
                    apps.iter().map(|app| app.app_ids.clone()).collect()
                });
                (surface.width > 0.0).then_some((icons, surface))
            },
            move |cx, layout| {
                if let Some((icons, surface)) = layout {
                    report(cx, dock, &icons, surface);
                }
                vstack(()).display_none()
            },
        )
    })
}

/// Opens the app behind `key`: through the compositor when it knows the icon, otherwise by
/// starting it.
pub fn open(cx: &mut Cx, dock: Dock, key: &AppKey) {
    let target = dock.apps.with(cx, |apps| {
        apps.iter()
            .find(|app| app.key == *key)
            .map(|app| (app.app_ids.first().cloned(), app.entry.clone()))
    });
    let Some((app_id, entry)) = target else {
        return;
    };
    let activated = app_id.is_some_and(|app_id| cx.activate_app_icon(&app_id));
    if !activated && let Some(entry) = entry {
        launcher::launch(entry);
    }
}

/// `icons` holds the app ids of each icon, in row order.
fn report(cx: &mut Cx, dock: Dock, icons: &[Vec<String>], surface: Size) {
    let shown: Vec<String> = icons.iter().flatten().cloned().collect();
    let previous = dock
        .reported
        .update(cx, |reported| std::mem::replace(reported, shown.clone()));
    for gone in previous.iter().filter(|app_id| !shown.contains(app_id)) {
        cx.remove_app_icon(gone);
    }
    let metrics = dock.metrics;
    let corner_radius = metrics.icon * PLACEHOLDER_CORNER;
    for (index, app_ids) in icons.iter().enumerate() {
        let rect = metrics.resting_icon(index, icons.len(), surface);
        for app_id in app_ids {
            cx.set_app_icon(app_id, rect, corner_radius);
        }
    }
}

fn launch(cx: &mut Cx, dock: Dock, app_id: &str) {
    let entry = dock.apps.with(cx, |apps| {
        apps.iter()
            .find(|app| app.knows(app_id))
            .and_then(|app| app.entry.clone())
    });
    match entry {
        Some(entry) => launcher::launch(entry),
        None => log::warn!("the compositor asked to start {app_id}, which the dock cannot start"),
    }
}

#[cfg(test)]
mod tests {
    use crowntest::{TestApp, role};
    use crownui::{
        ext::AppIconRequest,
        prelude::{Capabilities, Rect, Role},
    };

    use crate::dock::testing::{
        SURFACE, UNPIN_FIRST, dock_row, laid_out_icons, pinned, start, windows,
    };

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

    #[test]
    fn each_pinned_app_is_reported_where_the_row_lays_its_icon_out() {
        let mut app = start(&["org.example.A", "org.example.B", "org.example.C"]);
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
        let mut app = start(&["org.example.A", "org.example.B"]);
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

    #[test]
    fn a_running_app_that_is_not_pinned_is_reported_after_the_pinned_ones_until_it_closes() {
        let mut app = start(&["org.example.A"]);
        reported(&mut app);
        app.app_windows(windows("org.example.Unpinned", 1, false))
            .settle();
        let reported = reported(&mut app);
        let unpinned: Vec<Rect> = reported
            .iter()
            .filter(|(app_id, _)| app_id == "org.example.Unpinned")
            .map(|(_, rect)| *rect)
            .collect();
        assert_eq!(unpinned, laid_out_icons(&mut app)[1..]);
        app.app_windows(windows("org.example.Unpinned", 0, false))
            .settle();
        let requests = app.ui().take_app_icon_requests();
        assert_eq!(
            requests[0],
            AppIconRequest::Remove("org.example.Unpinned".into())
        );
    }

    #[test]
    fn a_click_on_a_running_app_hands_it_to_the_compositor() {
        let mut app = start(&["org.example.A"]);
        app.ui().set_platform_capabilities(Capabilities {
            app_icons: true,
            ..Capabilities::default()
        });
        app.app_windows(windows("org.example.Unpinned", 2, true))
            .settle();
        let center = laid_out_icons(&mut app)[1].center();
        app.pointer_down(center.x, center.y)
            .pointer_up(center.x, center.y);
        let requests = app.ui().take_app_icon_requests();
        assert_eq!(
            requests.last(),
            Some(&AppIconRequest::Activate("org.example.Unpinned".into()))
        );
    }
}
