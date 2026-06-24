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
    ui::{
        compute_layout,
        state::{DragRender, Vanishing},
        State, Ui,
    },
    wayland::background_effect::BackgroundEffect,
};

/// Time the pointer must dwell inside the trigger (or outside the dock)
/// before a show/hide transition kicks off.
const HOVER_DELAY: Duration = Duration::from_millis(300);

// Damped harmonic oscillator driving the slide. Slightly underdamped so the
// pill settles with a tiny, lively bounce instead of crawling to a stop —
// damping ratio ≈ 28 / (2·√240) ≈ 0.904.
const SPRING_STIFFNESS: f32 = 240.0;
const SPRING_DAMPING: f32 = 28.0;
// Fixed-size sub-step keeps the integrator stable independent of the frame
// rate the compositor wakes us at.
const SPRING_SUBSTEP: f32 = 1.0 / 240.0;
// Largest dt the integrator will accept in one tick. Anything longer (the
// surface was idle, a frame was missed) gets clamped so the spring can't
// explode.
const SPRING_MAX_DT: f32 = 1.0 / 30.0;
// Settle thresholds — both position and velocity must drop below these for
// the animation to be considered finished.
const SPRING_POSITION_EPSILON: f32 = 0.0005;
const SPRING_VELOCITY_EPSILON: f32 = 0.01;

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
    /// Spring position. 0.0 = fully off-screen (down), 1.0 = fully shown.
    /// May briefly exceed [0, 1] when the underdamped spring overshoots.
    pub position: f32,
    /// Spring velocity in position-units per second. Preserved across
    /// direction changes so reversing mid-animation feels continuous.
    pub velocity: f32,
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
    /// Last pointer position over the dock surface (surface-local px).
    pub pointer_pos: Option<(f64, f64)>,
    /// Active icon drag started from inside the dock.
    pub icon_drag: Option<IconDrag>,
    /// Live vanish animations (icon dragged off the dock).
    pub vanish_anims: Vec<VanishAnim>,
}

pub struct DragOfferRead {
    pub offer: DragOffer,
    pub data: Vec<u8>,
    pub token: Option<RegistrationToken>,
}

/// Distance the cursor must travel before a press-and-hold becomes a drag.
/// Below this threshold a release is treated as a click (no removal).
pub const ICON_DRAG_THRESHOLD: f32 = 6.0;
/// Length of the poof animation when an icon is removed.
pub const VANISH_DURATION: f32 = 0.30;

pub struct IconDrag {
    pub icon_idx: usize,
    pub origin_cx: f32,
    pub origin_cy: f32,
    pub start_px: (f64, f64),
    pub current_px: (f64, f64),
    pub armed: bool,
}

pub struct VanishAnim {
    pub path: std::path::PathBuf,
    pub image: Option<vello::peniko::ImageData>,
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub start: Instant,
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
            position: 0.0,
            velocity: 0.0,
            last_tick: None,
            hover_timer: None,
            frame_pending: false,
            background_effect,
            bg_effect_surface,
            data_device_manager,
            data_device: None,
            dnd_offers: Vec::new(),
            accept_counter: 0,
            pointer_pos: None,
            icon_drag: None,
            vanish_anims: Vec::new(),
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
        (1.0 - self.position) * self.height as f32
    }

    fn spring_target(&self) -> Option<f32> {
        match self.vis {
            VisState::Showing => Some(1.0),
            VisState::Hiding => Some(0.0),
            _ => None,
        }
    }

    fn step_spring(&mut self, target: f32, dt: f32) {
        let mut remaining = dt;
        while remaining > 0.0 {
            let h = remaining.min(SPRING_SUBSTEP);
            let acceleration = -SPRING_STIFFNESS * (self.position - target)
                - SPRING_DAMPING * self.velocity;
            self.velocity += acceleration * h;
            self.position += self.velocity * h;
            remaining -= h;
        }
    }

    fn spring_at_rest(&self, target: f32) -> bool {
        (self.position - target).abs() < SPRING_POSITION_EPSILON
            && self.velocity.abs() < SPRING_VELOCITY_EPSILON
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
                if self.position >= 1.0 && self.velocity.abs() < SPRING_VELOCITY_EPSILON {
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

    // ---- Icon drag (remove-by-drag-out) ----

    pub fn on_pointer_motion(&mut self, x: f64, y: f64) {
        self.pointer_pos = Some((x, y));
        if let Some(drag) = self.icon_drag.as_mut() {
            drag.current_px = (x, y);
            let dx = (drag.current_px.0 - drag.start_px.0) as f32;
            let dy = (drag.current_px.1 - drag.start_px.1) as f32;
            if !drag.armed && (dx * dx + dy * dy).sqrt() >= ICON_DRAG_THRESHOLD {
                drag.armed = true;
            }
            if drag.armed {
                self.request_frame();
            }
        }
    }

    pub fn on_pointer_press(&mut self, x: f64, y: f64) {
        self.pointer_pos = Some((x, y));
        let Some(layout) = compute_layout(self.state.icons.len(), self.width, self.height)
        else {
            return;
        };
        let y_offset = self.y_offset_px();
        if let Some(idx) = layout.hit_test(x as f32, y as f32, y_offset) {
            self.icon_drag = Some(IconDrag {
                icon_idx: idx,
                origin_cx: layout.slot_cx(idx),
                origin_cy: layout.y_center + y_offset,
                start_px: (x, y),
                current_px: (x, y),
                armed: false,
            });
        }
    }

    pub fn on_pointer_release(&mut self, x: f64, y: f64) {
        self.pointer_pos = Some((x, y));
        let Some(drag) = self.icon_drag.take() else {
            return;
        };
        if !drag.armed {
            // Treated as a click — no removal. (Future: hand off to launcher.)
            self.request_frame();
            return;
        }

        // Removal criterion: cursor pulled above the dock pill's interior
        // (visionOS-style "lift up to delete").
        let interior_top = DOCK_INSET_Y + self.y_offset_px();
        if (y as f32) < interior_top {
            self.start_vanish(&drag, x, y);
        }
        self.request_frame();
    }

    fn start_vanish(&mut self, drag: &IconDrag, x: f64, y: f64) {
        if drag.icon_idx >= self.state.icons.len() {
            return;
        }
        let icon = self.state.icons.remove(drag.icon_idx);
        let layout = compute_layout(
            self.state.icons.len().max(1),
            self.width,
            self.height,
        );
        let radius = layout.map(|l| l.radius).unwrap_or(drag.origin_cy.min(48.0));
        self.vanish_anims.push(VanishAnim {
            path: icon.path,
            image: icon.image,
            cx: x as f32,
            cy: y as f32,
            radius,
            start: Instant::now(),
        });
        self.last_tick = None;
    }

    /// Mirror live drag / vanish state into `self.state` so the renderer
    /// (which only sees `&State`) can paint it. Called every paint.
    fn sync_state_snapshots(&mut self) {
        self.state.drag_skip_idx = None;
        self.state.drag_render = None;

        if let Some(drag) = self.icon_drag.as_ref() {
            if drag.armed {
                if let Some(icon) = self.state.icons.get(drag.icon_idx) {
                    let dx = (drag.current_px.0 - drag.start_px.0) as f32;
                    let dy = (drag.current_px.1 - drag.start_px.1) as f32;
                    let cx = drag.origin_cx + dx;
                    let cy = drag.origin_cy + dy;
                    let lift = (dy.min(0.0).abs() / 80.0).clamp(0.0, 1.0);
                    let scale = 1.0 + 0.10 * lift;
                    let alpha = 1.0 - 0.15 * lift;
                    self.state.drag_skip_idx = Some(drag.icon_idx);
                    self.state.drag_render = Some(DragRender {
                        path: icon.path.clone(),
                        image: icon.image.clone(),
                        cx,
                        cy,
                        scale,
                        alpha,
                    });
                }
            }
        }

        self.state.vanishing = self
            .vanish_anims
            .iter()
            .map(|v| {
                let t = (v.start.elapsed().as_secs_f32() / VANISH_DURATION).clamp(0.0, 1.0);
                // Poof: scale 1.0 -> 1.5, alpha 1.0 -> 0.0.
                let scale = 1.0 + 0.5 * t;
                let alpha = 1.0 - t;
                Vanishing {
                    path: v.path.clone(),
                    image: v.image.clone(),
                    cx: v.cx,
                    cy: v.cy,
                    radius: v.radius,
                    scale,
                    alpha,
                }
            })
            .collect();
    }

    fn drop_finished_vanish(&mut self) {
        self.vanish_anims
            .retain(|v| v.start.elapsed().as_secs_f32() < VANISH_DURATION);
    }

    fn animation_in_flight(&self) -> bool {
        matches!(self.vis, VisState::Showing | VisState::Hiding)
            || !self.vanish_anims.is_empty()
            || self
                .icon_drag
                .as_ref()
                .map(|d| d.armed)
                .unwrap_or(false)
    }

    // ---- Animation driver ----

    /// Render the current state once and, if an animation is in flight,
    /// register a frame callback so we get woken on the next vsync.
    pub fn paint(&mut self) {
        self.sync_state_snapshots();
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
        if !self.animation_in_flight() {
            self.last_tick = None;
            self.paint();
            return;
        }

        let now = Instant::now();
        let dt = self
            .last_tick
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(1.0 / 60.0)
            .min(SPRING_MAX_DT);
        self.last_tick = Some(now);

        if let Some(target) = self.spring_target() {
            self.step_spring(target, dt);
            if self.spring_at_rest(target) {
                self.position = target;
                self.velocity = 0.0;
                match self.vis {
                    VisState::Showing => {
                        self.vis = VisState::Shown;
                        self.last_tick = None;
                    }
                    VisState::Hiding => {
                        self.vis = VisState::Hidden;
                        self.last_tick = None;
                        self.apply_input_region();
                    }
                    _ => {}
                }
            }
        }

        self.drop_finished_vanish();

        if self.animation_in_flight() {
            self.layer
                .wl_surface()
                .frame(&self.qh, self.layer.wl_surface().clone());
            self.frame_pending = true;
        }
        self.paint();
    }
}
