//! Patch or re-emit cached CSS animation content.
use super::{
    CssDeltaResult, DrawContext, GpuPaintContext, GpuPrimitive, LayoutNodeId, PrimitiveBatch,
    ReemitSnapshot, RenderContext, RenderState, RenderTree,
};

impl RenderContext {
    /// Repaint a CSS subtree into a background scratch batch; reject output that cannot be
    /// spliced safely.
    pub(super) fn reemit_css_subtree_bg(
        &self,
        tree: &blinc_layout::RenderTree,
        render_state: &blinc_layout::RenderState,
        node_id: LayoutNodeId,
        width: u32,
        height: u32,
    ) -> Option<blinc_gpu::PrimitiveBatch> {
        use blinc_core::DrawContext;
        use blinc_gpu::GpuPaintContext;

        let regions = tree.dynamic_regions();
        let region = regions.get(&node_id)?.clone();
        drop(regions);

        if !matches!(
            region.kind,
            blinc_layout::renderer::DynamicKind::CssAnimated { .. }
        ) {
            return None;
        }

        let mut scratch = GpuPaintContext::new(width as f32, height as f32);

        // DPI scale mirrors what `collect_dynamic_region_primitives`
        // does — ambient affine is physical-pixel-space; logical
        // walker translates need wrapping by the scale.
        let scale = tree.scale_factor();
        let has_scale = scale != 1.0;
        if has_scale {
            scratch.push_transform(blinc_core::Transform::scale(scale, scale));
        }

        // Keep CSS re-emission in the background owner so its text ordering remains intact.
        tree.render_dynamic_region(&mut scratch as &mut dyn DrawContext, &region, render_state);

        if has_scale {
            scratch.pop_transform();
        }

        let new_batch = scratch.take_batch();

        // Only primitive-vector splices are supported. Other channels require coordinated
        // index rebasing.
        if !new_batch.layer_commands.is_empty()
            || !new_batch.foreground_primitives.is_empty()
            || !new_batch.glass_primitives.is_empty()
            || !new_batch.nested_glass_primitives.is_empty()
            || !new_batch.paths.vertices.is_empty()
            || !new_batch.foreground_paths.vertices.is_empty()
            || !new_batch.foreground_dynamic_images.is_empty()
            || !new_batch.glyphs.is_empty()
            || !new_batch.viewports_3d.is_empty()
            || !new_batch.particle_viewports.is_empty()
            || !new_batch.aux_data.is_empty()
            || !new_batch.dynamic_images.is_empty()
        {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                "reemit_css_subtree_complex_batch_bail",
            );
            return None;
        }

        Some(new_batch)
    }

    /// Whether cached glyphs overlap any subtree bounds scheduled for re-emission.
    pub(super) fn glyphs_intersect_any_aabb(&self, snapshots: &[ReemitSnapshot]) -> bool {
        let aabbs: Vec<[f32; 4]> = snapshots.iter().filter_map(|(_, _, aabb)| *aabb).collect();
        if aabbs.is_empty() {
            return false;
        }
        let intersects = |g_bounds: [f32; 4], a: [f32; 4]| -> bool {
            let (gx, gy, gw, gh) = (g_bounds[0], g_bounds[1], g_bounds[2], g_bounds[3]);
            let (ax, ay, aw, ah) = (a[0], a[1], a[2], a[3]);
            !(gx + gw <= ax || gy + gh <= ay || gx >= ax + aw || gy >= ay + ah)
        };
        if let Some(by_layer) = self.cached_glyphs_by_layer.as_ref() {
            for glyphs in by_layer.values() {
                for bounds in glyphs.bounds() {
                    for a in &aabbs {
                        if intersects(bounds, *a) {
                            return true;
                        }
                    }
                }
            }
        }
        if let Some(fg) = self.cached_fg_glyphs.as_ref() {
            for bounds in fg.bounds() {
                for a in &aabbs {
                    if intersects(bounds, *a) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Re-emit CSS subtrees and replace their cached primitive ranges. Reject incompatible
    /// output or text that requires reshaping.
    pub(super) fn try_reemit_and_splice_css_subtrees(
        &mut self,
        tree: &blinc_layout::RenderTree,
        render_state: &blinc_layout::RenderState,
        reemit_nodes: &[LayoutNodeId],
        width: u32,
        height: u32,
    ) -> bool {
        if reemit_nodes.is_empty() {
            return true;
        }

        // Snapshot cached ranges and bounds before the walker replaces them with scratch
        // indices.
        let snapshots: Vec<ReemitSnapshot> = {
            let records = tree.css_anim_paint_records();
            reemit_nodes
                .iter()
                .filter_map(|&n| {
                    records
                        .get(&n)
                        .map(|m| (n, m.primitive_range.clone(), m.last_screen_aabb))
                })
                .collect()
        };
        if snapshots.is_empty() {
            return true;
        }

        // Repaint the full tree when cached glyphs intersect the subtree; this splice
        // cannot reshape text.
        let any_text_in_reemit = self.glyphs_intersect_any_aabb(&snapshots);
        if any_text_in_reemit {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                "reemit_text_intersect_bail",
            );
            return false;
        }

        // Re-emit each subtree, rejecting output that cannot be safely spliced.
        let mut new_prims: Vec<(
            LayoutNodeId,
            std::ops::Range<usize>,
            Vec<blinc_gpu::primitives::GpuPrimitive>,
        )> = Vec::with_capacity(snapshots.len());
        for (node, old_range, _aabb) in snapshots {
            match self.reemit_css_subtree_bg(tree, render_state, node, width, height) {
                Some(batch) => new_prims.push((node, old_range, batch.primitives)),
                None => return false,
            }
        }

        // Step 3: splice in reverse order of old_range.start. This
        // keeps each splice from invalidating earlier (smaller-start)
        // splices' indices.
        new_prims.sort_by_key(|entry| std::cmp::Reverse(entry.1.start));

        let Some(batch) = self.cached_bg_batch.as_mut() else {
            return false;
        };
        let mut records = tree.css_anim_paint_records_mut();
        for (node, old_range, prims) in new_prims {
            let old_len = old_range.end - old_range.start;
            let new_len = prims.len();
            let delta = new_len as isize - old_len as isize;
            batch.primitives.splice(old_range.clone(), prims);

            // Restore the record's cache range and damage both its old and new bounds.
            if let Some(meta) = records.get_mut(&node) {
                meta.primitive_range = old_range.start..(old_range.start + new_len);
                if let Some(rect) = meta.last_screen_aabb {
                    self.last_css_damage_rects.push(rect);
                }
            }

            // Shift every other record's range when its start is at
            // or past `old_range.end` — i.e., it lived in the
            // cache *after* the spliced region.
            if delta != 0 {
                for (other_node, other_meta) in records.iter_mut() {
                    if *other_node == node {
                        continue;
                    }
                    if other_meta.primitive_range.start >= old_range.end {
                        let new_start =
                            (other_meta.primitive_range.start as isize + delta) as usize;
                        let new_end = (other_meta.primitive_range.end as isize + delta) as usize;
                        other_meta.primitive_range = new_start..new_end;
                    }
                }
            }
        }

        true
    }

    /// Rebase primitive auxiliary references when concatenating batches.
    pub(super) fn shift_aux_offsets(prims: &mut [GpuPrimitive], shift: f32) {
        for prim in prims {
            prim.rebase_aux(shift);
        }
    }

    /// Patch supported CSS properties and collect damage bounds. Return records needing
    /// re-emission or request a full paint.
    pub fn apply_css_deltas(
        &mut self,
        tree: &blinc_layout::RenderTree,
        _scale: f32,
    ) -> CssDeltaResult {
        if [
            self.cached_bg_batch.as_ref(),
            self.cached_dynamic_batch.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|batch| {
            batch.has_ordered_draws()
                || batch.has_foreground_content()
                || batch.has_foreground_layers()
        }) {
            self.last_css_damage_rects.clear();
            return CssDeltaResult::Bail;
        }
        // CSS patching and re-emission index the static cache. Keep foreign
        // motion and scratch ranges out of that index space.
        if tree
            .css_anim_paint_records()
            .values()
            .any(|meta| meta.dynamic_batch || meta.composite_batch || meta.foreground_batch)
        {
            self.last_css_damage_rects.clear();
            return CssDeltaResult::Bail;
        }
        // CSS-animated primitives live on `cached_bg_batch` (the
        // walker doesn't push a `motion_subtree` for CSS animations
        // — that routing is motion-binding only). Patch through the
        // bg batch.
        let batch = match self.cached_bg_batch.as_mut() {
            Some(b) => b,
            None => {
                self.last_css_damage_rects.clear();
                return CssDeltaResult::Bail;
            }
        };
        let mut records = tree.css_anim_paint_records_mut();
        if records.is_empty() {
            // No CSS-animated nodes recorded at last paint. Nothing
            // to patch — caller re-uses the cache as-is.
            self.last_css_damage_rects.clear();
            return CssDeltaResult::Patched;
        }
        let store_arc = tree.css_anim_store();
        let store = match store_arc.lock() {
            Ok(g) => g,
            Err(_) => {
                self.last_css_damage_rects.clear();
                return CssDeltaResult::Bail;
            }
        };

        // Track whether any record changed.
        let mut any_changed = false;
        // Collect bounds for changed records; the caller discards them if patching fails.
        let mut damage_rects: Vec<[f32; 4]> = Vec::new();
        // Defer bail handling until record and batch borrows are released.
        let mut bail = false;
        // Re-emit records with unsupported properties and include their old bounds in cache
        // damage.
        let mut reemit_nodes: Vec<LayoutNodeId> = Vec::new();

        for (&node, meta) in records.iter_mut() {
            // Per-record patch tracking — distinct from `any_changed`
            // which spans the whole loop. Only push a damage rect
            // when at least one property on THIS record moved.
            let mut record_changed = false;
            // Prefer animation values over transition values, matching the full paint path.
            let active_anim = store
                .animations
                .get(&meta.stable_id)
                .filter(|a| a.is_playing);
            let active_trans = store
                .transitions
                .get(&meta.stable_id)
                .filter(|t| t.is_playing);

            let props_anim = active_anim.map(|a| &a.current_properties);
            let props_trans = active_trans.map(|t| &t.current_properties);

            // Bail if either source targets a property we can't
            // patch in place. The slow path handles those correctly.
            let touches_out_of_scope = |p: &blinc_animation::KeyframeProperties| {
                p.clip_inset.is_some()
                    || p.clip_circle_radius.is_some()
                    || p.clip_ellipse_radii.is_some()
                    || p.filter_blur.is_some()
                    || p.width.is_some()
                    || p.height.is_some()
                    || p.min_width.is_some()
                    || p.max_width.is_some()
                    || p.min_height.is_some()
                    || p.max_height.is_some()
                    || p.padding.is_some()
                    || p.margin.is_some()
                    || p.gap.is_some()
                    || p.backdrop_blur.is_some()
                    || p.backdrop_saturation.is_some()
                    || p.backdrop_brightness.is_some()
            };
            if props_anim.is_some_and(touches_out_of_scope)
                || props_trans.is_some_and(touches_out_of_scope)
            {
                // Re-emit all properties together for this record; patching it first would
                // be overwritten.
                reemit_nodes.push(node);
                if let Some(rect) = meta.last_screen_aabb {
                    damage_rects.push(rect);
                }
                continue;
            }

            // Helper: read a property from animation first, else
            // transition, else default to `None` (no change).
            let read_f32 = |get: fn(&blinc_animation::KeyframeProperties) -> Option<f32>| {
                props_anim
                    .and_then(get)
                    .or_else(|| props_trans.and_then(get))
            };
            let read_arr = |get: fn(&blinc_animation::KeyframeProperties) -> Option<[f32; 4]>| {
                props_anim
                    .and_then(get)
                    .or_else(|| props_trans.and_then(get))
            };

            // ---- opacity ----
            if let Some(new_opacity) = read_f32(|p| p.opacity) {
                if (new_opacity - meta.last_opacity).abs() > f32::EPSILON {
                    if meta.last_opacity.abs() < f32::EPSILON {
                        // Divide-by-zero — primitives were emitted
                        // at zero alpha last paint; can't recover
                        // the original color via ratio. Slow path.
                        bail = true;
                        break;
                    }
                    if let Some(push_idx) = meta.layer_push_index {
                        use blinc_gpu::primitives::LayerCommand;
                        match batch.layer_commands.get_mut(push_idx) {
                            Some(entry) => {
                                if let LayerCommand::Push { config } = &mut entry.command {
                                    config.opacity = new_opacity;
                                } else {
                                    bail = true;
                                    break;
                                }
                            }
                            None => {
                                bail = true;
                                break;
                            }
                        }
                    } else if let Some(prims) =
                        batch.primitives.get_mut(meta.primitive_range.clone())
                    {
                        let ratio = new_opacity / meta.last_opacity;
                        for p in prims.iter_mut() {
                            p.color[3] *= ratio;
                            p.color2[3] *= ratio;
                            p.border_color[3] *= ratio;
                            p.shadow_color[3] *= ratio;
                        }
                    }
                    meta.last_opacity = new_opacity;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- background_color ----
            // Matching-based: only patches primitives whose `color`
            // matches `meta.last_background_color` exactly, so a
            // child with its own bg stays untouched.
            if let Some(new_bg) = read_arr(|p| p.background_color) {
                if let Some(last_bg) = meta.last_background_color {
                    if new_bg != last_bg {
                        if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone())
                        {
                            for p in prims.iter_mut() {
                                if p.color == last_bg {
                                    p.color = new_bg;
                                }
                            }
                        }
                        meta.last_background_color = Some(new_bg);
                        any_changed = true;
                        record_changed = true;
                    }
                }
            }

            // ---- border_color ----
            if let Some(new_bc) = read_arr(|p| p.border_color) {
                if let Some(last_bc) = meta.last_border_color {
                    if new_bc != last_bc {
                        if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone())
                        {
                            for p in prims.iter_mut() {
                                if p.border_color == last_bc {
                                    p.border_color = new_bc;
                                }
                            }
                        }
                        meta.last_border_color = Some(new_bc);
                        any_changed = true;
                        record_changed = true;
                    }
                }
            }

            // ---- border_width ----
            if let Some(new_bw) = read_f32(|p| p.border_width) {
                if (new_bw - meta.last_border_width).abs() > f32::EPSILON {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if (p.border[0] - meta.last_border_width).abs() < f32::EPSILON {
                                p.border[0] = new_bw;
                            }
                        }
                    }
                    meta.last_border_width = new_bw;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- corner_radius ----
            if let Some(new_cr) = read_arr(|p| p.corner_radius) {
                if new_cr != meta.last_corner_radius {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if p.corner_radius == meta.last_corner_radius {
                                p.corner_radius = new_cr;
                            }
                        }
                    }
                    meta.last_corner_radius = new_cr;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- shadow_params ----
            if let Some(new_sp) = read_arr(|p| p.shadow_params) {
                if new_sp != meta.last_shadow_params {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if p.shadow == meta.last_shadow_params {
                                p.shadow = new_sp;
                            }
                        }
                    }
                    meta.last_shadow_params = new_sp;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- shadow_color ----
            if let Some(new_sc) = read_arr(|p| p.shadow_color) {
                if new_sc != meta.last_shadow_color {
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            if p.shadow_color == meta.last_shadow_color {
                                p.shadow_color = new_sc;
                            }
                        }
                    }
                    meta.last_shadow_color = new_sc;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- rotate_x (3D tilt) ----
            // perspective[0] = sin(rx), perspective[1] = cos(rx)
            if let Some(new_rx_deg) = read_f32(|p| p.rotate_x) {
                let new_rx_rad = new_rx_deg.to_radians();
                if (new_rx_rad - meta.last_rotate_x_rad).abs() > f32::EPSILON {
                    let sin_rx = new_rx_rad.sin();
                    let cos_rx = new_rx_rad.cos();
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            p.perspective[0] = sin_rx;
                            p.perspective[1] = cos_rx;
                        }
                    }
                    meta.last_rotate_x_rad = new_rx_rad;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- rotate_y (3D turn) ----
            // rotation[2] = sin(ry), rotation[3] = cos(ry)
            if let Some(new_ry_deg) = read_f32(|p| p.rotate_y) {
                let new_ry_rad = new_ry_deg.to_radians();
                if (new_ry_rad - meta.last_rotate_y_rad).abs() > f32::EPSILON {
                    let sin_ry = new_ry_rad.sin();
                    let cos_ry = new_ry_rad.cos();
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            p.rotation[2] = sin_ry;
                            p.rotation[3] = cos_ry;
                        }
                    }
                    meta.last_rotate_y_rad = new_ry_rad;
                    any_changed = true;
                    record_changed = true;
                }
            }

            // ---- filter values ----
            // filter_a = (grayscale, invert, sepia, hue_rotate_rad)
            // filter_b = (brightness, contrast, saturate, 0)
            let new_fa = [
                read_f32(|p| p.filter_grayscale).unwrap_or(meta.last_filter_a[0]),
                read_f32(|p| p.filter_invert).unwrap_or(meta.last_filter_a[1]),
                read_f32(|p| p.filter_sepia).unwrap_or(meta.last_filter_a[2]),
                read_f32(|p| p.filter_hue_rotate)
                    .map(|d| d.to_radians())
                    .unwrap_or(meta.last_filter_a[3]),
            ];
            let new_fb = [
                read_f32(|p| p.filter_brightness).unwrap_or(meta.last_filter_b[0]),
                read_f32(|p| p.filter_contrast).unwrap_or(meta.last_filter_b[1]),
                read_f32(|p| p.filter_saturate).unwrap_or(meta.last_filter_b[2]),
                0.0,
            ];
            if new_fa != meta.last_filter_a || new_fb != meta.last_filter_b {
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        p.filter_a = new_fa;
                        p.filter_b = new_fb;
                    }
                }
                meta.last_filter_a = new_fa;
                meta.last_filter_b = new_fb;
                any_changed = true;
                record_changed = true;
            }

            // Visual-only changes keep the recorded bounds. Records without bounds
            // contribute no damage.
            if record_changed {
                if let Some(rect) = meta.last_screen_aabb {
                    damage_rects.push(rect);
                }
            }
        }

        // Mirror `apply_binding_deltas`: drop the lock + the
        // record borrow before the caller composes the frame.
        let _ = any_changed;
        drop(store);
        drop(records);
        if bail {
            // Discard damage metadata after a partial failure; the full paint will replace
            // the cache.
            self.last_css_damage_rects.clear();
            return CssDeltaResult::Bail;
        }
        self.last_css_damage_rects = damage_rects;
        if reemit_nodes.is_empty() {
            CssDeltaResult::Patched
        } else {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                count = reemit_nodes.len(),
                "apply_css_deltas_reemit_pending",
            );
            CssDeltaResult::PatchedWithReemits { reemit_nodes }
        }
    }
}
