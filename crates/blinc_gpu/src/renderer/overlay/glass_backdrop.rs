//! Snapshot paint before background glass without sampling an active attachment.
use super::{GpuRenderer, LayerTexture, Node};
use crate::GpuImageInstance;

pub(super) fn frame(nodes: &[Node]) -> Option<(u32, u32)> {
    nodes.iter().find_map(|node| match node {
        Node::Glass(glass) => Some(glass.frame_size),
        Node::Layer(_, children) => frame(children),
        _ => None,
    })
}

impl GpuRenderer {
    pub(super) fn capture_glass_backdrop(
        &mut self,
        target: &wgpu::TextureView,
        origin: [f32; 2],
        frame: (u32, u32),
        inherited: Option<&wgpu::TextureView>,
    ) -> LayerTexture {
        let viewport = self.viewport_size;
        let straight = self
            .layer_texture_cache
            .acquire_exact(&self.device, viewport, false);
        self.ensure_overlay_alpha();
        self.overlay_alpha.as_ref().unwrap().straighten_view(
            &self.device,
            &self.queue,
            &self.bind_group_layouts.composite,
            &self.path_image_sampler,
            target,
            &straight,
        );
        let backdrop = self
            .layer_texture_cache
            .acquire_exact(&self.device, frame, false);
        self.viewport_size = frame;
        self.clear_target(&backdrop.view, wgpu::Color::BLACK);
        if let Some(inherited) = inherited {
            self.render_images(
                &backdrop.view,
                inherited,
                &[GpuImageInstance::new(
                    0.0,
                    0.0,
                    frame.0 as f32,
                    frame.1 as f32,
                )],
            );
        }
        self.render_images(
            &backdrop.view,
            &straight.view,
            &[GpuImageInstance::new(
                origin[0],
                origin[1],
                viewport.0 as f32,
                viewport.1 as f32,
            )],
        );
        self.viewport_size = viewport;
        self.layer_texture_cache.release(straight);
        backdrop
    }
}
