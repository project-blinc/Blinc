//! Collected text keeps geometry and a push-time frame, never foreign aux offsets.
use super::{
    Arc, DrawContext, GpuGlyph, GpuPaintContext, PrimitiveBatch, Rect, RenderContext, TextElement,
};
use blinc_core::{Affine2D, ClipPath, ClipShape, Path, Transform};

pub(super) struct CollectedTextPath {
    path: Path,
    frame: [f32; 6],
}

pub(super) fn prepare_glyphs(
    text: &TextElement,
    mut glyphs: Vec<GpuGlyph>,
    size: (u32, u32),
) -> PrimitiveBatch {
    if let Some(clip) = text.clip_bounds {
        for glyph in &mut glyphs {
            glyph.clip_bounds = clip;
        }
    }
    if let Some(path) = &text.path_clip {
        path.prepare(&glyphs, text.clip_bounds, size)
    } else {
        let mut owned = PrimitiveBatch::new();
        owned.glyphs = glyphs;
        owned
    }
}

impl RenderContext {
    pub(super) fn render_owned_text(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        if let (Some(atlas), Some(color)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer
                .render_text_batch(target, batch, atlas, color, self.text_ctx.sampler());
        }
    }
}

impl CollectedTextPath {
    pub(super) fn capture(
        clip: &ClipPath,
        bounds: [f32; 4],
        css: Option<[f32; 6]>,
        motion_scale: (f32, f32),
        motion_translate: (f32, f32),
        motion_center: Option<(f32, f32)>,
        scale: f32,
    ) -> Option<Arc<Self>> {
        let [x, y, w, h] = bounds;
        let points: Vec<_> = match clip {
            ClipPath::Path { vertices } => vertices.clone(),
            ClipPath::Polygon { points } => points
                .iter()
                .map(|(x, y)| (x.resolve(w), y.resolve(h)))
                .collect(),
            _ => return None,
        };
        let mut path = Path::new();
        if points.len() >= 3 {
            path = path.move_to(points[0].0, points[0].1);
            for &(x, y) in &points[1..] {
                path = path.line_to(x, y);
            }
            path = path.close();
        }
        let [a, b, c, d, tx, ty] = css.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let (sx, sy) = motion_scale;
        let (mx, my) = motion_translate;
        let (cx, cy) = motion_center.unwrap_or((0.0, 0.0));
        Some(Arc::new(Self {
            path,
            frame: [
                a * sx,
                b * sy,
                c * sx,
                d * sy,
                (a * x + c * y + tx) * sx + cx * (1.0 - sx) + mx,
                (b * x + d * y + ty) * sy + cy * (1.0 - sy) + my,
            ]
            .map(|v| v * scale),
        }))
    }

    pub(super) fn prepare(
        &self,
        glyphs: &[GpuGlyph],
        scissor: Option<[f32; 4]>,
        size: (u32, u32),
    ) -> PrimitiveBatch {
        let mut paint = GpuPaintContext::new(size.0 as f32, size.1 as f32);
        paint.push_transform(Transform::Affine2D(Affine2D {
            elements: self.frame,
        }));
        paint.push_clip(ClipShape::Path(self.path.clone()));
        paint.pop_transform();
        if let Some([x, y, w, h]) = scissor {
            paint.push_clip(ClipShape::rect(Rect::new(x, y, w, h)));
        }
        paint.push_prepared_glyphs(glyphs);
        paint.take_batch()
    }
}
