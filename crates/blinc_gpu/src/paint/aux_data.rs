//! Polygon payloads are cached separately for each batch that uses the clip.
use super::{ClipShape, GpuPaintContext};
use smallvec::SmallVec;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AuxBatch {
    Static,
    Dynamic,
    Composite(u64),
}

pub(super) struct PolygonClipData {
    pub packed_count: u32,
    offsets: SmallVec<[(AuxBatch, u32); 2]>,
}

impl PolygonClipData {
    pub fn new(batch: AuxBatch, offset: u32, packed_count: u32) -> Self {
        Self {
            packed_count,
            offsets: smallvec::smallvec![(batch, offset)],
        }
    }
}

impl GpuPaintContext<'_> {
    #[inline]
    pub(super) fn aux_batch(&self) -> AuxBatch {
        if let Some(node) = self.active_composite_layer {
            AuxBatch::Composite(node)
        } else if self.motion_subtree_depth > 0 {
            AuxBatch::Dynamic
        } else {
            AuxBatch::Static
        }
    }

    pub(super) fn polygon_clip_offset(&mut self, clip_index: usize) -> u32 {
        let batch = self.aux_batch();
        let metadata = self.clip_stack[clip_index].1.as_ref().unwrap();
        if let Some((_, offset)) = metadata.offsets.iter().find(|(key, _)| *key == batch) {
            return *offset;
        }
        let offset = self.active_batch_mut().aux_data.len() as u32;
        let count = match &self.clip_stack[clip_index].0 {
            ClipShape::Polygon(points) => points.len(),
            _ => unreachable!("polygon metadata belongs to a polygon clip"),
        };
        for index in (0..count).step_by(2) {
            let row = {
                let ClipShape::Polygon(points) = &self.clip_stack[clip_index].0 else {
                    unreachable!()
                };
                let first = points[index];
                let second = points.get(index + 1).copied().unwrap_or_default();
                [first.x, first.y, second.x, second.y]
            };
            self.active_batch_mut().aux_data.push(row);
        }
        self.clip_stack[clip_index]
            .1
            .as_mut()
            .unwrap()
            .offsets
            .push((batch, offset));
        offset
    }

    pub(super) fn forget_polygon_offsets(&mut self, batch: AuxBatch) {
        for (_, metadata, _) in &mut self.clip_stack {
            if let Some(metadata) = metadata {
                metadata.offsets.retain(|(key, _)| *key != batch);
            }
        }
    }

    pub(super) fn forget_composite_polygon_offsets(&mut self) {
        for (_, metadata, _) in &mut self.clip_stack {
            if let Some(metadata) = metadata {
                metadata
                    .offsets
                    .retain(|(key, _)| !matches!(key, AuxBatch::Composite(_)));
            }
        }
    }
}
