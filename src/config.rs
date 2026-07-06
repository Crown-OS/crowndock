use std::time::Duration;

pub static WINDOW_TITLE: &str = "Crowndock";
pub static WINDOW_WIDTH: u32 = 1044;
pub static WINDOW_HEIGHT: u32 = 100;

/// Horizontal inset of the dock pill from the layer surface edges.
pub const DOCK_INSET_X: f32 = 16.0;
/// Vertical inset of the dock pill from the layer surface edges.
pub const DOCK_INSET_Y: f32 = 8.0;
/// Time the pointer must dwell inside the trigger (or outside the dock)
/// before a show/hide transition kicks off.
pub const HOVER_DELAY: Duration = Duration::from_millis(300);

// Damped harmonic oscillator driving the slide. Slightly underdamped so the
// pill settles with a tiny, lively bounce instead of crawling to a stop —
// damping ratio ≈ 28 / (2·√240) ≈ 0.904.
pub const SPRING_STIFFNESS: f32 = 240.0;
pub const SPRING_DAMPING: f32 = 28.0;
// Fixed-size sub-step keeps the integrator stable independent of the frame
// rate the compositor wakes us at.
pub const SPRING_SUBSTEP: f32 = 1.0 / 240.0;
// Largest dt the integrator will accept in one tick. Anything longer (the
// surface was idle, a frame was missed) gets clamped so the spring can't
// explode.
pub const SPRING_MAX_DT: f32 = 1.0 / 30.0;
// Settle thresholds — both position and velocity must drop below these for
// the animation to be considered finished.
pub const SPRING_POSITION_EPSILON: f32 = 0.0005;
pub const SPRING_VELOCITY_EPSILON: f32 = 0.01;
