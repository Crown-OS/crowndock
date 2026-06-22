use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    output::OutputState,
    registry::RegistryState,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
        WaylandSurface,
    },
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_compositor::WlCompositor, wl_output::WlOutput},
    Connection, EventQueue, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use crate::{
    config::{WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH},
    ui::State,
    wayland::background_effect::BackgroundEffect,
};

pub struct Window {
    pub registry_state: RegistryState,
    pub output_state: OutputState,
    pub compositor_state: CompositorState,
    pub layer_shell: LayerShell,
    pub layer: LayerSurface,
    pub qh: QueueHandle<Window>,
    pub width: u32,
    pub height: u32,
    pub exclusive_zone: i32,
    pub exit: bool,
    pub first_configure: bool,
    pub state: State,
    pub background_effect: Option<BackgroundEffect>,
    pub bg_effect_surface: Option<ExtBackgroundEffectSurfaceV1>,
}

impl Window {
    pub fn new() -> anyhow::Result<(Connection, EventQueue<Window>, Window)> {
        let connection = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init(&connection)?;
        let qh: QueueHandle<Window> = event_queue.handle();

        let compositor_state = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)?;

        let layer = Self::build_layer(&compositor_state, &layer_shell, &qh, None);

        let background_effect = BackgroundEffect::bind(&globals, &qh);
        let bg_effect_surface = background_effect.as_ref().map(|bg| {
            bg.manager
                .get_background_effect(layer.wl_surface(), &qh, ())
        });

        let state = Self {
            state: State::default(),
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            compositor_state,
            layer_shell,
            layer,
            qh,
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
            exclusive_zone: WINDOW_HEIGHT as i32,
            exit: false,
            first_configure: true,
            background_effect,
            bg_effect_surface,
        };

        Ok((connection, event_queue, state))
    }

    fn build_layer(
        compositor_state: &CompositorState,
        layer_shell: &LayerShell,
        qh: &QueueHandle<Window>,
        output: Option<&WlOutput>,
    ) -> LayerSurface {
        let surface = compositor_state.create_surface(qh);
        let layer = layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some(WINDOW_TITLE),
            output,
        );

        layer.set_anchor(Anchor::BOTTOM);
        layer.set_size(WINDOW_WIDTH, WINDOW_HEIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(WINDOW_HEIGHT as i32);
        layer.commit();

        layer
    }

    /// Update the blur region attached to the layer surface so the
    /// compositor knows which part of the surface to apply the background
    /// effect to. Called after configure when the size is known.
    pub fn update_blur_region(&self, bounds: (i32, i32, i32, i32)) {
        let Some(effect_surface) = self.bg_effect_surface.as_ref() else {
            return;
        };
        let Some(bg) = self.background_effect.as_ref() else {
            return;
        };
        if !bg.supports_blur() {
            return;
        }
        let Ok(region) = Region::new(&self.compositor_state) else {
            return;
        };
        let (x, y, w, h) = bounds;
        if w > 0 && h > 0 {
            region.add(x, y, w, h);
        }
        effect_surface.set_blur_region(Some(region.wl_region()));
        self.layer.commit();
    }

    pub fn update_exclusive_zone(&mut self, zone: i32) {
        if self.exclusive_zone == zone {
            return;
        }
        self.exclusive_zone = zone;
        self.layer.set_exclusive_zone(zone);
        self.layer.commit();
    }

    pub fn update_monitor(&mut self, output: Option<&WlOutput>) {
        self.layer = Self::build_layer(&self.compositor_state, &self.layer_shell, &self.qh, output);
        self.layer.set_exclusive_zone(self.exclusive_zone);
        self.layer.commit();
        self.first_configure = true;
    }
}
