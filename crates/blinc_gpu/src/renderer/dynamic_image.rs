//! Raw uploads and retained textures in ordered batches or the explicit image pass.
use super::{GpuRenderer, ImageBlend, PrimitiveBatch};
use crate::{GpuImageInstance, primitives::DynamicImage};
use std::borrow::Cow;

impl GpuRenderer {
    /// Draw raw uploads or retained textures in the supplied order.
    /// Ordered batch and foreground methods already draw their image records;
    /// `render_unordered_dynamic_images` draws only separate legacy records.
    pub fn render_dynamic_images(&mut self, target: &wgpu::TextureView, images: &[DynamicImage]) {
        for image in images {
            self.render_dynamic_image_at(target, image, [0.0; 2]);
        }
    }

    /// Draw only records kept outside the ordered batch streams.
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
        self.render_dynamic_image_blend(target, img, origin, ImageBlend::Over);
    }

    pub(super) fn render_dynamic_image_blend(
        &mut self,
        target: &wgpu::TextureView,
        img: &DynamicImage,
        origin: [f32; 2],
        blend: ImageBlend,
    ) {
        if img.width == 0
            || img.height == 0
            || (img.retained.is_none()
                && img
                    .width
                    .checked_mul(img.height)
                    .and_then(|n| n.checked_mul(4))
                    .map(|n| n as usize)
                    != Some(img.data.len()))
            || img.opacity <= 0.0
            || !DynamicImage::valid_geometry(img.dest, img.transform)
        {
            return;
        }
        let [cx, cy, cw, ch] = img.clip_bounds;
        let no_clip = cx <= -9999.0;
        if !no_clip && (cw <= 0.0 || ch <= 0.0) {
            return;
        }
        let mut instance = img
            .retained
            .as_ref()
            .map_or_else(GpuImageInstance::default, |image| image.style);
        instance.dst_rect = [
            img.dest.x() - origin[0],
            img.dest.y() - origin[1],
            img.dest.width(),
            img.dest.height(),
        ];
        instance.transform = img.transform;
        instance.params[0] = img.corner_radius;
        instance.params[1] *= img.opacity;
        instance.clip_bounds = [-10000.0, -10000.0, 100000.0, 100000.0];
        instance.clip_radius = [0.0; 4];
        if instance.clip2_bounds[0] > -9999.0 {
            instance.clip2_bounds[0] -= origin[0];
            instance.clip2_bounds[1] -= origin[1];
        }
        if !no_clip {
            instance.clip_bounds = [cx - origin[0], cy - origin[1], cw, ch];
            instance.clip_radius = img.clip_radius;
        }
        let mut aux = Cow::Borrowed(img.clip_aux.as_slice());
        if aux.len() >= 5 {
            instance.abi_clip[0] = 1.0;
            if origin != [0.0; 2] {
                let rows = aux.to_mut();
                rows[2][0] -= origin[0];
                rows[2][1] -= origin[1];
                let shape = rows[4][3] as usize;
                if shape >= 1 && shape <= rows.len() - 2 {
                    let [a, b, c, d] = rows[shape - 1];
                    rows[shape][0] += a * origin[0] + c * origin[1];
                    rows[shape][1] += b * origin[0] + d * origin[1];
                }
            }
        }
        if let Some(image) = &img.retained {
            self.render_images_with_aux_blend(target, &image.view, &[instance], &aux, blend);
            return;
        }
        let image = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &img.data,
            img.width,
            img.height,
            Some("dynamic image"),
        );
        self.render_images_with_aux_blend(target, image.view(), &[instance], &aux, blend);
    }

    pub(super) fn render_image_backdrop(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        preserve: bool,
    ) {
        if batch.layer_commands.is_empty()
            && batch.dynamic_images.is_empty()
            && batch.paths.indices.is_empty()
        {
            self.render_to_backdrop(target, self.viewport_size, batch, preserve);
            return;
        }
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
    /// Bake background commands once for the visible target and its glass backdrop.
    /// Callback execution is shared; foreground and glass remain separate passes.
    pub fn render_background_pair(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: (&wgpu::TextureView, (u32, u32)),
        batch: &PrimitiveBatch,
        clear: Option<wgpu::Color>,
        preserve_backdrop: bool,
    ) {
        let viewport = self.viewport_size;
        let (texture, _) = self.render_subtree_to_layer_texture(
            batch,
            (0.0, 0.0),
            (viewport.0 as f32, viewport.1 as f32),
        );
        self.viewport_size = backdrop.1;
        if !preserve_backdrop {
            self.clear_target(backdrop.0, wgpu::Color::BLACK);
        }
        self.render_images(
            backdrop.0,
            &texture.view,
            &[GpuImageInstance::new(
                0.0,
                0.0,
                backdrop.1.0 as f32,
                backdrop.1.1 as f32,
            )],
        );
        self.viewport_size = viewport;
        if let Some(clear) = clear {
            self.clear_target(target, clear);
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
        self.update_aux_data_buffer(batch);
    }

    pub(super) fn render_image_glass_frame(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
        preserve: bool,
    ) {
        if batch.has_external_gpu_draws() {
            self.render_background_pair(
                target,
                (backdrop, backdrop_size),
                batch,
                (!preserve).then_some(wgpu::Color::BLACK),
                preserve,
            );
        } else {
            self.render_image_backdrop(backdrop, batch, preserve);
            if !preserve {
                self.clear_target(target, wgpu::Color::BLACK);
            }
            self.render_overlay_msaa(target, batch, 1);
        }
        if !batch.glass_primitives.is_empty() || !batch.nested_glass_primitives.is_empty() {
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
        self.render_foreground_with_glass(target, backdrop, batch);
    }
}
