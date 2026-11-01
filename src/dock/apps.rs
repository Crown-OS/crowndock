//! The icons the row shows at rest: the pinned items in their order, then the apps that have
//! windows without being pinned, in the order they appeared. Each running app belongs to the
//! first pinned item it was found to be, by its `.desktop` file or its app id.

use std::{collections::HashMap, path::PathBuf};

use crownui::prelude::AppWindows;

use super::{
    icons::IconSource,
    running::{Running, RunningApp},
};

/// What a dock icon stands for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppKey {
    /// A pinned `.desktop` file.
    Pinned(PathBuf),
    /// An app that has windows and is not pinned, by its app id.
    Running(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockApp {
    pub key: AppKey,
    /// The `.desktop` file that starts the app, when there is one.
    pub entry: Option<PathBuf>,
    pub icon: IconSource,
    /// The app ids its windows set, those with windows first. The compositor is told the icon
    /// under each of them, and the first is the one a click activates.
    pub app_ids: Vec<String>,
    pub windows: u32,
    pub focused: bool,
    /// Whether the compositor draws the icon itself, during a window's motion.
    pub carried: bool,
}

impl DockApp {
    pub fn pinned(item: PathBuf) -> Self {
        Self {
            key: AppKey::Pinned(item.clone()),
            entry: Some(item.clone()),
            icon: IconSource::Entry(item),
            app_ids: Vec::new(),
            windows: 0,
            focused: false,
            carried: false,
        }
    }

    fn running(app: &RunningApp) -> Self {
        let app_id = &app.windows.app_id;
        let icon = match &app.entry {
            Some(entry) => IconSource::Entry(entry.clone()),
            None => IconSource::Themed(app_id.clone()),
        };
        let mut running = Self {
            key: AppKey::Running(app_id.clone()),
            entry: app.entry.clone(),
            icon,
            app_ids: Vec::new(),
            windows: 0,
            focused: false,
            carried: false,
        };
        running.absorb(&app.windows);
        running
    }

    fn absorb(&mut self, windows: &AppWindows) {
        self.add_app_id(&windows.app_id);
        self.windows += windows.count;
        self.focused |= windows.focused;
    }

    fn add_app_id(&mut self, app_id: &str) {
        if !self.knows(app_id) {
            self.app_ids.push(app_id.to_owned());
        }
    }

    pub fn knows(&self, app_id: &str) -> bool {
        self.app_ids
            .iter()
            .any(|known| known.eq_ignore_ascii_case(app_id))
    }
}

/// The row at rest: `items` with the app ids `pinned_ids` gives them, then every running app
/// none of them claims.
pub fn rest(
    items: &[PathBuf],
    pinned_ids: &HashMap<PathBuf, String>,
    running: &Running,
) -> Vec<DockApp> {
    let mut claimed = vec![false; running.apps.len()];
    let mut row: Vec<DockApp> = items
        .iter()
        .map(|item| {
            let pinned_id = pinned_ids.get(item);
            let mut app = DockApp::pinned(item.clone());
            for (index, running_app) in running.apps.iter().enumerate() {
                let owned = running_app.entry.as_ref() == Some(item)
                    || pinned_id
                        .is_some_and(|id| id.eq_ignore_ascii_case(&running_app.windows.app_id));
                if owned && !claimed[index] {
                    claimed[index] = true;
                    app.absorb(&running_app.windows);
                }
            }
            if let Some(pinned_id) = pinned_id {
                app.add_app_id(pinned_id);
            }
            app
        })
        .collect();
    row.extend(
        running
            .apps
            .iter()
            .zip(claimed)
            .filter(|(_, claimed)| !claimed)
            .map(|(app, _)| DockApp::running(app)),
    );
    for app in &mut row {
        app.carried = running.carries(&app.app_ids);
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(running: &mut Running, app_id: &str, count: u32, entry: Option<&str>) {
        let update = AppWindows {
            app_id: app_id.to_owned(),
            count,
            focused: false,
        };
        running.remember(&update, |_| entry.map(PathBuf::from));
    }

    fn keys(row: &[DockApp]) -> Vec<AppKey> {
        row.iter().map(|app| app.key.clone()).collect()
    }

    #[test]
    fn unpinned_running_apps_follow_the_pinned_ones() {
        let items = vec![PathBuf::from("/a.desktop")];
        let mut running = Running::default();
        report(&mut running, "z", 2, None);
        report(&mut running, "y", 1, Some("/y.desktop"));
        let row = rest(&items, &HashMap::new(), &running);
        assert_eq!(
            keys(&row),
            [
                AppKey::Pinned("/a.desktop".into()),
                AppKey::Running("z".into()),
                AppKey::Running("y".into()),
            ]
        );
        assert_eq!(row[1].icon, IconSource::Themed("z".into()));
        assert_eq!(row[2].icon, IconSource::Entry("/y.desktop".into()));
    }

    #[test]
    fn a_pinned_item_owns_the_windows_of_its_app_id_and_of_its_desktop_file() {
        let items = vec![PathBuf::from("/code.desktop")];
        let pinned_ids = HashMap::from([(items[0].clone(), "Code".to_owned())]);
        let mut running = Running::default();
        report(&mut running, "code-url-handler", 1, Some("/code.desktop"));
        report(&mut running, "code", 2, None);
        let row = rest(&items, &pinned_ids, &running);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].windows, 3);
        assert_eq!(row[0].app_ids, ["code-url-handler", "code"]);
    }

    #[test]
    fn an_idle_pinned_item_is_known_by_its_own_app_id() {
        let items = vec![PathBuf::from("/a.desktop")];
        let pinned_ids = HashMap::from([(items[0].clone(), "a".to_owned())]);
        let row = rest(&items, &pinned_ids, &Running::default());
        assert_eq!(row[0].app_ids, ["a"]);
        assert_eq!(row[0].windows, 0);
    }

    #[test]
    fn an_icon_is_carried_when_any_of_its_app_ids_is() {
        let mut running = Running::default();
        report(&mut running, "z", 1, None);
        running.carry("Z", true);
        let row = rest(&[], &HashMap::new(), &running);
        assert!(row[0].carried);
    }
}
