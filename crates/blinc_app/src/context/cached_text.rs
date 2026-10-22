//! Cached text retains auxiliary ownership through selection and replay.
use super::{Arc, GpuGlyph, GpuPrimitive, PrimitiveBatch, RenderContext};

#[derive(Clone, Default)]
pub(super) struct TextBatch {
    pub(super) glyphs: Vec<GpuGlyph>,
    pub(super) primitives: Vec<GpuPrimitive>,
    pub(super) aux_data: Arc<Vec<[f32; 4]>>,
}

fn primitive_bounds(p: &GpuPrimitive) -> [f32; 4] {
    let [x, y, w, h] = p.bounds;
    let [a, b, c, d] = p.local_affine;
    let width = a.abs() * w + c.abs() * h;
    let height = b.abs() * w + d.abs() * h;
    [x + (w - width) * 0.5, y + (h - height) * 0.5, width, height]
}

impl TextBatch {
    pub(super) fn is_empty(&self) -> bool {
        self.glyphs.is_empty() && self.primitives.is_empty()
    }
    pub(super) fn bounds(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        self.glyphs
            .iter()
            .map(|g| g.bounds)
            .chain(self.primitives.iter().map(primitive_bounds))
    }
    pub(super) fn append(&mut self, mut batch: PrimitiveBatch) {
        let shift = self.aux_data.len() as f32;
        for glyph in &mut batch.glyphs {
            if glyph.flags[1] > 0.0 {
                glyph.flags[1] += shift;
            }
        }
        for primitive in &mut batch.primitives {
            primitive.rebase_aux(shift);
        }
        self.glyphs.extend(batch.glyphs);
        self.primitives.extend(batch.primitives);
        Arc::make_mut(&mut self.aux_data).extend(batch.aux_data);
    }
    pub(super) fn filtered(&self, mut member: impl FnMut([f32; 4]) -> bool) -> Self {
        Self {
            glyphs: self
                .glyphs
                .iter()
                .copied()
                .filter(|g| member(g.bounds))
                .collect(),
            primitives: self
                .primitives
                .iter()
                .copied()
                .filter(|p| member(primitive_bounds(p)))
                .collect(),
            // References keep their original offsets when earlier glyphs are omitted.
            aux_data: self.aux_data.clone(),
        }
    }
}

pub(super) fn convert_affine(batch: &mut PrimitiveBatch, affine: [f32; 6], dpi: f32, z: u32) {
    let [a, b, c, d, tx, ty] = affine;
    for glyph in batch.glyphs.drain(..) {
        let x = glyph.bounds[0] + glyph.bounds[2] * 0.5;
        let y = glyph.bounds[1] + glyph.bounds[3] * 0.5;
        let mut p = GpuPrimitive::from_glyph(&glyph);
        p.bounds[0] = a * x + c * y + tx * dpi - glyph.bounds[2] * 0.5;
        p.bounds[1] = b * x + d * y + ty * dpi - glyph.bounds[3] * 0.5;
        p.local_affine = [a, b, c, d];
        p.set_z_layer(z);
        batch.primitives.push(p);
    }
}

impl RenderContext {
    pub(super) fn render_text(&mut self, target: &wgpu::TextureView, batch: &TextBatch) {
        if batch.is_empty() {
            return;
        }
        self.renderer.update_aux_data_slice(&batch.aux_data);
        if !batch.primitives.is_empty() {
            if let (Some(atlas), Some(color)) =
                (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
            {
                self.renderer.render_primitives_overlay_with_glyphs(
                    target,
                    &batch.primitives,
                    atlas,
                    color,
                );
            }
        }
        self.render_legacy_text(target, &batch.glyphs);
    }
}
