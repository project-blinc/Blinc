//! Motion-aware desktop paint surface.
//!
//! `render_layer_with_motion` is the production paint walker on
//! desktop / mobile — every Blinc app that calls
//! `WindowedContext::render_with_motion` lands here for the actual
//! draw calls. It folds together the layered-render strategy
//! (background / glass / foreground) with motion-pre-replay
//! (sample `motion_bindings` for transforms + opacity), CSS layer
//! effects (blur, drop shadow, mask gradient), 3D / clip-path
//! resolution, and viewport culling for scroll containers that
//! opted in.
//!
//! Two `pub` / `pub(crate)` methods:
//!
//! - `render_with_motion` — public entry point. Pulls the root and
//!   dispatches to `render_layer_with_motion` with zeroed parent
//!   offset and identity cumulative scroll.
//! - `render_layer_with_motion` — the recursive walker. ~1,300 LOC
//!   of layered painting, motion application, layer-effect push/pop,
//!   3D-SDF group collection, mask-gradient/clip-path resolution,
//!   per-node scroll offset accumulation, and viewport-cull gating.

use blinc_core::{
    BlendMode, Brush, ClipShape, Color, CornerRadius, DrawContext, GlassStyle, Gradient,
    LayerConfig, LayerEffect, Point, Rect, Shadow, Size, Stroke, Transform,
};

use crate::canvas::CanvasBounds;
use crate::element::{Material, RenderLayer};
use crate::tree::LayoutNodeId;

use super::super::{ElementType, RenderTree};

/// Nodes recorded as painted only because their absolute bounds could
/// not be resolved — the conservative branch in the viewport gate.
/// Non-zero at idle means the visibility filter is failing open.
use std::sync::atomic::Ordering;

/// Nodes recorded as painted only because their absolute bounds could
pub static UNRESOLVED_BOUNDS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// On-screen Y of the last animating node recorded as painted, and the
/// scroll offset used to compute it. If a scrolled-away spinner reports
/// a Y inside the viewport, the scroll term never reached the gate.
pub static LAST_ANIMATING_ON_SCREEN_Y: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(i64::MIN);
pub static LAST_ANIMATING_SCROLL_Y: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);

impl RenderTree {
    /// Render with motion animations from RenderState
    ///
    /// This method applies animated opacity, scale, and translation from motion
    /// animations stored in RenderState. Use this when you have elements wrapped
    /// in motion() containers.
    pub fn render_with_motion(
        &self,
        ctx: &mut dyn DrawContext,
        render_state: &crate::render_state::RenderState,
    ) {
        // Reset the visible-animation flag for this frame. Set inside
        // `render_layer_with_motion` whenever a node that drives a
        // per-frame redraw (Canvas, motion bindings, active motion
        // state) is actually painted. Read by callers via
        // `visible_anim_active()` after this returns to gate the
        // end-of-frame redraw chain.
        self.visible_anim_active.set(false);
        // Same lifecycle for the canvas-detection flag: cleared at the
        // top of every full paint, set by `render_layer_with_motion`
        // when a Canvas node intersects the viewport. The fast path
        // reads this between paints — a `true` value forces it to
        // bail back to the walker so the canvas's draw callback
        // actually runs.
        self.had_canvas_painted.set(false);
        // Same lifecycle for the painted-node set: cleared here, grown
        // by the walk, queried via `painted_node_ids()` to filter
        // animating Statefuls down to those whose node is actually on
        // screen this frame.
        self.painted_node_ids.borrow_mut().clear();
        // Same lifecycle for composite-binding metadata: cleared at
        // the top of every full paint, repopulated by
        // `render_layer_with_motion` for any painted node whose
        // motion bindings are mid-flight. The Phase-4 fast path
        // reads this between full paints to patch the cached
        // primitive buffer in place.
        self.composite_bindings.borrow_mut().clear();
        // Phase 4b: per-CSS-animated-node paint metadata. Same
        // lifecycle as composite_bindings — cleared every full
        // paint, repopulated below for every node whose
        // `current_animation_status` is `Animating(Css)`.
        self.css_anim_paint_records.borrow_mut().clear();
        // Canvas paint records have the same lifecycle: the walker
        // rebuilds them on every full paint, the fast path consumes
        // them between full paints.
        self.canvas_paint_records.borrow_mut().clear();
        // Sibling pool for canvases nested inside `is_overlay_root`
        // subtrees — dispatched AFTER the popover's dyn-batch SDF
        // pass so caret canvases survive above their overlay's bg.
        self.overlay_canvas_paint_records.borrow_mut().clear();
        // Compositor v2 dynamic-region map — populated by the walker
        // (Phase 2: in parallel with the legacy records above; Phase 3:
        // becomes the sole source). Cleared at the top of every full
        // paint with the same lifecycle as the legacy maps.
        //
        // `current_animation_status` is expected to be populated by
        // the caller (`try_render_with_compositor` always runs
        // `compute_animation_status` before invoking the walker).
        // If we got here through some other path with no statuses,
        // the walker still functions — it just won't produce any
        // `DynamicRegion` entries this frame, and the legacy paths
        // continue to handle emission as before.
        self.dynamic_regions.borrow_mut().clear();

        if let Some(root) = self.root {
            // Bracket the entire walk against any inner-path transform
            // leak. The slow walker runs every frame under
            // continuous_redraw; even a one-push-per-frame leak
            // compounds into exponential scale growth (canvas zoom-
            // out flicker observed when an inline editor auto-focused
            // its input — see the 06-14 canvas_viewport_diag log
            // showing saved_affine going 2 → 4 → 8 → 16 per frame).
            // Snapshot the depth now and truncate back at the end so
            // any push that escapes its matching pop is forcibly
            // discarded, contained to the frame it happened on.
            let initial_depth = ctx.transform_stack_depth();
            // Apply DPI scale factor if set (for HiDPI display support)
            let has_scale = self.scale_factor != 1.0;
            if has_scale {
                ctx.push_transform(Transform::scale(self.scale_factor, self.scale_factor));
            }

            // Skip Glass / Foreground passes when no node in the tree
            // is on those layers. Each walker pass costs a full tree
            // traversal (transform pushes / viewport-cull checks /
            // child recursion) even when it ends up emitting nothing,
            // so on a typical UI without glass material or
            // `.foreground()` nodes — most cn / styling examples —
            // two of the three passes were ~99 % wasted work.
            //
            // Scanning `render_nodes` once is O(N) but the check is
            // a single `matches!` per node, an order of magnitude
            // cheaper than the walker's per-node bounds lookups +
            // prop reads + stack ops. Net win is roughly 2 × walker
            // cost when the page has no glass / foreground content.
            let mut has_glass = false;
            let mut has_foreground = false;
            for (_, node) in self.render_nodes.iter() {
                if !has_glass
                    && matches!(
                        node.props.material,
                        Some(crate::element::Material::Glass(_))
                    )
                {
                    has_glass = true;
                }
                if !has_foreground && matches!(node.props.layer, RenderLayer::Foreground) {
                    has_foreground = true;
                }
                if has_glass && has_foreground {
                    break;
                }
            }

            // Pass 1: Background (primitives go to background batch)
            ctx.set_foreground_layer(false);
            self.render_layer_with_motion(
                ctx,
                root,
                (0.0, 0.0),
                RenderLayer::Background,
                0,     // glass_depth
                false, // inside_foreground
                render_state,
                1.0, // Start with full opacity at root
                (0.0, 0.0),
            );

            // Pass 2: Glass (primitives go to glass batch)
            if has_glass {
                self.render_layer_with_motion(
                    ctx,
                    root,
                    (0.0, 0.0),
                    RenderLayer::Glass,
                    0,     // glass_depth
                    false, // inside_foreground
                    render_state,
                    1.0, // Start with full opacity at root
                    (0.0, 0.0),
                );
            }

            // Pass 3: Foreground (primitives go to foreground batch, rendered after glass)
            if has_foreground {
                ctx.set_foreground_layer(true);
                self.render_layer_with_motion(
                    ctx,
                    root,
                    (0.0, 0.0),
                    RenderLayer::Foreground,
                    0,     // glass_depth
                    false, // inside_foreground
                    render_state,
                    1.0, // Start with full opacity at root
                    (0.0, 0.0),
                );
                ctx.set_foreground_layer(false);
            }

            // Pop the DPI scale transform
            if has_scale {
                ctx.pop_transform();
            }

            // Defensive: if any inner code path skipped a pop_transform
            // for any reason, truncate back to the depth at entry. See
            // the comment at `initial_depth` above for the failure mode
            // this contains (per-frame current_affine doubling).
            ctx.restore_transform_stack(initial_depth);
        }
    }

    /// Re-walk a single dynamic region's subtree onto `ctx`, replaying
    /// the saved ambient state captured during the last full paint.
    ///
    /// Used by the compositor's fast path on frames where the only
    /// thing that changed is a CSS animation / transition: instead of
    /// running the full walker against the whole tree (which is what
    /// `try_render_with_compositor` does today when `css_anim_active`
    /// flips the cache-invalidation gate), the compositor iterates
    /// `dynamic_regions` and calls this for each `CssAnimated` /
    /// `MotionSubtree` region. The static-cache texture is left
    /// untouched and only the dynamic batch is refreshed with fresh
    /// primitive values.
    ///
    /// The walker emits the region's primitives into `ctx`'s active
    /// batch — when caller pre-pushes `push_motion_subtree` the emit
    /// routes into `dynamic_batch`, exactly mirroring the main
    /// walker's behaviour at the same subtree root.
    ///
    /// Side-effect HashMaps on `RenderTree` (`composite_bindings`,
    /// `canvas_paint_records`, `painted_node_ids`, `dynamic_regions`)
    /// are re-written by the recursive walk. Each entry is idempotent
    /// — same key, same content modulo the freshened animation values
    /// — so re-invocation produces the same map state plus updated
    /// per-binding deltas. The compositor doesn't `clear()` these
    /// before re-walking; it relies on overwrite semantics.
    ///
    /// All three render layers (Background / Glass / Foreground) are
    /// walked in sequence; the caller's `ctx.set_foreground_layer`
    /// toggle is restored on return.
    #[allow(clippy::too_many_arguments)]
    pub fn render_dynamic_region(
        &self,
        ctx: &mut dyn DrawContext,
        region: &super::super::DynamicRegion,
        render_state: &crate::render_state::RenderState,
    ) {
        let ambient = &region.ambient;

        // Replay the ancestor clip stack BEFORE the affine — the same
        // ordering `collect_canvas_overlay` uses, so the clip rect
        // is kept in screen coords by pushing on the identity affine.
        let mut pushed_clip = false;
        if let Some([cx, cy, cw, ch]) = ambient.clip_aabb {
            if cw > 0.0 && ch > 0.0 {
                ctx.push_clip(blinc_core::layer::ClipShape::rect(blinc_core::Rect::new(
                    cx, cy, cw, ch,
                )));
                pushed_clip = true;
            } else {
                // Empty intersection — region is fully clipped out
                // this frame. Skip emission.
                return;
            }
        }

        // Same defensive bracket as render_with_motion — see that
        // function's comment for the failure mode (continuous_redraw
        // + walker leak = exponential canvas scale doubling).
        let initial_depth = ctx.transform_stack_depth();
        // Push the parent-environment affine that was active when the
        // walker reached the region's root last full paint.
        let parent_affine = blinc_core::layer::Affine2D {
            elements: ambient.affine,
        };
        ctx.push_transform(blinc_core::Transform::Affine2D(parent_affine));
        ctx.push_opacity(ambient.opacity);
        let saved_z = ctx.z_layer();
        ctx.set_z_layer(ambient.z_layer);

        // Background pass.
        ctx.set_foreground_layer(false);
        self.render_layer_with_motion(
            ctx,
            region.root,
            (0.0, 0.0),
            crate::element::RenderLayer::Background,
            0,
            false,
            render_state,
            1.0,
            (0.0, 0.0),
        );
        // Glass pass.
        self.render_layer_with_motion(
            ctx,
            region.root,
            (0.0, 0.0),
            crate::element::RenderLayer::Glass,
            0,
            false,
            render_state,
            1.0,
            (0.0, 0.0),
        );
        // Foreground pass.
        ctx.set_foreground_layer(true);
        self.render_layer_with_motion(
            ctx,
            region.root,
            (0.0, 0.0),
            crate::element::RenderLayer::Foreground,
            0,
            false,
            render_state,
            1.0,
            (0.0, 0.0),
        );

        // Tear down: leave the foreground flag in the same state we
        // entered with (the foreground pass set it true on the last
        // call). z-layer, opacity, transform, optional clip restored
        // in reverse order so the caller's paint context returns to
        // its pre-call state.
        ctx.set_foreground_layer(false);
        ctx.set_z_layer(saved_z);
        ctx.pop_opacity();
        ctx.pop_transform();
        if pushed_clip {
            ctx.pop_clip();
        }
        // Defensive: truncate any inner-path transform leak. Same
        // rationale as render_with_motion's bracket.
        ctx.restore_transform_stack(initial_depth);
    }

    /// Render a layer with motion animation support
    ///
    /// The `inherited_opacity` parameter allows parent motion containers to pass
    /// their opacity down to children, ensuring the entire motion group fades together.
    ///
    /// The `inside_foreground` parameter tracks whether we're inside a foreground element,
    /// ensuring all descendants of foreground elements also render in the foreground pass.
    #[allow(clippy::too_many_arguments)]
    fn render_layer_with_motion(
        &self,
        ctx: &mut dyn DrawContext,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        target_layer: RenderLayer,
        glass_depth: u32,
        inside_foreground: bool,
        render_state: &crate::render_state::RenderState,
        inherited_opacity: f32,
        cumulative_scroll: (f32, f32),
    ) {
        // Debug: uncomment to trace all nodes
        // eprintln!("render_layer_with_motion: visiting node {:?}, target_layer={:?}", node, target_layer);

        // Use animated bounds if a layout animation is active, otherwise use layout bounds
        let Some(bounds) = self.get_render_bounds(node, parent_offset) else {
            return;
        };

        // Check if this node has an active layout animation (for clipping children)
        // Need to check both node ID based and stable key based animations
        let has_layout_animation = self.is_layout_animating(node);

        let Some(render_node) = self.render_nodes.get(&node) else {
            tracing::trace!(
                "render_layer_with_motion: no render_node for {:?}, skipping",
                node
            );
            // eprintln!("render_layer_with_motion: no render_node for {:?}", node);
            return;
        };

        // Check if this node should be skipped (motion removed)
        // For stable-keyed motions, check by key; for node-based, check by node_id
        let motion_removed = if let Some(ref stable_key) = render_node.props.motion_stable_id {
            render_state.is_stable_motion_removed(stable_key)
        } else {
            render_state.is_motion_removed(node)
        };
        if motion_removed {
            return;
        }

        // CSS visibility: hidden — skip rendering but preserve layout space
        if !render_node.props.visible {
            return;
        }

        // Past every cull/skip gate above — this node is being painted
        // this frame. Record it so the windowed app can intersect with
        // animating Statefuls / CSS animations and skip the redraw
        // chain when their node is off-screen.
        //
        // We additionally clip the recorded set against the window
        // viewport: scroll containers without `viewport_cull(true)`
        // still walk and paint their off-screen children (the GPU
        // clips them at draw time), but for redraw-gating purposes
        // those children are NOT visible. Without this filter the
        // styling_demo — which has ~25 `infinite` keyframes laid out
        // far below the fold — kept the redraw chain alive at idle
        // even though the user couldn't see any of them.
        //
        // We MUST use absolute bounds here (`get_absolute_bounds`),
        // not the `bounds` variable above. `bounds` comes from
        // `get_render_bounds(node, parent_offset)` which the recursion
        // calls with `parent_offset = (0, 0)` — the parent's actual
        // offset is captured in the draw context's transform stack,
        // not in the bounds value. Comparing parent-relative bounds
        // against the absolute window viewport produced false
        // negatives for nested elements (an `#anim-pulse` deep inside
        // a section was excluded even when visually on screen),
        // breaking every keyframe animation.
        //
        // If the viewport hasn't been initialised yet (rect is empty
        // — true on the very first frame, before
        // `RenderState::set_viewport_size` is called) we fall back to
        // recording every painted node. Otherwise the gate would
        // filter the entire tree out and the chain would never start.
        let viewport = render_state.viewport();
        let viewport_known = viewport.width() > 0.0 && viewport.height() > 0.0;
        let intersects_viewport = !viewport_known
            || match self.layout_tree.get_absolute_bounds(node) {
                Some(abs) => {
                    let on_screen_x = abs.x + cumulative_scroll.0;
                    let on_screen_y = abs.y + cumulative_scroll.1;
                    on_screen_x < viewport.x() + viewport.width()
                        && on_screen_x + abs.width > viewport.x()
                        && on_screen_y < viewport.y() + viewport.height()
                        && on_screen_y + abs.height > viewport.y()
                }
                // No absolute bounds resolved — conservatively include
                // the node rather than filtering it out. Same posture
                // as the `viewport_known == false` branch above.
                //
                // Counted: this fails OPEN, so a node whose bounds
                // never resolve is permanently "on screen" and any
                // animation on it pins the redraw chain regardless of
                // where the user has scrolled. If this count is
                // non-zero at idle, that is the culling bypass.
                None => {
                    UNRESOLVED_BOUNDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    true
                }
            };
        if intersects_viewport {
            self.painted_node_ids.borrow_mut().insert(node);
            // Record the on-screen Y of any node whose motion bindings
            // are mid-flight. Bounds resolve for every DSL node
            // (measured), so if an animating node is being recorded as
            // painted while scrolled away, the scroll term is what is
            // wrong — this reports the value actually used.
            if self
                .motion_bindings
                .get(&node)
                .is_some_and(|b| b.is_any_animating())
                && let Some(abs) = self.layout_tree.get_absolute_bounds(node)
            {
                LAST_ANIMATING_ON_SCREEN_Y
                    .store((abs.y + cumulative_scroll.1) as i64, Ordering::Relaxed);
                LAST_ANIMATING_SCROLL_Y.store(cumulative_scroll.1 as i64, Ordering::Relaxed);
            }
        }

        // Get motion values from RenderState (for entry/exit animations)
        // For stable-keyed motions (overlays), look up by key; otherwise by node_id
        let motion_values = if let Some(ref stable_key) = render_node.props.motion_stable_id {
            render_state.get_stable_motion_values(stable_key)
        } else {
            render_state.get_motion_values(node)
        };

        // Get motion bindings from RenderTree (for continuous AnimatedValue animations).
        //
        // Single HashMap lookup, then field-level queries on the reference.
        // Previously each of `get_motion_transform/opacity/scale/rotation`
        // did its own `motion_bindings.get(&node)` — for the ~95% of
        // nodes without bindings we paid 4 lookups every render pass to
        // get four `None`s. The `and_then` chains short-circuit at the
        // outer Option so non-bound nodes never reach the mutex-locked
        // accessors at all.
        let motion_bindings_ref = self.motion_bindings.get(&node);
        let binding_transform = motion_bindings_ref.and_then(|b| b.get_transform());
        let binding_opacity = motion_bindings_ref.and_then(|b| b.get_opacity());

        // Snapshot the BG batch length BEFORE this node emits any
        // primitives. Together with the count we capture after the
        // child recursion completes (just before transform pops),
        // this gives the compositor-path fast Phase 4 the
        // inclusive-exclusive primitive range this binding's subtree
        // owns.
        //
        // Record every painted node with motion bindings — not just
        // ones currently animating. Reason: an animation can become
        // active *after* a full paint runs (e.g. `on_ready`-triggered
        // `set_target`, `query_motion().start()`, timer callbacks).
        // The cache from that paint has no entry for the bound node,
        // so the next-frame fast path has nothing to patch and the
        // animation appears frozen until a full repaint is triggered
        // by something else (mouse moving, etc.). Recording the
        // binding's primitive range up front means
        // `apply_binding_deltas` has a target to write to as soon as
        // the spring starts moving — no second full paint required.
        //
        // Cost is tiny: a `HashMap` entry per motion-bound painted
        // node (cn_demo has ~13). Bindings whose values match
        // `last_*` are early-out'd inside `apply_binding_deltas` so
        // no GPU work happens for them.
        // Tell the context this subtree is motion-bound so all
        // primitive emissions route into the dynamic batch instead
        // of the static cache batch. Paired with a `pop` at the end
        // of this fn (any return path). The `composite_bg_start`
        // captured immediately after now indexes into the dynamic
        // batch, which is exactly what `apply_binding_deltas` and
        // the per-frame overlay dispatch read.
        //
        // CSS-animated subtrees are deliberately NOT routed here.
        // Earlier (Phase 3a) we routed `Animating(Css)` nodes too,
        // hoping the per-region re-walk (Phase 3c) would refresh
        // them each frame from the dynamic batch. But CSS keyframes
        // commonly animate non-transform properties (height, width,
        // color) where the text inside the animated subtree has no
        // `css_affine`, so the text falls through to the regular
        // `render_text` dispatch into the static cache — while the
        // parent's bg / border SDF lived in the overlay (dynamic
        // batch). The overlay drew the parent bg ON TOP of the
        // cache, covering the text → "text disappears mid-keyframe"
        // (user report on styling_demo's #layout-height transition
        // and #layout-anim grow-shrink keyframes). Keeping CSS
        // content in the static batch keeps the entire subtree
        // (bg + border + child text + child SVG + …) co-located,
        // so z-order falls out naturally from walker traversal
        // order. Phase 3c can come back when we have a way to
        // route regular text + SVG + image dispatch into the same
        // overlay alongside the SDF (`motion_batch` / `css_batch`
        // split or similar).
        // Route stack_layer subtrees (overlay stack, toast tray, legacy
        // overlay manager) and motion FSM subtrees to the dynamic batch
        // alongside motion-binding subtrees.
        //
        // The bug this fixes: the SVG dispatch in the static-cache pass
        // emits ALL static SVGs as one batched draw call AFTER every SDF
        // primitive in the cache (per-z text dispatch doesn't help — by
        // then SDF has already committed). When an overlay's bg SDF
        // lived in the static cache alongside the page's static SVGs,
        // a sibling button's chevron-down SVG at z=0 would paint ON TOP
        // of the overlay's z=1 bg primitive that should have occluded
        // it. Symptom: opening one cn::dropdown_menu showed the chevron
        // of an adjacent unopened dropdown trigger bleeding through the
        // open menu's panel.
        //
        // Routing the overlay/tray subtree to the dynamic batch makes
        // its SDF paint in `composite_frame`'s overlay pass — AFTER the
        // static cache (chevrons and all) is blitted onto the surface —
        // so the panel correctly occludes whatever's underneath. The
        // collector (in blinc_app's `collect_elements_recursive`)
        // already routes text/SVG/image for these subtrees via its
        // `props.motion.is_some()` + `is_stack_layer` check, so this
        // change brings the walker into alignment.
        //
        // CSS animations are deliberately still left in the static
        // batch (see history comment further down for the
        // text-disappears-mid-keyframe regression). `props.motion` here
        // is set by explicit Motion FSM containers (e.g.
        // `motion_derived(...)`) when they propagate enter/exit anims to
        // children — CSS keyframes don't take the same propagation path,
        // so this change doesn't reactivate that regression.
        let in_motion_subtree = motion_bindings_ref.is_some()
            || render_node.props.motion.is_some()
            || render_node.props.is_overlay_root;
        if in_motion_subtree {
            ctx.push_motion_subtree();
        }
        // Track overlay-root subtrees with a separate counter (NOT
        // the combined motion gate) so canvas inserts inside a
        // portal/popover/dropdown route into the overlay-canvas
        // pool. Motion-bound canvases outside an overlay keep their
        // pre-dyn-batch slot.
        let in_overlay_subtree_node = render_node.props.is_overlay_root;
        if in_overlay_subtree_node {
            ctx.push_overlay_subtree();
        }
        let composite_bg_start = in_motion_subtree.then(|| ctx.bg_primitive_count());
        // Phase 4b: bracket CSS-animated subtrees the same way
        // `composite_bg_start` brackets motion-bound subtrees, so
        // the post-walk recording at the bottom of this fn can
        // emit a `CssAnimPaintMeta { primitive_range: start..end,
        // ... }` for `apply_css_deltas` to patch. Gated on
        // `Animating(Css)` so settled CSS store entries (kept for
        // the same-target restart guard in
        // `detect_and_start_transitions`) don't get recorded.
        // Motion-bound nodes that also have a CSS animation are
        // handled by `composite_bindings` / `apply_binding_deltas`
        // (motion takes precedence in `compute_animation_status`)
        // and skip this CSS bookkeeping.
        let in_css_subtree = matches!(
            self.current_animation_status.borrow().get(&node).copied(),
            Some(super::super::AnimationStatus::Animating(
                super::super::AnimatedKind::Css
            ))
        );

        // Composite-layer promotion: if the node is CSS-animated AND
        // its current properties are composite-promotable (only
        // opacity / 2D transform), route its emit into a per-node
        // scratch batch. The compositor rasterizes the scratch
        // batch into a `LayerTexture` at end of paint and blits the
        // texture per frame with the active CSS animation transform
        // applied — no walker re-entry on animation ticks.
        //
        // The scope ends when this node finishes walking; we pop at
        // the bottom of the function alongside the other balanced
        // pushes. Nested promotions are routed to the outermost
        // (see `GpuPaintContext::push_composite_layer`).
        //
        // MUST run BEFORE the `css_anim_bg_start` snapshot below —
        // `bg_primitive_count` reads the ACTIVE batch (scratch when
        // composite_layer is pushed), so both `start` and `end` need
        // to come from the same batch.
        // Composite-layer push for promoted CSS-animated subtrees
        // (transform / opacity / scale animations whose
        // `is_composite_promotable()` predicate matched).
        // P4.3 Option B: capture the ancestor clip AABB BEFORE the
        // composite layer push so the bake can re-apply it at blit
        // time. The push strips ancestor clips from per-primitive
        // emit (so scale/translate animations don't drag a baked
        // ancestor clip along with them). The overlay reapplies the
        // captured AABB as a scissor.
        let css_anim_ancestor_clip =
            if in_css_subtree && self.composite_promotion.borrow().contains(&node) {
                ctx.current_clip_rounded()
            } else {
                None
            };
        let pushed_for_css = if in_css_subtree && self.composite_promotion.borrow().contains(&node)
        {
            // Use the slotmap key bits as the routing key — stable
            // for the duration of the paint (slotmap version doesn't
            // bump within a single paint pass). `Key::data().as_ffi()`
            // packs (index, version) into a single u64.
            use slotmap::Key as _;
            let key = node.data().as_ffi();
            ctx.push_composite_layer(key);
            true
        } else {
            false
        };
        // Phase 4.3 — composite-layer push for motion-bound subtrees
        // detected as texture-safe by `compute_subtree_texture_candidates`
        // (root has animating transform/opacity binding AND no descendant
        // carries an independent animation source). The walker emits the
        // subtree's primitives into a per-node scratch batch on the
        // first paint after detection; the end-of-paint hook
        // (`try_render_with_compositor`) rasterizes it into a
        // `LayerTexture` and the per-frame compositor blits the cached
        // pixels with the current motion-binding transform applied —
        // no walker re-entry, no re-emission.
        //
        // P4.3 walker gate (Option B clip-aware bake) — see
        // [paint.rs:composite_layer_nested_push_preserves_outer_clip_base]
        // for the clip-stack stack semantics this depends on.
        let ambient_clip_for_bake =
            if !pushed_for_css && self.subtree_texture_candidates.borrow().contains(&node) {
                ctx.current_clip_rounded()
            } else {
                None
            };
        let pushed_for_motion_subtree =
            if !pushed_for_css && self.subtree_texture_candidates.borrow().contains(&node) {
                use slotmap::Key as _;
                let key = node.data().as_ffi();
                ctx.push_composite_layer(key);
                true
            } else {
                false
            };
        let pushed_composite_layer = pushed_for_css || pushed_for_motion_subtree;
        let css_anim_bg_start = in_css_subtree.then(|| ctx.bg_primitive_count());
        // P4.3 sibling of `css_anim_bg_start`. We snapshot the scratch
        // batch index AFTER `push_composite_layer` so the AABB at the
        // insert site below covers exactly the primitives this subtree
        // emitted into its scratch batch.
        let motion_subtree_bg_start = pushed_for_motion_subtree.then(|| ctx.bg_primitive_count());

        // Compositor v2 ambient snapshot for motion-bound subtrees.
        // Captured BEFORE this node pushes any of its own transforms
        // so the saved affine represents the parent environment —
        // re-walking the subtree will re-push the node's own
        // position/motion/rotation/etc. transforms on top of this.
        //
        // Mirrors `composite_bg_start` above: we only snapshot when
        // motion bindings are present. Canvas nodes get their own
        // snapshot at the canvas paint site (with the post-transform
        // affine, because the canvas closure executes after the
        // node's transforms have all pushed).
        //
        // The actual `DynamicRegion` insert happens at the bottom of
        // this fn alongside the legacy `composite_bindings` insert,
        // gated on the node's `AnimationStatus` being `Animating`.
        let v2_motion_ambient = in_motion_subtree.then(|| {
            (
                ctx.current_affine_elements(),
                ctx.current_opacity(),
                ctx.current_clip_aabb(),
                ctx.z_layer(),
            )
        });

        // We've passed all the cull / visibility / motion-removed
        // gates; this node is going to paint. Record whether it
        // drives a per-frame redraw — that flag is consulted at end
        // of frame to decide whether the animation-redraw signal
        // should keep the chain alive. Without this gate, an
        // off-screen spinner whose paint is culled still pinned the
        // chain at vsync because the scheduler's needs_redraw stays
        // true regardless of visibility.
        //
        // Gate on `intersects_viewport`: spinners / canvas / active
        // bindings that have scrolled out of the viewport should NOT
        // keep the chain alive. The walker still walks them (the GPU
        // clips them at draw time) but their motion has no visible
        // effect. Without this gate, cn_demo's 3 always-rotating
        // spinners pinned the chain at vsync forever even after
        // scrolling past them — 30 % CPU at idle.
        if intersects_viewport {
            let canvas_paints = matches!(render_node.element_type, ElementType::Canvas(_));
            // Static canvases (e.g. notch) don't need a per-frame
            // overlay re-paint — their bg primitives land in the
            // static cache once and stay valid until layout
            // changes invalidate the cache. Treat them as
            // not-painting so the compositor fast-path gate
            // (`!had_canvas_painted`) can still engage.
            let canvas_paints_dynamic = if canvas_paints {
                matches!(
                    &render_node.element_type,
                    ElementType::Canvas(canvas_data) if !canvas_data.is_static
                )
            } else {
                false
            };
            // Record canvas presence regardless of whether
            // `visible_anim_active` is already set — the fast path
            // needs to know about any in-viewport canvas, not just
            // the first one the walker sees.
            if canvas_paints_dynamic {
                self.had_canvas_painted.set(true);
            }
            if !self.visible_anim_active.get() {
                // Bindings only count as a redraw signal when the
                // underlying animated value is *actually* mid-flight.
                // A settled spring binding (e.g. `cn::progress_animated`
                // after it reached 75 %) leaves the binding in place but
                // the value is now constant — including it here pinned
                // the chain at vsync forever.
                let has_active_binding = motion_bindings_ref.is_some_and(|b| b.is_any_animating());
                let has_active_motion =
                    if let Some(ref stable_key) = render_node.props.motion_stable_id {
                        render_state.is_stable_motion_active(stable_key)
                    } else {
                        render_state.is_motion_active(node)
                    };
                if canvas_paints_dynamic || has_active_binding || has_active_motion {
                    self.visible_anim_active.set(true);
                }
            }
        }

        // Calculate this node's motion opacity (combine motion values, bindings, and element opacity)
        //
        // Phase 4.3: bake mode skips the binding_opacity contribution
        // so the cached texture is invariant to opacity changes. The
        // per-frame blit (`composite_motion_layers_overlay`) reads
        // the current binding opacity and applies it at composite
        // time. Matches the binding-transform skip above.
        let binding_opacity_for_bake = if pushed_for_motion_subtree {
            None
        } else {
            binding_opacity
        };
        // Composite-promoted CSS subtree: the animation opacity is written
        // into `props.opacity` every frame (see css.rs) AND re-applied at
        // composite time (`composite_css_layers_overlay` reads it from the
        // store and applies it to the blit). Baking it in here double-applies
        // it and — because the fast paths never re-bake — freezes the texture
        // at the frame it was first baked (near-zero on a fade-in), so the
        // overlay sits frozen mid-fade until a mouse-move forces a slow-path
        // re-bake. Bake at base state, mirroring the motion path's
        // `binding_opacity_for_bake` strip above; the per-frame composite blit
        // drives the fade instead.
        let bake_opacity = if pushed_for_css {
            1.0
        } else {
            render_node.props.opacity
        };
        let node_motion_opacity = motion_values
            .and_then(|m| m.opacity)
            .unwrap_or_else(|| binding_opacity_for_bake.unwrap_or(1.0))
            * bake_opacity;

        // Combine with inherited opacity from parent motion containers
        // This ensures children fade together with their parent motion container
        let motion_opacity = inherited_opacity * node_motion_opacity;

        // Skip rendering if completely transparent — UNLESS this
        // node has an active motion binding or motion FSM that
        // could ramp the opacity back up. Bailing on a node whose
        // opacity-binding spring is mid-flight from 0 → 1 leaves
        // its subtree's primitives out of the cached batch
        // entirely; the fast-path delta patcher then has nothing
        // to patch when the spring climbs above 0.001, and the
        // subtree stays invisible until a slow-path repaint
        // catches up (typically triggered by mouse move via the
        // render-cache invalidation). Symptom: cn_demo switch
        // background color doesn't fade in on click — the thumb
        // moves but the colored track stays hidden.
        //
        // Keep the early-return for static transparency (a div
        // with `opacity: 0` and no animation) so we don't waste
        // cycles emitting primitives no one will see.
        let has_pending_motion = motion_bindings_ref.is_some_and(|b| b.is_any_animating())
            || if let Some(ref stable_key) = render_node.props.motion_stable_id {
                render_state.is_stable_motion_active(stable_key)
            } else {
                render_state.is_motion_active(node)
            };
        if motion_opacity <= 0.001 && !has_pending_motion {
            // Pop the motion-subtree push from above if we set it,
            // so the dynamic batch's emit gate stays balanced even
            // when this early-out fires.
            if in_motion_subtree {
                ctx.pop_motion_subtree();
            }
            if in_overlay_subtree_node {
                ctx.pop_overlay_subtree();
            }
            // Same balancing for the composite-layer push.
            if pushed_composite_layer {
                ctx.pop_composite_layer();
            }
            return;
        }

        // Push position transform
        ctx.push_transform(Transform::translate(bounds.x, bounds.y));

        // Apply motion translation
        if let Some(motion) = motion_values {
            let (tx, ty) = motion.resolved_translate();
            if tx.abs() > 0.001 || ty.abs() > 0.001 {
                ctx.push_transform(Transform::translate(tx, ty));
            }
        }

        // Apply motion scale (centered)
        let has_motion_scale = motion_values
            .map(|m| {
                let (sx, sy) = m.resolved_scale();
                (sx - 1.0).abs() > 0.001 || (sy - 1.0).abs() > 0.001
            })
            .unwrap_or(false);

        if has_motion_scale {
            let (sx, sy) = motion_values.unwrap().resolved_scale();
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::scale(sx, sy));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply motion binding transform if present (continuous AnimatedValue-driven animation)
        // Translation is NOT centered (moves element from its position)
        //
        // Phase 4.3: when this node was promoted to a baked texture
        // (`pushed_for_motion_subtree`), skip pushing the motion-binding
        // transforms — we want the scratch primitives at the subtree's
        // base position so the cached texture is invariant to binding
        // values. The per-frame `composite_motion_layers_overlay` reads
        // the current binding values and applies them at blit time.
        let bake_skip = pushed_for_motion_subtree;
        let has_binding_transform = binding_transform.is_some() && !bake_skip;
        if has_binding_transform && let Some(ref transform) = binding_transform {
            ctx.push_transform(transform.clone());
        }

        // Apply motion binding scale if present (centered around element).
        // Reuses the bindings reference fetched above — no extra HashMap lookup.
        let binding_scale = motion_bindings_ref.and_then(|b| b.get_scale());
        let has_binding_scale = binding_scale.is_some() && !bake_skip;
        if let Some((sx, sy)) = binding_scale
            && has_binding_scale
        {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::scale(sx, sy));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply motion binding rotation if present (centered around element).
        // Reuses the bindings reference fetched above — no extra HashMap lookup.
        let binding_rotation = motion_bindings_ref.and_then(|b| b.get_rotation());
        let has_binding_rotation = binding_rotation.is_some() && !bake_skip;
        if let Some(deg) = binding_rotation
            && has_binding_rotation
        {
            let center_x = bounds.width / 2.0;
            let center_y = bounds.height / 2.0;
            ctx.push_transform(Transform::translate(center_x, center_y));
            ctx.push_transform(Transform::rotate(deg.to_radians()));
            ctx.push_transform(Transform::translate(-center_x, -center_y));
        }

        // Apply element-specific transform if present
        let has_element_transform = render_node.props.transform.is_some();
        if let Some(ref transform) = render_node.props.transform {
            // Use transform-origin if set, otherwise default to center
            let (origin_x, origin_y) =
                if let Some([ox_pct, oy_pct]) = render_node.props.transform_origin {
                    (
                        bounds.width * ox_pct / 100.0,
                        bounds.height * oy_pct / 100.0,
                    )
                } else {
                    (bounds.width / 2.0, bounds.height / 2.0)
                };
            ctx.push_transform(Transform::translate(origin_x, origin_y));
            ctx.push_transform(transform.clone());
            ctx.push_transform(Transform::translate(-origin_x, -origin_y));
        }

        // Determine if this node is a glass element
        let is_glass = matches!(render_node.props.material, Some(Material::Glass(_)));
        let children_glass_depth = if is_glass {
            glass_depth + 1
        } else {
            glass_depth
        };

        // Determine if this node is a foreground element
        let is_foreground = render_node.props.layer == RenderLayer::Foreground;
        let children_inside_foreground = inside_foreground || is_foreground;

        // Increment z_layer for Stack children for proper interleaved rendering
        // This ensures primitives AND text in each Stack layer render together
        let is_stack_layer = render_node.props.is_stack_layer;
        if is_stack_layer {
            let current_z = ctx.z_layer();
            ctx.set_z_layer(current_z + 1);
        }

        // Apply CSS z-index to z_layer for stacking order
        // Save current z_layer so we can restore it after this subtree
        let saved_z_layer = ctx.z_layer();
        let has_z_index = render_node.props.z_index > 0;
        if has_z_index {
            ctx.set_z_layer(render_node.props.z_index as u32);
        }

        // Determine effective layer:
        // - Children of glass elements (that aren't glass themselves) render in foreground
        // - Children of foreground elements also render in foreground
        // - Glass elements render in glass layer (both top-level and nested)
        // - Otherwise, use the node's explicit layer setting
        let effective_layer = if (glass_depth > 0 && !is_glass) || inside_foreground {
            RenderLayer::Foreground
        } else if is_glass {
            RenderLayer::Glass
        } else {
            render_node.props.layer
        };

        // Push layer if this node has partial opacity OR layer effects OR 3D CSS transform.
        // Children inside the layer automatically inherit the opacity via GPU composition.
        // Layer effects (blur, drop shadow, glow, color matrix) are applied when layer is composited.
        // 3D CSS transforms (rotate-x/rotate-y) use layer-based compositing: the entire subtree
        // (including text) renders flat to a texture, then the texture is composited with perspective
        // distortion. This ensures ALL children visually transform with the parent.
        // IMPORTANT: Only push layer when element's layer matches current target to avoid duplicate
        // layer commands across multiple render passes
        let has_layer_effects = !render_node.props.layer_effects.is_empty();
        let node_blend_mode = render_node
            .props
            .mix_blend_mode
            .unwrap_or(BlendMode::Normal);
        let has_blend_mode = node_blend_mode != BlendMode::Normal;
        // Detect 3D CSS transform (rotate-x/rotate-y on a FLAT container, not a 3D SDF shape)
        let has_3d_css_transform =
            render_node.props.rotate_x.is_some() || render_node.props.rotate_y.is_some();
        let has_3d_shape =
            render_node.props.depth.unwrap_or(0.0) > 0.0 || render_node.props.shape_3d.is_some();
        let use_3d_layer = has_3d_css_transform && !has_3d_shape;
        // Hybrid opacity flatten (Phase 4a): if `opacity < 1.0` is
        // the *only* reason we'd push a layer (no blur / drop-shadow /
        // blend / 3D), and the element is structurally simple enough
        // that per-primitive alpha gives the correct result, skip the
        // layer push entirely and multiply opacity into descendants'
        // primitive colours instead. Same model `apply_binding_deltas`
        // uses for motion-bound opacity.
        //
        // Pre-fix, pure-opacity layers were pushed but then silently
        // dropped by `render_with_clear_simple` / `render_with_layer_effects`
        // (their `effect_layers` gate only matched layers with
        // effects / blend / 3D), so the configured opacity went
        // nowhere — CSS `@keyframes` like pulse / glow that animate
        // only `opacity` never actually rendered the keyframed alpha.
        //
        // The `safe_to_flatten` check is conservative for first cut:
        // we flatten only when the element has at most one child.
        // Anything more complex (multiple children, possible overlap,
        // nested opacity) takes the push_layer path, which the
        // renderer now processes correctly via the relaxed
        // `effect_layers` gate in `render_with_layer_effects`.
        let only_opacity_drives_layer =
            node_motion_opacity < 1.0 && !has_layer_effects && !has_blend_mode && !use_3d_layer;
        // Don't flatten when a motion-binding opacity is in play.
        // Flattening makes children inherit the parent's current
        // motion_opacity — and when that opacity is at / near 0 on
        // the slow-path frame (e.g. cn::switch's
        // `motion().opacity(color_anim)` animating off → on with
        // the spring just starting from 0.0), descendants hit the
        // transparency guard above (their own `has_pending_motion`
        // is false because the binding is on the ancestor, not on
        // them) and get skipped entirely. Their primitives never
        // land in `cached_dynamic_batch`; `apply_binding_deltas`
        // has nothing to multiply when the spring climbs above
        // 0.001, and the subtree stays invisible until a slow-path
        // repaint forced by something else (mouse motion via
        // state-style apply). Symptom: switch toggle off → on
        // shows no bg fade.
        //
        // Pushing a layer is correct here: children inherit 1.0
        // (layer composite owns the opacity), so they emit
        // primitives regardless of the binding's current value.
        // `apply_binding_deltas` patches `LayerConfig.opacity` at
        // the recorded push index so the spring becomes visible.
        let has_motion_opacity_binding = motion_bindings_ref.is_some_and(|b| b.opacity.is_some());
        let safe_to_flatten =
            !has_motion_opacity_binding && self.layout_tree.children(node).len() <= 1;
        let can_flatten_opacity = only_opacity_drives_layer && safe_to_flatten;
        let has_opacity_layer = !can_flatten_opacity
            && (node_motion_opacity < 1.0 || has_layer_effects || has_blend_mode || use_3d_layer);
        let should_push_layer = has_opacity_layer && effective_layer == target_layer;
        if should_push_layer {
            // Scale layer effect radii by DPI factor (CSS px → physical px)
            let scaled_effects: Vec<LayerEffect> = render_node
                .props
                .layer_effects
                .iter()
                .map(|e| match e {
                    LayerEffect::Blur { radius, quality } => LayerEffect::Blur {
                        radius: radius * self.scale_factor,
                        quality: *quality,
                    },
                    LayerEffect::DropShadow {
                        offset_x,
                        offset_y,
                        blur,
                        spread,
                        color,
                    } => LayerEffect::DropShadow {
                        offset_x: offset_x * self.scale_factor,
                        offset_y: offset_y * self.scale_factor,
                        blur: blur * self.scale_factor,
                        spread: spread * self.scale_factor,
                        color: *color,
                    },
                    other => other.clone(),
                })
                .collect();
            // Build 3D transform params for layer compositing
            let transform_3d = if use_3d_layer {
                let rx = render_node.props.rotate_x.unwrap_or(0.0).to_radians();
                let ry = render_node.props.rotate_y.unwrap_or(0.0).to_radians();
                let d = render_node.props.perspective.unwrap_or(800.0);
                Some(blinc_core::Transform3DParams {
                    sin_rx: rx.sin(),
                    cos_rx: rx.cos(),
                    sin_ry: ry.sin(),
                    cos_ry: ry.cos(),
                    perspective_d: d * self.scale_factor,
                })
            } else {
                None
            };
            ctx.push_layer(LayerConfig {
                id: None,
                position: Some(blinc_core::Point::new(bounds.x, bounds.y)),
                size: Some(blinc_core::Size::new(bounds.width, bounds.height)),
                blend_mode: node_blend_mode,
                opacity: node_motion_opacity,
                depth: false,
                effects: scaled_effects,
                transform_3d,
            });
        }
        // Phase 4b: capture the layer-command index of the push we
        // just made, so the CSS post-walk recording can drop it
        // into `CssAnimPaintMeta`. `apply_css_deltas` patches
        // `LayerConfig.opacity` at this index for opacity
        // animations that took the layered (non-flattened) path.
        // `bg_layer_command_count` returns the count AFTER the
        // push, so the push's own index is `count - 1`.
        let css_anim_layer_push_index = if in_css_subtree && should_push_layer {
            let n = ctx.bg_layer_command_count();
            if n > 0 { Some(n - 1) } else { None }
        } else {
            None
        };
        // Same index, captured for the motion-binding case so
        // `apply_binding_deltas` can patch `LayerConfig.opacity` at
        // the binding's owning node. With the Phase 4a flatten
        // disabled for motion bindings (above), opacity bindings
        // always take the layered path and need this index to
        // animate visibly.
        let motion_binding_layer_push_index = if motion_bindings_ref.is_some() && should_push_layer
        {
            let n = ctx.bg_layer_command_count();
            if n > 0 { Some(n - 1) } else { None }
        } else {
            None
        };

        // Corner shape setup (superellipse per-corner) — MUST be set before draw_shadow
        // so shadows use the same corner_shape as the fill+border SDF.
        //
        // Resolved through the active theme's ShapeTokens so Universal
        // HID variants auto-substitute squircle on rounded corners
        // that pass the threshold check; explicit per-element overrides
        // win, and themes that don't opt in (every existing platform
        // bundle + Catppuccin) keep circular corners via the trait's
        // default off-state.
        // Tolerate an uninitialised ThemeState (snapshot / GPU
        // integration tests render through this path without calling
        // `ThemeState::init_*` first). See basic.rs for the same
        // fall-back rationale.
        let (theme_shape_m, radius_full_m) = match blinc_theme::ThemeState::try_get() {
            Some(theme) => (theme.shape(), theme.radii().radius_full),
            None => (blinc_theme::ShapeTokens::default(), 9999.0),
        };
        let resolved_corner_shape_m = super::helpers::resolve_corner_shape(
            render_node.props.corner_shape,
            render_node.props.border_radius,
            (bounds.width, bounds.height),
            &theme_shape_m,
            radius_full_m,
            render_node.props.corner_shape_locked,
        );
        let has_corner_shape = !resolved_corner_shape_m.is_round();
        if has_corner_shape {
            ctx.set_corner_shape(resolved_corner_shape_m.to_array());
        }

        // Draw shadow BEFORE pushing clip (shadows extend beyond element bounds)
        // This must be done before the clip is applied so shadows aren't clipped
        let rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
        let radius = render_node.props.border_radius;
        if effective_layer == target_layer {
            // Glass elements have shadows handled by the GPU glass system
            if !matches!(render_node.props.material, Some(Material::Glass(_))) {
                // Iterate the shadow stack back-to-front so the ambient layer
                // paints first and the tight key-light layer lands on top.
                for shadow in render_node.props.shadow.iter().rev() {
                    // When using opacity layer, draw shadow at full opacity (layer handles it)
                    // Otherwise, apply motion opacity to shadow color for fallback
                    let layer = if !has_opacity_layer && motion_opacity < 1.0 {
                        Shadow {
                            color: Color::rgba(
                                shadow.color.r,
                                shadow.color.g,
                                shadow.color.b,
                                shadow.color.a * motion_opacity,
                            ),
                            ..*shadow
                        }
                    } else {
                        *shadow
                    };
                    ctx.draw_shadow(rect, radius, layer);
                }
            }
        }

        // Determine if this element clips its content (overflow:hidden, scroll, or layout animation).
        // The actual clip push is deferred to after border/outline drawing so that the
        // overflow clip doesn't double-AA with the border SDF at the same boundary.
        // Per CSS spec, overflow clips the element's *content* (children), not its decoration
        // (background/border), which are already SDF-constrained to the element bounds.
        let clips_content = render_node.props.clips_content || has_layout_animation;

        // Push clip-path if set on this element
        let has_clip_path = render_node.props.clip_path.is_some();
        if has_clip_path {
            if let Some(cs) =
                Self::resolve_clip_path(render_node.props.clip_path.as_ref().unwrap(), &bounds)
            {
                ctx.push_clip(cs);
            }
        }

        // Render if this node matches target layer
        // Debug: see what layers we're checking
        // let is_canvas = matches!(&render_node.element_type, ElementType::Canvas(_));
        // if is_canvas {
        //     let matches = effective_layer == target_layer;
        //     // eprintln!(
        //     //     "render_layer_with_motion: Canvas node {:?}, effective_layer={:?}, target_layer={:?}, matches={}",
        //     //     node, effective_layer, target_layer, matches
        //     // );
        //     // if matches {
        //     //     eprintln!("  >>> Canvas layer MATCHES - will invoke callback");
        //     // }
        // }
        // Set up 3D transform params on the paint context if this element has any.
        // When use_3d_layer is true, 3D CSS rotation is handled by layer compositing
        // (perspective distortion applied to the blit quad), NOT per-primitive.
        let has_3d = render_node.props.rotate_x.is_some()
            || render_node.props.rotate_y.is_some()
            || render_node.props.perspective.is_some()
            || render_node.props.depth.unwrap_or(0.0) > 0.0
            || render_node.props.translate_z.is_some()
            || render_node.props.shape_3d.is_some();

        if has_3d && !use_3d_layer {
            let rx = render_node.props.rotate_x.unwrap_or(0.0).to_radians();
            let ry = render_node.props.rotate_y.unwrap_or(0.0).to_radians();
            let d = render_node.props.perspective.unwrap_or(800.0);
            ctx.set_3d_transform(rx, ry, d);

            let is_3d_group = render_node.props.shape_3d == Some(6.0);
            if render_node.props.depth.unwrap_or(0.0) > 0.0 || is_3d_group {
                ctx.set_3d_shape(
                    render_node.props.shape_3d.unwrap_or(1.0),
                    render_node.props.depth.unwrap_or(0.0),
                    render_node.props.ambient.unwrap_or(0.3),
                    render_node.props.specular.unwrap_or(32.0),
                );
                ctx.set_3d_light(
                    render_node
                        .props
                        .light_direction
                        .unwrap_or([-0.5, -1.0, 0.5]),
                    render_node.props.light_intensity.unwrap_or(0.8),
                );
            }

            if let Some(tz) = render_node.props.translate_z {
                ctx.set_3d_translate_z(tz);
            }
        }

        // CSS filter setup
        let has_filter = render_node.props.filter.is_some();
        if let Some(f) = &render_node.props.filter {
            if !f.is_identity() {
                ctx.set_css_filter(
                    f.grayscale,
                    f.invert,
                    f.sepia,
                    f.hue_rotate,
                    f.brightness,
                    f.contrast,
                    f.saturate,
                );
            }
        }

        // Mask gradient setup (gradient masks are per-primitive, URL masks use LayerEffect)
        let has_mask_gradient = matches!(
            render_node.props.mask_image,
            Some(blinc_core::MaskImage::Gradient(_))
        );
        if let Some(blinc_core::MaskImage::Gradient(ref gradient)) = render_node.props.mask_image {
            let mask_mode_luminance = matches!(
                render_node.props.mask_mode,
                Some(blinc_core::MaskMode::Luminance)
            );
            match gradient {
                blinc_core::Gradient::Linear {
                    start, end, stops, ..
                } => {
                    let (start_alpha, end_alpha) =
                        Self::extract_mask_alphas(stops, mask_mode_luminance);
                    ctx.set_mask_gradient(
                        [start.x, start.y, end.x, end.y],
                        [1.0, start_alpha, end_alpha, 0.0],
                    );
                }
                blinc_core::Gradient::Radial {
                    center,
                    radius,
                    stops,
                    ..
                } => {
                    let (start_alpha, end_alpha) =
                        Self::extract_mask_alphas(stops, mask_mode_luminance);
                    ctx.set_mask_gradient(
                        [center.x, center.y, *radius, 0.0],
                        [2.0, start_alpha, end_alpha, 0.0],
                    );
                }
                blinc_core::Gradient::Conic { center, stops, .. } => {
                    // Treat conic as radial for mask purposes
                    let (start_alpha, end_alpha) =
                        Self::extract_mask_alphas(stops, mask_mode_luminance);
                    ctx.set_mask_gradient(
                        [center.x, center.y, 0.5, 0.0],
                        [2.0, start_alpha, end_alpha, 0.0],
                    );
                }
            }
        }

        // (corner_shape already set above, before draw_shadow)

        // 3D Group composition: collect child shapes into compound SDF
        // MUST happen before fill_rect so the primitive gets the group shape descriptors.
        let is_3d_group = render_node.props.shape_3d == Some(6.0);
        let mut group_3d_children: Vec<LayoutNodeId> = Vec::new();

        if is_3d_group {
            let mut raw_descs: Vec<[f32; 16]> = Vec::new();
            let group_cx = bounds.x + bounds.width * 0.5;
            let group_cy = bounds.y + bounds.height * 0.5;

            for child_id in self.layout_tree.children(node) {
                if let Some(child_node) = self.render_nodes.get(&child_id) {
                    if let Some(child_shape) = child_node.props.shape_3d {
                        if child_shape > 0.0 && child_shape < 6.0 {
                            group_3d_children.push(child_id);
                            let child_bounds = self.get_render_bounds(child_id, (0.0, 0.0));
                            if let Some(cb) = child_bounds {
                                let ox = cb.x + cb.width * 0.5 - group_cx;
                                let oy = cb.y + cb.height * 0.5 - group_cy;
                                let oz = child_node.props.translate_z.unwrap_or(0.0);
                                let cr = child_node
                                    .props
                                    .border_radius
                                    .top_left
                                    .min(child_node.props.depth.unwrap_or(20.0) * 0.5);
                                let child_depth = child_node.props.depth.unwrap_or(20.0);
                                let half_w = cb.width * 0.5;
                                let half_h = cb.height * 0.5;
                                let half_d = child_depth * 0.5;
                                let op_type = child_node.props.op_3d.unwrap_or(0.0);
                                let blend = child_node.props.blend_3d.unwrap_or(0.0);

                                // Get child color for per-shape coloring
                                let color = if let Some(blinc_core::Brush::Solid(c)) =
                                    &child_node.props.background
                                {
                                    [c.r, c.g, c.b, c.a]
                                } else {
                                    [0.8, 0.8, 0.8, 1.0]
                                };

                                // Pack as [offset(4), params(4), half_ext(4), color(4)]
                                raw_descs.push([
                                    ox,
                                    oy,
                                    oz,
                                    cr,
                                    child_shape,
                                    child_depth,
                                    op_type,
                                    blend,
                                    half_w,
                                    half_h,
                                    half_d,
                                    0.0,
                                    color[0],
                                    color[1],
                                    color[2],
                                    color[3],
                                ]);
                            }
                        }
                    }
                }
            }

            if !raw_descs.is_empty() {
                ctx.set_3d_group_raw(&raw_descs);
            }
        }

        if effective_layer == target_layer {
            // Motion opacity is now handled via push_layer when has_opacity_layer=true
            // The opacity layer applies opacity to all content via GPU composition

            // Pre-resolve per-side border widths and color.
            // When all border colors are the same, we merge into a single SDF primitive
            // (fill_rect_with_per_side_border) to avoid AA fringe from overlapping
            // fill + border primitives at rounded/squircle corners.
            let has_per_side_border = render_node.props.border_sides.has_any();
            let per_side_data: Option<([f32; 4], Color, bool)> = if has_per_side_border {
                let sides = &render_node.props.border_sides;
                let uw = render_node.props.border_width;
                let uc = render_node.props.border_color.unwrap_or(Color::TRANSPARENT);
                let top = sides
                    .top
                    .as_ref()
                    .map(|b| (b.width, b.color))
                    .unwrap_or((uw, uc));
                let right = sides
                    .right
                    .as_ref()
                    .map(|b| (b.width, b.color))
                    .unwrap_or((uw, uc));
                let bottom = sides
                    .bottom
                    .as_ref()
                    .map(|b| (b.width, b.color))
                    .unwrap_or((uw, uc));
                let left = sides
                    .left
                    .as_ref()
                    .map(|b| (b.width, b.color))
                    .unwrap_or((uw, uc));
                let widths = [top.0, right.0, bottom.0, left.0];
                let all_same_color = top.1 == right.1 && right.1 == bottom.1 && bottom.1 == left.1;
                let dominant = top.1; // all same when mergeable, otherwise pick first
                Some((widths, dominant, all_same_color))
            } else {
                None
            };
            // Merge per-side borders into the fill SDF when all border colors match.
            // Glass elements need separate foreground borders for compositing.
            // For clips_content elements, children are already clipped to inside the border
            // (inset clip at padding box), so merging is safe — no child can render over it.
            let all_same_per_side = per_side_data.as_ref().map(|d| d.2).unwrap_or(false);
            let merge_per_side = has_per_side_border && all_same_per_side && !is_glass;

            if let Some(Material::Glass(glass)) = &render_node.props.material {
                let glass_brush = Brush::Glass(GlassStyle {
                    blur: glass.blur,
                    tint: glass.tint,
                    saturation: glass.saturation,
                    brightness: glass.brightness,
                    noise: glass.noise,
                    border_thickness: glass.border_thickness,
                    // GlassStyle still carries a single shadow.
                    shadow: render_node.props.shadow.first().copied(),
                    simple: glass.simple,
                    depth: glass_depth,
                    border_color: render_node.props.border_color,
                });
                ctx.fill_rect(rect, radius, glass_brush);
            } else {
                // Shadow already drawn before clip was pushed

                // Merge border into the fill primitive to avoid AA fringe at corners.
                // Only glass needs separate foreground borders (special compositing).
                // For clips_content: children are clipped to inside the border (inset clip),
                // so merging the border with the fill is safe.
                let has_uniform_border = !has_per_side_border
                    && render_node.props.border_width > 0.0
                    && render_node.props.border_color.is_some();
                let merge_border = (has_uniform_border && !is_glass) || merge_per_side;

                if let Some(ref bg) = render_node.props.background {
                    // When using opacity layer, draw at full opacity (layer handles it)
                    // Otherwise, apply motion opacity to brush for fallback
                    let brush = if !has_opacity_layer && motion_opacity < 1.0 {
                        super::helpers::apply_opacity_to_brush(bg, motion_opacity)
                    } else {
                        bg.clone()
                    };
                    if merge_per_side {
                        // Per-side border merged with fill for squircle/bevel/scoop support
                        let (widths, mut bc, _) = per_side_data.unwrap();
                        if !has_opacity_layer && motion_opacity < 1.0 {
                            bc.a *= motion_opacity;
                        }
                        ctx.fill_rect_with_per_side_border(rect, radius, brush, widths, bc);
                    } else if merge_border {
                        // Single primitive with fill + border — no AA overlap
                        let bw = render_node.props.border_width;
                        let mut bc = *render_node.props.border_color.as_ref().unwrap();
                        if !has_opacity_layer && motion_opacity < 1.0 {
                            bc.a *= motion_opacity;
                        }
                        ctx.fill_rect_with_per_side_border(
                            rect,
                            radius,
                            brush,
                            [bw, bw, bw, bw],
                            bc,
                        );
                    } else {
                        ctx.fill_rect(rect, radius, brush);
                    }
                } else if is_3d_group {
                    // 3D group elements need a primitive even without a background —
                    // the shader renders the compound SDF from child shape descriptors.
                    ctx.fill_rect(rect, radius, Brush::Solid(Color::TRANSPARENT));
                } else if merge_per_side {
                    // No background but per-side border with squircle — transparent fill
                    let (widths, mut bc, _) = per_side_data.unwrap();
                    if !has_opacity_layer && motion_opacity < 1.0 {
                        bc.a *= motion_opacity;
                    }
                    ctx.fill_rect_with_per_side_border(
                        rect,
                        radius,
                        Brush::Solid(Color::TRANSPARENT),
                        widths,
                        bc,
                    );
                } else if merge_border {
                    // No background but has uniform border — merge with transparent fill
                    let bw = render_node.props.border_width;
                    let mut bc = *render_node.props.border_color.as_ref().unwrap();
                    if !has_opacity_layer && motion_opacity < 1.0 {
                        bc.a *= motion_opacity;
                    }
                    ctx.fill_rect_with_per_side_border(
                        rect,
                        radius,
                        Brush::Solid(Color::TRANSPARENT),
                        [bw, bw, bw, bw],
                        bc,
                    );
                }
            }

            // Only glass needs foreground borders (special compositing).
            // For clips_content: children are clipped to inside the border by the inset
            // clip pushed later (padding box), so the merged border is never covered.
            let border_in_foreground = is_glass;
            if border_in_foreground {
                ctx.set_foreground_layer(true);
            }

            // Draw borders that weren't merged with the fill.
            // This only runs for per-side borders with different colors (can't merge)
            // or glass foreground borders.
            if has_per_side_border && !merge_per_side {
                if all_same_per_side {
                    // Same color but not merged (glass) — single SDF border primitive
                    let (widths, mut bc, _) = per_side_data.unwrap();
                    if !has_opacity_layer && motion_opacity < 1.0 {
                        bc.a *= motion_opacity;
                    }
                    ctx.fill_rect_with_per_side_border(
                        rect,
                        radius,
                        Brush::Solid(Color::TRANSPARENT),
                        widths,
                        bc,
                    );
                } else {
                    // Different colors per side — group by color, one SDF primitive per group.
                    // Each fill_rect_with_per_side_border call gets proper corner radius
                    // from the shader instead of using rectangular strips with clip.
                    let sides = &render_node.props.border_sides;
                    let uniform_width = render_node.props.border_width;
                    let uniform_color =
                        render_node.props.border_color.unwrap_or(Color::TRANSPARENT);

                    let apply_motion = |color: Color| -> Color {
                        if !has_opacity_layer && motion_opacity < 1.0 {
                            Color::rgba(color.r, color.g, color.b, color.a * motion_opacity)
                        } else {
                            color
                        }
                    };

                    // Resolve each side: (width, color)
                    let side_data: [(f32, Color); 4] = [
                        sides
                            .top
                            .as_ref()
                            .map(|b| (b.width, b.color))
                            .unwrap_or((uniform_width, uniform_color)),
                        sides
                            .right
                            .as_ref()
                            .map(|b| (b.width, b.color))
                            .unwrap_or((uniform_width, uniform_color)),
                        sides
                            .bottom
                            .as_ref()
                            .map(|b| (b.width, b.color))
                            .unwrap_or((uniform_width, uniform_color)),
                        sides
                            .left
                            .as_ref()
                            .map(|b| (b.width, b.color))
                            .unwrap_or((uniform_width, uniform_color)),
                    ];

                    // Group sides by color: collect unique colors and their widths
                    let mut color_groups: Vec<(Color, [f32; 4])> = Vec::with_capacity(4);
                    for (i, &(w, c)) in side_data.iter().enumerate() {
                        if w <= 0.0 {
                            continue;
                        }
                        if let Some(group) = color_groups.iter_mut().find(|(gc, _)| *gc == c) {
                            group.1[i] = w;
                        } else {
                            let mut widths = [0.0f32; 4];
                            widths[i] = w;
                            color_groups.push((c, widths));
                        }
                    }

                    for (color, widths) in color_groups {
                        ctx.fill_rect_with_per_side_border(
                            rect,
                            radius,
                            Brush::Solid(Color::TRANSPARENT),
                            widths,
                            apply_motion(color),
                        );
                    }
                }
            } else if render_node.props.border_width > 0.0 && border_in_foreground {
                // Glass uniform border — rendered in foreground on top of glass compositing
                if let Some(ref border_color) = render_node.props.border_color {
                    let bw = render_node.props.border_width;
                    let mut bc = *border_color;
                    if !has_opacity_layer && motion_opacity < 1.0 {
                        bc.a *= motion_opacity;
                    }
                    ctx.fill_rect_with_per_side_border(
                        rect,
                        radius,
                        Brush::Solid(Color::TRANSPARENT),
                        [bw, bw, bw, bw],
                        bc,
                    );
                }
            }

            // Draw outline outside the border
            if render_node.props.outline_width > 0.0 {
                if let Some(ref outline_color) = render_node.props.outline_color {
                    let ow = render_node.props.outline_width;
                    let offset = render_node.props.outline_offset;
                    let expand = offset + ow / 2.0;
                    let outline_rect = Rect::new(
                        -expand,
                        -expand,
                        bounds.width + expand * 2.0,
                        bounds.height + expand * 2.0,
                    );
                    let outline_radius = CornerRadius {
                        top_left: (radius.top_left + expand).max(0.0),
                        top_right: (radius.top_right + expand).max(0.0),
                        bottom_right: (radius.bottom_right + expand).max(0.0),
                        bottom_left: (radius.bottom_left + expand).max(0.0),
                    };
                    let stroke = Stroke::new(ow);
                    let brush = if !has_opacity_layer && motion_opacity < 1.0 {
                        let mut color = *outline_color;
                        color.a *= motion_opacity;
                        Brush::Solid(color)
                    } else {
                        Brush::Solid(*outline_color)
                    };
                    ctx.stroke_rect(outline_rect, outline_radius, &stroke, brush);
                }
            }

            // Restore foreground layer state after border/outline rendering
            if border_in_foreground {
                ctx.set_foreground_layer(false);
            }

            // Handle canvas elements.
            //
            // Only push a clip if the element explicitly opts into
            // overflow clipping (via `overflow_clip`). Unconditionally
            // clipping to the element's bbox breaks elements like the
            // notch, whose custom render emits primitives whose vertex
            // bounds LEGITIMATELY extend past the layout box (concave
            // corner expansion for the flares, blur expansion for a
            // drop shadow, etc.). Parent clips (e.g. scroll containers)
            // still apply via the clip stack, so honouring the
            // element's own overflow setting is enough.
            if let ElementType::Canvas(canvas_data) = &render_node.element_type {
                if let Some(render_fn) = &canvas_data.render_fn {
                    let should_clip = render_node.props.clips_content;
                    if should_clip {
                        let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
                        ctx.push_clip(ClipShape::rect(clip_rect));
                    }

                    // `bounds.x` / `bounds.y` are already translated
                    // onto the DrawContext by the `push_transform` at
                    // the top of `render_node`, so in canvas-local
                    // space the origin is (0, 0). Surfacing the
                    // pre-translate offset to the callback is a
                    // diagnostic breadcrumb, not a correction; forward
                    // zero for x/y so `Rect::new(bounds.x, bounds.y,
                    // …)` in callback code resolves to the canvas's
                    // actual origin without double-offsetting.
                    let canvas_bounds = crate::canvas::CanvasBounds {
                        x: 0.0,
                        y: 0.0,
                        width: bounds.width,
                        height: bounds.height,
                    };

                    // Snapshot the transform / opacity / z-layer
                    // state at canvas paint time. The fast path (and
                    // the layer compositor's overlay pass) replays
                    // into a scratch `GpuPaintContext` with these
                    // values pre-pushed so the canvas closure emits
                    // primitives at exactly the same screen coords /
                    // opacity / draw-order it had on this full paint.
                    let saved_affine = ctx.current_affine_elements();
                    let saved_opacity = ctx.current_opacity();
                    let saved_z_layer = ctx.z_layer();
                    // Snapshot the ancestor clip stack BEFORE the
                    // canvas's own clip (if any) is pushed below —
                    // the overlay pass replays this so canvas
                    // content scrolled out of its parent's
                    // viewport stays hidden.
                    let saved_ancestor_clip = ctx.current_clip_aabb();
                    let canvas_start = ctx.bg_primitive_count();
                    let skip_drawing_flag = self.skip_canvas_drawing.get();

                    // The layer compositor uses `skip_canvas_drawing`
                    // to keep the cached static texture transparent
                    // in canvas regions — fresh canvas content is
                    // overlaid each frame on top of the cache. When
                    // the flag is set, we still walk the canvas (to
                    // record the paint state for the overlay pass)
                    // but skip the actual `render_fn` invocation, so
                    // no primitives land in the static batch.
                    //
                    // Static canvases (e.g. notch) take the opposite
                    // route: they ALWAYS emit into the static cache
                    // and never go through the overlay. Their output
                    // is deterministic in bounds + closure captures,
                    // so the cache-only path matches reality and
                    // doesn't overdraw children layered on top.
                    let skip_drawing = skip_drawing_flag && !canvas_data.is_static;
                    if !skip_drawing {
                        render_fn(ctx, canvas_bounds);
                    }

                    let canvas_end = ctx.bg_primitive_count();

                    if should_clip {
                        ctx.pop_clip();
                    }

                    // Only record when on the matching layer pass —
                    // mirrors the `composite_bindings` recording at
                    // the bottom of this fn. The walker runs three
                    // times per frame (Background / Glass /
                    // Foreground); on the two non-matching passes the
                    // canvas doesn't emit to the bg batch and a
                    // captured `(start..start)` empty range would
                    // clobber the real one via HashMap::insert. When
                    // `skip_drawing` is set the primitive range is
                    // empty (no emission) — record it anyway so the
                    // overlay pass has a `render_fn` + transform
                    // state to replay; the empty range is just a
                    // bookkeeping marker.
                    let layer_matches = effective_layer == target_layer;
                    let has_emission = canvas_end > canvas_start;
                    // Static canvases (notch) emit to the static
                    // cache once and stay valid until a layout
                    // change forces a full repaint. Skipping the
                    // overlay record means `composite_frame` won't
                    // re-invoke `render_fn` on top of the surface
                    // each frame — which would otherwise overdraw
                    // any children the walker layered on top of
                    // the canvas in the static cache. The matching
                    // `DynamicRegion` insert below is also skipped
                    // so the dynamic-batch path doesn't replay the
                    // static canvas every frame either.
                    let is_static_canvas = canvas_data.is_static;
                    if layer_matches && (has_emission || skip_drawing) && !is_static_canvas {
                        // Route into the overlay-subtree pool when
                        // the walker is currently inside an
                        // `is_overlay_root` ancestor. That pool is
                        // dispatched AFTER the popover's dyn-batch
                        // SDF so a caret canvas nested in a focused
                        // cn::input stays above the popover bg/
                        // border instead of being overdrawn by it.
                        let nested_overlay = ctx.in_overlay_subtree();
                        let record = super::super::CanvasPaintRecord {
                            primitive_range: canvas_start..canvas_end,
                            affine: saved_affine,
                            bounds_wh: (bounds.width, bounds.height),
                            render_fn: render_fn.clone(),
                            clips_content: should_clip,
                            ancestor_clip_aabb: saved_ancestor_clip,
                            z_layer: saved_z_layer,
                            opacity: saved_opacity,
                            in_overlay_subtree: nested_overlay,
                        };
                        if nested_overlay {
                            self.overlay_canvas_paint_records
                                .borrow_mut()
                                .insert(node, record);
                        } else {
                            self.canvas_paint_records.borrow_mut().insert(node, record);
                        }

                        // Compositor v2 parallel-populate: every
                        // canvas is unconditionally
                        // `Animating(Canvas)` in the new status
                        // model, so record a `DynamicRegion`
                        // alongside the legacy paint record. The
                        // composite path won't read these yet
                        // (Phase 3 makes the swap); this is purely
                        // verification scaffolding.
                        let region_screen_aabb = Self::affine_screen_aabb(
                            &saved_affine,
                            bounds.width,
                            bounds.height,
                            self.scale_factor,
                        );
                        let region_clip_aabb = saved_ancestor_clip;
                        let visit_seq = self.dynamic_regions.borrow().len() as u32;
                        self.record_dynamic_region(
                            node,
                            super::super::DynamicRegion {
                                root: node,
                                screen_aabb: region_screen_aabb,
                                ambient: super::super::AmbientPaintState {
                                    affine: saved_affine,
                                    opacity: saved_opacity,
                                    clip_aabb: region_clip_aabb,
                                    z_layer: saved_z_layer,
                                },
                                kind: super::super::DynamicKind::Canvas {
                                    render_fn: render_fn.clone(),
                                    clips_content: should_clip,
                                    bounds_wh: (bounds.width, bounds.height),
                                },
                                visit_seq,
                            },
                        );
                    }
                }
            }
        }

        // Clear corner shape before rendering children — corner-shape is NOT inherited.
        // It only affects the current node's own fill_rect/stroke_rect primitives.
        // Without this, a parent's corner-shape (e.g. squircle on .chat-card) would
        // leak into all descendant nodes that don't set their own corner-shape.
        if has_corner_shape {
            ctx.clear_corner_shape();
        }

        // Determine if this element has a border (needed for clip decisions below).
        let has_border =
            render_node.props.border_width > 0.0 || render_node.props.border_sides.has_any();

        // Push overflow clip for children. This is deferred from before the render block
        // so that the border/outline SDF doesn't get double-AA'd by an overlapping clip.
        // Background and borders are SDF-constrained; only children need the overflow clip.
        //
        // When there IS a border, skip the outer rounded clip entirely: the inset clip
        // (padding box) already prevents children from overflowing, and a rounded clip
        // at the same boundary as the border SDF creates visible AA doubling at corners.
        let push_outer_clip = clips_content && !has_border;
        if push_outer_clip {
            // Set overflow fade before pushing clip — fade distances consumed by push_clip
            if !render_node.props.overflow_fade.is_none() {
                ctx.set_overflow_fade(render_node.props.overflow_fade.to_array());
            }
            let clip_rect = Rect::new(0.0, 0.0, bounds.width, bounds.height);
            let clip_shape = if radius.is_uniform() && radius.top_left > 0.0 {
                ClipShape::rounded_rect(clip_rect, radius)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Push inset clip for children if this element has borders.
        // This prevents children (including their shadows) from rendering
        // over the parent's border stroke.  The clip is at the padding box
        // (inside border, but padding area is still visible) per CSS spec.
        //
        // IMPORTANT: This clip must be pushed BEFORE the scroll transform so it
        // stays fixed in the element's viewport space.  If pushed after the
        // scroll transform the clip would drift with the scrolled content.
        let push_children_clip = clips_content && has_border;
        if push_children_clip {
            // Set overflow fade before pushing clip (when outer clip was skipped)
            if !push_outer_clip && !render_node.props.overflow_fade.is_none() {
                ctx.set_overflow_fade(render_node.props.overflow_fade.to_array());
            }
            // Calculate border insets from either uniform border or per-side borders
            let sides = &render_node.props.border_sides;
            let uniform_border = render_node.props.border_width;

            let border_left = sides
                .left
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);
            let border_right = sides
                .right
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);
            let border_top = sides
                .top
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);
            let border_bottom = sides
                .bottom
                .as_ref()
                .map(|b| b.width)
                .unwrap_or(uniform_border);

            let clip_rect = Rect::new(
                border_left,
                border_top,
                (bounds.width - border_left - border_right).max(0.0),
                (bounds.height - border_top - border_bottom).max(0.0),
            );

            // Adjust corner radius for border inset
            let radius = render_node.props.border_radius;
            let max_inset = border_left
                .max(border_right)
                .max(border_top)
                .max(border_bottom);
            let inset_radius = if radius.is_uniform() && radius.top_left > max_inset {
                CornerRadius::uniform((radius.top_left - max_inset).max(0.0))
            } else {
                CornerRadius::default()
            };

            let clip_shape = if inset_radius.top_left > 0.0 {
                // Use the resolved shape so the overflow:clip on a
                // squircle / scoop / bevel parent follows the same
                // curve as the parent's fill.
                ClipShape::rounded_rect_shaped(clip_rect, inset_radius, resolved_corner_shape_m)
            } else {
                ClipShape::rect(clip_rect)
            };
            ctx.push_clip(clip_shape);
        }

        // Apply scroll offset (AFTER children inset clip so clip stays fixed)
        let scroll_offset = self.get_scroll_offset(node);
        let has_scroll = scroll_offset.0.abs() > 0.001 || scroll_offset.1.abs() > 0.001;
        if has_scroll {
            ctx.push_transform(Transform::translate(scroll_offset.0, scroll_offset.1));
        }

        // Render children, passing down the effective opacity and layer inheritance
        // When we pushed an opacity layer, pass 1.0 to children (layer handles the opacity)
        // Otherwise, pass the combined opacity for brush-based fallback
        let child_inherited_opacity = if has_opacity_layer {
            1.0
        } else {
            motion_opacity
        };

        // Compute new cumulative scroll for children
        // Reset cumulative scroll when entering a scroll container.
        // Sticky/fixed positioning is relative to the nearest scroll ancestor, not all ancestors.
        let is_scroll_container = self.scroll_physics.contains_key(&node);
        let new_cumulative_scroll = if is_scroll_container {
            (scroll_offset.0, scroll_offset.1)
        } else {
            (
                cumulative_scroll.0 + scroll_offset.0,
                cumulative_scroll.1 + scroll_offset.1,
            )
        };

        // Viewport culling: when this node opted in (`scroll().viewport_cull(true)`),
        // set the cull rect to its absolute layout bounds. The intersect
        // test below also reads absolute bounds for each child, so both
        // sides live in the same coordinate frame regardless of how
        // deeply nested the child is. The scroll's *offset* (which moves
        // children visually but not their layout coords) is applied to
        // each child's absolute position before the test — that's what
        // makes scrolled-out children fall outside the rect.
        let prev_cull_viewport = self.cull_viewport.get();
        let entered_cull = self.viewport_cull_scrolls.contains(&node);
        if entered_cull {
            if let Some(abs) = self.layout_tree.get_absolute_bounds(node) {
                self.cull_viewport
                    .set(Some((abs.x, abs.y, abs.width, abs.height)));
            }
        }

        for child_id in self.layout_tree.children(node) {
            // Skip 3D children of a group node — they're composed into the group SDF
            if is_3d_group && group_3d_children.contains(&child_id) {
                continue;
            }

            let child_render = self.render_nodes.get(&child_id);
            let child_is_fixed = child_render.map(|n| n.props.is_fixed).unwrap_or(false);
            let child_is_sticky = child_render.map(|n| n.props.is_sticky).unwrap_or(false);

            // Viewport cull: skip painting children whose post-scroll
            // *visual* position falls outside the active cull viewport.
            // Both `cb` and the cull rect are in absolute layout coords;
            // `new_cumulative_scroll` is the offset that the renderer
            // will apply via the transform stack when drawing this
            // descendant, so adding it to the absolute layout position
            // gives the child's actual on-screen rect. Fixed and sticky
            // children opt out — their visual position isn't determined
            // by `new_cumulative_scroll` alone.
            if let Some((cx, cy, cw, ch)) = self.cull_viewport.get() {
                if !child_is_fixed && !child_is_sticky {
                    if let Some(cb) = self.layout_tree.get_absolute_bounds(child_id) {
                        // 200 px overscan on each axis so a smooth scroll
                        // doesn't pop content in/out at the viewport edge.
                        const OVERSCAN: f32 = 200.0;
                        let vx0 = cx - OVERSCAN;
                        let vy0 = cy - OVERSCAN;
                        let vx1 = cx + cw + OVERSCAN;
                        let vy1 = cy + ch + OVERSCAN;
                        let bx0 = cb.x + new_cumulative_scroll.0;
                        let by0 = cb.y + new_cumulative_scroll.1;
                        let bx1 = bx0 + cb.width;
                        let by1 = by0 + cb.height;
                        let intersects = bx1 > vx0 && bx0 < vx1 && by1 > vy0 && by0 < vy1;
                        if !intersects {
                            continue;
                        }
                    }
                }
            }

            // Fixed: push counter-scroll to cancel ALL accumulated scroll
            let has_fixed_counter = child_is_fixed
                && (new_cumulative_scroll.0.abs() > 0.001 || new_cumulative_scroll.1.abs() > 0.001);
            if has_fixed_counter {
                ctx.push_transform(Transform::translate(
                    -new_cumulative_scroll.0,
                    -new_cumulative_scroll.1,
                ));
            }

            // Sticky: compute corrective offset when element would scroll past threshold
            let mut has_sticky_correction = false;
            if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = self.get_render_bounds(child_id, (0.0, 0.0)) {
                        // cb.y = element's layout y relative to parent
                        // new_cumulative_scroll.1 = total scroll from ALL ancestors
                        let visual_y = cb.y + new_cumulative_scroll.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            ctx.push_transform(Transform::translate(0.0, correction));
                            has_sticky_correction = true;
                        }
                    }
                }
            }

            let child_cumulative = if child_is_fixed {
                (0.0, 0.0) // Fixed cancels all accumulated scroll
            } else {
                new_cumulative_scroll
            };

            self.render_layer_with_motion(
                ctx,
                child_id,
                (0.0, 0.0),
                target_layer,
                children_glass_depth,
                children_inside_foreground,
                render_state,
                child_inherited_opacity,
                child_cumulative,
            );

            // Pop sticky correction
            if has_sticky_correction {
                ctx.pop_transform();
            }
            // Pop fixed counter-scroll
            if has_fixed_counter {
                ctx.pop_transform();
            }
        }

        // Restore the parent scope's cull viewport now that this
        // subtree is fully rendered. Pairs with the `set` above.
        if entered_cull {
            self.cull_viewport.set(prev_cull_viewport);
        }

        // Pop scroll transform (reverse of push order: scroll was pushed after children clip)
        if has_scroll {
            ctx.pop_transform();
        }

        // Render scrollbar overlay if this is a scroll container
        // Scrollbar is rendered after scroll transform is popped (in viewport space)
        // but before children inset clip is popped (clipped within content area)
        if effective_layer == target_layer {
            if let Some(physics) = self.scroll_physics.get(&node) {
                if let Ok(p) = physics.try_lock() {
                    let info = p.scrollbar_render_info();
                    if info.opacity > 0.01 {
                        self.render_scrollbar(ctx, bounds.width, bounds.height, &info);
                    }
                }
            }
        }

        // Compositor-path metadata. If this node had an active motion
        // binding when we entered, capture the primitive range its
        // subtree contributed plus the motion values that landed in
        // those primitives. The Phase-4 fast path reads this map to
        // patch `bounds.xy` (translate), `local_affine` + bounds-around-
        // centre (scale / rotation), and `color.a` (opacity) without
        // re-walking the tree on every spring step.
        //
        // Values read here MUST match what got composed into the
        // emitted primitives:
        //  - `node_motion_translate` already combines RenderState +
        //    binding translate; that's exactly the (tx, ty) that
        //    `push_transform` shifted children by.
        //  - `node_motion_opacity` includes the binding's opacity and
        //    the element's own opacity, multiplied. The fast path
        //    delta-applies the *ratio* `new/last` so this captures the
        //    full multiplier.
        //  - Scale / rotation come from `motion_values.resolved_scale`
        //    and the binding's rotation getter. Captured even when 1.0
        //    / 0.0 so the fast path can transition out of an animation
        //    cleanly.
        //  - Centre is the absolute (post-parent-offset) midpoint of
        //    the element, in DPI-pre-scale logical pixels. The
        //    consumer scales by the renderer's DPI factor.
        // Only record on the pass that actually emits the node's
        // primitives — i.e. the pass whose `target_layer` matches
        // the node's `effective_layer`. The walker is invoked three
        // times per frame (Background / Glass / Foreground); on the
        // two non-matching passes the subtree walks but emits
        // nothing, so the captured `(start..end)` would be an empty
        // range. Without this guard, the BG pass's correct range
        // gets clobbered by Glass / Foreground's empty range when
        // `insert` overwrites — the fast path then iterates zero
        // primitives, patches nothing, and the bar appears frozen.
        if let Some(start) = composite_bg_start.filter(|_| effective_layer == target_layer) {
            let end = ctx.bg_primitive_count();
            if end > start {
                let (binding_tx, binding_ty) = motion_bindings_ref
                    .map(|b| {
                        let tx = b
                            .translate_x
                            .as_ref()
                            .and_then(|v| v.lock().ok().map(|g| g.get()))
                            .unwrap_or(0.0);
                        let ty = b
                            .translate_y
                            .as_ref()
                            .and_then(|v| v.lock().ok().map(|g| g.get()))
                            .unwrap_or(0.0);
                        (tx, ty)
                    })
                    .unwrap_or((0.0, 0.0));
                let (state_tx, state_ty) = motion_values
                    .map(|m| m.resolved_translate())
                    .unwrap_or((0.0, 0.0));
                let last_translate = (state_tx + binding_tx, state_ty + binding_ty);
                let (state_sx, state_sy) = motion_values
                    .map(|m| m.resolved_scale())
                    .unwrap_or((1.0, 1.0));
                let binding_scale_xy = motion_bindings_ref
                    .map(|b| {
                        let s = b
                            .scale
                            .as_ref()
                            .and_then(|v| v.lock().ok().map(|g| g.get()))
                            .unwrap_or(1.0);
                        let sx = b
                            .scale_x
                            .as_ref()
                            .and_then(|v| v.lock().ok().map(|g| g.get()))
                            .unwrap_or(1.0);
                        let sy = b
                            .scale_y
                            .as_ref()
                            .and_then(|v| v.lock().ok().map(|g| g.get()))
                            .unwrap_or(1.0);
                        (sx * s, sy * s)
                    })
                    .unwrap_or((1.0, 1.0));
                let last_scale = (state_sx * binding_scale_xy.0, state_sy * binding_scale_xy.1);
                // Use the same `get_rotation()` accessor the walker
                // and fast path both consume, so timeline-driven
                // rotations (spinners) get a non-zero baseline here
                // instead of falling back to 0 and producing a
                // spurious "180° jump" on the next fast-path frame.
                let last_rotation_rad = motion_bindings_ref
                    .and_then(|b| b.get_rotation())
                    .map(|deg| deg.to_radians())
                    .unwrap_or(0.0);
                let last_opacity = node_motion_opacity;
                // Centre MUST be in absolute logical-pixel coords (the
                // consumer in `apply_binding_deltas` multiplies by DPI
                // to get a physical-pixel pivot for rotating primitive
                // centres). For a binding with no ancestor motion
                // bindings, `get_absolute_bounds(node).centre +
                // cumulative_scroll` is the right value. For a binding
                // whose subtree sits inside another motion binding's
                // translate (cn::slider's halo nested under the
                // thumb's `motion().translate_x`), the primitives are
                // baked at `layout + ancestor_translate`, but the
                // layout-bounds centre doesn't know about
                // `ancestor_translate`. Scaling around a centre that
                // lags the primitives by `ancestor_translate` produces
                // a positional error of `ancestor_translate ×
                // (new_scale - 1)`.
                //
                // Fix: apply the current transform stack — which
                // already includes every ancestor's position translate,
                // motion translate, motion binding translate, and any
                // scroll — to the node's local centre
                // `(bounds.width/2, bounds.height/2)`. The result is
                // the world-space pivot at bake time. Subsequent
                // ancestor-translate changes are handled by the
                // `inherited_shifts` pre-pass in `apply_binding_deltas`,
                // which advances `meta.centre` by the per-frame
                // ancestor delta to keep it tracking world space.
                //
                // Note: even though the binding's own scale + rotation
                // around the same centre have already been pushed by
                // this point, they leave the centre point fixed
                // (`T(c)*S*T(-c)` fixes `c`), so `transform_point`
                // gives the right answer regardless of where this sits
                // in the sequence above.
                // `meta.centre` is stored in LOGICAL pixels — the
                // patcher (`apply_binding_deltas`) multiplies by the
                // DPI scale factor when it needs physical-pixel pivot
                // for rotating / scaling primitive centres.
                //
                // `ctx.current_transform()` returns a composite that
                // includes the DPI scale push made at the top of
                // `render_layer_with_motion` (line ~98), so the raw
                // `transform_point` result is in PHYSICAL pixels. Divide
                // by `scale_factor` to recover logical-pixel world
                // coords. The DPI scale is uniform and only present at
                // the bottom of the stack, so this divides out cleanly
                // regardless of any rotation / scale / translate the
                // node or its ancestors have layered on top.
                //
                // For the no-ancestor-motion case this matches the
                // legacy `get_absolute_bounds + cumulative_scroll`
                // formula bit-for-bit (both compute logical world
                // centre of the node). For nested-motion subtrees —
                // cn::slider's halo under the thumb's outer translate
                // — it correctly includes the ancestor translate that
                // `get_absolute_bounds` doesn't know about.
                let centre = {
                    let local_cx = bounds.width / 2.0;
                    let local_cy = bounds.height / 2.0;
                    let scale_factor = self.scale_factor.max(f32::EPSILON);
                    match ctx.current_transform() {
                        Transform::Affine2D(a) => {
                            let p = a.transform_point(Point::new(local_cx, local_cy));
                            (p.x / scale_factor, p.y / scale_factor)
                        }
                        // 3D path — no current consumer hits this for
                        // a 2D-bound motion node; fall back to the
                        // legacy layout-bounds calculation so the
                        // existing 3D spinner / card flip workflows
                        // keep their previous (correct-for-them)
                        // behaviour.
                        Transform::Mat4(_) => self
                            .layout_tree
                            .get_absolute_bounds(node)
                            .map(|abs| {
                                (
                                    abs.x + abs.width / 2.0 + cumulative_scroll.0,
                                    abs.y + abs.height / 2.0 + cumulative_scroll.1,
                                )
                            })
                            .unwrap_or((
                                bounds.x + bounds.width / 2.0,
                                bounds.y + bounds.height / 2.0,
                            )),
                    }
                };
                let last_screen_aabb = ctx.bg_primitive_aabb(start, end);
                // CSS-only nodes (no `MotionBindings`) reach this block
                // because they pushed `motion_subtree` to route their
                // primitives into the dynamic batch, but they have no
                // motion state for `apply_binding_deltas` to patch.
                // Skip `composite_bindings` for them — the fast-path
                // delta patcher would read default translate/scale/
                // rotation/opacity and corrupt the cached primitives
                // on every frame. The Compositor v2 dynamic-regions
                // path below still records them so they're visible to
                // the per-region dispatch.
                //
                // Also skip for P4.3 motion-subtree candidates: their
                // primitives were routed to a per-node SCRATCH batch
                // (not `cached_dynamic_batch`), so the `start..end`
                // range above indexes into scratch — not the dynamic
                // batch that `apply_binding_deltas` patches. Without
                // this gate the fast-path patcher walks scratch-range
                // indices against `cached_dynamic_batch`, corrupting
                // unrelated motion-bound primitives at those indices.
                // The symptom (cn::switch Dark mode distortion when
                // Notifications is toggled): Notifications becomes a
                // P4.3 candidate, scratch indices `[0..1]` get
                // recorded as Notifications' `primitive_range`,
                // `apply_binding_deltas` patches `cached_dynamic_batch[0..1]`
                // every fast-paint frame with deltas computed from
                // Notifications' spring — and those indices happen to
                // be Dark mode's `on_layer` / thumb primitives in the
                // dynamic batch, so Dark mode visibly drifts. P4.3's
                // bake-and-blit overlay handles motion updates for
                // candidates instead.
                if motion_bindings_ref.is_some() && !pushed_for_motion_subtree {
                    self.composite_bindings.borrow_mut().insert(
                        node,
                        super::super::CompositeBindingMeta {
                            primitive_range: start..end,
                            last_translate,
                            last_scale,
                            last_rotation_rad,
                            last_opacity,
                            layer_push_index: motion_binding_layer_push_index,
                            centre,
                            last_screen_aabb,
                        },
                    );
                }

                // Compositor v2 parallel-populate for motion-bound
                // subtrees. Gated on the node's status being
                // `Animating(Motion)` or `Animating(Css)` — settled
                // bindings (no recent oscillation) sit in the static
                // cache; only bindings that the status computation
                // flagged as in-flight (or in hysteresis cooldown)
                // get a region. The legacy `composite_bindings`
                // entry above stays in place either way because the
                // fast-path delta-patcher needs the primitive range
                // for any bound node (settled or not).
                let status = self.current_animation_status.borrow().get(&node).copied();
                let kind_match = match status {
                    Some(super::super::AnimationStatus::Animating(
                        super::super::AnimatedKind::Motion,
                    )) => Some(super::super::DynamicKind::MotionSubtree),
                    Some(super::super::AnimationStatus::Animating(
                        super::super::AnimatedKind::Css,
                    )) => Some(super::super::DynamicKind::CssAnimated {
                        // Sentinel — this code path only fires for
                        // nodes that ALSO have motion bindings
                        // (v2_motion_ambient is Some), and motion
                        // takes precedence over Css in
                        // `compute_animation_status`, so in practice
                        // this arm is unreachable. The composited-
                        // layer path populates real `natural_size`
                        // values from a separate insertion site in
                        // the CSS bracket below.
                        natural_size: (0, 0),
                        ambient_clip: None,
                    }),
                    _ => None,
                };
                if let (Some(kind), Some((amb_affine, amb_opacity, amb_clip, amb_z))) =
                    (kind_match, v2_motion_ambient)
                {
                    let region_screen_aabb = Self::affine_screen_aabb(
                        &amb_affine,
                        // Bounds at this point are in parent-local
                        // coords; the parent's offset is baked into
                        // `amb_affine` already, so we transform
                        // (bounds.x, bounds.y, w, h) — NOT (0, 0,
                        // w, h) — through the parent affine.
                        // `affine_screen_aabb` takes the local-space
                        // box and the parent transform; pass bounds
                        // with origin (bounds.x, bounds.y) baked in
                        // by translating the affine first.
                        bounds.width,
                        bounds.height,
                        self.scale_factor,
                    );
                    // Shift the AABB by the node's parent-relative
                    // offset (bounds.x / bounds.y), scaled by DPI,
                    // so the captured region matches the node's
                    // actual on-screen position.
                    let dpi = self.scale_factor.max(1.0);
                    let offset_x = bounds.x * dpi;
                    let offset_y = bounds.y * dpi;
                    let shifted_aabb = [
                        region_screen_aabb[0] + offset_x,
                        region_screen_aabb[1] + offset_y,
                        region_screen_aabb[2] + offset_x,
                        region_screen_aabb[3] + offset_y,
                    ];
                    let visit_seq = self.dynamic_regions.borrow().len() as u32;
                    self.record_dynamic_region(
                        node,
                        super::super::DynamicRegion {
                            root: node,
                            screen_aabb: shifted_aabb,
                            ambient: super::super::AmbientPaintState {
                                affine: amb_affine,
                                opacity: amb_opacity,
                                clip_aabb: amb_clip,
                                z_layer: amb_z,
                            },
                            kind,
                            visit_seq,
                        },
                    );
                }
            }
        }

        // Phase 4b: CSS-animated paint record. Same `effective_layer
        // == target_layer` gate as the `composite_bindings` block
        // above — only the pass that actually emits this node's
        // primitives gets to record, otherwise the BG-pass entry
        // would be clobbered by Glass / Foreground's empty range.
        // `apply_css_deltas` reads this map to patch the cached
        // batch from current `css_anim_store` values.
        //
        // First cut populates the fields the walker has easy access
        // to here (opacity from `node_motion_opacity`, colours /
        // corner radius / border / shadow / filter / 3D rotations
        // from `render_node.props`). The transform-decomposition
        // fields (`last_translate` / `last_scale` /
        // `last_rotation_rad`) default to identity; Phase 4c can
        // extend the recording to populate them when patching 2D
        // transforms becomes a target case.
        if let Some(start) = css_anim_bg_start.filter(|_| effective_layer == target_layer) {
            let end = ctx.bg_primitive_count();
            if end > start {
                if let Some(stable_id) = self.stable_id(node) {
                    let centre = self
                        .layout_tree
                        .get_absolute_bounds(node)
                        .map(|abs| {
                            (
                                abs.x + abs.width / 2.0 + cumulative_scroll.0,
                                abs.y + abs.height / 2.0 + cumulative_scroll.1,
                            )
                        })
                        .unwrap_or((
                            bounds.x + bounds.width / 2.0,
                            bounds.y + bounds.height / 2.0,
                        ));
                    let last_screen_aabb = ctx.bg_primitive_aabb(start, end);

                    // Extract solid background colour if present
                    // (gradient / image brushes return None — those
                    // animate via gradient_start/end_color which
                    // are out of first-cut scope).
                    let last_background_color =
                        render_node.props.background.as_ref().and_then(|brush| {
                            if let Brush::Solid(c) = brush {
                                Some([c.r, c.g, c.b, c.a])
                            } else {
                                None
                            }
                        });
                    let last_border_color =
                        render_node.props.border_color.map(|c| [c.r, c.g, c.b, c.a]);
                    let cr = &render_node.props.border_radius;
                    let last_corner_radius =
                        [cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left];
                    let last_border_width = render_node.props.border_width;
                    // CssAnimPaintMeta records the FIRST shadow layer only —
                    // composite-layer fast-path animations interpolate one
                    // shadow. Multi-layer animation needs a per-layer
                    // CssAnimPaintMeta extension (follow-up).
                    let (last_shadow_params, last_shadow_color) =
                        match render_node.props.shadow.first() {
                            Some(s) => (
                                [s.offset_x, s.offset_y, s.blur, s.spread],
                                [s.color.r, s.color.g, s.color.b, s.color.a],
                            ),
                            None => ([0.0; 4], [0.0; 4]),
                        };
                    let (last_filter_a, last_filter_b) = match &render_node.props.filter {
                        Some(f) => (
                            [f.grayscale, f.invert, f.sepia, f.hue_rotate.to_radians()],
                            [f.brightness, f.contrast, f.saturate, 0.0],
                        ),
                        None => ([0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 1.0, 0.0]),
                    };
                    let last_rotate_x_rad = render_node.props.rotate_x.unwrap_or(0.0).to_radians();
                    let last_rotate_y_rad = render_node.props.rotate_y.unwrap_or(0.0).to_radians();

                    // Skip `CssAnimPaintMeta` for composite-promoted
                    // nodes — their primitives live in a per-node
                    // scratch batch (now rasterized into a
                    // `LayerTexture`), NOT in `cached_bg_batch`.
                    // Inserting the record would point
                    // `primitive_range` at scratch indices; the next
                    // frame's `apply_css_deltas` would treat those
                    // as indices into `cached_bg_batch` and mutate
                    // unrelated primitives there (corruption). The
                    // composite-layer path handles all animation
                    // updates for promoted nodes via per-frame
                    // `blit_tight_texture_to_target` instead.
                    if !pushed_composite_layer {
                        self.css_anim_paint_records.borrow_mut().insert(
                            node,
                            super::super::CssAnimPaintMeta {
                                stable_id,
                                primitive_range: start..end,
                                layer_push_index: css_anim_layer_push_index,
                                last_opacity: node_motion_opacity,
                                last_translate: (0.0, 0.0),
                                last_scale: (1.0, 1.0),
                                last_rotation_rad: 0.0,
                                last_rotate_x_rad,
                                last_rotate_y_rad,
                                last_background_color,
                                last_border_color,
                                last_corner_radius,
                                last_border_width,
                                last_shadow_params,
                                last_shadow_color,
                                last_filter_a,
                                last_filter_b,
                                centre,
                                last_screen_aabb,
                            },
                        );
                    }

                    // Composite-layer DynamicRegion. Only emit for
                    // CSS-animated nodes that the promotion predicate
                    // selected — the walker routed their emit into
                    // a per-node scratch batch (see the
                    // `pushed_composite_layer` branch earlier in
                    // this fn). `natural_size` is the screen-pixel
                    // dimensions of the emitted primitives' AABB
                    // (physical pixels — `last_screen_aabb` is post-
                    // DPI). The compositor reads this region's
                    // scratch batch via
                    // `GpuPaintContext::take_composite_layer_batches`,
                    // rasterizes it into a `LayerTexture` of size
                    // `natural_size`, and blits it per frame with
                    // the active animation transform applied.
                    //
                    // Captured at the CSS bracket (not the motion
                    // bracket above) so we don't need
                    // `v2_motion_ambient` to be Some — pure-CSS
                    // animated nodes have no motion bindings.
                    if pushed_for_css {
                        let aabb = last_screen_aabb.unwrap_or([0.0, 0.0, 0.0, 0.0]);
                        let natural_w = aabb[2].max(1.0).ceil() as u32;
                        let natural_h = aabb[3].max(1.0).ceil() as u32;
                        let ambient = super::super::AmbientPaintState {
                            affine: ctx.current_affine_elements(),
                            opacity: ctx.current_opacity(),
                            clip_aabb: ctx.current_clip_aabb(),
                            z_layer: ctx.z_layer(),
                        };
                        let visit_seq = self.dynamic_regions.borrow().len() as u32;
                        self.record_dynamic_region(
                            node,
                            super::super::DynamicRegion {
                                root: node,
                                screen_aabb: aabb,
                                ambient,
                                kind: super::super::DynamicKind::CssAnimated {
                                    natural_size: (natural_w, natural_h),
                                    ambient_clip: css_anim_ancestor_clip,
                                },
                                visit_seq,
                            },
                        );
                    }
                }
            }
        }
        // Phase 4.3 sibling of the CssAnimated record above. For
        // motion-bound texture candidates the in_css_subtree gate
        // doesn't fire, so we record outside that block. The bake
        // hook in `try_render_with_compositor` looks up `screen_aabb`
        // + `natural_size` here exactly like it does for the CSS
        // path.
        if pushed_for_motion_subtree {
            let start = motion_subtree_bg_start.unwrap_or(0);
            let end = ctx.bg_primitive_count();
            let aabb = ctx
                .bg_primitive_aabb(start, end)
                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let natural_w = aabb[2].max(1.0).ceil() as u32;
            let natural_h = aabb[3].max(1.0).ceil() as u32;
            let ambient = super::super::AmbientPaintState {
                affine: ctx.current_affine_elements(),
                opacity: ctx.current_opacity(),
                clip_aabb: ctx.current_clip_aabb(),
                z_layer: ctx.z_layer(),
            };
            let visit_seq = self.dynamic_regions.borrow().len() as u32;
            self.record_dynamic_region(
                node,
                super::super::DynamicRegion {
                    root: node,
                    screen_aabb: aabb,
                    ambient,
                    kind: super::super::DynamicKind::MotionSubtreeTexture {
                        natural_size: (natural_w, natural_h),
                        ambient_clip: ambient_clip_for_bake,
                    },
                    visit_seq,
                },
            );
        }

        // Pop children inset clip (pushed before scroll, so popped after)
        if push_children_clip {
            ctx.pop_clip();
        }

        // Pop outer overflow clip (only pushed for non-bordered elements)
        if push_outer_clip {
            ctx.pop_clip();
        }

        // Pop clip-path
        if has_clip_path {
            ctx.pop_clip();
        }

        // Pop opacity layer (must be after clips, before transforms)
        if should_push_layer {
            ctx.pop_layer();
        }

        // Pop element transforms
        if has_element_transform {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding rotation (3 transforms for centering)
        if has_binding_rotation {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding scale (3 transforms for centering)
        if has_binding_scale {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion binding translation (1 transform)
        if has_binding_transform {
            ctx.pop_transform();
        }

        // Pop motion scale transforms (from RenderState motion)
        if has_motion_scale {
            ctx.pop_transform();
            ctx.pop_transform();
            ctx.pop_transform();
        }

        // Pop motion translation
        if motion_values
            .map(|m| {
                let (tx, ty) = m.resolved_translate();
                tx.abs() > 0.001 || ty.abs() > 0.001
            })
            .unwrap_or(false)
        {
            ctx.pop_transform();
        }

        // Clear 3D transient state
        if has_3d {
            ctx.clear_3d();
        }

        // Clear CSS filter transient state
        if has_filter {
            ctx.clear_css_filter();
        }

        // Clear mask gradient transient state
        if has_mask_gradient {
            ctx.clear_mask_gradient();
        }

        // (corner_shape already cleared before children — see above)

        // Restore z_layer after this subtree
        if has_z_index {
            ctx.set_z_layer(saved_z_layer);
        }

        // Pop position transform
        ctx.pop_transform();

        // Balance the motion-subtree push from earlier so the
        // dynamic-batch emit gate returns to depth-0 at this node's
        // exit. Earlier early-return paths handle their own pop.
        if in_motion_subtree {
            ctx.pop_motion_subtree();
        }
        if in_overlay_subtree_node {
            ctx.pop_overlay_subtree();
        }
        // Balance the composite-layer push (same reasoning — the
        // walker emits this node's primitives into the per-node
        // scratch batch; once we exit, route emits back to the
        // surrounding scope).
        if pushed_composite_layer {
            ctx.pop_composite_layer();
        }
    }
}
