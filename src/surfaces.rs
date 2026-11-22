//! One dock on each output the compositor gives a dock, all in one process. The docks share
//! their pinned items through `dock.ron`, which each one follows, and the compositor reports the
//! running apps to every dock surface.

use crownui::{
    AppConfig, Surfaces,
    prelude::{
        Anchor, AnyView, CornerShape, KeyboardInteractivity, LayerExtent, LayerOptions,
        TRANSPARENT, View,
    },
    surfaces::Output,
};

use crate::{
    dock::{self, Tint},
    settings::{DockSettings, Metrics},
};

const NAMESPACE: &str = "crowndock";
/// Width the surface starts at until the compositor stretches it across the output.
const FALLBACK_WIDTH: u32 = 1280;

pub struct DockSurfaces;

impl Surfaces for DockSurfaces {
    fn placement(&mut self, output: &Output) -> Option<u64> {
        let wanted = output.crown.as_ref().is_none_or(|crown| crown.shell.dock());
        wanted.then_some(0)
    }

    fn build(&mut self, _output: &Output) -> Vec<(AnyView, AppConfig)> {
        let settings: DockSettings = crownconfig::load_section(DockSettings::SECTION);
        let config = app_config(&settings);
        vec![(root(settings).boxed(), config)]
    }
}

/// The dock for `settings`, tinted for the configured appearance.
pub fn root(settings: DockSettings) -> impl View {
    let metrics = Metrics::new(&settings);
    let tint = Tint::new(&crownconfig::load_appearance());
    dock::root(settings, metrics, tint)
}

/// The layer surface along the bottom edge that holds the dock for `settings`.
pub fn app_config(settings: &DockSettings) -> AppConfig {
    let metrics = Metrics::new(settings);
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
    AppConfig::new("CrownDock")
        .app_id(NAMESPACE)
        .size(FALLBACK_WIDTH, height)
        .clear_color(TRANSPARENT)
        .accessibility(false)
        .corner_shape(CornerShape::Squircle)
        .layer(layer)
}
