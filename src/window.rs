use std::time::{Duration, Instant};

use calloop::{LoopHandle, RegistrationToken, timer::Timer};
use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    data_device_manager::{
        data_device::DataDevice, data_offer::DragOffer, DataDeviceManagerState,
    },
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
    },
};
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output::WlOutput, wl_pointer::WlPointer},
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use crate::{
    config::{DOCK_INSET_X, DOCK_INSET_Y, WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH},
    ui::{State, Ui},
    wayland::background_effect::BackgroundEffect,
};

/// Time the pointer must dwell inside the trigger (or outside the dock)
/// before a show/hide transition kicks off.
const HOVER_DELAY: Duration = Duration::from_millis(300);
/// Length of the slide animation.
const ANIM_DURATION: f32 = 0.32;

/// Cubic ease-in-out — slow start, fast middle, slow finish.
/// Matches the feel of macOS dock auto-hide.
fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let f = -2.0 * t + 2.0;
        1.0 - f * f * f * 0.5
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisState {
    /// Off-screen, only the bottom 1px trigger strip catches the pointer.
    Hidden,
    /// Pointer is sitting on the trigger, waiting out the dwell timer.
    PendingShow,
    /// Sliding up.
    Showing,
    /// Fully visible.
    Shown,
    /// Pointer has left the dock, waiting out the dwell timer.
    PendingHide,
    /// Sliding down.
    Hiding,
}

pub struct Window {
    // Renderer first so wgpu's surface (which holds a raw pointer to wl_surface)
    // is dropped before the LayerSurface that owns the wl_surface.
    pub ui: Option<Ui>,
    pub registry_state: RegistryState,
    pub output_state: OutputState,
    pub seat_state: SeatState,
    pub compositor_state: CompositorState,
    pub layer_shell: LayerShell,
    pub layer: LayerSurface,
    pub pointer: Option<WlPointer>,
    pub qh: QueueHandle<Window>,
    pub loop_handle: LoopHandle<'static, Window>,
    pub width: u32,
    pub height: u32,
    pub exit: bool,
    pub first_configure: bool,
    pub vis: VisState,
    /// 0.0 = fully off-screen (down), 1.0 = fully shown.
    pub progress: f32,
    pub last_tick: Option<Instant>,
    pub hover_timer: Option<RegistrationToken>,
    pub frame_pending: bool,
    pub state: State,
    pub background_effect: Option<BackgroundEffect>,
    pub bg_effect_surface: Option<ExtBackgroundEffectSurfaceV1>,
    pub data_device_manager: DataDeviceManagerState,
    pub data_device: Option<DataDevice>,
    /// In-flight DnD offers being read on the pipe source.
    pub dnd_offers: Vec<DragOfferRead>,
    pub accept_counter: u32,
}

pub struct DragOfferRead {
    pub offer: DragOffer,
    pub data: Vec<u8>,
    pub token: Option<RegistrationToken>,
}

impl Window {
    pub fn new(
        loop_handle: LoopHandle<'static, Window>,
    ) -> anyhow::Result<(Connection, EventQueue<Window>, Window)> {
        let connection = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init(&connection)?;
        let qh: QueueHandle<Window> = event_queue.handle();

        let compositor_state = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)?;
        let seat_state = SeatState::new(&globals, &qh);
        let data_device_manager = DataDeviceManagerState::bind(&globals, &qh)?;

        let layer = Self::build_layer(&compositor_state, &layer_shell, &qh, None);

        let background_effect = BackgroundEffect::bind(&globals, &qh);
        let bg_effect_surface = background_effect.as_ref().map(|bg| {
            bg.manager
                .get_background_effect(layer.wl_surface(), &qh, ())
        });

        let state = Self {
            ui: None,
            state: State::default(),
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            seat_state,
            compositor_state,
            layer_shell,
            layer,
            pointer: None,
            qh,
            loop_handle,
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
            exit: false,
            first_configure: true,
            vis: VisState::Hidden,
            progress: 0.0,
            last_tick: None,
            hover_timer: None,
            frame_pending: false,
            background_effect,
            bg_effect_surface,
            data_device_manager,
            data_device: None,
            dnd_offers: Vec::new(),
            accept_counter: 0,
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
        layer.commit();

        layer
    }

    /// Apply the input region matching the current state — a 1px strip at the
    /// bottom when hidden, the full surface otherwise.
    pub fn apply_input_region(&self) {
        let Ok(region) = Region::new(&self.compositor_state) else {
            return;
        };
        match self.vis {
            VisState::Hidden | VisState::PendingShow => {
                let y = (self.height as i32 - 1).max(0);
                region.add(0, y, self.width as i32, 1);
            }
            _ => {
                region.add(0, 0, self.width as i32, self.height as i32);
            }
        }
        self.layer
            .wl_surface()
            .set_input_region(Some(region.wl_region()));
        self.layer.commit();
    }

    /// Update the blur region in lockstep with the slide animation so the
    /// frosted backdrop tracks the pill rather than staying at rest.
    pub fn apply_blur_region(&self) {
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
        let inset_x = DOCK_INSET_X.round() as i32;
        let inset_y = DOCK_INSET_Y.round() as i32;
        let w = (self.width as i32 - 2 * inset_x).max(0);
        let h = (self.height as i32 - 2 * inset_y).max(0);
        let y_offset = self.y_offset_px().round() as i32;
        if w > 0 && h > 0 {
            region.add(inset_x, inset_y + y_offset, w, h);
        }
        effect_surface.set_blur_region(Some(region.wl_region()));
        self.layer.commit();
    }

    fn y_offset_px(&self) -> f32 {
        (1.0 - ease_in_out(self.progress)) * self.height as f32
    }

    // ---- Pointer hover ----

    pub fn on_pointer_enter(&mut self) {
        match self.vis {
            VisState::Hidden => {
                self.vis = VisState::PendingShow;
                self.arm_timer();
            }
            VisState::PendingHide => {
                self.cancel_timer();
                if self.progress >= 1.0 {
                    self.vis = VisState::Shown;
                } else {
                    self.vis = VisState::Showing;
                    self.last_tick = None;
                    self.request_frame();
                }
            }
            VisState::Hiding => {
                self.vis = VisState::Showing;
                self.last_tick = None;
                self.request_frame();
            }
            _ => {}
        }
    }

    pub fn on_pointer_leave(&mut self) {
        match self.vis {
            VisState::PendingShow => {
                self.cancel_timer();
                self.vis = VisState::Hidden;
            }
            VisState::Shown | VisState::Showing => {
                self.vis = VisState::PendingHide;
                self.arm_timer();
            }
            _ => {}
        }
    }

    fn arm_timer(&mut self) {
        self.cancel_timer();
        let timer = Timer::from_duration(HOVER_DELAY);
        let token = self
            .loop_handle
            .insert_source(timer, |_, _, window: &mut Window| {
                window.on_hover_timeout();
                calloop::timer::TimeoutAction::Drop
            });
        match token {
            Ok(t) => self.hover_timer = Some(t),
            Err(e) => log::warn!("failed to schedule hover timer: {e}"),
        }
    }

    fn cancel_timer(&mut self) {
        if let Some(token) = self.hover_timer.take() {
            self.loop_handle.remove(token);
        }
    }

    fn on_hover_timeout(&mut self) {
        self.hover_timer = None;
        match self.vis {
            VisState::PendingShow => {
                self.vis = VisState::Showing;
                self.apply_input_region();
                self.last_tick = None;
                self.request_frame();
            }
            VisState::PendingHide => {
                self.vis = VisState::Hiding;
                self.last_tick = None;
                self.request_frame();
            }
            _ => {}
        }
    }

    // ---- Animation driver ----

    /// Render the current state once and, if an animation is in flight,
    /// register a frame callback so we get woken on the next vsync.
    pub fn paint(&mut self) {
        let y_offset = self.y_offset_px();
        if let Some(ui) = self.ui.as_mut() {
            ui.set_y_offset(y_offset);
            if let Err(e) = ui.render(&self.state) {
                log::error!("render failed: {e}");
            }
        }
        self.apply_blur_region();
    }

    pub fn request_frame(&mut self) {
        if self.frame_pending {
            return;
        }
        self.frame_pending = true;
        self.layer.wl_surface().frame(&self.qh, self.layer.wl_surface().clone());
        self.paint();
    }

    pub fn on_frame(&mut self) {
        self.frame_pending = false;
        let animating = matches!(self.vis, VisState::Showing | VisState::Hiding);
        if !animating {
            self.last_tick = None;
            return;
        }

        let now = Instant::now();
        let dt = self
            .last_tick
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        self.last_tick = Some(now);

        let step = dt / ANIM_DURATION;
        match self.vis {
            VisState::Showing => {
                self.progress = (self.progress + step).min(1.0);
                if self.progress >= 1.0 {
                    self.vis = VisState::Shown;
                    self.last_tick = None;
                }
            }
            VisState::Hiding => {
                self.progress = (self.progress - step).max(0.0);
                if self.progress <= 0.0 {
                    self.vis = VisState::Hidden;
                    self.last_tick = None;
                    self.apply_input_region();
                }
            }
            _ => {}
        }

        let still_animating = matches!(self.vis, VisState::Showing | VisState::Hiding);
        if still_animating {
            self.layer
                .wl_surface()
                .frame(&self.qh, self.layer.wl_surface().clone());
            self.frame_pending = true;
        }
        self.paint();
    }
}
