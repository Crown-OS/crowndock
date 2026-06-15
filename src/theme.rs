use serde::{Deserialize, Serialize};

/// Normalized linear RGBA color in `[0.0, 1.0]`. Kept as a plain struct so that
/// TOML stays human-friendly: `background = [r, g, b, a]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f32; 4]", into = "[f32; 4]")]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn with_alpha(mut self, a: f32) -> Self {
        self.a = a.clamp(0.0, 1.0);
        self
    }

    pub fn as_rgba8(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0) as u8,
            (self.g.clamp(0.0, 1.0) * 255.0) as u8,
            (self.b.clamp(0.0, 1.0) * 255.0) as u8,
            (self.a.clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }
}

impl From<[f32; 4]> for Color {
    fn from([r, g, b, a]: [f32; 4]) -> Self {
        Self::new(r, g, b, a)
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        [c.r, c.g, c.b, c.a]
    }
}

/// Visual + dimensional parameters. Held inside [`crate::config::Config`] so a
/// future "compact" or "translucent" theme can be swapped in without touching
/// behavioral fields like `pinned`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub height: u32,
    pub icon_size: u32,
    pub icon_gap: u32,
    pub padding: u32,
    pub corner_radius: f32,
    pub background: Color,
    pub hover: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            height: 72,
            icon_size: 48,
            icon_gap: 8,
            padding: 12,
            corner_radius: 18.0,
            background: Color::new(0.08, 0.08, 0.10, 0.85),
            hover: Color::new(1.0, 1.0, 1.0, 0.10),
        }
    }
}

impl Theme {
    /// Width and height of a single icon cell, including its inner padding.
    pub fn cell_size(&self) -> u32 {
        self.icon_size + 12
    }
}
