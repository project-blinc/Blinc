//! Prepare atlas raster pixels for a native group's ordered image position.
use super::{DrawContext, GpuPaintContext, PrimitiveBatch, Rect, RenderContext, SvgElement};
use blinc_core::{Affine2D, ClipShape, Transform};

impl RenderContext {
    pub(super) fn prepare_captured_svg(
        &mut self,
        svg: &SvgElement,
        viewport: (u32, u32),
        dpi: f32,
    ) -> PrimitiveBatch {
        let Some((key, tintable)) = self.prepare_svg_raster(svg) else {
            return PrimitiveBatch::new();
        };
        self.svg_atlas.mark_used(key);
        let Some((mut pixels, width, height)) = self.svg_atlas.copy_pixels(key) else {
            return PrimitiveBatch::new();
        };
        if tintable && let Some(tint) = svg.tint {
            for pixel in pixels.as_chunks_mut::<4>().0 {
                for (channel, factor) in pixel.iter_mut().zip([tint.r, tint.g, tint.b, tint.a]) {
                    *channel = (*channel as f32 * factor).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        let mut paint = if let Some(path) = &svg.path_clip {
            path.clip_context(svg.clip_bounds, viewport)
        } else {
            let mut paint = GpuPaintContext::new(viewport.0 as f32, viewport.1 as f32);
            if let Some([x, y, w, h]) = svg.clip_bounds {
                paint.push_clip(ClipShape::rect(Rect::new(x, y, w, h)));
            }
            paint
        };
        let (x, y) = if let Some([a, b, c, d, tx, ty]) = svg.css_affine {
            paint.push_transform(Transform::Affine2D(Affine2D {
                elements: [a, b, c, d, tx * dpi, ty * dpi],
            }));
            (svg.x, svg.y)
        } else {
            (svg.x.round(), svg.y.round())
        };
        paint.draw_rgba_pixels(
            &pixels,
            width,
            height,
            Rect::new(x, y, svg.width, svg.height),
        );
        paint.take_batch()
    }
}
