//! Prepare loaded textures and placeholders at native group paint positions.
use super::{DrawContext, GpuPaintContext, ImageElement, PrimitiveBatch, Rect, RenderContext};
use blinc_core::{Affine2D, ClipShape, Color, CornerRadius, DrawContextExt, Transform};

impl RenderContext {
    pub(super) fn prepare_captured_image(
        &mut self,
        image: &ImageElement,
        viewport: (u32, u32),
        dpi: f32,
    ) -> PrimitiveBatch {
        let mut paint = if let Some(path) = &image.path_clip {
            path.clip_context(None, viewport)
        } else {
            GpuPaintContext::new(viewport.0 as f32, viewport.1 as f32)
        };
        if let Some([x, y, w, h]) = image.clip_bounds {
            let [tl, tr, br, bl] = image.clip_radius;
            paint.push_clip(ClipShape::RoundedRect {
                rect: Rect::new(x, y, w, h),
                corner_radius: CornerRadius {
                    top_left: tl,
                    top_right: tr,
                    bottom_right: br,
                    bottom_left: bl,
                },
                corner_shape: blinc_core::CornerShape::ROUND,
            });
        }
        if let Some([a, b, c, d, tx, ty]) = image.css_affine {
            paint.push_transform(Transform::Affine2D(Affine2D {
                elements: [a, b, c, d, tx * dpi, ty * dpi],
            }));
        }
        let mut content = image.clone();
        content.css_affine = None;
        content.opacity = image.content_opacity;
        let loaded = self.image_cache.contains(&image.source);
        let source = if loaded {
            Some(&image.source)
        } else if image.placeholder_type == 2 {
            image.placeholder_image.as_ref()
        } else {
            None
        };
        if let Some(gpu) = source.and_then(|source| self.image_cache.get(source)) {
            let fade = if loaded && image.fade_duration_ms > 0 {
                self.image_load_times
                    .get(&image.source)
                    .map_or(1.0, |time| {
                        (time.elapsed().as_secs_f32() * 1000.0 / image.fade_duration_ms as f32)
                            .clamp(0.0, 1.0)
                    })
            } else {
                1.0
            };
            if !loaded {
                content.object_fit = 0;
                content.object_position = [0.5; 2];
                content.tint = [1.0; 4];
                content.filter_a = [0.0; 4];
                content.filter_b = [1.0, 1.0, 1.0, 0.0];
                content.border_width = 0.0;
                content.mask_info = [0.0; 4];
            }
            let style = Self::image_instance(&content, gpu.width(), gpu.height(), fade, dpi);
            let [x, y, w, h] = style.dst_rect;
            paint.draw_texture_image(
                gpu.view(),
                gpu.width(),
                gpu.height(),
                Rect::new(x, y, w, h),
                style,
            );
        } else if !loaded && matches!(image.placeholder_type, 1 | 3) {
            paint.push_opacity(image.content_opacity);
            let [r, g, b, a] = image.placeholder_color;
            let radius = CornerRadius::uniform(image.border_radius);
            let rect = Rect::new(image.x, image.y, image.width, image.height);
            let alpha = if image.placeholder_type == 3 {
                a.max(0.4)
            } else {
                a
            };
            paint.fill_rounded_rect(rect, radius, Color::rgba(r, g, b, alpha));
            if image.placeholder_type == 3 {
                let t = self.frame_count.saturating_mul(16).rem_euclid(2400) as f32 / 2400.0;
                let width = (image.width * 0.25).max(40.0);
                paint.push_clip(ClipShape::rounded_rect(rect, image.border_radius));
                paint.fill_rounded_rect(
                    Rect::new(
                        image.x + (image.width + width) * t - width,
                        image.y,
                        width,
                        image.height,
                    ),
                    radius,
                    Color::rgba(
                        (r + 0.15).min(1.0),
                        (g + 0.15).min(1.0),
                        (b + 0.15).min(1.0),
                        (alpha + 0.25).min(1.0),
                    ),
                );
            }
        }
        paint.take_batch()
    }
}
