use std::path::PathBuf;

use crownui::prelude::{Layer, Rect, Size};
use serde::{Deserialize, Serialize};

/// The stacking band the dock lives in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockLayer {
    Bottom,
    #[default]
    Top,
    Overlay,
}

impl From<DockLayer> for Layer {
    fn from(layer: DockLayer) -> Self {
        match layer {
            DockLayer::Bottom => Layer::Bottom,
            DockLayer::Top => Layer::Top,
            DockLayer::Overlay => Layer::Overlay,
        }
    }
}

/// `~/.config/crownos/dock.ron`. The pinned items and the behaviour follow edits live; the
/// geometry and layer are told to the compositor once, so they apply on the next start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockSettings {
    /// Pinned `.desktop` files, in dock order.
    pub items: Vec<PathBuf>,
    /// Icon edge in logical pixels.
    pub icon_size: u16,
    /// Space between icons.
    pub spacing: u16,
    /// Space between the icons and the dock's edge.
    pub padding: u16,
    /// Space between the dock and the screen edge.
    pub margin: u16,
    /// Corner radius; `None` rounds the ends fully.
    pub radius: Option<u16>,
    pub layer: DockLayer,
    /// Slide away until the pointer touches the bottom edge.
    pub autohide: bool,
    pub reveal_delay_ms: u64,
    pub hide_delay_ms: u64,
    /// How much a hovered icon grows.
    pub hover_scale: f32,
}

impl Default for DockSettings {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            icon_size: 52,
            spacing: 10,
            padding: 10,
            margin: 8,
            radius: None,
            layer: DockLayer::Top,
            autohide: true,
            reveal_delay_ms: 120,
            hide_delay_ms: 500,
            hover_scale: 1.12,
        }
    }
}

impl DockSettings {
    pub const SECTION: &'static str = "dock";
}

/// The dock's fixed geometry in logical pixels, worked out once from the settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub icon: f32,
    pub spacing: f32,
    pub padding: f32,
    pub margin: f32,
    pub radius: f32,
    pub hover_scale: f32,
}

/// Strip along the screen edge that reveals a hidden dock.
pub const TRIGGER_HEIGHT: f32 = 2.0;

impl Metrics {
    pub fn new(settings: &DockSettings) -> Self {
        let icon = f32::from(settings.icon_size.max(16));
        let padding = f32::from(settings.padding);
        let height = icon + 2.0 * padding;
        Self {
            icon,
            spacing: f32::from(settings.spacing),
            padding,
            margin: f32::from(settings.margin),
            radius: settings
                .radius
                .map_or(height / 2.0, |radius| f32::from(radius).min(height / 2.0)),
            hover_scale: settings.hover_scale.clamp(1.0, 2.0),
        }
    }

    pub fn pill_height(&self) -> f32 {
        self.icon + 2.0 * self.padding
    }

    /// The glass around `tiles` icons; an empty dock keeps room for one, as a drop target.
    pub fn pill_width(&self, tiles: usize) -> f32 {
        self.row_width(tiles.max(1)) + 2.0 * self.padding
    }

    /// The row of `tiles` icons and the spaces between them.
    pub fn row_width(&self, tiles: usize) -> f32 {
        (tiles as f32 * self.step() - self.spacing).max(0.0)
    }

    /// How far above the screen edge the dock's foot is at `reveal`: its margin when up, its
    /// whole height below the edge when hidden.
    pub fn lift(&self, reveal: f32) -> f32 {
        self.margin - (1.0 - reveal) * (self.margin + self.pill_height())
    }

    /// Where icon `index` of a row of `tiles` rests on a `surface` with the dock fully up: the
    /// row is centred and its foot is the padding above the dock's.
    pub fn resting_icon(&self, index: usize, tiles: usize, surface: Size) -> Rect {
        let left = (surface.width - self.row_width(tiles)) / 2.0 + index as f32 * self.step();
        let top = surface.height - self.lift(1.0) - self.padding - self.icon;
        Rect::from_xywh(left, top, self.icon, self.icon)
    }

    /// Distance between neighbouring icon centres.
    pub fn step(&self) -> f32 {
        self.icon + self.spacing
    }

    /// The surface: the dock, its margin and room above for a hovered icon to grow into.
    pub fn surface_height(&self) -> u32 {
        let growth = self.icon * (self.hover_scale - 1.0);
        (self.margin + self.pill_height() + growth).ceil() as u32
    }

    /// Space kept free of windows when the dock stays up.
    pub fn reserved(&self) -> i32 {
        (self.margin + self.pill_height()).ceil() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_file_keeps_the_rest_of_its_defaults() {
        let settings: DockSettings = parse("(icon_size: 64, autohide: false)");
        assert_eq!(settings.icon_size, 64);
        assert!(!settings.autohide);
        assert_eq!(settings.spacing, DockSettings::default().spacing);
    }

    #[test]
    fn the_pill_fits_its_icons_and_keeps_room_for_one_when_empty() {
        let metrics = Metrics::new(&DockSettings::default());
        assert_eq!(metrics.pill_width(0), metrics.pill_width(1));
        assert_eq!(metrics.pill_width(3), 3.0 * 52.0 + 2.0 * 10.0 + 2.0 * 10.0);
        assert_eq!(metrics.radius, metrics.pill_height() / 2.0);
    }

    #[test]
    fn resting_icons_are_centred_with_their_foot_above_the_margin_and_padding() {
        let metrics = Metrics::new(&DockSettings::default());
        let surface = Size::new(1000.0, 90.0);
        let row = 3.0 * 52.0 + 2.0 * 10.0;
        let first = metrics.resting_icon(0, 3, surface);
        let last = metrics.resting_icon(2, 3, surface);
        assert_eq!(first.origin.x, (1000.0 - row) / 2.0);
        assert_eq!(last.max_x(), (1000.0 + row) / 2.0);
        assert_eq!(first.max_y(), 90.0 - 8.0 - 10.0);
        assert_eq!(first.size, Size::new(52.0, 52.0));
    }

    fn parse(text: &str) -> DockSettings {
        match ron::from_str(text) {
            Ok(settings) => settings,
            Err(error) => panic!("{error}"),
        }
    }
}
