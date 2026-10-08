//! Whether the dock should be up: something is over it, or one of its icons is being dragged.
//! Each change starts a short timer, so brushing the screen edge or passing over the dock on
//! the way elsewhere does not flick it up and down.

use std::time::Duration;

use crownui::prelude::Cx;

use super::{motion, state::Dock};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Presence {
    pub pointer: bool,
    pub drag: bool,
    pub lifting: bool,
}

impl Presence {
    fn wants_dock(self) -> bool {
        self.pointer || self.drag || self.lifting
    }
}

pub fn changed(cx: &mut Cx, dock: Dock) {
    let (autohide, reveal_delay, hide_delay) = dock.settings.with(cx, |settings| {
        (
            settings.autohide,
            settings.reveal_delay_ms,
            settings.hide_delay_ms,
        )
    });
    let wanted = !autohide || dock.presence.with(cx, |presence| presence.wants_dock());
    let target = if wanted { 1.0 } else { 0.0 };
    let generation = dock.timer.update(cx, |generation| {
        *generation += 1;
        *generation
    });
    if dock.motion.with(cx, |motion| motion.reveal.target()) == target {
        return;
    }
    let delay = Duration::from_millis(if wanted { reveal_delay } else { hide_delay });
    cx.spawn(tokio::time::sleep(delay), move |cx, ()| {
        let current = dock.timer.with(cx, |latest| *latest == generation);
        if current && dock.motion.update(cx, |motion| motion.reveal.aim(target)) {
            motion::animate(cx, dock);
        }
    });
}
