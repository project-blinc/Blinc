//! Cached text retains auxiliary ownership through selection and replay.
use super::{Arc, GpuGlyph, GpuPrimitive, LayoutNodeId, PrimitiveBatch, RenderContext};

/// The glyph primitives one text produced.
#[derive(Clone)]
pub(super) struct TextRun {
    pub(super) range: std::ops::Range<usize>,
    /// The node that owns the text.
    pub(super) node: LayoutNodeId,
    /// The index in `TextBatch::elements` of the text it came from.
    pub(super) element: usize,
    /// How many times finer than its laid-out size it was rasterized.
    pub(super) k: f32,
    /// Whether the glyphs sit snapped to whole device pixels at the painted pose, as they do while
    /// their node is at rest. They are not while it moves, so that they move smoothly with it.
    pub(super) snapped: bool,
    /// Set while the glyphs are baked at the pose their node settled at, with no row: the node
    /// slot lane they had before, which they take again when the node moves.
    pub(super) baked: Option<[f32; 4]>,
}

#[derive(Clone, Default)]
pub(super) struct TextBatch {
    pub(super) glyphs: Vec<GpuGlyph>,
    pub(super) primitives: Vec<GpuPrimitive>,
    pub(super) aux_data: Arc<Vec<[f32; 4]>>,
    /// The runs of `primitives` that came from one text each, with the node that owns it. Kept
    /// only for the glyphs of motion subtrees.
    pub(super) owners: Vec<TextRun>,
    /// The texts the runs were prepared from, so a run can be prepared again at another size.
    pub(super) elements: Arc<Vec<super::TextElement>>,
    /// Which assignment of node slots the primitives carry, zero for none.
    pub(super) node_slots_epoch: u64,
}

/// How much narrower than its measured text a box may be before the text wraps, in physical
/// pixels. Layout rounds a box to a whole logical pixel, so the allowance grows with the density.
pub(super) fn wrap_slack(scale_factor: f32) -> f32 {
    2.0 * scale_factor.max(1.0)
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
            owners: Vec::new(),
            elements: Arc::default(),
            node_slots_epoch: 0,
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
        if !batch.primitives.is_empty()
            && let (Some(atlas), Some(color)) =
                (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_primitives_overlay_with_glyphs(
                target,
                &batch.primitives,
                atlas,
                color,
            );
        }
        self.render_legacy_text(target, &batch.glyphs);
    }
}

#[cfg(test)]
mod tests {
    use super::wrap_slack;

    #[test]
    fn the_wrap_allowance_grows_with_density_and_never_drops_below_two_pixels() {
        assert_eq!(wrap_slack(1.0), 2.0);
        assert_eq!(wrap_slack(2.0), 4.0);
        // A two-digit label whose box layout rounded down by 0.87 logical pixels is 2.6 physical
        // pixels short at 3x; the allowance must cover it, or the digits wrap one per line.
        assert!(wrap_slack(3.0) > 0.87 * 3.0);
        assert_eq!(wrap_slack(0.5), 2.0);
    }
}
