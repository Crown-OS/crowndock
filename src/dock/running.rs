//! What the compositor reports about the apps that have windows: how many each has, whether one
//! is focused, and which icons it is carrying through a window's open or minimize motion.

use std::path::PathBuf;

use crownui::prelude::{AppWindows, Cx};

use super::{HIDDEN, state::Dock};
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
    /// The icons the compositor is carrying through a window's motion.
    carried: Vec<Carried>,
}

/// What the compositor copied when it lifted an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconCopy {
    /// The icon, off a shown dock: the dock leaves its own undrawn until it lands.
    Drawn,
    /// Nothing, off a hidden dock: the dock keeps drawing its own for the window to land on.
    Blank,
}

#[derive(Debug)]
struct Carried {
    app_id: String,
    copy: IconCopy,
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

    /// Marks the icon of `app_id` as lifted by the compositor, or landed when `lifted` is
    /// `None`; returns whether that changed.
    pub fn carry(&mut self, app_id: &str, lifted: Option<IconCopy>) -> bool {
        let held = self.carried.iter().position(|held| held.app_id == app_id);
        match (held, lifted) {
            (None, Some(copy)) => self.carried.push(Carried {
                app_id: app_id.to_owned(),
                copy,
            }),
            (Some(index), None) => {
                self.carried.remove(index);
            }
            _ => return false,
        }
        true
    }

    /// Whether the compositor draws the icon of any of `app_ids`.
    pub fn carries(&self, app_ids: &[String]) -> bool {
        self.carried
            .iter()
            .filter(|held| held.copy == IconCopy::Drawn)
            .any(|held| {
                app_ids
                    .iter()
                    .any(|app_id| app_id.eq_ignore_ascii_case(&held.app_id))
            })
    }

    /// Whether a window is on its way into or out of any icon.
    pub fn carrying(&self) -> bool {
        !self.carried.is_empty()
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
        let copy = match dock.reveal.get(cx) < HIDDEN {
            true => IconCopy::Blank,
            false => IconCopy::Drawn,
        };
        let lifted = lifted.then_some(copy);
        if dock
            .running
            .update(cx, |running| running.carry(app_id, lifted))
        {
            dock.refresh(cx);
        }
        let carrying = dock.running.with(cx, Running::carrying);
        dock.update_presence(cx, |presence| presence.carrying = carrying);
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
        assert!(running.carry("a", Some(IconCopy::Drawn)));
        assert!(!running.carry("a", Some(IconCopy::Drawn)));
        assert!(running.carries(&["x".into(), "A".into()]));
        assert!(running.carrying());
        assert!(running.carry("a", None));
        assert!(!running.carry("a", None));
        assert!(!running.carries(&["a".into()]));
        assert!(!running.carrying());
    }

    #[test]
    fn an_icon_lifted_off_a_hidden_dock_stays_drawn_while_carried() {
        let mut running = Running::default();
        running.carry("a", Some(IconCopy::Blank));
        assert!(!running.carries(&["a".into()]));
        assert!(running.carrying());
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
