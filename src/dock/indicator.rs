//! The marks under a running app's icon, as on the Windows 11 taskbar: a muted dot per window,
//! up to a few, or one longer accent pill while the app is focused. Marks spring in and out
//! with the rest of the dock.

use crownui::prelude::{Cx, Signal, Styled, View, for_each, vstack};

use super::{glass::Tint, slots::Slot};
use crate::settings::Metrics;

/// The most window dots an icon shows, however many windows its app has.
pub const MAX_DOTS: u32 = 3;
pub const MARK_HEIGHT: f32 = 3.0;
pub const DOT_WIDTH: f32 = 4.0;
pub const PILL_WIDTH: f32 = 16.0;
const MARK_GAP: f32 = 3.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Indicator {
    /// The app has no windows.
    #[default]
    Closed,
    /// The app has windows, none focused: this many dots.
    Open(u32),
    Focused,
}

impl Indicator {
    pub fn new(windows: u32, focused: bool) -> Self {
        match (windows, focused) {
            (0, _) => Self::Closed,
            (_, true) => Self::Focused,
            (windows, false) => Self::Open(windows.min(MAX_DOTS)),
        }
    }

    fn marks(self) -> Vec<Mark> {
        match self {
            Self::Closed => Vec::new(),
            Self::Open(dots) => (0..dots).map(Mark::Window).collect(),
            Self::Focused => vec![Mark::Focus],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Mark {
    Window(u32),
    Focus,
}

impl Mark {
    fn key(mark: &Mark) -> Mark {
        *mark
    }
}

/// The marks of the app in `slot`, centred under its icon in the dock's padding.
pub fn indicator(cx: &mut Cx, slot: Signal<Slot>, metrics: Metrics, tint: Tint) -> impl View {
    let marks = cx.signal(Vec::new());
    cx.effect(move |runtime| {
        let wanted = slot.with(runtime, |slot| slot.indicator.marks());
        if marks.with_untracked(runtime, |current| *current != wanted) {
            marks.set(runtime, wanted);
        }
    });
    for_each(marks, Mark::key, move |cx, mark| {
        let mark = mark.with(cx, |mark| *mark);
        mark_view(mark, tint)
    })
    .flex_row()
    .gap(MARK_GAP)
    .justify_center()
    .items_center()
    .absolute()
    .left(0.0)
    .right(0.0)
    .top(metrics.mark_top(MARK_HEIGHT))
    .h(MARK_HEIGHT)
    .pointer_events_none()
}

fn mark_view(mark: Mark, tint: Tint) -> impl View {
    let (width, color) = match mark {
        Mark::Window(_) => (DOT_WIDTH, tint.mark()),
        Mark::Focus => (PILL_WIDTH, tint.accent()),
    };
    vstack(())
        .w(width)
        .h(MARK_HEIGHT)
        .rounded(MARK_HEIGHT / 2.0)
        .bg(color)
        .enter(|style| style.scale(0.0).opacity(0.0))
        .exit(|style| style.scale(0.0).opacity(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::testing::{painted, start, tint, windows};

    #[test]
    fn each_window_gets_a_dot_up_to_the_most_shown() {
        assert_eq!(Indicator::new(0, false).marks(), []);
        assert_eq!(
            Indicator::new(2, false).marks(),
            [Mark::Window(0), Mark::Window(1)]
        );
        assert_eq!(Indicator::new(7, false).marks().len(), MAX_DOTS as usize);
    }

    #[test]
    fn a_focused_app_shows_one_pill_however_many_windows_it_has() {
        assert_eq!(Indicator::new(1, true).marks(), [Mark::Focus]);
        assert_eq!(Indicator::new(5, true).marks(), [Mark::Focus]);
        assert_eq!(Indicator::new(0, true), Indicator::Closed);
    }

    #[test]
    fn a_running_app_shows_a_dot_per_window_until_it_is_focused() {
        let mut app = start(&["org.example.A"]);
        assert_eq!(painted(&mut app, tint().mark()), 0);
        app.app_windows(windows("org.example.A", 2, false)).settle();
        app.app_windows(windows("org.example.Other", 5, false))
            .settle();
        assert_eq!(painted(&mut app, tint().mark()), 2 + MAX_DOTS as usize);
        app.app_windows(windows("org.example.A", 2, true)).settle();
        assert_eq!(painted(&mut app, tint().mark()), MAX_DOTS as usize);
        assert_eq!(painted(&mut app, tint().accent()), 1);
    }

    #[test]
    fn the_marks_go_when_the_last_window_closes() {
        let mut app = start(&["org.example.A"]);
        app.app_windows(windows("org.example.A", 1, true)).settle();
        app.app_windows(windows("org.example.A", 0, false)).settle();
        assert_eq!(painted(&mut app, tint().mark()), 0);
        assert_eq!(painted(&mut app, tint().accent()), 0);
    }
}
