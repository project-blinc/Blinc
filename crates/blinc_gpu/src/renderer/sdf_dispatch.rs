//! Sorting and uploading primitives, and dispatching them through the split SDF pipelines.

use super::{GpuRenderer, MsaaPipelines, Pipelines, SdfPrimitiveRanges};
use crate::primitives::{GpuPrimitive, SdfPipelineCategory, SdfVertexInstance};

impl GpuRenderer {
    /// Write only the given subranges of `primitives` to the GPU
    /// storage buffer, leaving the rest as it was after the previous
    /// upload. Used by the compositor fast path: when an animation
    /// frame patched only the primitives covered by a handful of
    /// motion-bound subtrees, we re-upload just those byte ranges
    /// (each one `range.len() × sizeof::<GpuPrimitive>()` bytes) via
    /// `queue.write_buffer(offset, slice)` instead of pushing the
    /// whole batch.
    ///
    /// Caller guarantees:
    ///  - `primitives` is the SAME-length view as what was uploaded
    ///    on the previous full paint (no insertions / deletions).
    ///  - Each range is in-bounds (`end <= primitives.len()`).
    ///  - DT-mode (no storage buffers) is **not** in use — partial
    ///    upload via `write_texture` isn't worth the complexity for
    ///    a fallback path; fall back to the full
    ///    `write_primitives_safe` if `has_storage_buffers` is false.
    ///
    /// Cost (cn_demo benchmark): full upload was ~100 KB / frame
    /// (~150 µs of `queue.write_buffer` overhead). A typical fast-path
    /// frame patches a single binding's range — for the
    /// `cn::progress_animated` indicator that's <10 primitives or
    /// ~2.5 KB / frame, putting the write closer to ~5 µs.
    pub fn write_primitives_partial(
        &self,
        primitives: &[GpuPrimitive],
        ranges: &[std::ops::Range<usize>],
    ) {
        if primitives.is_empty() || ranges.is_empty() || !self.has_storage_buffers {
            return;
        }
        let stride = std::mem::size_of::<GpuPrimitive>() as u64;
        let max_primitives = self.config.max_primitives;
        for range in ranges {
            // Clip to the same capacity bounds `write_primitives_safe`
            // uses for the full upload — keeps both paths consistent
            // when an app accidentally over-emits.
            let end = range.end.min(primitives.len()).min(max_primitives);
            if range.start >= end {
                continue;
            }
            let slice = &primitives[range.start..end];
            let bytes = bytemuck::cast_slice::<GpuPrimitive, u8>(slice);
            let offset_bytes = (range.start as u64) * stride;
            self.queue
                .write_buffer(&self.buffers.primitives, offset_bytes, bytes);
        }
    }

    /// Safely write primitives to buffer, truncating if necessary to prevent overflow
    pub(super) fn write_primitives_safe(&self, primitives: &[GpuPrimitive]) {
        if primitives.is_empty() {
            return;
        }
        let max_primitives = self.config.max_primitives;
        let primitives_to_write = if primitives.len() > max_primitives {
            tracing::warn!(
                "Primitive count {} exceeds buffer capacity {}, truncating",
                primitives.len(),
                max_primitives
            );
            &primitives[..max_primitives]
        } else {
            primitives
        };

        if !self.has_storage_buffers {
            // DT mode: upload to data texture instead of storage buffer.
            // One complete primitive per row, including integer fields as raw bits.
            if let Some(ref tex) = self.buffers.prim_data_texture {
                let bytes = bytemuck::cast_slice::<GpuPrimitive, u8>(primitives_to_write);
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(std::mem::size_of::<GpuPrimitive>() as u32),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: GpuPrimitive::DATA_TEXTURE_TEXELS,
                        height: primitives_to_write.len() as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        } else {
            // Tier 1/2: write to storage buffer as before
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(primitives_to_write),
            );
        }
    }

    /// Sort primitives by `SdfPipelineCategory` and compute contiguous ranges.
    ///
    /// Returns a new sorted `Vec` and the corresponding `SdfPrimitiveRanges`.
    /// Text primitives are included in the sorted output (and tracked in ranges)
    /// but should NOT be drawn by the split SDF pipelines — they use the separate
    /// text pipeline.
    pub(super) fn sort_primitives_by_category(
        primitives: &[GpuPrimitive],
    ) -> (Vec<GpuPrimitive>, SdfPrimitiveRanges) {
        if primitives.is_empty() {
            return (Vec::new(), SdfPrimitiveRanges::default());
        }
        let mut sorted: Vec<GpuPrimitive> = primitives.to_vec();
        sorted.sort_by_key(|p| p.pipeline_category());

        let mut ranges = SdfPrimitiveRanges::default();
        let mut i = 0u32;
        let len = sorted.len() as u32;
        while i < len {
            let cat = sorted[i as usize].pipeline_category();
            let start = i;
            while i < len && sorted[i as usize].pipeline_category() == cat {
                i += 1;
            }
            let range = start..i;
            match cat {
                SdfPipelineCategory::Core
                | SdfPipelineCategory::CoreSimple
                | SdfPipelineCategory::CorePlain => ranges.core = range,
                SdfPipelineCategory::Shadow => ranges.shadow = range,
                SdfPipelineCategory::Sdf3D => ranges.sdf_3d = range,
                SdfPipelineCategory::Notch => ranges.notch = range,
                SdfPipelineCategory::Text => ranges.text = range,
            }
        }
        (sorted, ranges)
    }

    /// Sort primitives, upload to the GPU buffer (with safety truncation), and return ranges.
    ///
    /// When `has_vertex_storage` is `false`, also builds and uploads the
    /// `SdfVertexInstance` buffer used by the VB fallback shaders.
    pub(super) fn upload_sorted_primitives(
        &mut self,
        primitives: &[GpuPrimitive],
    ) -> SdfPrimitiveRanges {
        if primitives.is_empty() {
            return SdfPrimitiveRanges::default();
        }
        // Preserve instance-index z-order. Scan once to (a) populate the
        // per-category ranges (used for has-text / has-shadow checks) and
        // (b) build the contiguous-run list that `draw_split_sdf` iterates
        // in order so shadow/3D/notch don't get overdrawn by later core
        // primitives (app backgrounds, section containers).
        let mut ranges = SdfPrimitiveRanges::default();
        let len = primitives.len() as u32;
        let full = 0..len;
        for p in primitives {
            match p.pipeline_category() {
                SdfPipelineCategory::Core
                | SdfPipelineCategory::CoreSimple
                | SdfPipelineCategory::CorePlain => {
                    if ranges.core.is_empty() {
                        ranges.core = full.clone();
                    }
                }
                SdfPipelineCategory::Shadow => {
                    if ranges.shadow.is_empty() {
                        ranges.shadow = full.clone();
                    }
                }
                SdfPipelineCategory::Sdf3D => {
                    if ranges.sdf_3d.is_empty() {
                        ranges.sdf_3d = full.clone();
                    }
                }
                SdfPipelineCategory::Notch => {
                    if ranges.notch.is_empty() {
                        ranges.notch = full.clone();
                    }
                }
                SdfPipelineCategory::Text => {
                    if ranges.text.is_empty() {
                        ranges.text = full.clone();
                    }
                }
            }
        }
        ranges.runs = Self::compute_category_runs(primitives);
        self.write_primitives_safe(primitives);

        // VERTEX_STORAGE fallback: build instance data and upload to VB
        if !self.has_vertex_storage {
            let instances: Vec<SdfVertexInstance> = primitives
                .iter()
                .map(SdfVertexInstance::from_primitive)
                .collect();
            let bytes = bytemuck::cast_slice::<SdfVertexInstance, u8>(&instances);
            let needed = bytes.len() as u64;

            // Create or resize the vertex buffer if necessary
            let needs_new_buffer = match &self.buffers.sdf_vertex_instances {
                Some(buf) => buf.size() < needed,
                None => true,
            };
            if needs_new_buffer {
                self.buffers.sdf_vertex_instances =
                    Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("SDF Vertex Instances (VB Fallback)"),
                        size: needed,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }));
            }
            if let Some(buf) = &self.buffers.sdf_vertex_instances {
                self.queue.write_buffer(buf, 0, bytes);
            }
        }

        ranges
    }

    /// Returns the SDF vertex instance buffer for the VB fallback path,
    /// or `None` when VERTEX_STORAGE is supported.
    pub(super) fn sdf_vb_buffer(&self) -> Option<&wgpu::Buffer> {
        if self.has_vertex_storage {
            None
        } else {
            self.buffers.sdf_vertex_instances.as_ref()
        }
    }

    /// Issue draw calls for split SDF pipelines using pre-computed ranges.
    ///
    /// The bind group must already be set on the render pass before calling this.
    /// If `overlay` is true, the overlay pipeline variants are used (1x sampled).
    /// When `vb_buffer` is `Some`, the instance vertex buffer is bound at slot 0
    /// before each pipeline draw (VERTEX_STORAGE fallback path).
    pub(super) fn draw_split_sdf<'a>(
        render_pass: &mut wgpu::RenderPass<'a>,
        pipelines: &'a Pipelines,
        ranges: &SdfPrimitiveRanges,
        overlay: bool,
        vb_buffer: Option<&'a wgpu::Buffer>,
    ) {
        if let Some(buf) = vb_buffer {
            render_pass.set_vertex_buffer(0, buf.slice(..));
        }
        // Issue one draw per contiguous same-category run. That's how the
        // monolithic shader preserved cross-type z-order: one pipeline,
        // per-instance branching. The split pipelines enforce the same
        // ordering by switching pipelines between runs. Drawing each
        // pipeline over the full range (the old approach) lost ordering —
        // e.g. a section background at index 5 would paint over a shadow at
        // index 10 because the core pipeline ran after the shadow pipeline
        // regardless of instance order.
        for &(cat, start, end) in &ranges.runs {
            if start >= end {
                continue;
            }
            let pipeline = match (cat, overlay) {
                (SdfPipelineCategory::Shadow, false) => &pipelines.sdf_shadow,
                (SdfPipelineCategory::Shadow, true) => &pipelines.sdf_shadow_overlay,
                (SdfPipelineCategory::Notch, false) => &pipelines.sdf_notch,
                (SdfPipelineCategory::Notch, true) => &pipelines.sdf_notch_overlay,
                (SdfPipelineCategory::Sdf3D, false) => &pipelines.sdf_3d,
                (SdfPipelineCategory::Sdf3D, true) => &pipelines.sdf_3d_overlay,
                // Core and Text both ride the sdf_core pipeline — its
                // PRIM_TEXT branch samples the glyph atlas for prim_type=7.
                (SdfPipelineCategory::Core, false) | (SdfPipelineCategory::Text, false) => {
                    &pipelines.sdf_core
                }
                (SdfPipelineCategory::Core, true) | (SdfPipelineCategory::Text, true) => {
                    &pipelines.sdf_core_overlay
                }
                (SdfPipelineCategory::CoreSimple, false) => &pipelines.sdf_core_simple,
                (SdfPipelineCategory::CoreSimple, true) => &pipelines.sdf_core_simple_overlay,
                (SdfPipelineCategory::CorePlain, false) => &pipelines.sdf_plain,
                (SdfPipelineCategory::CorePlain, true) => &pipelines.sdf_plain_overlay,
            };
            render_pass.set_pipeline(pipeline);
            render_pass.draw(0..6, start..end);
        }
    }

    /// Walk the primitive slice and emit `(category, start_index, end_index)`
    /// tuples covering every contiguous run of same-category primitives.
    /// Drawing these in order preserves instance-index z-order across all
    /// pipeline categories.
    pub(super) fn compute_category_runs(
        primitives: &[GpuPrimitive],
    ) -> Vec<(SdfPipelineCategory, u32, u32)> {
        let mut runs = Vec::new();
        if primitives.is_empty() {
            return runs;
        }
        let mut start = 0u32;
        let mut cat = primitives[0].pipeline_category();
        for (i, p) in primitives.iter().enumerate().skip(1) {
            let c = p.pipeline_category();
            if c != cat {
                runs.push((cat, start, i as u32));
                start = i as u32;
                cat = c;
            }
        }
        runs.push((cat, start, primitives.len() as u32));
        runs
    }

    /// Issue draw calls for split SDF pipelines using MSAA pipeline variants.
    ///
    /// Used by `render_overlay_msaa` where a specific sample count is in play.
    /// When `vb_buffer` is `Some`, the instance vertex buffer is bound at slot 0
    /// (VERTEX_STORAGE fallback path).
    pub(super) fn draw_split_sdf_msaa<'a>(
        render_pass: &mut wgpu::RenderPass<'a>,
        msaa: &'a MsaaPipelines,
        ranges: &SdfPrimitiveRanges,
        vb_buffer: Option<&'a wgpu::Buffer>,
    ) {
        if let Some(buf) = vb_buffer {
            render_pass.set_vertex_buffer(0, buf.slice(..));
        }
        // Full range for z-order preservation (same approach as draw_split_sdf)
        let total_start = [
            &ranges.core,
            &ranges.shadow,
            &ranges.sdf_3d,
            &ranges.notch,
            &ranges.text,
        ]
        .iter()
        .filter(|r| !r.is_empty())
        .map(|r| r.start)
        .min();
        let total_end = [
            &ranges.core,
            &ranges.shadow,
            &ranges.sdf_3d,
            &ranges.notch,
            &ranges.text,
        ]
        .iter()
        .filter(|r| !r.is_empty())
        .map(|r| r.end)
        .max();
        let full_range = match (total_start, total_end) {
            (Some(s), Some(e)) => s..e,
            _ => return,
        };
        // Per-run draws preserve cross-category z-order (same approach as
        // `draw_split_sdf`). `full_range` is now only used to short-circuit
        // empty batches.
        let _ = full_range;
        for &(cat, start, end) in &ranges.runs {
            if start >= end {
                continue;
            }
            let pipeline = match cat {
                SdfPipelineCategory::Shadow => &msaa.sdf_shadow,
                SdfPipelineCategory::Notch => &msaa.sdf_notch,
                SdfPipelineCategory::Sdf3D => &msaa.sdf_3d,
                // Core + Text both ride sdf_core (its PRIM_TEXT branch
                // samples the glyph atlas for prim_type=7).
                SdfPipelineCategory::Core
                | SdfPipelineCategory::CoreSimple
                | SdfPipelineCategory::CorePlain
                | SdfPipelineCategory::Text => &msaa.sdf_core,
            };
            render_pass.set_pipeline(pipeline);
            render_pass.draw(0..6, start..end);
        }
    }
}
