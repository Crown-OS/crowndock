//! What the compositor reports about the apps that have windows: how many each has, whether one
//! is focused, and which icons it is carrying through a window's open or minimize motion.

use std::path::PathBuf;

use crownui::prelude::{AppWindows, Cx};

use super::state::Dock;
use crate::desktop_entry;

/// An app with windows, as the compositor last reported it.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningApp {
    pub windows: AppWindows,
    /// The installed `.desktop` file describing it, looked up once when it appeared.
    pub entry: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct Running {
    /// In order of first appearance.
    pub apps: Vec<RunningApp>,
    /// The app ids whose icon the compositor draws instead of the dock.
    carried: Vec<String>,
}

impl Running {
    /// Records `update`, looking the app's `.desktop` file up with `find_entry` when it is new and
    /// forgetting it once it has no window left.
    pub fn remember(
        &mut self,
        update: &AppWindows,
        find_entry: impl FnOnce(&str) -> Option<PathBuf>,
    ) {
        let known = self
            .apps
            .iter()
            .position(|app| app.windows.app_id.eq_ignore_ascii_case(&update.app_id));
        match (known, update.count) {
            (Some(index), 0) => {
                self.apps.remove(index);
            }
            (Some(index), _) => self.apps[index].windows.clone_from(update),
            (None, 0) => {}
            (None, _) => self.apps.push(RunningApp {
                windows: update.clone(),
                entry: find_entry(&update.app_id),
            }),
        }
    }

    /// Marks the icon of `app_id` as drawn by the compositor or by the dock; returns whether
    /// that changed.
    pub fn carry(&mut self, app_id: &str, carried: bool) -> bool {
        let held = self.carried.iter().position(|held| held == app_id);
        match (held, carried) {
            (None, true) => self.carried.push(app_id.to_owned()),
            (Some(index), false) => {
                self.carried.remove(index);
            }
            _ => return false,
        }
        true
    }

    /// Whether the compositor draws the icon of any of `app_ids`.
    pub fn carries(&self, app_ids: &[String]) -> bool {
        self.carried.iter().any(|held| {
            app_ids
                .iter()
                .any(|app_id| app_id.eq_ignore_ascii_case(held))
        })
    }
}

/// Follows the compositor's reports for as long as the calling component lives.
pub fn track(cx: &mut Cx, dock: Dock) {
    cx.on_app_windows(move |cx, update| {
        dock.running
            .update(cx, |running| running.remember(update, desktop_entry::find));
        dock.refresh(cx);
    });
    cx.on_app_icon_lifted(move |cx, app_id, lifted| {
        if dock
            .running
            .update(cx, |running| running.carry(app_id, lifted))
        {
            dock.refresh(cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::testing::{laid_out_icons, start};

    use crate::dock::testing::windows;

    fn ids(running: &Running) -> Vec<&str> {
        running
            .apps
            .iter()
            .map(|app| app.windows.app_id.as_str())
            .collect()
    }

    #[test]
    fn apps_keep_the_order_they_first_appeared_in() {
        let mut running = Running::default();
        running.remember(&windows("b", 1, false), |_| None);
        running.remember(&windows("a", 1, true), |_| None);
        running.remember(&windows("B", 2, false), |_| None);
        assert_eq!(ids(&running), ["B", "a"]);
        assert_eq!(running.apps[0].windows, windows("B", 2, false));
    }

    #[test]
    fn the_desktop_file_is_looked_up_once_and_the_app_forgotten_without_windows() {
        let mut running = Running::default();
        running.remember(&windows("a", 1, false), |_| Some("/a.desktop".into()));
        running.remember(&windows("a", 2, false), |_| panic!("looked up twice"));
        assert_eq!(running.apps[0].entry, Some("/a.desktop".into()));
        running.remember(&windows("a", 0, false), |_| None);
        running.remember(&windows("c", 0, false), |_| None);
        assert!(running.apps.is_empty());
    }

    #[test]
    fn a_carried_icon_is_held_until_it_lands() {
        let mut running = Running::default();
        assert!(running.carry("a", true));
        assert!(!running.carry("a", true));
        assert!(running.carries(&["x".into(), "A".into()]));
        assert!(running.carry("a", false));
        assert!(!running.carry("a", false));
        assert!(!running.carries(&["a".into()]));
    }

    #[test]
    fn a_running_app_that_is_not_pinned_shows_after_the_pinned_ones_until_it_closes() {
        let mut app = start(&["org.example.A", "org.example.B"]);
        app.app_windows(windows("org.example.Unpinned", 1, false))
            .settle();
        let icons = laid_out_icons(&mut app);
        assert_eq!(icons.len(), 3);
        app.app_windows(windows("org.example.Unpinned", 0, false))
            .settle();
        let closed_up = laid_out_icons(&mut app);
        assert_eq!(closed_up.len(), 2);
        assert!(closed_up[0].origin.x > icons[0].origin.x);
    }

    #[test]
    fn a_running_pinned_app_keeps_its_one_icon() {
        let mut app = start(&["org.example.A"]);
        app.app_windows(windows("ORG.EXAMPLE.a", 2, true)).settle();
        assert_eq!(laid_out_icons(&mut app).len(), 1);
    }
}
