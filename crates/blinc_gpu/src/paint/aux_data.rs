//! Clip metadata and auxiliary offsets cached per batch.
use super::path_clip::PathClipData;
use super::{ClipShape, GpuPaintContext};
use smallvec::SmallVec;

pub(super) enum ClipData {
    Polygon(PolygonClipData),
    Path(PathClipData),
}

impl ClipData {
    pub fn polygon(&self) -> Option<&PolygonClipData> {
        match self {
            Self::Polygon(polygon) => Some(polygon),
            Self::Path(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        let metadata = self.clip_stack[clip_index]
            .1
            .as_ref()
            .unwrap()
            .polygon()
            .unwrap();
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
        let Some(ClipData::Polygon(metadata)) = &mut self.clip_stack[clip_index].1 else {
            unreachable!()
        };
        metadata.offsets.push((batch, offset));
        offset
    }

    pub(super) fn forget_clip_offsets(&mut self, batch: AuxBatch) {
        for (_, metadata, _) in &mut self.clip_stack {
            match metadata {
                Some(ClipData::Polygon(metadata)) => {
                    metadata.offsets.retain(|(key, _)| *key != batch)
                }
                Some(ClipData::Path(metadata)) => metadata.offsets.retain(|(key, _)| *key != batch),
                None => {}
            }
        }
    }

    pub(super) fn forget_composite_clip_offsets(&mut self) {
        for (_, metadata, _) in &mut self.clip_stack {
            let offsets = match metadata {
                Some(ClipData::Polygon(metadata)) => &mut metadata.offsets,
                Some(ClipData::Path(metadata)) => &mut metadata.offsets,
                None => continue,
            };
            offsets.retain(|(key, _)| !matches!(key, AuxBatch::Composite(_)));
        }
    }
}
