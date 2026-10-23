//! Collect and refresh independently dispatched canvas output.
use super::{
    CanvasOverlay, DrawContext, GpuPaintContext, GpuPrimitive, PrimitiveBatch, Rect, RenderContext,
    RenderState, RenderTree,
};

impl RenderContext {
    /// Refresh independent canvas primitives in place. Return false when output or
    /// ownership requires a full paint.
    pub fn redraw_canvases(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> bool {
        if tree.had_retained_canvas_painted() {
            return false;
        }
        use blinc_core::{DrawContext, Rect, Transform, layer::Affine2D};
        use blinc_gpu::GpuPaintContext;

        let batch = match self.cached_bg_batch.as_mut() {
            Some(b) => b,
            None => return false,
        };

        if batch.has_ordered_draws()
            || batch.has_foreground_content()
            || batch.has_foreground_layers()
        {
            return false;
        }
        let records_ref = tree.canvas_paint_records();
        if records_ref.is_empty() {
            return true;
        }
        // Clone records out of the RefCell so we don't hold the
        // borrow across the canvas closure invocation (closures can
        // reach back into the scheduler / state and we don't want
        // to deadlock the renderer's bookkeeping).
        let mut sorted: Vec<blinc_layout::renderer::CanvasPaintRecord> =
            records_ref.values().cloned().collect();
        drop(records_ref);
        sorted.sort_by_key(|r| r.primitive_range.start);

        for record in &sorted {
            // Scratch context. Width / height match the real frame
            // so any viewport-dependent calculations inside fill_*
            // (currently none, but defensively correct) get the
            // right reference frame.
            let mut scratch = GpuPaintContext::new(width as f32, height as f32);

            // Replay the transform stack. `push_transform` composes
            // with `Affine2D::IDENTITY` from the fresh stack, so
            // pushing the saved affine reproduces the exact affine
            // the walker had at canvas paint time.
            scratch.push_transform(Transform::Affine2D(Affine2D {
                elements: record.affine,
            }));
            scratch.push_opacity(record.opacity);
            scratch.set_z_layer(record.z_layer);

            let local_clip_rect = if record.clips_content {
                let clip_rect = Rect::new(0.0, 0.0, record.bounds_wh.0, record.bounds_wh.1);
                scratch.push_clip(blinc_core::layer::ClipShape::rect(clip_rect));
                Some(clip_rect)
            } else {
                None
            };
            let _ = local_clip_rect;

            let canvas_bounds = blinc_layout::canvas::CanvasBounds {
                x: 0.0,
                y: 0.0,
                width: record.bounds_wh.0,
                height: record.bounds_wh.1,
            };
            (record.render_fn)(&mut scratch as &mut dyn DrawContext, canvas_bounds);

            if record.clips_content {
                scratch.pop_clip();
            }

            // The scratch batch contains only the callback output to splice into the cache.
            let new_batch = scratch.take_batch();
            // This splice replaces primitives only; referenced rows need a full paint.
            if !new_batch.aux_data.is_empty()
                || new_batch.has_ordered_draws()
                || new_batch.has_foreground_content()
                || new_batch.has_foreground_layers()
            {
                return false;
            }
            let new_count = new_batch.primitives.len();
            let old_count = record.primitive_range.end - record.primitive_range.start;
            if new_count != old_count {
                // Splice would shift every subsequent range — bail.
                return false;
            }
            // Reject stale ranges after a tree rebuild.
            if record.primitive_range.end > batch.primitives.len() {
                return false;
            }
            for (i, p) in new_batch.primitives.into_iter().enumerate() {
                batch.primitives[record.primitive_range.start + i] = p;
            }
        }

        true
    }

    /// Collect independent canvas primitives for callers that do not consume other output
    /// channels.
    pub fn collect_canvas_overlay_primitives(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> Vec<blinc_gpu::primitives::GpuPrimitive> {
        let overlay = self.collect_canvas_overlay(tree, width, height);
        let mut primitives = overlay.primitives;
        let mut shift = 0.0;
        for mut batch in overlay.ordered_batches {
            Self::shift_aux_offsets(&mut batch.primitives, shift);
            shift += batch.aux_data.len() as f32;
            primitives.extend(batch.primitives);
        }
        primitives
    }

    /// Collect every independent canvas output channel in paint order.
    pub fn collect_canvas_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> CanvasOverlay {
        let records_ref = tree.canvas_paint_records();
        let records: Vec<blinc_layout::renderer::CanvasPaintRecord> =
            records_ref.values().cloned().collect();
        drop(records_ref);
        self.collect_canvas_overlay_from(records, width, height)
    }

    /// Collect canvases belonging to overlay subtrees for dispatch above their hosts.
    pub fn collect_overlay_canvas_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        width: u32,
        height: u32,
    ) -> CanvasOverlay {
        let records_ref = tree.overlay_canvas_paint_records();
        let records: Vec<blinc_layout::renderer::CanvasPaintRecord> =
            records_ref.values().cloned().collect();
        drop(records_ref);
        self.collect_canvas_overlay_from(records, width, height)
    }

    /// Replay the supplied canvas records into ordered output batches.
    pub(super) fn collect_canvas_overlay_from(
        &mut self,
        mut records: Vec<blinc_layout::renderer::CanvasPaintRecord>,
        width: u32,
        height: u32,
    ) -> CanvasOverlay {
        use blinc_core::{DrawContext, Rect, Transform, layer::Affine2D};
        use blinc_gpu::GpuPaintContext;

        if records.is_empty() {
            return CanvasOverlay::default();
        }

        // Image-only callbacks have no primitive cursor to break z ties.
        records.sort_by_key(|record| (record.z_layer, record.visit_seq));

        // Reuse the scratch context across callbacks and attach the text context for canvas
        // glyphs.
        let mut scratch = GpuPaintContext::new(width as f32, height as f32);
        scratch.set_text_context(&mut self.text_ctx);
        let mut overlay = CanvasOverlay::default();
        let mut batches = Vec::new();
        for record in records {
            // Apply screen-space ancestor clips before the canvas affine. The saved
            // intersection already includes the canvas clip.
            let mut pushed_ancestor_clip = false;
            if let Some([cx, cy, cw, ch]) = record.ancestor_clip_aabb {
                if cw > 0.0 && ch > 0.0 {
                    scratch.push_clip(blinc_core::layer::ClipShape::rect(Rect::new(
                        cx, cy, cw, ch,
                    )));
                    pushed_ancestor_clip = true;
                } else {
                    // Empty intersection — canvas is fully clipped
                    // out, emit no primitives this frame.
                    continue;
                }
            }

            scratch.push_transform(Transform::Affine2D(Affine2D {
                elements: record.affine,
            }));
            scratch.push_opacity(record.opacity);
            scratch.set_z_layer(record.z_layer);

            // Only push the canvas's own clip when we don't have an
            // ancestor snapshot (e.g. root-level canvas). When the
            // ancestor snapshot exists it already includes the
            // canvas's own clip.
            let push_local_clip = record.clips_content && record.ancestor_clip_aabb.is_none();
            if push_local_clip {
                scratch.push_clip(blinc_core::layer::ClipShape::rect(Rect::new(
                    0.0,
                    0.0,
                    record.bounds_wh.0,
                    record.bounds_wh.1,
                )));
            }
            let canvas_bounds = blinc_layout::canvas::CanvasBounds {
                x: 0.0,
                y: 0.0,
                width: record.bounds_wh.0,
                height: record.bounds_wh.1,
            };
            (record.render_fn)(&mut scratch as &mut dyn DrawContext, canvas_bounds);
            if push_local_clip {
                scratch.pop_clip();
            }
            // Restore the stacks explicitly; draining the batch does not reset them.
            scratch.pop_transform();
            scratch.pop_opacity();
            if pushed_ancestor_clip {
                scratch.pop_clip();
            }

            // Drain every output channel before reusing the scratch context.
            batches.push(scratch.take_batch());

            overlay.meshes.extend(scratch.take_pending_meshes());
            overlay.gpu_passes.extend(scratch.take_pending_gpu_passes());
        }
        overlay.finish_batches(batches);
        overlay
    }

    /// Replay independently dispatched motion regions without replacing their saved ambient
    /// state.
    pub(super) fn collect_dynamic_region_primitives(
        &self,
        tree: &blinc_layout::RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
    ) -> blinc_gpu::PrimitiveBatch {
        use blinc_core::DrawContext;
        use blinc_gpu::{GpuPaintContext, PrimitiveBatch};

        let regions = tree.dynamic_regions();
        if regions.is_empty() {
            return PrimitiveBatch::new();
        }

        // Reuse one scratch context; each region restores its ambient stacks.
        let mut scratch = GpuPaintContext::new(width as f32, height as f32);

        // Replay motion regions in paint order. Canvas and CSS regions have separate
        // dispatch paths; rewalking them here would overwrite their saved affine with an
        // extra DPI transform.
        let mut ordered: Vec<_> = regions
            .values()
            .filter(|r| {
                !matches!(
                    r.kind,
                    blinc_layout::renderer::DynamicKind::CssAnimated { .. }
                        | blinc_layout::renderer::DynamicKind::Canvas { .. }
                )
            })
            .cloned()
            .collect();
        drop(regions);
        if ordered.is_empty() {
            return PrimitiveBatch::new();
        }
        ordered.sort_by(|a, b| {
            a.ambient
                .z_layer
                .cmp(&b.ambient.z_layer)
                .then_with(|| a.root.cmp(&b.root))
        });

        // Keep the main walk's region metadata; scratch coordinates already include its DPI
        // transform.
        tree.set_region_capture_suppressed(true);
        for region in &ordered {
            // The ambient affine already maps logical coordinates to physical pixels. Route
            // the replay into the dynamic owner without another DPI transform.
            scratch.push_motion_subtree();
            tree.render_dynamic_region(&mut scratch as &mut dyn DrawContext, region, render_state);
            scratch.pop_motion_subtree();
        }
        tree.set_region_capture_suppressed(false);

        // Only dynamic output is expected. Static output would reference a different
        // auxiliary buffer.
        let out = scratch.take_dynamic_batch();
        debug_assert!(
            scratch.take_batch().primitives.is_empty(),
            "collect_dynamic_region_primitives: scratch.batch should be empty — \
             region re-walk emitted outside its motion-subtree gate"
        );
        out
    }
}
