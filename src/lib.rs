mod desktop_entry;
mod dock;
mod icon;
mod launcher;
pub mod settings;
mod util;

use anyhow::{Context, Result};
use crownui::{
    AppConfig,
    prelude::{Anchor, KeyboardInteractivity, LayerExtent, LayerOptions, TRANSPARENT},
};

use dock::Tint;
use settings::{DockSettings, Metrics};

const NAMESPACE: &str = "crowndock";
/// Width the surface starts at until the compositor stretches it across the output.
const FALLBACK_WIDTH: u32 = 1280;

pub fn app() -> Result<()> {
    let settings: DockSettings = crownconfig::load_section(DockSettings::SECTION);
    let metrics = Metrics::new(&settings);
    let tint = Tint::new(crownconfig::load_appearance().dark_mode);
    let height = metrics.surface_height();
    let reserved = if settings.autohide {
        0
    } else {
        metrics.reserved()
    };
    let layer = LayerOptions::new(NAMESPACE, Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
        .layer(settings.layer.into())
        .height(LayerExtent::Fixed(height))
        .exclusive_zone(reserved)
        .keyboard(KeyboardInteractivity::None);
    let config = AppConfig::new("CrownDock")
        .app_id(NAMESPACE)
        .size(FALLBACK_WIDTH, height)
        .clear_color(TRANSPARENT)
        .accessibility(false)
        .layer(layer);
    crownui::run(dock::root(settings, metrics, tint), config).context("the dock stopped")
}
