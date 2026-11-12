//! Frame phases 1 and 2: decide whether the tree needs a rebuild, then rebuild it.

#![allow(
    unused_variables,
    unused_mut,
    unused_assignments,
    clippy::too_many_arguments
)]

use super::super::frame::FrameVars;
use crate::windowed::{
    RefDirtyFlag, SharedAnimationScheduler, SharedElementRegistry, SharedReactiveGraph,
    SharedReadyCallbacks, WindowState, WindowedContext, invoke_ui_builder,
};
use crate::{ElementBuilder, RenderTree, div, reset_call_counters};
use blinc_core::reactive::State;
use blinc_layout::prelude::*;
use blinc_platform::{ControlFlow, Window};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Re-applies the stylesheet after `RenderTree::incremental_update`.
///
/// The update replaces node props with the new element tree's props, which
/// carry no style from class selectors (`collect_render_props` only applies
/// ID-based CSS). Only the replaced nodes are restyled, layout rules included,
/// so the cost follows the change and not the size of the tree.
fn restyle_after_incremental_update(
    tree: &mut RenderTree,
    result: blinc_layout::UpdateResult,
    router: Option<&blinc_layout::event_router::EventRouter>,
) {
    use blinc_layout::UpdateResult;

    match result {
        UpdateResult::NoChanges => {
            tracing::debug!("Incremental update: NoChanges - skipping rebuild");
        }
        UpdateResult::VisualOnly => {
            tracing::debug!("Incremental update: VisualOnly - skipping layout");
            tree.restyle_replaced_nodes(router);
        }
        UpdateResult::LayoutChanged => {
            tracing::debug!("Incremental update: LayoutChanged - recomputing layout");
            tree.restyle_replaced_nodes(router);
        }
        UpdateResult::ChildrenChanged => {
            tracing::debug!("Incremental update: ChildrenChanged - subtrees rebuilt");
            tree.apply_stylesheet_base_styles();
            tree.apply_stylesheet_layout_overrides();
        }
    }
}

/// Phase 1: check whether the tree structure needs a rebuild.
pub(super) fn check_rebuild(
    ws: &mut WindowState,
    window: &blinc_platform_desktop::DesktopWindow,
    frame_dirty: &mut Arc<AtomicBool>,
    ref_dirty_flag: &mut RefDirtyFlag,
    reactive: &mut SharedReactiveGraph,
    animations: &mut SharedAnimationScheduler,
    element_registry: &mut SharedElementRegistry,
    v: &mut FrameVars,
) -> Option<ControlFlow> {
    let (Some(blinc_app), Some(config), Some(windowed_ctx), Some(rs)) = (
        &mut ws.app,
        &ws.surface_config,
        &mut ws.ctx,
        &mut ws.render_state,
    ) else {
        return None;
    };
    let FrameVars {
        mut frame_start,
        mut t_phase1,
        mut t_phase2,
        mut t_phase3,
        mut t_phase4,
        mut t_phase5,
        mut did_rebuild,
        mut dirty_spring_count,
        mut current_time,
        mut scroll_animating,
        mut dt_ms,
        mut theme_animating,
        mut needs_vsync,
        mut css_active,
        mut css_only_composite_promotable,
        mut has_visible_overlays,
        mut keep_alive,
    } = *v;
    // =========================================================
    // PHASE 1: Check if tree structure needs rebuild
    // Only structural changes require tree rebuild
    // =========================================================
    let phase1_start = std::time::Instant::now();

    #[cfg(all(
        feature = "css-hot-reload",
        debug_assertions,
        any(target_os = "macos", target_os = "linux", target_os = "windows")
    ))]
    if windowed_ctx.reload_changed_css() {
        ws.needs_rebuild = true;
        blinc_app.invalidate_render_cache_tagged("css-reload");
    }

    // Check if event handlers marked anything dirty (auto-rebuild).
    //
    // The `tracing::info!` calls below are diagnostic for the
    // double-build-on-launch investigation: every rebuild source
    // names itself so the source of a spurious rebuild is obvious
    // from the default log output. Lower to `debug!` once the
    // root cause is fixed and the diagnostic isn't needed.
    if let Some(ref tree) = ws.render_tree {
        if tree.needs_rebuild() {
            tracing::info!(
                target: "blinc_app::rebuild_source",
                "Rebuild triggered by: dirty_tracker"
            );
            ws.needs_rebuild = true;
        }
    }

    // Check if element refs were modified (triggers rebuild)
    if ref_dirty_flag.swap(false, Ordering::SeqCst) {
        tracing::info!(
            target: "blinc_app::rebuild_source",
            "Rebuild triggered by: ref_dirty_flag (State::set or ctx.request_rebuild)"
        );
        ws.needs_rebuild = true;
    }

    // Check if text widgets requested a rebuild (focus/text changes)
    if blinc_layout::widgets::take_needs_rebuild() {
        tracing::info!(
            target: "blinc_app::rebuild_source",
            "Rebuild triggered by: blinc_layout::widgets::request_rebuild \
             (text-input focus change, request_full_rebuild from theme, etc.)"
        );
        ws.needs_rebuild = true;
    }

    // Check if a full relayout was requested (e.g., theme changes)
    if blinc_layout::widgets::take_needs_relayout() {
        tracing::info!(
            target: "blinc_app::rebuild_source",
            "Relayout triggered by: blinc_layout::widgets::request_relayout \
             (theme color scheme change, font reload, etc.)"
        );
        ws.needs_relayout = true;
    }

    // Check if CSS stylesheets need reparsing (e.g., theme color scheme changed)
    // This must happen before tree rebuild so the new stylesheet is available
    if blinc_layout::widgets::take_needs_css_reparse() {
        tracing::debug!("Reparsing CSS stylesheets due to theme change");
        windowed_ctx.reparse_css();
    }

    // Process pending motion exit starts BEFORE overlay update
    // This is critical: when an overlay closes, it queues a motion exit via
    // query_motion(key).exit(). The overlay's update() method then checks
    // if the motion is done animating. If we don't process the exit queue
    // first, the motion won't be in Exiting state yet, and update() will
    // incorrectly think the exit animation is complete.
    rs.process_global_motion_exit_starts();
    rs.process_global_motion_exit_cancels();
    // Process suspended motion starts queued via query_motion(key).start()
    rs.process_global_motion_starts();

    // Sync motion states to shared store so overlay can query them
    // This must happen after processing exits but before overlay update
    rs.sync_shared_motion_states();

    // Update overlay manager viewport and state for subtree rebuilds
    // This must happen BEFORE checking is_dirty() so build_overlay_layer() works correctly
    windowed_ctx.overlay_manager.set_viewport_with_scale(
        windowed_ctx.width,
        windowed_ctx.height,
        windowed_ctx.scale_factor as f32,
    );
    windowed_ctx.overlay_manager.update(current_time);
    // Input-driven dismissals (backdrop, Escape)
    // queue their callbacks under the stack's
    // mutex; run them here, where nothing holds
    // it. See `drain_close_callbacks`.
    blinc_layout::widgets::overlay_stack::drain_close_callbacks();
    // Phase 3 transition: also tick the new OverlayStack +
    // ToastTray. Each is the authoritative manager for the
    // widgets that have already been migrated.
    {
        use blinc_layout::overlay_state::{overlay_stack, toast_tray};
        if let Ok(mut s) = overlay_stack().lock() {
            s.set_viewport_with_scale(
                windowed_ctx.width,
                windowed_ctx.height,
                windowed_ctx.scale_factor as f32,
            );
            s.update(current_time);
        }
        if let Ok(mut t) = toast_tray().lock() {
            t.update(current_time);
        }
    }

    // Check if overlay content changed (new overlay opened/closed)
    // NOTE: We only rebuild on actual content changes, NOT during animations.
    // Animation visual updates (backdrop opacity, motion transforms) are handled
    // by the motion system and render-time interpolation, not content rebuilds.
    // Rebuilding during animation breaks event handlers because node IDs change.
    let overlay_content_dirty = windowed_ctx.overlay_manager.is_dirty();

    if overlay_content_dirty {
        tracing::debug!(
            "Overlay rebuild: dirty={}, has_visible={}",
            overlay_content_dirty,
            windowed_ctx.overlay_manager.has_visible_overlays()
        );
        // Look up the overlay layer node by its element ID
        if let Some(overlay_node_id) =
            element_registry.get(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID)
        {
            tracing::debug!(
                "Overlay changed - queueing subtree rebuild for node {:?}",
                overlay_node_id
            );
            // Build the new overlay content and queue the subtree rebuild
            // What the overlays create goes when the layer is rebuilt next.
            let owner = blinc_core::owner::Owner::default();
            let overlay_content = owner.run(|| windowed_ctx.overlay_manager.build_overlay_layer());
            blinc_layout::stateful::queue_subtree_rebuild_owned(
                overlay_node_id,
                overlay_content,
                owner,
            );
        } else {
            tracing::warn!(
                "Overlay changed but node '{}' not found in registry - will rebuild on next frame",
                blinc_layout::widgets::overlay::OVERLAY_LAYER_ID
            );
        }
        // Consume the dirty flag
        windowed_ctx.overlay_manager.take_dirty();
    }

    // Phase 3 transition: same incremental rebuild path
    // for the new OverlayStack + ToastTray. Generic helper
    // handles the dirty-check + registry-lookup +
    // queue_subtree_rebuild dance per surface, so adding
    // a new overlay layer in the future is a one-liner.
    {
        use blinc_layout::overlay_state::{
            overlay_stack, rebuild_overlay_subtree_if_dirty, toast_tray,
        };
        use blinc_layout::widgets::overlay_stack::OVERLAY_STACK_LAYER_ID;
        use blinc_layout::widgets::toast_tray::TOAST_TRAY_LAYER_ID;

        rebuild_overlay_subtree_if_dirty(
            &element_registry,
            OVERLAY_STACK_LAYER_ID,
            overlay_stack()
                .lock()
                .map(|s| s.take_dirty())
                .unwrap_or(false),
            || {
                overlay_stack()
                    .lock()
                    .ok()
                    .map(|s| s.build_overlay_layer())
                    .unwrap_or_else(div)
            },
        );

        let viewport = (windowed_ctx.width, windowed_ctx.height);
        rebuild_overlay_subtree_if_dirty(
            &element_registry,
            TOAST_TRAY_LAYER_ID,
            toast_tray().lock().map(|t| t.take_dirty()).unwrap_or(false),
            || {
                toast_tray()
                    .lock()
                    .ok()
                    .map(|t| t.build_tray_layer(viewport))
                    .unwrap_or_else(div)
            },
        );
    }

    // Check if stateful elements requested a redraw (hover/press changes)
    // Apply incremental prop updates without full rebuild
    let has_stateful_updates = blinc_layout::take_needs_redraw();
    let has_pending_rebuilds = blinc_layout::has_pending_subtree_rebuilds();

    if has_stateful_updates || has_pending_rebuilds {
        if has_stateful_updates {
            tracing::debug!("Redraw requested by: stateful state change");
        }

        // Drain the unified property channel
        // ([[project-reactive-architecture-v2]]). Accumulated
        // side effects decide whether the drain forces a layout
        // pass.
        let prop_updates = ws
            .render_tree
            .as_ref()
            .map(|tree| tree.take_pending_property_updates())
            .unwrap_or_default();
        let had_prop_updates = !prop_updates.is_empty();
        let mut prop_effects = blinc_layout::SideEffects::default();
        if let Some(ref mut tree) = ws.render_tree {
            prop_effects = prop_effects.or(tree.apply_partial_property_updates(prop_updates));
        }

        // Process subtree rebuilds (from stateful changes OR overlay changes).
        // Pass the router so the per-rebuild base-style pass
        // re-applies matching `:focus` / `:hover` / `:active`
        // rules on top of the just-written base CSS — without
        // this, animation-tick frames (a Stateful refresh while
        // a spring is still running) reset `.cn-input` etc to
        // their idle CSS and Phase 4's gated state pass skips,
        // leaving the popup looking unfocused until a mouse-
        // move bumps the router fingerprint.
        let mut needs_layout = prop_effects.needs_layout;
        let mut had_subtree_rebuild = false;
        if let Some(ref mut tree) = ws.render_tree {
            let rebuilt =
                tree.process_pending_subtree_rebuilds_routed(Some(&windowed_ctx.event_router));
            needs_layout |= rebuilt;
            had_subtree_rebuild = rebuilt;
        }
        // Subtree rebuild only mutates render
        // nodes / base_styles / registry; it
        // does NOT set ws.did_rebuild and does
        // NOT touch the compositor cache. Next
        // frame's fast path would blit stale
        // bg + dispatch stale dyn batch until
        // some other input invalidated the
        // cache. Visible as a freshly-focused
        // cn::input losing its focused bg /
        // border for ~200ms post-open: the
        // Stateful refreshed Idle → Focused
        // but the cached primitives still hold
        // the Idle visuals. Invalidate here so
        // the post-rebuild paint runs the slow
        // walker once and repopulates.
        if had_subtree_rebuild {
            blinc_app.invalidate_render_cache_tagged("subtree_rebuild_applied");
        }

        // Drain deferred-focus queues NOW that the
        // subtree rebuilds have actually applied
        // (Stateful::build set the focused widget's
        // node_id). Previously this ran BEFORE the
        // rebuild processing, so the focus was
        // applied to the data cell but
        // focused_text_input_node_id() returned
        // None (stateful_state.node_id still
        // unset). The downstream EventRouter
        // focus-sync at Phase 4 then read None,
        // didn't bridge focus to the cn::input's
        // node, and the CSS :focus selectors
        // never matched — visible as the input
        // appearing to lose its focus border /
        // bg the first few frames after open
        // (the cn::textarea case got lucky
        // because the mouse landed on it,
        // putting the FSM in FocusedHovered via
        // POINTER_ENTER instead). Draining post-
        // rebuild means node_id is set when
        // focus_text_input runs, so the very
        // next focus-sync sees the right
        // LayoutNodeId and the input stays
        // visually focused from frame 1.
        blinc_layout::widgets::process_pending_input_focus();
        blinc_layout::widgets::process_pending_area_focus();

        // focus_text_input / focus_text_area
        // drive the Stateful's `shared.state`
        // to Focused and call `refresh_stateful`,
        // which routes through `refresh_props_internal`
        // and `queue_prop_update(node, focused_props)`.
        // The drain at the top of this block
        // already ran before focus, so the
        // freshly-queued Focused props would sit
        // in the partial-update channel until the
        // NEXT frame — meaning this frame's slow
        // walker (the one we just invalidated for
        // above) still reads the Idle render
        // props baked by the rebuild and the
        // popup paints with no focus bg/border
        // until either a mouse-move triggers
        // another invalidation or the prop
        // update lands. Drain again now so the
        // Focused props flow into render_node.props
        // before Phase 4 paints this frame.
        let post_focus_updates = ws
            .render_tree
            .as_ref()
            .map(|tree| tree.take_pending_property_updates())
            .unwrap_or_default();
        let had_post_focus_updates = !post_focus_updates.is_empty();
        if had_post_focus_updates {
            if let Some(ref mut tree) = ws.render_tree {
                prop_effects =
                    prop_effects.or(tree.apply_partial_property_updates(post_focus_updates));
            }
            needs_layout |= prop_effects.needs_layout;
            blinc_app.invalidate_render_cache_tagged("post_focus_prop_update");
        }

        // `refresh_props_internal` (driven by
        // focus_text_input / focus_text_area
        // via refresh_stateful) queues BOTH a
        // partial prop update AND a subtree
        // rebuild — the rebuild lives in
        // PENDING_SUBTREE_REBUILDS, NOT in the
        // partial channel, so the drain above
        // did NOT process it. cn::input /
        // cn::textarea hit the topology
        // rebuild path because their on_state
        // callback adds the cursor canvas only
        // when visual.is_focused(); Idle →
        // Focused therefore queues a full
        // structural rebuild whose post-step
        // calls apply_stylesheet_base_styles_for_subtree
        // and silently clobbers the focused
        // bg/border with the `.cn-input` /
        // `.cn-textarea` base CSS rule. Phase 4's
        // apply_stylesheet_state_styles is gated
        // on a router-state-fingerprint change;
        // focus didn't change between the
        // build frame and the next paint frame,
        // so the gate fails and `:focus` never
        // re-applies. The popup appears
        // unfocused until a mouse-move bumps
        // the fingerprint. Fix: unconditionally
        // drain the queued subtree rebuilds and
        // clear last_router_state_fp here so
        // (1) the focus-triggered rebuild
        // applies on the SAME frame as focus,
        // and (2) Phase 4 re-runs the state-
        // style pass and restores the :focus
        // visuals on top of the just-clobbered
        // base.
        let pending_after_focus = blinc_layout::has_pending_subtree_rebuilds();
        if had_post_focus_updates || pending_after_focus {
            // Bridge EventRouter.focus to the
            // freshly-focused text input BEFORE
            // the rebuild runs, so the routed
            // base-pass sees the correct focused
            // node when it evaluates `:focus`
            // selectors against the subtree.
            // (Phase 4's standard focus sync at
            // line ~6354 runs too late for the
            // state pass that fires inside
            // process_pending_subtree_rebuilds_routed.)
            let text_focus = blinc_layout::widgets::text_input::focused_text_input_node_id()
                .or_else(blinc_layout::widgets::text_input::focused_text_area_node_id);
            windowed_ctx.event_router.sync_text_focus(text_focus);
            if let Some(ref mut tree) = ws.render_tree {
                if tree.process_pending_subtree_rebuilds_routed(Some(&windowed_ctx.event_router)) {
                    needs_layout = true;
                }
            }
            blinc_app.invalidate_render_cache_tagged("post_focus_subtree_rebuild");
        }

        if needs_layout {
            if let Some(ref mut tree) = ws.render_tree {
                tracing::debug!("Subtree rebuilds processed, recomputing layout");
                tree.apply_stylesheet_layout_overrides();
                tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                // FLIP: detect position changes and start CSS transitions
                tree.apply_flip_transitions();
                // Update FLIP bounds cache for next rebuild
                tree.update_flip_bounds();
                // Begin/end motion frame to track which motions are still in tree
                rs.begin_stable_motion_frame();
                tree.initialize_motion_animations(rs);
                rs.end_stable_motion_frame();
                rs.process_global_motion_replays();
                // Start CSS animations for elements with animation properties
                tree.start_all_css_animations();
            }
            // Re-run hit-test at the current mouse position so newly
            // mounted subtrees (notably dropdown / context-menu items
            // that just appeared under the stationary cursor) get
            // POINTER_ENTER + CSS :hover immediately. Without this,
            // hover state only updates on the next mouse-move event —
            // so a freshly-opened dropdown shows no hover bg on the
            // item the cursor is already over.
            if let Some(ref mut tree) = ws.render_tree {
                let (mx, my) = windowed_ctx.event_router.mouse_position();
                if mx.is_finite() && my.is_finite() {
                    let _ = windowed_ctx.event_router.on_mouse_move(tree, mx, my);
                }
            }
        }
        if had_prop_updates && !needs_layout {
            tracing::trace!("Visual-only prop updates, skipping layout");
        }

        // Stateful animations (springs, keyframes,
        // timelines) queue prop updates here when their
        // scheduler-driven `refresh_callback` re-runs
        // the `on_state` callback. Without an explicit
        // cache invalidate, the fast path's
        // `cached_bg_batch` would keep the previous
        // frame's primitives — and the walker (which is
        // what reads `render_node.props` to emit fresh
        // primitives) doesn't run on the fast path.
        // Symptom: timeline_demo's bouncing ball,
        // motion_demo's pull-to-refresh contents, and
        // any other stateful-animation-driven visual
        // stay frozen on the previous frame until some
        // input (mouse move, scroll) trips a different
        // invalidate. The state-driven path through
        // `handle_event_internal` already covers
        // event-fired transitions via `state_changed`
        // above; this catches the scheduler-driven half.
        if had_prop_updates {
            blinc_app.invalidate_render_cache_tagged("stateful_prop_update");
        }

        // Structural subtree rebuilds replace render
        // nodes wholesale (new ids, fresh props from
        // `collect_render_props_boxed`, CSS class base
        // styles re-applied). The static-cache batch
        // still holds the *previous* frame's
        // primitives, so the first paint after a
        // dropdown-open / overlay-push / stateful
        // structural transition keeps showing stale
        // visuals (search input renders without its
        // configured bg, freshly-mounted `--selected`
        // rows show no highlight, etc.) until the
        // next user input trips `state_changed`. The
        // gate above only fires on prop updates;
        // mirror it here for the rebuild path. Same
        // root cause class as the `motion_just_settled`
        // cache-stamping fix — the walker has new data
        // but the compositor's cache predicate hasn't
        // been told to discard the old.
        if has_pending_rebuilds {
            blinc_app.invalidate_render_cache_tagged("structural_subtree_rebuild");
        }

        // Visual-only updates (e.g. hover state flip)
        // happened mid-frame — make sure the next
        // frame renders rather than getting skipped
        // by the start-of-frame dirty gate.
        frame_dirty.store(true, Ordering::Release);
        window.request_redraw();
    }

    t_phase1 = phase1_start.elapsed();

    *v = FrameVars {
        frame_start,
        t_phase1,
        t_phase2,
        t_phase3,
        t_phase4,
        t_phase5,
        did_rebuild,
        dirty_spring_count,
        current_time,
        scroll_animating,
        dt_ms,
        theme_animating,
        needs_vsync,
        css_active,
        css_only_composite_promotable,
        has_visible_overlays,
        keep_alive,
    };
    None
}

/// Phase 2: build or rebuild the tree for structural changes.
pub(super) fn build_tree<F, E>(
    ws: &mut WindowState,
    window: &blinc_platform_desktop::DesktopWindow,
    animations: &mut SharedAnimationScheduler,
    css_anim_store: &mut Arc<Mutex<blinc_layout::CssAnimationStore>>,
    element_registry: &mut SharedElementRegistry,
    ready_callbacks: &mut SharedReadyCallbacks,
    ui_builder: &mut F,
    v: &mut FrameVars,
) -> Option<ControlFlow>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    let (Some(blinc_app), Some(config), Some(windowed_ctx), Some(rs)) = (
        &mut ws.app,
        &ws.surface_config,
        &mut ws.ctx,
        &mut ws.render_state,
    ) else {
        return None;
    };
    let FrameVars {
        mut frame_start,
        mut t_phase1,
        mut t_phase2,
        mut t_phase3,
        mut t_phase4,
        mut t_phase5,
        mut did_rebuild,
        mut dirty_spring_count,
        mut current_time,
        mut scroll_animating,
        mut dt_ms,
        mut theme_animating,
        mut needs_vsync,
        mut css_active,
        mut css_only_composite_promotable,
        mut has_visible_overlays,
        mut keep_alive,
    } = *v;
    // =========================================================
    // PHASE 2: Build/rebuild tree only for structural changes
    // This must happen BEFORE tick() so motion animations are available
    // =========================================================
    let phase2_start = std::time::Instant::now();

    // Begin stable motion frame tracking
    // This clears the "used" set so we can detect which motions are no longer in the tree
    rs.begin_stable_motion_frame();

    // Hot-reload: a `subsecond::apply_patch` succeeded since
    // the last frame, so the user's UI closure body has been
    // swapped underneath us. Force a full tree rebuild so the
    // closure (wrapped in `subsecond::call`) gets re-invoked
    // — otherwise we'd keep painting the cached pre-patch tree.
    //
    // Also reset accumulated stylesheet state so `ctx.add_css`
    // calls in the patched closure produce a fresh sheet. The
    // common `rebuild_count == 0` guard around `add_css` would
    // otherwise skip re-registration, and `add_css` itself
    // cascades — deleted CSS rules would linger forever.
    #[cfg(feature = "hot-reload")]
    {
        // Drain dx asset invalidations FIRST, so cached
        // decoded copies are gone before the rebuild
        // re-loads anything. Path-keyed: image cache,
        // SVG cache, SVG atlas. Glyph caches and font
        // faces aren't path-keyed and would need
        // separate plumbing — out of scope for this
        // pass.
        let asset_paths = crate::hot_reload::take_invalidations();
        for path in &asset_paths {
            blinc_app.context().invalidate_asset_path(path);
        }

        if crate::hot_reload::take_rebuild_pending() {
            tracing::info!("hot-reload: forcing tree rebuild");
            ws.needs_rebuild = true;
            // The builder runs again and the diff
            // carries the edit into the live tree.
            // Keeping the tree keeps scroll offsets,
            // focus and node identity, and costs one
            // incremental update instead of a full
            // build of the whole window.
            //
            // CSS is the part the diff can't see: an
            // edited stylesheet string is re-parsed
            // into `windowed_ctx.stylesheet`, but the
            // rules only reach nodes when a stylesheet
            // pass runs, and `NoChanges` / `VisualOnly`
            // don't run one. Flag it for the pass below.
            ws.hot_reload_restyle = true;
            windowed_ctx.reset_for_hot_reload();
            // The tree updates but the renderer's cached
            // scene doesn't, so a reloaded literal would
            // composite under the glyphs of the string
            // it replaced.
            blinc_app.invalidate_render_cache_tagged("hot-reload");
        }
    }

    if ws.needs_rebuild || ws.render_tree.is_none() {
        // What the widgets create while building, the effects above
        // all, belongs to this build and goes when the next replaces it.
        let build_pass = ws.build_scope.begin();
        // Reset call counters for stable key generation
        reset_call_counters();
        blinc_layout::click_outside::clear_click_outside_handlers();

        // Reset stable motions so they replay on full rebuild
        // This ensures motion animations play when UI is reconstructed
        rs.reset_stable_motions_for_rebuild();

        // Note: Viewport and overlay state are already updated in PHASE 1
        // so build_overlay_layer() has correct dimensions

        // Build UI element tree
        let user_ui = invoke_ui_builder(&mut *ui_builder, windowed_ctx);

        // Drain any CSS queued via
        // `BlincContextState::queue_stylesheet`
        // (e.g. by `BlincDsl::view_widget`'s
        // first invocation). Same pattern
        // as `drain_custom_passes` below —
        // the queue exists because DSL /
        // plugin code doesn't see
        // `WindowedContext` directly.
        {
            let queued = blinc_core::BlincContextState::get().drain_stylesheets();
            for css in queued {
                windowed_ctx.add_css(&css);
            }
        }

        // Compose user UI with overlay layer using a regular Div container
        // We use position:relative with the overlay absolutely positioned on top.
        let overlay_layer = windowed_ctx.overlay_manager.build_overlay_layer();
        // Phase 3 transition: the new OverlayStack composites alongside
        // the legacy manager. Each widget migration moves one widget
        // from the legacy layer to this layer; both render until
        // Phase 5 deletes the legacy module.
        let stack_layer = {
            use blinc_layout::overlay_state::overlay_stack;
            let stack = overlay_stack();
            let guard = stack.lock().ok();
            match guard {
                Some(mut s) => {
                    s.set_viewport_with_scale(
                        windowed_ctx.width,
                        windowed_ctx.height,
                        windowed_ctx.scale_factor as f32,
                    );
                    s.build_overlay_layer()
                }
                None => div(),
            }
        };
        // Toast tray composites above the overlay stack
        // (notification queue lives above modal stack).
        let tray_layer = {
            use blinc_layout::overlay_state::toast_tray;
            let viewport = (windowed_ctx.width, windowed_ctx.height);
            toast_tray()
                .lock()
                .ok()
                .map(|t| t.build_tray_layer(viewport))
                .unwrap_or_else(div)
        };
        let ui = div()
            .w(windowed_ctx.width)
            .h(windowed_ctx.height)
            .relative() // positioning context for overlay
            .child(user_ui)
            .child(overlay_layer)
            .child(stack_layer)
            .child(tray_layer);

        // A fresh tree replaces the old one when there is none or on a resize.
        let replaces_tree = ws.render_tree.is_none() || ws.needs_relayout;

        // Use incremental update if we have an existing tree
        // BUT: Skip incremental update during resize - do full rebuild instead
        // This ensures parent constraints properly propagate to all children
        if let Some(ref mut existing_tree) = ws.render_tree {
            if ws.needs_relayout {
                // Window resize: bypass incremental update, do full rebuild
                // This ensures proper constraint propagation from parents to children
                tracing::debug!("Window resize: full tree rebuild (bypassing incremental update)");

                // Clear layout bounds storages before rebuild
                existing_tree.clear_layout_bounds_storages();

                // Full rebuild: create new tree from element with shared registry
                // Pass registry to from_element_with_registry so IDs are registered during build
                let mut tree =
                    RenderTree::from_element_with_registry(&ui, Arc::clone(&element_registry));

                // Set animation scheduler for scroll bounce springs
                tree.set_animations(&windowed_ctx.animations);

                // Share the CSS animation store (ticked by scheduler thread)
                tree.set_css_anim_store(Arc::clone(&css_anim_store));

                // Set DPI scale factor for HiDPI rendering
                tree.set_scale_factor(windowed_ctx.scale_factor as f32);

                // Set CSS stylesheet for automatic style application
                if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                    tree.set_stylesheet_arc(stylesheet.clone());
                }
                // Apply CSS visual + layout styles in a single optimized pass
                // (builds class index once, iterates rules once)
                tree.apply_all_stylesheet_styles();

                // Register pointer-space elements from stylesheet
                if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                    windowed_ctx
                        .pointer_query
                        .register_from_stylesheet(stylesheet);
                }

                // Compute layout with new viewport dimensions
                tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                tree.update_flip_bounds();

                // Initialize motion animations for any nodes wrapped in motion() containers
                tree.initialize_motion_animations(rs);
                // End motion frame to detect unmounted motions and trigger exit animations
                rs.end_stable_motion_frame();
                // Process any motion replay requests queued during tree building
                rs.process_global_motion_replays();
                // Start CSS animations for elements with animation properties
                tree.start_all_css_animations();

                // Replace existing tree with fresh one
                *existing_tree = tree;

                // Clear relayout flag after full rebuild
                ws.needs_relayout = false;
            } else {
                // Normal incremental update (no resize)
                use blinc_layout::UpdateResult;

                // Update stylesheet in case it changed between frames
                if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                    existing_tree.set_stylesheet_arc(stylesheet.clone());
                }

                let update_result = existing_tree.incremental_update(&ui);

                // An edited stylesheet leaves every
                // element hash untouched, so the diff
                // reports NoChanges and no pass runs.
                // Re-apply the rules the reload just
                // re-parsed, then let the match below
                // handle whatever else changed.
                let restyle = std::mem::take(&mut ws.hot_reload_restyle);
                if restyle {
                    existing_tree.apply_all_stylesheet_styles();
                    existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                    existing_tree.update_flip_bounds();
                }

                restyle_after_incremental_update(
                    existing_tree,
                    update_result,
                    Some(&windowed_ctx.event_router),
                );

                match update_result {
                    UpdateResult::NoChanges | UpdateResult::VisualOnly => {}
                    UpdateResult::LayoutChanged => {
                        // Layout changed - recompute layout
                        existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        existing_tree.update_flip_bounds();
                    }
                    UpdateResult::ChildrenChanged => {
                        // Recompute layout since structure changed
                        existing_tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                        // FLIP: detect position changes and start CSS transitions
                        existing_tree.apply_flip_transitions();
                        existing_tree.update_flip_bounds();

                        // Re-register pointer-space elements (new elements may have pointer-space)
                        if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                            windowed_ctx
                                .pointer_query
                                .register_from_stylesheet(stylesheet);
                        }

                        // Initialize motion animations for any new nodes wrapped in motion() containers
                        existing_tree.initialize_motion_animations(rs);
                        // End motion frame to detect unmounted motions and trigger exit animations
                        rs.end_stable_motion_frame();

                        // Process any global motion replays that were queued during tree building
                        rs.process_global_motion_replays();
                        // Start CSS animations for elements with animation properties
                        existing_tree.start_all_css_animations();
                    }
                }
            }
        } else {
            // No existing tree - create new with shared registry
            let mut tree =
                RenderTree::from_element_with_registry(&ui, Arc::clone(&element_registry));

            // Set animation scheduler for scroll bounce springs
            tree.set_animations(&windowed_ctx.animations);

            // Share the CSS animation store (ticked by scheduler thread)
            tree.set_css_anim_store(Arc::clone(&css_anim_store));

            // Set DPI scale factor for HiDPI rendering
            tree.set_scale_factor(windowed_ctx.scale_factor as f32);

            // Set CSS stylesheet for automatic style application
            if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                tree.set_stylesheet_arc(stylesheet.clone());
            }
            // Apply CSS visual + layout styles in a single optimized pass
            tree.apply_all_stylesheet_styles();

            // Register pointer-space elements from stylesheet
            if let Some(ref stylesheet) = windowed_ctx.stylesheet {
                windowed_ctx
                    .pointer_query
                    .register_from_stylesheet(stylesheet);
            }

            // Compute layout in logical pixels
            tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
            tree.update_flip_bounds();

            // Initialize motion animations for any nodes wrapped in motion() containers
            tree.initialize_motion_animations(rs);
            // End motion frame to detect unmounted motions and trigger exit animations
            rs.end_stable_motion_frame();

            // Process any global motion replays that were queued during tree building
            rs.process_global_motion_replays();
            // Start CSS animations for elements with animation properties
            tree.start_all_css_animations();

            ws.render_tree = Some(tree);
        }

        ws.needs_rebuild = false;
        if replaces_tree {
            ws.build_scope.finish(build_pass);
        } else {
            ws.build_scope.keep(build_pass);
        }
        did_rebuild = true;
        let was_first_rebuild = windowed_ctx.rebuild_count == 0;
        windowed_ctx.rebuild_count = windowed_ctx.rebuild_count.saturating_add(1);

        // Execute on_ready callbacks after first rebuild
        if was_first_rebuild {
            if let Ok(mut callbacks) = ready_callbacks.lock() {
                for callback in callbacks.drain(..) {
                    callback();
                }
            }
        }
    } else {
        // No rebuild needed - still need to end the motion frame
        // If an existing tree exists, initialize motions to mark them as used
        if let Some(ref tree) = ws.render_tree {
            tree.initialize_motion_animations(rs);
        }
        rs.end_stable_motion_frame();
    }

    // Note: on_ready callbacks are only executed after the FIRST rebuild
    // (in the was_first_rebuild block above). Callbacks registered
    // after the first rebuild are executed immediately since the UI
    // is already ready at that point.

    t_phase2 = phase2_start.elapsed();

    *v = FrameVars {
        frame_start,
        t_phase1,
        t_phase2,
        t_phase3,
        t_phase4,
        t_phase5,
        did_rebuild,
        dirty_spring_count,
        current_time,
        scroll_animating,
        dt_ms,
        theme_animating,
        needs_vsync,
        css_active,
        css_only_composite_promotable,
        has_visible_overlays,
        keep_alive,
    };
    None
}

#[cfg(test)]
mod tests {
    use super::restyle_after_incremental_update;
    use blinc_core::{Brush, Color};
    use blinc_layout::div::{Div, div};
    use blinc_layout::prelude::Stylesheet;
    use blinc_layout::{RenderTree, UpdateResult};

    const CSS: &str = ".panel { background: #ff0000; }";

    /// A panel styled by a class rule next to a button whose colour is set in Rust.
    fn ui(button_color: Color, button_width: f32) -> Div {
        div()
            .w(400.0)
            .h(200.0)
            .child(div().id("panel").class("panel").w(200.0).h(100.0))
            .child(div().id("button").bg(button_color).w(button_width).h(30.0))
    }

    /// The first pass, as the windowed loop runs it for a new tree.
    fn first_pass() -> RenderTree {
        let mut tree = RenderTree::from_element(&ui(Color::BLUE, 80.0));
        tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
        tree.apply_all_stylesheet_styles();
        tree.compute_layout(400.0, 200.0);
        tree
    }

    fn panel_is_red(tree: &RenderTree) -> bool {
        let panel = tree.query_by_id("panel").expect("panel node");
        let node = tree.get_render_node(panel).expect("panel render node");
        matches!(node.props.background, Some(Brush::Solid(c)) if c == Color::RED)
    }

    /// The case that must keep working: only the button's colour changes.
    #[test]
    fn class_background_survives_a_visual_only_update() {
        let mut tree = first_pass();
        assert!(panel_is_red(&tree), "the first pass applies the class rule");

        let result = tree.incremental_update(&ui(Color::GREEN, 80.0));
        assert_eq!(result, UpdateResult::VisualOnly);
        restyle_after_incremental_update(&mut tree, result, None);

        assert!(
            panel_is_red(&tree),
            "the panel keeps its class background after a VisualOnly update"
        );
    }

    /// The case that must keep working: the button's width changes.
    #[test]
    fn class_background_survives_a_layout_changed_update() {
        let mut tree = first_pass();

        let result = tree.incremental_update(&ui(Color::BLUE, 120.0));
        assert_eq!(result, UpdateResult::LayoutChanged);
        restyle_after_incremental_update(&mut tree, result, None);

        assert!(
            panel_is_red(&tree),
            "the panel keeps its class background after a LayoutChanged update"
        );
    }

    /// A panel whose own props change: its class background must come back with the restyle.
    fn ui_with_panel_width(panel_width: f32) -> Div {
        div()
            .w(400.0)
            .h(200.0)
            .child(div().id("panel").class("panel").w(panel_width).h(100.0))
    }

    /// Proves what the tests above guard: a node whose props the update replaces loses
    /// the class background unless the restyle runs (issue #70). A node whose own hash is
    /// unchanged keeps its props and is not replaced.
    #[test]
    fn incremental_update_alone_drops_the_class_background_of_a_replaced_node() {
        let first = || {
            let mut tree = RenderTree::from_element(&ui_with_panel_width(200.0));
            tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
            tree.apply_all_stylesheet_styles();
            tree.compute_layout(400.0, 200.0);
            tree
        };

        let mut tree = first();
        assert!(panel_is_red(&tree));
        let result = tree.incremental_update(&ui_with_panel_width(220.0));
        assert_ne!(result, UpdateResult::NoChanges);
        assert!(!panel_is_red(&tree));
        restyle_after_incremental_update(&mut tree, result, None);
        assert!(panel_is_red(&tree));
    }
}
