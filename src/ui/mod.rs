pub mod container;
pub mod rect;
pub mod state;

use anyhow::Result;
use vello::{wgpu, Scene};

pub use state::State;

use crate::renderer::Renderer;
use rect::RectShader;

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

    pub fn render(&mut self) -> Result<()> {
        let Some(surface_texture) = self.renderer.acquire() else {
            return Ok(());
        };

        self.renderer.render_scene(&self.scene)?;

        let surface_view = surface_texture.texture.create_view(&Default::default());
        let mut encoder = self
            .renderer
            .device()
            .create_command_encoder(&Default::default());

        self.renderer.blit_to(&mut encoder, &surface_view);

        let (width, height) = self.renderer.surface_size();
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
}
