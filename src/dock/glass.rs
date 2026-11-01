//! The dock's body is the compositor's frosted glass: blur, tint, rim and shadow are all drawn
//! by the compositor behind the surface, so the dock paints nothing but its icons and their
//! window marks.

use crownconfig::Appearance;
use crownui::{
    kit::accent_stops,
    prelude::{Color, Material, MaterialBlur, MaterialShadow, Vector},
};

const BLUR_RADIUS: f32 = 40.0;
const VIBRANCY: f32 = 1.6;
const RIM: f32 = 1.0;
const SHADOW_RADIUS: f32 = 24.0;
const SHADOW_OFFSET: f32 = 8.0;
const SHADOW_ALPHA: f32 = 0.30;
/// Opacity of the tint when the compositor cannot blur: glass over an unblurred desktop is
/// unreadable, so it is nearly solid instead.
const SOLID_ALPHA: f32 = 0.88;

/// The colours the dock paints with, for the desktop's light or dark appearance and accent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    /// Laid over the blur.
    glass: Color,
    /// The window marks of an app that is not focused.
    mark: Color,
    /// The mark of the focused app.
    accent: Color,
}

impl Tint {
    pub fn new(appearance: &Appearance) -> Self {
        let accent = accent_stops(appearance.accent);
        if appearance.dark_mode {
            Self {
                glass: Color::new(0.07, 0.08, 0.10, 0.32),
                mark: Color::new(1.0, 1.0, 1.0, 0.55),
                accent: accent.start,
            }
        } else {
            Self {
                glass: Color::new(1.0, 1.0, 1.0, 0.38),
                mark: Color::new(0.0, 0.0, 0.0, 0.42),
                accent: accent.end,
            }
        }
    }

    pub fn solid(self) -> Color {
        Color {
            a: SOLID_ALPHA,
            ..self.glass
        }
    }

    pub fn mark(self) -> Color {
        self.mark
    }

    pub fn accent(self) -> Color {
        self.accent
    }
}

/// The glass at `reveal`: the blur and shadow come up with the dock, squared so they are gone
/// before it is, and never frost an empty patch of desktop.
pub fn material(reveal: f32, radius: f32, tint: Tint) -> Material {
    let strength = reveal.clamp(0.0, 1.0).powi(2);
    Material {
        corner_radius: radius,
        border_width: RIM,
        blur: Some(MaterialBlur {
            radius: BLUR_RADIUS * strength,
            tint: Color {
                a: tint.glass.a * strength,
                ..tint.glass
            },
            saturation: 1.0 + (VIBRANCY - 1.0) * strength,
        }),
        shadow: Some(MaterialShadow {
            radius: SHADOW_RADIUS,
            offset: Vector::new(0.0, SHADOW_OFFSET),
            color: Color::new(0.0, 0.0, 0.0, SHADOW_ALPHA * strength),
        }),
    }
}
