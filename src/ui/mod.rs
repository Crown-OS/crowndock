pub mod container;
pub mod rect;
pub mod state;

use anyhow::Result;
use vello::{
    kurbo::{Affine, Circle},
    peniko::{Color, Fill},
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
        if state.icons.is_empty() {
            return;
        }

        let inset_x = DOCK_INSET_X;
        let inset_y = DOCK_INSET_Y;
        let interior_x0 = inset_x;
        let interior_x1 = (width as f32 - inset_x).max(inset_x);
        let interior_y0 = inset_y;
        let interior_y1 = (height as f32 - inset_y).max(inset_y);
        let interior_w = (interior_x1 - interior_x0).max(0.0);
        let interior_h = (interior_y1 - interior_y0).max(0.0);

        // Icon diameter is bounded by the pill height (minus padding) and
        // shrinks to fit horizontally when many icons are present.
        let diameter_by_height = (interior_h - 2.0 * ICON_PADDING_Y).max(0.0);
        let n = state.icons.len() as f32;
        let diameter_by_width =
            ((interior_w - MIN_ICON_GAP * (n + 1.0)) / n).max(0.0);
        let diameter = diameter_by_height.min(diameter_by_width).max(0.0);
        if diameter <= 0.0 {
            return;
        }
        let radius = diameter * 0.5;

        // Distribute remaining horizontal space evenly into n+1 gaps.
        let used = diameter * n;
        let gap = ((interior_w - used) / (n + 1.0)).max(0.0);

        let y_center = interior_y0 + interior_h * 0.5 + self.rect_shader.y_offset;
        let mut x = interior_x0 + gap + radius;

        for icon in &state.icons {
            let [r, g, b, a] = icon.color();
            let color = Color::new([r, g, b, a]);
            let circle = Circle::new((x as f64, y_center as f64), radius as f64);
            self.scene
                .fill(Fill::NonZero, Affine::IDENTITY, color, None, &circle);
            x += diameter + gap;
        }
    }
}
