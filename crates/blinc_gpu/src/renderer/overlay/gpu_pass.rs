//! Apply callback edits through the same image clips as ordinary canvas output.
use super::{GpuRenderer, Node};
use crate::{
    GpuImageInstance, paint::GroupGpuPass, primitives::RetainedImage, renderer::ImageBlend,
};
use std::sync::Arc;

pub(super) fn has_gpu_pass(items: &[Node]) -> bool {
    items.iter().any(|node| match node {
        Node::GpuPass(_) => true,
        Node::Layer(_, children) => has_gpu_pass(children),
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
        let edit = self
            .layer_texture_cache
            .acquire_exact(&self.device, pass.frame_size, false);
        let straight = self
            .layer_texture_cache
            .acquire_exact(&self.device, pass.frame_size, false);
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
        let context = crate::custom_pass::RenderPassContext {
            device: &self.device,
            queue: &self.queue,
            target: &edit.view,
            viewport_width: width,
            viewport_height: height,
            texture_format: self.texture_format,
            scale_factor: pass.scale_factor,
            view_proj: None,
            inv_view_proj: None,
            camera_pos: None,
            viewport,
        };
        pass.pending.pass.initialize_and_render(
            &self.device,
            &self.queue,
            self.texture_format,
            &context,
        );
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
        let mut mask = pass.paint.clone();
        mask.data = vec![255; 4];
        self.render_dynamic_image_blend(target, &mask, origin, ImageBlend::Erase);
        let mut image = pass.paint.clone();
        image.width = width;
        image.height = height;
        image.retained = Some(Arc::new(RetainedImage {
            view: straight.view.clone(),
            style: GpuImageInstance::default(),
        }));
        self.render_dynamic_image_blend(target, &image, origin, ImageBlend::Add);
        self.layer_texture_cache.release(edit);
        self.layer_texture_cache.release(straight);
    }
}
