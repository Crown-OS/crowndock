use std::path::PathBuf;
use std::time::Instant;

use crownshell::{DragOffer, DropPayload, Scene, SurfaceCtx, SurfaceHandler};
use smithay_client_toolkit::{compositor::Region, shell::WaylandSurface};
use vello::peniko::ImageData;

use crate::{
    config::{
        DOCK_INSET_X, DOCK_INSET_Y, HOVER_DELAY, SPRING_DAMPING, SPRING_MAX_DT,
        SPRING_POSITION_EPSILON, SPRING_STIFFNESS, SPRING_SUBSTEP, SPRING_VELOCITY_EPSILON,
    },
    ui::{
        build_scene, compute_layout,
        state::{DragRender, Vanishing},
        State,
    },
};

pub const ICON_DRAG_THRESHOLD: f32 = 6.0;
pub const VANISH_DURATION: f32 = 0.30;
const BLUR_STRIPS: i32 = 32;
const URI_LIST_MIME: &str = "text/uri-list";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisState {
    Hidden,
    PendingShow,
    Showing,
    Shown,
    PendingHide,
    Hiding,
}

pub struct IconDrag {
    pub icon_idx: usize,
    pub origin_cx: f32,
    pub origin_cy: f32,
    pub start_px: (f64, f64),
    pub current_px: (f64, f64),
    pub armed: bool,
}

pub struct VanishAnim {
    pub path: PathBuf,
    pub image: Option<ImageData>,
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub start: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputKind {
    Trigger,
    Full,
}

pub struct DockHandler {
    pub state: State,
    vis: VisState,
    position: f32,
    velocity: f32,
    last_tick: Option<Instant>,
    pending_start: Option<Instant>,
    width: u32,
    height: u32,
    icon_drag: Option<IconDrag>,
    vanish_anims: Vec<VanishAnim>,
    last_input_kind: Option<InputKind>,
}

impl DockHandler {
    pub fn new() -> Self {
        Self {
            state: State::load(),
            vis: VisState::Hidden,
            position: 0.0,
            velocity: 0.0,
            last_tick: None,
            pending_start: None,
            width: 0,
            height: 0,
            icon_drag: None,
            vanish_anims: Vec::new(),
            last_input_kind: None,
        }
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
            let accel = -SPRING_STIFFNESS * (self.position - target) - SPRING_DAMPING * self.velocity;
            self.velocity += accel * h;
            self.position += self.velocity * h;
            remaining -= h;
        }
    }

    fn spring_at_rest(&self, target: f32) -> bool {
        (self.position - target).abs() < SPRING_POSITION_EPSILON
            && self.velocity.abs() < SPRING_VELOCITY_EPSILON
    }

    fn animation_in_flight(&self) -> bool {
        matches!(
            self.vis,
            VisState::Showing | VisState::Hiding | VisState::PendingShow | VisState::PendingHide
        ) || !self.vanish_anims.is_empty()
            || self.icon_drag.as_ref().map(|d| d.armed).unwrap_or(false)
    }

    fn desired_input_kind(&self) -> InputKind {
        match self.vis {
            VisState::Hidden | VisState::PendingShow => InputKind::Trigger,
            _ => InputKind::Full,
        }
    }

    fn apply_input_region(&mut self, ctx: &SurfaceCtx<'_>) {
        let desired = self.desired_input_kind();
        if self.last_input_kind == Some(desired) {
            return;
        }
        let Ok(region) = Region::new(ctx.compositor_state) else {
            return;
        };
        match desired {
            InputKind::Trigger => {
                let y = (ctx.size.1 as i32 - 1).max(0);
                region.add(0, y, ctx.size.0 as i32, 1);
            }
            InputKind::Full => {
                region.add(0, 0, ctx.size.0 as i32, ctx.size.1 as i32);
            }
        }
        ctx.layer
            .wl_surface()
            .set_input_region(Some(region.wl_region()));
        ctx.layer.commit();
        self.last_input_kind = Some(desired);
    }

    fn apply_blur_region(&self, ctx: &SurfaceCtx<'_>) {
        let Some(effect) = ctx.bg_effect_surface else {
            return;
        };
        let Ok(region) = Region::new(ctx.compositor_state) else {
            return;
        };
        let inset_x = DOCK_INSET_X.round() as i32;
        let inset_y = DOCK_INSET_Y.round() as i32;
        let w = (ctx.size.0 as i32 - 2 * inset_x).max(0);
        let h = (ctx.size.1 as i32 - 2 * inset_y).max(0);
        let y_offset = self.y_offset_px().round() as i32;
        if w > 0 && h > 0 {
            let r = (w.min(h) as f32) * 0.5;
            let cy = h as f32 * 0.5;
            let flat_half_h = (h as f32 * 0.5 - r).max(0.0);
            for i in 0..BLUR_STRIPS {
                let y_top = i * h / BLUR_STRIPS;
                let y_bot = (i + 1) * h / BLUR_STRIPS;
                if y_bot <= y_top {
                    continue;
                }
                let y_mid = (y_top + y_bot) as f32 * 0.5;
                let cap_excess = ((y_mid - cy).abs() - flat_half_h).clamp(0.0, r);
                let half_chord = (r * r - cap_excess * cap_excess).max(0.0).sqrt();
                let inset = (r - half_chord).round() as i32;
                let strip_w = (w - 2 * inset).max(0);
                if strip_w == 0 {
                    continue;
                }
                region.add(
                    inset_x + inset,
                    inset_y + y_offset + y_top,
                    strip_w,
                    y_bot - y_top,
                );
            }
        }
        effect.set_blur_region(Some(region.wl_region()));
        ctx.layer.commit();
    }

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

    fn start_vanish(&mut self, drag: &IconDrag, x: f64, y: f64) {
        let Some(icon) = self.state.remove_icon(drag.icon_idx) else {
            return;
        };
        let layout = compute_layout(self.state.icons.len().max(1), self.width, self.height);
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
}

impl Default for DockHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceHandler for DockHandler {
    fn paint(&mut self, scene: &mut Scene, ctx: SurfaceCtx<'_>) {
        self.width = ctx.size.0;
        self.height = ctx.size.1;
        self.sync_state_snapshots();
        self.apply_input_region(&ctx);
        self.apply_blur_region(&ctx);
        let y_offset = self.y_offset_px();
        build_scene(scene, &self.state, ctx.size, y_offset);
    }

    fn on_pointer_enter(&mut self, _x: f64, _y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        match self.vis {
            VisState::Hidden => {
                self.vis = VisState::PendingShow;
                self.pending_start = Some(Instant::now());
                true
            }
            VisState::PendingHide => {
                self.pending_start = None;
                if self.position >= 1.0 && self.velocity.abs() < SPRING_VELOCITY_EPSILON {
                    self.vis = VisState::Shown;
                    false
                } else {
                    self.vis = VisState::Showing;
                    self.last_tick = None;
                    true
                }
            }
            VisState::Hiding => {
                self.vis = VisState::Showing;
                self.last_tick = None;
                true
            }
            _ => false,
        }
    }

    fn on_pointer_leave(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        match self.vis {
            VisState::PendingShow => {
                self.pending_start = None;
                self.vis = VisState::Hidden;
                false
            }
            VisState::Shown | VisState::Showing => {
                self.vis = VisState::PendingHide;
                self.pending_start = Some(Instant::now());
                true
            }
            _ => false,
        }
    }

    fn on_pointer_motion(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        if let Some(drag) = self.icon_drag.as_mut() {
            drag.current_px = (x, y);
            let dx = (drag.current_px.0 - drag.start_px.0) as f32;
            let dy = (drag.current_px.1 - drag.start_px.1) as f32;
            if !drag.armed && (dx * dx + dy * dy).sqrt() >= ICON_DRAG_THRESHOLD {
                drag.armed = true;
            }
            drag.armed
        } else {
            false
        }
    }

    fn on_pointer_press(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let Some(layout) = compute_layout(self.state.icons.len(), self.width, self.height) else {
            return false;
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
        false
    }

    fn on_pointer_release(&mut self, x: f64, y: f64, _ctx: SurfaceCtx<'_>) -> bool {
        let Some(drag) = self.icon_drag.take() else {
            return false;
        };
        if !drag.armed {
            return true;
        }
        let interior_top = DOCK_INSET_Y + self.y_offset_px();
        if (y as f32) < interior_top {
            self.start_vanish(&drag, x, y);
        }
        true
    }

    fn on_drag_enter(&mut self, offer: DragOffer<'_>, ctx: SurfaceCtx<'_>) -> Option<String> {
        let accepts = offer.mime_types.iter().any(|m| m == URI_LIST_MIME);
        // Drive the show state machine off the drag focus as well — pointer
        // events are suppressed during DnD, so this is what lets the dock
        // reveal when the user drags an icon toward the bottom of the screen.
        let _ = self.on_pointer_enter(offer.x, offer.y, ctx);
        if accepts {
            Some(URI_LIST_MIME.to_string())
        } else {
            None
        }
    }

    fn on_drag_motion(&mut self, x: f64, y: f64, ctx: SurfaceCtx<'_>) -> bool {
        self.on_pointer_motion(x, y, ctx)
    }

    fn on_drag_leave(&mut self, ctx: SurfaceCtx<'_>) -> bool {
        self.on_pointer_leave(ctx)
    }

    fn on_drop(&mut self, drop: DropPayload, _ctx: SurfaceCtx<'_>) -> bool {
        if drop.mime_type != URI_LIST_MIME {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&drop.data) else {
            log::warn!("dropped payload is not valid utf-8");
            return false;
        };
        let mut added = false;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(path) = uri_to_path(line) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            self.state.add_icon(path);
            added = true;
        }
        added
    }

    fn on_frame(&mut self, _ctx: SurfaceCtx<'_>) -> bool {
        if let Some(start) = self.pending_start {
            if start.elapsed() >= HOVER_DELAY {
                self.pending_start = None;
                match self.vis {
                    VisState::PendingShow => {
                        self.vis = VisState::Showing;
                        self.last_tick = None;
                    }
                    VisState::PendingHide => {
                        self.vis = VisState::Hiding;
                        self.last_tick = None;
                    }
                    _ => {}
                }
            }
        }

        if let Some(target) = self.spring_target() {
            let now = Instant::now();
            let dt = self
                .last_tick
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(1.0 / 60.0)
                .min(SPRING_MAX_DT);
            self.last_tick = Some(now);
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
                    }
                    _ => {}
                }
            }
        }

        self.drop_finished_vanish();
        self.animation_in_flight()
    }
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(rest)))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
