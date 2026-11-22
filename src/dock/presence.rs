//! Whether the dock should be up: something is over it, one of its icons is being dragged, or a
//! window is on its way into or out of one. Each change starts a short timer, so brushing the
//! screen edge or passing over the dock on the way elsewhere does not flick it up and down. A
//! window on its way comes up at once, so the dock is there before it lands.

use std::time::Duration;

use crownui::prelude::Cx;

use super::{motion, state::Dock};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Presence {
    pub pointer: bool,
    pub drag: bool,
    pub lifting: bool,
    pub carrying: bool,
}

impl Presence {
    fn wants_dock(self) -> bool {
        self.pointer || self.drag || self.lifting || self.carrying
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
    let presence = dock.presence.with(cx, |presence| *presence);
    let wanted = !autohide || presence.wants_dock();
    let target = if wanted { 1.0 } else { 0.0 };
    let generation = dock.timer.update(cx, |generation| {
        *generation += 1;
        *generation
    });
    if dock.motion.with(cx, |motion| motion.reveal.target()) == target {
        return;
    }
    let delay = match (wanted, presence.carrying) {
        (true, true) => Duration::ZERO,
        (true, false) => Duration::from_millis(reveal_delay),
        (false, _) => Duration::from_millis(hide_delay),
    };
    cx.spawn(
        async move { tokio::time::sleep(delay).await },
        move |cx, ()| {
            let current = dock.timer.with(cx, |latest| *latest == generation);
            if current && dock.motion.update(cx, |motion| motion.reveal.aim(target)) {
                motion::animate(cx, dock);
            }
        },
    );
}
