pub mod container;
pub mod icon;
pub mod rect;
pub mod state;

use anyhow::Result;
use vello::{
    kurbo::{Affine, Circle, Point, Rect, Stroke},
    peniko::{
        color::DynamicColor, Color, ColorStop, ColorStops, Extend, Fill, Gradient,
    },
    wgpu, Scene,
};

pub use state::State;

use crate::{
    config::{DOCK_INSET_X, DOCK_INSET_Y},
    renderer::Renderer,
};
use rect::RectShader;

/// Padding between an icon and the dock pill's interior edge (top/bottom).
const ICON_PADDING_Y: f32 = 10.0;
/// Minimum horizontal gap between icons when many are packed in.
const MIN_ICON_GAP: f32 = 8.0;

pub struct Ui {
    renderer: Renderer,
    pub scene: Scene,
    pub rect_shader: RectShader,
}

impl Ui {
    pub fn new(renderer: Renderer) -> Result<Self> {
        let rect_shader = RectShader::new(renderer.device(), renderer.surface_format())?;
        Ok(Self {
            renderer,
            scene: Scene::new(),
            rect_shader,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    pub fn set_y_offset(&mut self, offset: f32) {
        self.rect_shader.y_offset = offset;
    }

    pub fn render(&mut self, state: &State) -> Result<()> {
        let Some(surface_texture) = self.renderer.acquire() else {
            return Ok(());
        };

        let (width, height) = self.renderer.surface_size();
        self.build_scene(state, width, height);
        self.renderer.render_scene(&self.scene)?;

        let surface_view = surface_texture.texture.create_view(&Default::default());
        let mut encoder = self
            .renderer
            .device()
            .create_command_encoder(&Default::default());

        self.renderer.blit_to(&mut encoder, &surface_view);

        self.rect_shader
            .write_uniforms(self.renderer.queue(), width, height);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rect shader pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.rect_shader.draw(&mut pass);
        }

        self.renderer.queue().submit([encoder.finish()]);
        surface_texture.present();

        Ok(())
    }

    fn build_scene(&mut self, state: &State, width: u32, height: u32) {
        self.scene.reset();

        let Some(layout) = compute_layout(state.icons.len(), width, height) else {
            return;
        };
        let radius = layout.radius;
        let y_center = layout.y_center + self.rect_shader.y_offset;

        // Paint icons in slots, skipping the one being dragged (it'll be
        // drawn last, at the cursor position).
        for (i, icon) in state.icons.iter().enumerate() {
            if Some(i) == state.drag_skip_idx {
                continue;
            }
            let cx = layout.slot_cx(i);
            draw_icon(&mut self.scene, icon, cx, y_center, radius, 1.0, 1.0);
        }

        // Mid-flight vanish animations: scale up + fade out.
        for v in &state.vanishing {
            draw_vanishing(&mut self.scene, v);
        }

        // The dragged icon rides above everything else at the cursor.
        if let Some(drag) = state.drag_render.as_ref() {
            let alpha = drag.alpha.clamp(0.0, 1.0);
            let scale = drag.scale.max(0.01);
            let dummy = state::Icon {
                path: drag.path.clone(),
                image: drag.image.clone(),
            };
            draw_icon(
                &mut self.scene,
                &dummy,
                drag.cx,
                drag.cy,
                radius,
                scale,
                alpha,
            );
        }
    }
}

/// Geometric layout of dock icon slots. Computed from surface size and icon
/// count, deterministic so the renderer and the hit-tester can agree.
#[derive(Debug, Clone, Copy)]
pub struct DockLayout {
    pub count: usize,
    pub radius: f32,
    pub y_center: f32,
    pub first_cx: f32,
    pub step: f32,
}

impl DockLayout {
    pub fn slot_cx(&self, i: usize) -> f32 {
        self.first_cx + self.step * i as f32
    }

    /// Hit-test a point in surface coordinates against the icon slots.
    /// `y_offset` is the current slide offset of the dock (px).
    pub fn hit_test(&self, x: f32, y: f32, y_offset: f32) -> Option<usize> {
        let cy = self.y_center + y_offset;
        let r = self.radius;
        let dy = y - cy;
        if dy.abs() > r {
            return None;
        }
        for i in 0..self.count {
            let cx = self.slot_cx(i);
            let dx = x - cx;
            if dx * dx + dy * dy <= r * r {
                return Some(i);
            }
        }
        None
    }
}

pub fn compute_layout(count: usize, width: u32, height: u32) -> Option<DockLayout> {
    if count == 0 {
        return None;
    }
    let inset_x = DOCK_INSET_X;
    let inset_y = DOCK_INSET_Y;
    let interior_w = (width as f32 - 2.0 * inset_x).max(0.0);
    let interior_h = (height as f32 - 2.0 * inset_y).max(0.0);

    let n = count as f32;
    let diameter_by_height = (interior_h - 2.0 * ICON_PADDING_Y).max(0.0);
    let diameter_by_width = ((interior_w - MIN_ICON_GAP * (n + 1.0)) / n).max(0.0);
    let diameter = diameter_by_height.min(diameter_by_width).max(0.0);
    if diameter <= 0.0 {
        return None;
    }
    let radius = diameter * 0.5;

    let used = diameter * n;
    let gap = ((interior_w - used) / (n + 1.0)).max(0.0);

    let y_center = inset_y + interior_h * 0.5;
    let first_cx = inset_x + gap + radius;
    let step = diameter + gap;

    Some(DockLayout {
        count,
        radius,
        y_center,
        first_cx,
        step,
    })
}

/// Paints one dock icon: soft drop shadow, circular clip of the icon artwork
/// (or fallback color), an interior radial darkening + top sheen for depth,
/// and a top-bright / bottom-dark rim stroke. The aim is the visionOS-style
/// circular icon look — slightly inset into a glass surface rather than
/// printed onto it.
fn draw_icon(
    scene: &mut Scene,
    icon: &state::Icon,
    cx: f32,
    cy: f32,
    base_radius: f32,
    scale_mul: f32,
    alpha: f32,
) {
    if alpha <= 0.0 || scale_mul <= 0.0 {
        return;
    }
    let radius = base_radius * scale_mul;
    let r = radius as f64;
    let cx_f = cx as f64;
    let cy_f = cy as f64;
    let bounds = Rect::new(cx_f - r, cy_f - r, cx_f + r, cy_f + r);
    let circle = Circle::new(Point::new(cx_f, cy_f), r);

    // 1. Drop shadow — blurred circle, offset slightly below the icon.
    let shadow_dy = (r * 0.10).max(2.0);
    let shadow_blur = (r * 0.30).max(4.0);
    let shadow_rect = Rect::new(
        bounds.x0,
        bounds.y0 + shadow_dy,
        bounds.x1,
        bounds.y1 + shadow_dy,
    );
    let shadow_alpha = ((110.0 * alpha) as u32).min(255) as u8;
    scene.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        shadow_rect,
        Color::from_rgba8(0, 0, 0, shadow_alpha),
        r,
        shadow_blur,
    );

    // 2. Clip to circle and paint the icon (or fallback color).
    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &circle);

    if let Some(image) = icon.image.as_ref() {
        let sx = (2.0 * r) / image.width as f64;
        let sy = (2.0 * r) / image.height as f64;
        let cover = sx.max(sy);
        let img_w = image.width as f64 * cover;
        let img_h = image.height as f64 * cover;
        let dx = bounds.x0 + (2.0 * r - img_w) * 0.5;
        let dy = bounds.y0 + (2.0 * r - img_h) * 0.5;
        let xform = Affine::translate((dx, dy)) * Affine::scale(cover);
        if alpha >= 1.0 {
            scene.draw_image(image, xform);
        } else {
            let brush = vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(alpha);
            let img_rect = Rect::new(0.0, 0.0, image.width as f64, image.height as f64);
            scene.fill(Fill::NonZero, xform, &brush, None, &img_rect);
        }
    } else {
        let [cr, cg, cb, _] = icon.fallback_color();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::new([cr, cg, cb, alpha]),
            None,
            &circle,
        );
    }

    // 3a. Inner shadow — radial darkening biased toward the bottom rim.
    let inner_shadow = Gradient::new_two_point_radial(
        Point::new(cx_f, cy_f - r * 0.15),
        0.0,
        Point::new(cx_f, cy_f),
        radius,
    )
    .with_extend(Extend::Pad)
    .with_stops(make_stops([
        (0.0, [0.0, 0.0, 0.0, 0.0]),
        (0.70, [0.0, 0.0, 0.0, 0.0]),
        (1.0, [0.0, 0.0, 0.0, 0.22 * alpha]),
    ]));
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &inner_shadow,
        None,
        &bounds,
    );

    // 3b. Top specular sheen — subtle linear gradient over the top.
    let sheen = Gradient::new_linear(
        Point::new(cx_f, cy_f - r),
        Point::new(cx_f, cy_f - r * 0.30),
    )
    .with_extend(Extend::Pad)
    .with_stops(make_stops([
        (0.0, [1.0, 1.0, 1.0, 0.30 * alpha]),
        (1.0, [1.0, 1.0, 1.0, 0.0]),
    ]));
    scene.fill(Fill::NonZero, Affine::IDENTITY, &sheen, None, &bounds);

    scene.pop_layer();

    // 4. Rim stroke — bright on top, dark on bottom, fakes the depth bevel.
    let rim = Gradient::new_linear(
        Point::new(cx_f, cy_f - r),
        Point::new(cx_f, cy_f + r),
    )
    .with_extend(Extend::Pad)
    .with_stops(make_stops([
        (0.0, [1.0, 1.0, 1.0, 0.50 * alpha]),
        (0.5, [1.0, 1.0, 1.0, 0.10 * alpha]),
        (1.0, [0.0, 0.0, 0.0, 0.22 * alpha]),
    ]));
    let stroke_width = (radius * 0.04).max(1.0) as f64;
    scene.stroke(&Stroke::new(stroke_width), Affine::IDENTITY, &rim, None, &circle);
}

fn draw_vanishing(scene: &mut Scene, v: &state::Vanishing) {
    let icon = state::Icon {
        path: v.path.clone(),
        image: v.image.clone(),
    };
    draw_icon(scene, &icon, v.cx, v.cy, v.radius, v.scale, v.alpha);
}

fn make_stops<const N: usize>(items: [(f32, [f32; 4]); N]) -> ColorStops {
    let mut stops = ColorStops::default();
    for (offset, [r, g, b, a]) in items {
        stops.push(ColorStop {
            offset,
            color: DynamicColor::from_alpha_color(Color::new([r, g, b, a])),
        });
    }
    stops
}
