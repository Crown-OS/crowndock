//! The dock's body is the compositor's frosted glass: blur, tint, rim and shadow are all drawn
//! by the compositor behind the surface, so the dock paints nothing but its icons.

use crownui::prelude::{Color, Material, MaterialBlur, MaterialShadow, Vector};

const BLUR_RADIUS: f32 = 40.0;
const VIBRANCY: f32 = 1.6;
const RIM: f32 = 1.0;
const SHADOW_RADIUS: f32 = 24.0;
const SHADOW_OFFSET: f32 = 8.0;
const SHADOW_ALPHA: f32 = 0.30;
/// Opacity of the tint when the compositor cannot blur: glass over an unblurred desktop is
/// unreadable, so it is nearly solid instead.
const SOLID_ALPHA: f32 = 0.88;

/// The colour laid over the blur, for the desktop's light or dark appearance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint(Color);

impl Tint {
    pub fn new(dark: bool) -> Self {
        Self(if dark {
            Color::new(0.07, 0.08, 0.10, 0.32)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.38)
        })
    }

    pub fn solid(self) -> Color {
        Color {
            a: SOLID_ALPHA,
            ..self.0
        }
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
                a: tint.0.a * strength,
                ..tint.0
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
