use tiny_skia::{
    Color as SkColor, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

use crate::dock::{CellRect, DockState};
use crate::item::DockItem;
use crate::theme::Theme;

/// Immutable description of the dock fed to a renderer.
pub struct Scene<'a> {
    pub width: u32,
    pub height: u32,
    pub theme: &'a Theme,
    pub items: &'a [DockItem],
    pub cells: &'a [CellRect],
    pub hovered: Option<usize>,
    pub drop_active: bool,
}

impl<'a> Scene<'a> {
    pub fn from_state(state: &'a DockState, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            theme: state.theme(),
            items: &state.items,
            cells: &state.cells,
            hovered: state.hovered,
            drop_active: state.drop_active,
        }
    }
}

/// Pluggable paint backend. Implementors paint `scene` into the supplied
/// `canvas`, which is a BGRA-premultiplied byte slice sized
/// `scene.width * scene.height * 4`.
///
/// Add a new backend (Vulkan, GL, …) by implementing this trait and swapping
/// the renderer at the `wayland::App` construction site.
pub trait Renderer {
    fn paint(&mut self, scene: &Scene<'_>, canvas: &mut [u8]);
}

/// Software renderer based on `tiny_skia`. Reuses a single staging `Pixmap` to
/// avoid per-frame allocation.
pub struct TinySkiaRenderer {
    staging: Option<Pixmap>,
}

impl TinySkiaRenderer {
    pub fn new() -> Self {
        Self { staging: None }
    }

    fn staging_mut(&mut self, width: u32, height: u32) -> Option<&mut Pixmap> {
        let needs_resize = self
            .staging
            .as_ref()
            .map(|p| p.width() != width || p.height() != height)
            .unwrap_or(true);
        if needs_resize {
            self.staging = Pixmap::new(width, height);
        }
        self.staging.as_mut()
    }
}

impl Default for TinySkiaRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for TinySkiaRenderer {
    fn paint(&mut self, scene: &Scene<'_>, canvas: &mut [u8]) {
        let width = scene.width;
        let height = scene.height;

        let Some(pix) = self.staging_mut(width, height) else {
            log::error!("renderer: pixmap allocation failed for {width}x{height}");
            return;
        };
        pix.fill(SkColor::TRANSPARENT);
        paint_bar(pix, scene);
        paint_hover_fill(pix, scene);
        paint_icons(pix, scene);
        paint_active_indicator(pix, scene);
        paint_drop_outline(pix, scene);
        paint_empty_hint(pix, scene);

        copy_rgba_to_bgra(pix.data(), canvas, width, height);
    }
}

fn bar_rect(scene: &Scene<'_>) -> (f32, f32, f32, f32) {
    let theme = scene.theme;
    let cell = theme.cell_size();
    let n = scene.items.len() as u32;
    let total_w = if n == 0 {
        300.0
    } else {
        (n * cell + n.saturating_sub(1) * theme.icon_gap) as f32
    };
    let bar_h = cell as f32 + theme.padding as f32;
    let bar_w = total_w + (theme.padding * 2) as f32;
    let bar_x = (scene.width as f32 - bar_w) / 2.0;
    let bar_y = (scene.height as f32 - bar_h) / 2.0;
    (bar_x, bar_y, bar_w, bar_h)
}

fn paint_bar(pix: &mut Pixmap, scene: &Scene<'_>) {
    let (x, y, w, h) = bar_rect(scene);
    let radius = h / 2.0;
    let path = rounded_rect_path(x, y, w, h, radius);

    let bg = if scene.drop_active {
        scene.theme.background.with_alpha(scene.theme.background.a + 0.10)
    } else {
        scene.theme.background
    };
    let mut fill = Paint::default();
    fill.anti_alias = true;
    let [r, g, b, a] = bg.as_rgba8();
    fill.set_color_rgba8(r, g, b, a);
    pix.fill_path(&path, &fill, FillRule::Winding, Transform::identity(), None);

    let mut stroke_paint = Paint::default();
    stroke_paint.anti_alias = true;
    stroke_paint.set_color_rgba8(255, 255, 255, if scene.drop_active { 80 } else { 22 });
    pix.stroke_path(
        &path,
        &stroke_paint,
        &Stroke {
            width: 1.0,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

fn paint_hover_fill(pix: &mut Pixmap, scene: &Scene<'_>) {
    let Some(h) = scene.hovered else { return };
    let Some(cell) = scene.cells.get(h) else { return };

    let mut paint = Paint::default();
    paint.anti_alias = true;
    let [r, g, b, a] = scene.theme.hover.as_rgba8();
    paint.set_color_rgba8(r, g, b, a);
    let path = rounded_rect_path(cell.x, cell.y, cell.w, cell.h, 12.0);
    pix.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

fn paint_icons(pix: &mut Pixmap, scene: &Scene<'_>) {
    let img_paint = PixmapPaint::default();
    for (i, item) in scene.items.iter().enumerate() {
        let Some(cell) = scene.cells.get(i) else { break };
        match &item.icon {
            Some(icon) => {
                let icon_x = cell.x + (cell.w - icon.width() as f32) / 2.0;
                let icon_y = cell.y + (cell.h - icon.height() as f32) / 2.0;
                pix.draw_pixmap(
                    icon_x as i32,
                    icon_y as i32,
                    icon.as_ref(),
                    &img_paint,
                    Transform::identity(),
                    None,
                );
            }
            None => paint_icon_placeholder(pix, cell),
        }
    }
}

fn paint_icon_placeholder(pix: &mut Pixmap, cell: &CellRect) {
    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(255, 255, 255, 60);
    let cx = cell.x + cell.w / 2.0;
    let cy = cell.y + cell.h / 2.0;
    let r = (cell.w.min(cell.h) - 16.0) / 2.0;
    if let Some(circle) = PathBuilder::from_circle(cx, cy, r) {
        pix.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn paint_active_indicator(pix: &mut Pixmap, scene: &Scene<'_>) {
    let Some(h) = scene.hovered else { return };
    let Some(cell) = scene.cells.get(h) else { return };
    let (_, bar_y, _, bar_h) = bar_rect(scene);

    let dot_w = 16.0;
    let dot_x = cell.x + (cell.w - dot_w) / 2.0;
    let dot_y = bar_y + bar_h - 5.0;
    let path = rounded_rect_path(dot_x, dot_y, dot_w, 3.0, 1.5);

    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(255, 255, 255, 220);
    pix.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

fn paint_drop_outline(pix: &mut Pixmap, scene: &Scene<'_>) {
    if !scene.drop_active {
        return;
    }
    let (x, y, w, h) = bar_rect(scene);
    let path = rounded_rect_path(x + 2.0, y + 2.0, w - 4.0, h - 4.0, (h - 4.0) / 2.0);

    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(120, 180, 255, 200);
    pix.stroke_path(
        &path,
        &paint,
        &Stroke {
            width: 2.0,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

fn paint_empty_hint(pix: &mut Pixmap, scene: &Scene<'_>) {
    if !scene.items.is_empty() {
        return;
    }
    let (x, y, w, h) = bar_rect(scene);
    let path = rounded_rect_path(x + 12.0, y + 12.0, w - 24.0, h - 24.0, 8.0);

    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(255, 255, 255, 100);
    pix.stroke_path(
        &path,
        &paint,
        &Stroke {
            width: 2.0,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

fn copy_rgba_to_bgra(src: &[u8], dst: &mut [u8], width: u32, height: u32) {
    let n = (width * height) as usize;
    for i in 0..n {
        let o = i * 4;
        dst[o] = src[o + 2];
        dst[o + 1] = src[o + 1];
        dst[o + 2] = src[o];
        dst[o + 3] = src[o + 3];
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.max(0.0).min(w.min(h) / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().expect("rounded rect path")
}

