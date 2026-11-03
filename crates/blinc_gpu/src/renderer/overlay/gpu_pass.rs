//! Apply callback and mesh-scene edits through ordinary canvas image coverage.
use super::{GpuRenderer, Node};
use crate::{
    GpuImageInstance,
    paint::{GroupGpuPass, GroupMeshes},
    primitives::{DynamicImage, RetainedImage},
    renderer::ImageBlend,
};
use std::sync::Arc;

pub(super) fn has_external_gpu_draws(items: &[Node]) -> bool {
    items.iter().any(|node| match node {
        Node::GpuPass(_) | Node::Meshes(_) => true,
        Node::Layer(_, children) => has_external_gpu_draws(children),
        _ => false,
    })
}

impl GpuRenderer {
    pub(super) fn draw_group_gpu_pass(
        &mut self,
        target: &wgpu::TextureView,
        pass: &GroupGpuPass,
        origin: [f32; 2],
    ) {
        if pass.paint.opacity <= 0.0 || pass.frame_size.0 == 0 || pass.frame_size.1 == 0 {
            return;
        }
        let (width, height) = pass.frame_size;
        let viewport = match pass.pending.viewport {
            Some([x, y, w, h]) => {
                if ![x, y, w, h].into_iter().all(f32::is_finite) {
                    return;
                }
                let left = x.clamp(0.0, width as f32);
                let top = y.clamp(0.0, height as f32);
                let right = (x + w).clamp(left, width as f32);
                let bottom = (y + h).clamp(top, height as f32);
                if right - left < 1.0 || bottom - top < 1.0 {
                    return;
                }
                Some([left, top, right - left, bottom - top])
            }
            None => None,
        };
        self.edit_group_target(
            target,
            &pass.paint,
            pass.frame_size,
            origin,
            |renderer, edit| {
                let context = crate::custom_pass::RenderPassContext {
                    device: &renderer.device,
                    queue: &renderer.queue,
                    target: edit,
                    viewport_width: width,
                    viewport_height: height,
                    texture_format: renderer.texture_format,
                    scale_factor: pass.scale_factor,
                    view_proj: None,
                    inv_view_proj: None,
                    camera_pos: None,
                    viewport,
                };
                if let Some(flow) = &pass.pending.flow {
                    renderer.render_pending_flow(edit, flow, context.viewport);
                    return;
                }
                pass.pending.pass.initialize_and_render(
                    &renderer.device,
                    &renderer.queue,
                    renderer.texture_format,
                    &context,
                );
            },
        );
    }

    pub(super) fn draw_group_meshes(
        &mut self,
        target: &wgpu::TextureView,
        scene: &GroupMeshes,
        origin: [f32; 2],
    ) {
        self.edit_group_target(
            target,
            &scene.paint,
            scene.frame_size,
            origin,
            |renderer, edit| {
                renderer.render_pending_meshes(
                    edit,
                    scene.frame_size.0,
                    scene.frame_size.1,
                    &scene.meshes,
                );
            },
        );
    }

    fn edit_group_target(
        &mut self,
        target: &wgpu::TextureView,
        paint: &DynamicImage,
        frame_size: (u32, u32),
        origin: [f32; 2],
        edit_draw: impl FnOnce(&mut Self, &wgpu::TextureView),
    ) {
        if paint.opacity <= 0.0 || frame_size.0 == 0 || frame_size.1 == 0 {
            return;
        }
        let edit = self
            .layer_texture_cache
            .acquire_exact(&self.device, frame_size, false);
        let straight = self
            .layer_texture_cache
            .acquire_exact(&self.device, frame_size, false);
        self.ensure_overlay_alpha();
        self.overlay_alpha.as_ref().unwrap().capture(
            &self.device,
            &self.queue,
            &self.bind_group_layouts.composite,
            &self.path_image_sampler,
            target,
            &edit,
            origin,
        );
        edit_draw(self, &edit.view);
        self.overlay_alpha.as_ref().unwrap().straighten(
            &self.device,
            &self.queue,
            &self.bind_group_layouts.composite,
            &self.path_image_sampler,
            &edit,
            &straight,
        );
        // Erase by coverage, then add the edited pixels. Transparent clears and
        // fractional opacity replace content without counting its alpha twice.
        let mut mask = paint.clone();
        mask.data = vec![255; 4];
        self.render_dynamic_image_blend(target, &mask, origin, ImageBlend::Erase);
        let mut image = paint.clone();
        image.width = frame_size.0;
        image.height = frame_size.1;
        image.retained = Some(Arc::new(RetainedImage {
            view: straight.view.clone(),
            style: GpuImageInstance::default(),
        }));
        self.render_dynamic_image_blend(target, &image, origin, ImageBlend::Add);
        self.layer_texture_cache.release(edit);
        self.layer_texture_cache.release(straight);
    }
}
