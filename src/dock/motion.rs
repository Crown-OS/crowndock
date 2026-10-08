//! The dock's own springs: how far it is revealed and how wide its glass is. They are sampled
//! from crownui's closed-form springs on the same clock as the render thread, so the glass
//! keeps pace with the icons the render thread moves, and frames are asked for only while one
//! of them is moving.

use std::time::Duration;

use crownui::prelude::{Cx, Spring};
use motion::{Animation, Curve};

use super::state::Dock;

/// One spring for the glass, the icons and the slide, so they move as one body.
pub fn spring() -> Spring {
    Spring::new(0.34, 0.82)
}

#[derive(Debug, Clone, Copy)]
pub struct SpringValue {
    animation: Animation<f32>,
    aim: Option<f32>,
    pub value: f32,
}

impl SpringValue {
    pub fn new(value: f32) -> Self {
        Self {
            animation: Animation::new(value, value, 0.0, Duration::ZERO, Curve::Spring(spring())),
            aim: None,
            value,
        }
    }

    pub fn target(&self) -> f32 {
        self.aim.unwrap_or_else(|| self.animation.target())
    }

    /// Sends the value toward `target` from the next frame, keeping its velocity. Returns
    /// whether the target moved.
    pub fn aim(&mut self, target: f32) -> bool {
        let moved = self.target() != target;
        if moved {
            self.aim = Some(target);
        }
        moved
    }

    /// Samples the spring at `now`; returns whether it is still moving.
    fn step(&mut self, now: Duration) -> bool {
        if let Some(target) = self.aim.take() {
            self.animation = self
                .animation
                .retarget(now, target, Curve::Spring(spring()));
        }
        let sample = self.animation.sample(now);
        self.value = sample.value;
        !sample.finished
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Motion {
    pub reveal: SpringValue,
    pub width: SpringValue,
    scheduled: bool,
}

impl Motion {
    pub fn new(reveal: f32, width: f32) -> Self {
        Self {
            reveal: SpringValue::new(reveal),
            width: SpringValue::new(width),
            scheduled: false,
        }
    }
}

/// Asks for a frame unless one is already coming.
pub fn animate(cx: &mut Cx, dock: Dock) {
    let schedule = dock
        .motion
        .update(cx, |motion| !std::mem::replace(&mut motion.scheduled, true));
    if schedule {
        cx.request_animation_frame(move |cx, now| frame(cx, dock, now));
    }
}

fn frame(cx: &mut Cx, dock: Dock, now: Duration) {
    let (moving, reveal, width) = dock.motion.update(cx, |motion| {
        motion.scheduled = false;
        let revealing = motion.reveal.step(now);
        let resizing = motion.width.step(now);
        (
            revealing || resizing,
            motion.reveal.value,
            motion.width.value,
        )
    });
    dock.reveal.set(cx, reveal);
    dock.width.set(cx, width);
    if moving {
        animate(cx, dock);
    }
}
