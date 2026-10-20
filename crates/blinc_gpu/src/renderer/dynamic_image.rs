//! RGBA upload drawing shared by ordered batches and the explicit image pass.
use super::{GpuRenderer, PrimitiveBatch};
use crate::{GpuImageInstance, primitives::DynamicImage};

impl GpuRenderer {
    /// Upload and draw a sequence of raw RGBA images in the supplied order.
    /// Full-batch methods already draw background records; use
    /// `render_unordered_dynamic_images` for their separate foreground pass.
    pub fn render_dynamic_images(&mut self, target: &wgpu::TextureView, images: &[DynamicImage]) {
        for image in images {
            self.render_dynamic_image_at(target, image, [0.0; 2]);
        }
    }

    /// Draw only images kept outside the batch's ordered background stream.
    pub fn render_unordered_dynamic_images(
        &mut self,
        target: &wgpu::TextureView,
        images: &[DynamicImage],
    ) {
        for image in images.iter().filter(|image| image.paint_order.is_none()) {
            self.render_dynamic_image_at(target, image, [0.0; 2]);
        }
    }

    pub(super) fn render_dynamic_image_at(
        &mut self,
        target: &wgpu::TextureView,
        img: &DynamicImage,
        origin: [f32; 2],
    ) {
        if img.width == 0
            || img.height == 0
            || img
                .width
                .checked_mul(img.height)
                .and_then(|n| n.checked_mul(4))
                .map(|n| n as usize)
                != Some(img.data.len())
            || img.opacity <= 0.0
        {
            return;
        }
        let [cx, cy, cw, ch] = img.clip_bounds;
        let no_clip = cx <= -9999.0;
        if !no_clip && (cw <= 0.0 || ch <= 0.0) {
            return;
        }
        let mut instance = GpuImageInstance::new(
            img.dest.x() - origin[0],
            img.dest.y() - origin[1],
            img.dest.width(),
            img.dest.height(),
        )
        .with_opacity(img.opacity)
        .with_border_radius(img.corner_radius);
        if !no_clip {
            instance.clip_bounds = [cx - origin[0], cy - origin[1], cw, ch];
            instance.clip_radius = img.clip_radius;
        }
        let image = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &img.data,
            img.width,
            img.height,
            Some("dynamic image"),
        );
        self.render_images(target, image.view(), &[instance]);
    }

    pub(super) fn render_image_backdrop(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        preserve: bool,
    ) {
        // Compose at screen resolution so nested layer scissors use screen pixels,
        // then sample the straight-alpha bake into the smaller backdrop.
        let viewport = self.viewport_size;
        let (texture, _) = self.render_subtree_to_layer_texture(
            batch,
            (0.0, 0.0),
            (viewport.0 as f32, viewport.1 as f32),
        );
        if !preserve {
            self.clear_target(target, wgpu::Color::BLACK);
        }
        self.render_images(
            target,
            &texture.view,
            &[GpuImageInstance::new(
                0.0,
                0.0,
                viewport.0 as f32,
                viewport.1 as f32,
            )],
        );
        self.layer_texture_cache.release(texture);
    }
    pub(super) fn render_image_glass_frame(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
        preserve: bool,
    ) {
        self.render_image_backdrop(backdrop, batch, preserve);
        if !preserve {
            self.clear_target(target, wgpu::Color::BLACK);
        }
        self.render_overlay_msaa(target, batch, 1);
        if self.has_storage_buffers
            && (!batch.glass_primitives.is_empty() || !batch.nested_glass_primitives.is_empty())
        {
            // Reuse the native glass sequence without replaying the background.
            let mut glass = PrimitiveBatch::new();
            glass.glass_primitives.clone_from(&batch.glass_primitives);
            glass
                .nested_glass_primitives
                .clone_from(&batch.nested_glass_primitives);
            glass.aux_data.clone_from(&batch.aux_data);
            self.render_glass_frame(target, backdrop, backdrop_size, &glass, true);
        }
        self.update_aux_data_buffer(batch);
        self.render_primitives_overlay(target, &batch.foreground_primitives);
    }
}
