//! Batch routing for motion subtrees and composite layers, and the active-batch accessors.

use super::{AuxBatch, GpuPaintContext};
use crate::primitives::PrimitiveBatch;
use blinc_core::ClipShape;

impl GpuPaintContext<'_> {
    /// Mark that the walker has entered a motion-bound subtree.
    /// Subsequent emit calls route into `dynamic_batch` unless an open
    /// static group retains them for joint composition, until the
    /// matching `pop_motion_subtree`. Nested motion subtrees track
    /// via a depth counter — only the outermost pop returns to
    /// static-batch emission.
    pub fn push_motion_subtree(&mut self) {
        self.motion_subtree_depth = self.motion_subtree_depth.saturating_add(1);
        if self.static_motion_depth.is_none() && self.has_static_group() {
            self.static_motion_depth = Some(self.motion_subtree_depth);
        }
    }

    /// Pair with [`Self::push_motion_subtree`]. No-op when depth
    /// is already zero (defensive — should never happen in
    /// well-formed walker traversal but avoids underflow if a
    /// future caller forgets balance).
    pub fn pop_motion_subtree(&mut self) {
        if self.static_motion_depth == Some(self.motion_subtree_depth) {
            self.static_motion_depth = None;
        }
        self.motion_subtree_depth = self.motion_subtree_depth.saturating_sub(1);
    }

    pub(super) fn has_static_group(&self) -> bool {
        self.layer_stack
            .iter()
            .any(|state| state.command_batch == AuxBatch::Static)
    }

    /// Whether the walker is inside a motion-bound subtree. An ancestor
    /// group may retain its emission in the static batch.
    #[inline]
    pub fn in_motion_subtree(&self) -> bool {
        self.motion_subtree_depth > 0
    }

    /// Begin routing subsequent emit calls into the per-node scratch
    /// batch for `node_id` instead of the bg batch.
    ///
    /// Used by the walker to capture a composite-promotable CSS
    /// subtree's primitives into its own buffer — at end of paint
    /// the buffer is rasterized into a `LayerTexture` and the
    /// per-frame composite blits that texture with the current
    /// animation transform applied. The bg batch never sees these
    /// primitives, so the static cache stays unchanged across
    /// animation ticks.
    ///
    /// Nested composite layers are not supported; the second push
    /// silently keeps routing to the outermost layer. Callers must
    /// pair each push with [`Self::pop_composite_layer`].
    pub fn push_composite_layer(&mut self, node_id: u64) {
        // Snapshot the clip-stack depth at promotion time. Any clip
        // the SUBTREE ITSELF pushes (e.g. an inner overflow_clip on
        // a child) stacks on top of this base and IS baked into the
        // texture; ancestor clips below the OUTERMOST `clip_base`
        // are stripped during emission and re-applied at blit time
        // via the motion-overlay scissor. See [`Self::get_clip_data`]
        // for the strip logic.
        //
        // We push the clip-base unconditionally so push/pop are
        // balanced, even for nested promotions. The active scratch
        // routing target (`active_composite_layer`) only updates
        // for the OUTERMOST push — nested promoted nodes still emit
        // into the outer's scratch batch, matching the
        // "no-nesting" routing contract while keeping clip-base
        // bookkeeping correct.
        self.composite_layer_clip_base.push(self.clip_stack.len());
        if self.active_composite_layer.is_none() {
            self.active_composite_layer = Some(node_id);
            // Pre-create the entry so emit sites see a stable
            // mutable handle and don't pay a hashmap lookup on
            // every primitive.
            self.composite_layer_batches.entry(node_id).or_default();
        }
    }

    /// Pair with [`Self::push_composite_layer`]. Pops the innermost
    /// clip-base snapshot; when that was the outermost push, also
    /// clears the active scratch routing target.
    pub fn pop_composite_layer(&mut self) {
        self.composite_layer_clip_base.pop();
        if self.composite_layer_clip_base.is_empty() {
            self.active_composite_layer = None;
        }
    }

    /// Return the AABB of the ancestor clips that were stripped
    /// from primitive emission while a composite layer was active.
    /// Returns `None` when no composite layer ever pushed in this
    /// paint, or when the layer was pushed at clip depth 0 (no
    /// ancestor clips to strip).
    ///
    /// Called from the per-frame motion overlay AFTER pop: the saved
    /// `composite_layer_clip_base` is consumed by the walker's record
    /// site (which copies it onto the `DynamicRegion`); this getter
    /// answers the snapshot itself for callers that need to peek mid-
    /// paint without copying the stack.
    pub fn ambient_clip_aabb(&self, clip_base: usize) -> Option<[f32; 4]> {
        self.ambient_clip_rounded(clip_base).map(|(aabb, _)| aabb)
    }

    /// Capture the ambient clip as both its intersected AABB AND the
    /// topmost rounded-rect corner radius if one exists in the
    /// ancestor stack. The compositor overlay uses this to drive the
    /// blit shader's rounded-rect scissor — without the radius, a
    /// motion-bound subtree blitted across a rounded ancestor (the
    /// progress indicator inside an overflow_clip-rounded track, the
    /// switch thumb inside a rounded pill) gets its corners squared
    /// off by an AABB scissor.
    ///
    /// Returns `(aabb_xywh, corner_radius_tl_tr_br_bl)`; clips that do not overlap give a
    /// zero-size AABB, which hides everything. The radius
    /// is `[0; 4]` when no rounded-rect clip is present in the
    /// ancestor chain (plain rects, polygons, no clip) — semantically
    /// "square corners," which the blit shader handles as a normal
    /// AABB scissor.
    ///
    /// For multiple stacked rounded-rect clips the TOPMOST one wins
    /// — same heuristic `get_clip_data` uses for its scissor's corner
    /// radius. Robust handling of arbitrary intersections is
    /// follow-up work; the topmost-wins fallback matches what users
    /// see when the bake path is OFF.
    pub fn ambient_clip_rounded(&self, clip_base: usize) -> Option<([f32; 4], [f32; 4])> {
        if clip_base == 0 {
            return None;
        }
        let mut min_x = f32::NEG_INFINITY;
        let mut min_y = f32::NEG_INFINITY;
        let mut max_x = f32::INFINITY;
        let mut max_y = f32::INFINITY;
        let mut any = false;
        // Walk top-down so the LAST RoundedRect we see is the
        // outermost — but we want the TOPMOST (innermost), which is
        // the LAST entry in the iteration order. Track the most-
        // recent rounded-rect we've seen and let later iterations
        // overwrite it.
        let mut topmost_rounded: Option<([f32; 4], [f32; 4])> = None;
        for (shape, _, _) in &self.clip_stack[..clip_base] {
            let (x0, y0, x1, y1) = match shape {
                ClipShape::Rect(r) => (r.x(), r.y(), r.x() + r.width(), r.y() + r.height()),
                ClipShape::RoundedRect {
                    rect,
                    corner_radius,
                    ..
                } => {
                    let bounds = [rect.x(), rect.y(), rect.width(), rect.height()];
                    let radii = [
                        corner_radius.top_left,
                        corner_radius.top_right,
                        corner_radius.bottom_right,
                        corner_radius.bottom_left,
                    ];
                    topmost_rounded = Some((bounds, radii));
                    (
                        rect.x(),
                        rect.y(),
                        rect.x() + rect.width(),
                        rect.y() + rect.height(),
                    )
                }
                _ => continue,
            };
            min_x = min_x.max(x0);
            min_y = min_y.max(y0);
            max_x = max_x.min(x1);
            max_y = max_y.min(y1);
            any = true;
        }
        if !any {
            return None;
        }
        // Clips that do not overlap leave nothing visible: a zero-size rect, which callers skip,
        // and not `None`, which means no clip and would draw everything.
        if max_x <= min_x || max_y <= min_y {
            return Some(([min_x, min_y, 0.0, 0.0], [0.0; 4]));
        }
        let aabb = [min_x, min_y, max_x - min_x, max_y - min_y];
        // If a rounded-rect exists in the ancestor chain AND the
        // intersected AABB equals its bounds, return its radii. When
        // the intersection trims the rounded-rect's edges (a tighter
        // ancestor rect cuts into it), the rounded corners may no
        // longer be at the intersection's edge — fall back to square
        // scissor in that case. This matches `get_clip_data`'s
        // dominant-corner logic conservatively without re-implementing
        // it here.
        let radius = match topmost_rounded {
            Some((bounds, radii)) if aabb == bounds => radii,
            _ => [0.0; 4],
        };
        Some((aabb, radius))
    }

    /// Outermost active composite-layer clip-base snapshot — the
    /// ancestor-clip cutoff that applies for primitives emitted
    /// inside the outermost push. Used by the walker to record the
    /// ambient clip on `DynamicRegion::MotionSubtreeTexture` and by
    /// the strip-clip path in `get_clip_data`. Returns `None`
    /// outside a composite-layer scope.
    pub fn composite_layer_clip_base(&self) -> Option<usize> {
        self.composite_layer_clip_base.first().copied()
    }

    /// Drain the per-node composite-layer scratch batches. Called
    /// after the walker finishes; the compositor reads each batch's
    /// primitives + AABB to rasterize it into a `LayerTexture`.
    pub fn take_composite_layer_batches(
        &mut self,
    ) -> std::collections::HashMap<u64, PrimitiveBatch> {
        self.forget_composite_clip_offsets();
        std::mem::take(&mut self.composite_layer_batches)
    }

    /// The active batch for emission. While inside a composite-
    /// layer scope, points at the per-node scratch batch; otherwise
    /// while inside a motion-bound subtree, points at `dynamic_batch`;
    /// otherwise at `batch`. Internal helper used by every primitive /
    /// path / glass emit site so the routing is centralised.
    #[inline]
    pub(super) fn active_batch_mut(&mut self) -> &mut PrimitiveBatch {
        match self.aux_batch() {
            AuxBatch::Composite(node_id) => {
                self.composite_layer_batches.entry(node_id).or_default()
            }
            AuxBatch::Dynamic => &mut self.dynamic_batch,
            AuxBatch::Static => &mut self.batch,
        }
    }

    pub(super) fn layer_command_batch(&self) -> AuxBatch {
        self.aux_batch()
    }

    pub(super) fn layer_command_batch_mut(&mut self, owner: AuxBatch) -> &mut PrimitiveBatch {
        match owner {
            AuxBatch::Static => &mut self.batch,
            AuxBatch::Dynamic => &mut self.dynamic_batch,
            AuxBatch::Composite(node) => self.composite_layer_batches.entry(node).or_default(),
        }
    }

    /// Effective foreground routing for primitive emission.
    ///
    /// Honors `is_foreground` normally, but forces BACKGROUND routing
    /// while a composite layer is active. Inside a composite-promoted
    /// subtree the whole scratch batch is baked to one texture and
    /// blitted as a unit, so the global foreground/background split is
    /// meaningless there — and worse, routing to
    /// `foreground_primitives` strands those prims: the region bracket
    /// (`bg_primitive_count`), its AABB (`bg_primitive_aabb`) and the
    /// bake (`render_subtree_to_layer_texture`) all read only
    /// `.primitives`. A `.foreground()` panel with a CSS enter
    /// animation would otherwise emit its bg/border/shadow into
    /// `foreground_primitives`, record an empty region (end == start),
    /// never bake, and never blit — the exact select/combobox fade
    /// break. Collapsing to `.primitives` while promoted makes the
    /// existing single-list bracket + bake work unchanged.
    #[inline]
    pub(super) fn emit_foreground(&self) -> bool {
        self.is_foreground && self.active_composite_layer.is_none()
    }

    /// Immutable counterpart to [`Self::active_batch_mut`] — used
    /// by readers like `bg_primitive_count` and `bg_primitive_aabb`
    /// so they bracket the right batch.
    #[inline]
    pub(super) fn active_batch(&self) -> &PrimitiveBatch {
        match self.aux_batch() {
            AuxBatch::Composite(node_id) => {
                // If push happened, the entry must exist; fall back to
                // `batch` defensively so reads don't panic on an
                // unbalanced API call.
                self.composite_layer_batches
                    .get(&node_id)
                    .unwrap_or(&self.batch)
            }
            AuxBatch::Dynamic => &self.dynamic_batch,
            AuxBatch::Static => &self.batch,
        }
    }

    /// Drain the dynamic batch and return it. The caller (compositor
    /// fast path) dispatches it per frame on top of the static cache
    /// blit. Leaves `dynamic_batch` empty for the next paint pass.
    pub fn take_dynamic_batch(&mut self) -> PrimitiveBatch {
        self.forget_clip_offsets(AuxBatch::Dynamic);
        self.dynamic_batch.resolve_cross_stream_groups();
        self.rejected_cross_stream_groups.clear();
        self.overlay_ancestor_ids.clear();
        std::mem::take(&mut self.dynamic_batch)
    }

    pub(super) fn bg_primitive_aabb_impl(&self, start: usize, end: usize) -> Option<[f32; 4]> {
        let prims = &self.active_batch().primitives;
        if start >= end || end > prims.len() {
            return None;
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for p in &prims[start..end] {
            let [x, y, w, h] = p.bounds;
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            // Drop shadows render OUTSIDE `bounds`: the shadow vertex
            // shader expands the quad by `blur * 3 + |offset|` and the
            // fragment adds spread. Without padding here, the
            // composite-layer bake (which sizes its tight texture from
            // this AABB) crops the halo, so promoted subtrees lose
            // their box-shadow for the duration of the promotion.
            // 3 = Shadow, 5 = CircleShadow (inner shadows paint inside
            // `bounds` and need no padding).
            let expand = if p.type_info[0] == 3 || p.type_info[0] == 5 {
                p.shadow[2] * 3.0 + p.shadow[0].abs() + p.shadow[1].abs() + p.shadow[3].max(0.0)
            } else {
                0.0
            };
            min_x = min_x.min(x - expand);
            min_y = min_y.min(y - expand);
            max_x = max_x.max(x + w + expand);
            max_y = max_y.max(y + h + expand);
        }
        if min_x.is_finite() && max_x > min_x && max_y > min_y {
            Some([min_x, min_y, max_x - min_x, max_y - min_y])
        } else {
            None
        }
    }
}
