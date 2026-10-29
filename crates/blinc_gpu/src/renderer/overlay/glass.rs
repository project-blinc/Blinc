//! Render tight glass surfaces while retaining screen-space backdrop coordinates.
use super::GpuRenderer;
use crate::primitives::RetainedImage;
use crate::{GpuImageInstance, PrimitiveBatch, paint::GroupGlass};
use std::sync::Arc;

impl GpuRenderer {
    pub(super) fn draw_group_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        glass: &GroupGlass,
        origin: [f32; 2],
    ) {
        if glass.paint.opacity <= 0.0 || glass.frame_size.0 == 0 || glass.frame_size.1 == 0 {
            return;
        }
        if origin == [0.0; 2]
            && self.viewport_size == glass.frame_size
            && glass.paint.opacity == 1.0
            && glass.paint.clip_aux.is_empty()
            && glass.paint.clip_bounds[0] <= -9999.0
        {
            let mut batch = PrimitiveBatch::new();
            batch.glass_primitives.push(glass.primitive);
            self.render_glass(target, backdrop, &batch);
            return;
        }
        let rect = glass.paint.dest;
        let size = (rect.width() as u32, rect.height() as u32);
        let viewport = self.viewport_size;
        let texture = self
            .layer_texture_cache
            .acquire_exact(&self.device, size, false);
        let straight = self
            .layer_texture_cache
            .acquire_exact(&self.device, size, false);
        self.viewport_size = size;
        self.clear_target(&texture.view, wgpu::Color::TRANSPARENT);
        let mut batch = PrimitiveBatch::new();
        batch.glass_primitives.push(glass.primitive);
        self.render_glass_at(
            &texture.view,
            backdrop,
            &batch,
            [glass.frame_size.0 as f32, glass.frame_size.1 as f32],
            [rect.x(), rect.y()],
            true,
        );
        self.ensure_overlay_alpha();
        self.overlay_alpha.as_ref().unwrap().straighten(
            &self.device,
            &self.queue,
            &self.bind_group_layouts.composite,
            &self.path_image_sampler,
            &texture,
            &straight,
        );
        self.viewport_size = viewport;
        let mut image = glass.paint.clone();
        image.width = size.0;
        image.height = size.1;
        image.retained = Some(Arc::new(RetainedImage {
            view: straight.view.clone(),
            style: GpuImageInstance::default(),
        }));
        self.render_dynamic_image_at(target, &image, origin);
        self.layer_texture_cache.release(texture);
        self.layer_texture_cache.release(straight);
    }
}
