//! Path clips for draws consumed by the native primitive pipelines.
use super::{ClipData, ClipShape, ClipType, GpuPaintContext, GpuPrimitive};

impl GpuPaintContext<'_> {
    fn native_path_clip(&mut self) -> Option<f32> {
        let start = self.composite_layer_clip_base.first().copied().unwrap_or(0);
        let index = self.clip_stack[start..].iter().rposition(|(shape, _, _)| {
            matches!(
                shape,
                ClipShape::Circle { .. }
                    | ClipShape::Ellipse { .. }
                    | ClipShape::Polygon(_)
                    | ClipShape::Path(_)
            )
        })? + start;
        let owner = self.aux_batch();
        let (ClipShape::Path(shape), Some(ClipData::Path(path)), _) = &mut self.clip_stack[index]
        else {
            return None;
        };
        if let Some((_, offset)) = path.offsets.iter().find(|(key, _)| *key == owner) {
            return Some(*offset as f32 + 1.0);
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
        Some(offset as f32 + 1.0)
    }

    pub(super) fn native_primitive_path_clip(&mut self) -> Option<([f32; 4], f32)> {
        let pointer = self.native_path_clip()?;
        let (scissor, _, _, _) = self.get_clip_data_with_path_scissor(true);
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

    pub(super) fn push_native_primitive(&mut self, primitive: GpuPrimitive) {
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
        let (bounds, radius, kind) =
            if let Some((bounds, pointer)) = self.native_primitive_path_clip() {
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
    pub(super) fn emit_native_glass(&mut self, mut glass: super::GpuGlassPrimitive, nested: bool) {
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
        if nested {
            self.active_batch_mut().push_nested_glass(glass);
        } else {
            self.active_batch_mut().push_glass(glass);
        }
    }
}
