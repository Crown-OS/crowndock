use wayland_client::{globals::GlobalList, Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, Capability, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::{self, ExtBackgroundEffectSurfaceV1},
};

use crate::window::Window;

pub struct BackgroundEffect {
    pub manager: ExtBackgroundEffectManagerV1,
    pub capabilities: Capability,
}

impl BackgroundEffect {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Window>) -> Option<Self> {
        let manager = globals
            .bind::<ExtBackgroundEffectManagerV1, _, _>(qh, 1..=1, ())
            .ok()?;
        Some(Self {
            manager,
            capabilities: Capability::empty(),
        })
    }

    pub fn supports_blur(&self) -> bool {
        self.capabilities.contains(Capability::Blur)
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Window {
    fn event(
        state: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            if let Some(bg) = state.background_effect.as_mut() {
                bg.capabilities = flags.into_result().unwrap_or(Capability::empty());
            }
        }
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, ()> for Window {
    fn event(
        _: &mut Self,
        _: &ExtBackgroundEffectSurfaceV1,
        _: ext_background_effect_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
