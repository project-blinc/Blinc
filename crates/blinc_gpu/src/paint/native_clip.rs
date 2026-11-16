//! Path clips for draws consumed by the native primitive pipelines.
use super::{ClipData, ClipShape, GpuPaintContext};
use crate::primitives::{ClipType, GpuGlassPrimitive, GpuPrimitive};

impl GpuPaintContext<'_> {
    /// Append prepared screen-space glyphs to the active batch's separate text
    /// stream, capturing the selected path, rectangular scissor and fade.
    /// Other nonrectangular clips retain the separate glyph stream's rect fallback.
    /// Incoming clip metadata is replaced; its auxiliary rows belong to its source.
    /// Glyph positions and colors are already prepared and are left unchanged.
    /// Render with `GpuRenderer::render_text_batch`, or convert within this batch.
    pub fn push_prepared_glyphs(&mut self, glyphs: &[crate::primitives::GpuGlyph]) {
        if glyphs.is_empty() {
            return;
        }
        let (bounds, pointer) = if let Some(clip) = self.native_primitive_path_clip() {
            clip
        } else {
            (self.get_clip_data().0, 0.0)
        };
        let fade = self.get_clip_fade();
        self.active_batch_mut()
            .glyphs
            .extend(glyphs.iter().map(|g| {
                let mut glyph = *g;
                glyph.clip_bounds = bounds;
                glyph.clip_fade = fade;
                glyph.flags[1] = pointer;
                glyph
            }));
    }

    /// The shape clip a primitive emitted now carries: one more than its rows' offset in the
    /// batch, and whether it is the inner clips of content that rows move (which then stay out of
    /// its scissor). `None` when the topmost shape clip is not one drawn from aux rows here.
    pub(super) fn native_path_clip(&mut self) -> Option<(f32, bool)> {
        let start = self.composite_layer_clip_base.first().copied().unwrap_or(0);
        let index = self.clip_stack[start..]
            .iter()
            .rposition(|(shape, meta, _)| {
                matches!(meta, Some(ClipData::Inner(_)))
                    || matches!(
                        shape,
                        ClipShape::Circle { .. }
                            | ClipShape::Ellipse { .. }
                            | ClipShape::Polygon(_)
                            | ClipShape::Path(_)
                    )
            })?
            + start;
        let owner = self.aux_batch();
        if matches!(self.clip_stack[index].1, Some(ClipData::Inner(_))) {
            return Some((self.inner_clip_rows(start, index, owner), true));
        }
        let (ClipShape::Path(shape), Some(ClipData::Path(path)), _) = &mut self.clip_stack[index]
        else {
            return None;
        };
        if let Some((_, offset)) = path.offsets.iter().find(|(key, _)| *key == owner) {
            return Some((*offset as f32 + 1.0, false));
        }
        // Native shape headers start at the frame; the five image-only rows stay out.
        let rows = &path.rows(shape)[5..];
        let batch = match owner {
            super::AuxBatch::Static => &mut self.batch,
            super::AuxBatch::Dynamic => &mut self.dynamic_batch,
            super::AuxBatch::Composite(node) => {
                self.composite_layer_batches.entry(node).or_default()
            }
        };
        let offset = batch.aux_data.len() as u32;
        batch.aux_data.extend_from_slice(rows);
        path.offsets.push((owner, offset));
        Some((offset as f32 + 1.0, false))
    }

    /// The pointer to one rounded-rect shape (type 4: a radius and a corner shape for each
    /// corner) for the inner clips in force up to `index`, their rects intersected and the
    /// largest radius at each corner with its shape, written to `owner`'s aux rows once and kept
    /// on the entry at `index`.
    fn inner_clip_rows(&mut self, start: usize, index: usize, owner: super::AuxBatch) -> f32 {
        if let Some(ClipData::Inner(inner)) = &self.clip_stack[index].1
            && let Some((_, offset)) = inner.offsets.iter().find(|(key, _)| *key == owner)
        {
            return *offset as f32 + 1.0;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MIN, f32::MIN, f32::MAX, f32::MAX);
        let mut radii = [0.0f32; 4];
        let mut shapes = [1.0f32; 4];
        for (shape, meta, _) in &self.clip_stack[start..=index] {
            if !matches!(meta, Some(ClipData::Inner(_))) {
                continue;
            }
            let (rect, r, n) = match shape {
                ClipShape::Rect(rect) => (*rect, [0.0; 4], [1.0; 4]),
                ClipShape::RoundedRect {
                    rect,
                    corner_radius: c,
                    corner_shape: k,
                } => (
                    *rect,
                    [c.top_left, c.top_right, c.bottom_right, c.bottom_left],
                    [k.top_left, k.top_right, k.bottom_right, k.bottom_left],
                ),
                _ => continue,
            };
            x0 = x0.max(rect.x());
            y0 = y0.max(rect.y());
            x1 = x1.min(rect.x() + rect.width());
            y1 = y1.min(rect.y() + rect.height());
            for corner in 0..4 {
                if r[corner] > radii[corner] {
                    radii[corner] = r[corner];
                    shapes[corner] = n[corner];
                }
            }
        }
        let rows = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 4.0, 0.0],
            [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)],
            radii,
            shapes,
        ];
        let batch = match owner {
            super::AuxBatch::Static => &mut self.batch,
            super::AuxBatch::Dynamic => &mut self.dynamic_batch,
            super::AuxBatch::Composite(node) => {
                self.composite_layer_batches.entry(node).or_default()
            }
        };
        let offset = batch.aux_data.len() as u32;
        batch.aux_data.extend_from_slice(&rows);
        if let Some(ClipData::Inner(inner)) = &mut self.clip_stack[index].1 {
            inner.offsets.push((owner, offset));
        }
        offset as f32 + 1.0
    }

    pub(super) fn native_primitive_path_clip(&mut self) -> Option<([f32; 4], f32)> {
        let (pointer, inner) = self.native_path_clip()?;
        let (scissor, _, _, _) = self.get_clip_data_with_path_scissor(true, inner);
        Some((scissor, pointer))
    }

    pub(super) fn emit_native_primitive(&mut self, mut primitive: GpuPrimitive) {
        if let Some((scissor, pointer)) = self.native_primitive_path_clip() {
            primitive.clip_bounds = scissor;
            primitive.clip_radius = [0.0; 4];
            primitive.clip_corner_shape = [1.0; 4];
            primitive.type_info[2] = ClipType::Rect as u32;
            primitive.clip_shape[0] = pointer;
        }
        self.push_native_primitive(primitive);
    }

    pub(super) fn push_native_primitive(&mut self, mut primitive: GpuPrimitive) {
        primitive.set_node_ordinal(self.motion_node_ordinal);
        if self.emit_foreground() {
            self.active_batch_mut().push_foreground(primitive);
        } else {
            self.active_batch_mut().push(primitive);
        }
    }
    pub(super) fn emit_native_path(
        &mut self,
        mut path: crate::path::TessellatedPath,
        brush: &crate::path::PathBrushInfo,
    ) {
        // Inner clips stay a plain clip rect here: a path's clip shape orders the batch, and rows
        // do not move path vertices.
        let inner = self.native_path_clip().is_some_and(|(_, inner)| inner);
        let (bounds, radius, kind) = if let Some((bounds, pointer)) = (!inner)
            .then(|| self.native_primitive_path_clip())
            .flatten()
        {
            for v in &mut path.vertices {
                v.clip_shape = pointer as u32;
            }
            (bounds, [0.0; 4], ClipType::Rect)
        } else {
            let (bounds, radius, _, kind) = self.get_clip_data();
            (bounds, radius, kind)
        };
        if self.emit_foreground() {
            self.active_batch_mut()
                .push_foreground_path_with_brush_info(path, bounds, radius, kind, brush);
        } else {
            self.active_batch_mut()
                .push_path_with_brush_info(path, bounds, radius, kind, brush);
        }
    }
    pub(super) fn emit_native_glass(&mut self, mut glass: GpuGlassPrimitive, nested: bool) {
        if let Some((bounds, pointer)) = self.native_primitive_path_clip() {
            glass.clip_bounds = bounds;
            glass.clip_radius = [pointer, 0.0, 0.0, -1.0];
        } else if glass.clip_bounds[0] < -5000.0 {
            let (bounds, radius, _, kind) = self.get_clip_data();
            if kind == ClipType::Rect {
                glass.clip_bounds = bounds;
                glass.clip_radius = radius;
            }
        }
        if self.capture_group_glass(glass, nested) {
            return;
        }
        if nested {
            self.active_batch_mut().push_nested_glass(glass);
        } else {
            self.active_batch_mut().push_glass(glass);
        }
    }
}
