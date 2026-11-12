//! Frame phases 4 and 5: render, present and schedule the next frame.

#![allow(
    unused_variables,
    unused_mut,
    unused_assignments,
    clippy::too_many_arguments
)]

use super::super::frame::FrameVars;
use crate::windowed::{FpsAdapter, SharedAnimationScheduler, SharedElementRegistry, WindowState};
use blinc_layout::widgets::OverlayManagerExt;
use blinc_layout::widgets::overlay::OverlayManager;
use blinc_platform::{ControlFlow, Window};
use blinc_platform_desktop::DesktopPlatform;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Phases 4 and 5: render the frame, manage overlays, request the next frame.
pub(super) fn render_and_schedule(
    ws: &mut WindowState,
    window: &blinc_platform_desktop::DesktopWindow,
    platform: &mut DesktopPlatform,
    animation_fps_cap: &mut Option<u32>,
    animation_fps_cap_atomic: &mut Arc<std::sync::atomic::AtomicU32>,
    fps_adapter: &mut Option<Arc<Mutex<FpsAdapter>>>,
    wake_proxy_for_pacing: &mut blinc_platform_desktop::WakeProxy,
    frame_dirty: &mut Arc<AtomicBool>,
    visible_anim_for_wake: &mut Arc<AtomicBool>,
    animations: &mut SharedAnimationScheduler,
    element_registry: &mut SharedElementRegistry,
    overlays: &mut OverlayManager,
    frame: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
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
    let render_tex: &wgpu::Texture = &frame.texture;
    // =========================================================
    // PHASE 4: Render
    // Combines stable tree structure with dynamic render state
    // =========================================================
    let phase4_start = std::time::Instant::now();

    // Sync text input/textarea focus to EventRouter so CSS :focus matching works
    {
        let text_focus = blinc_layout::widgets::text_input::focused_text_input_node_id()
            .or_else(blinc_layout::widgets::text_input::focused_text_area_node_id);
        windowed_ctx.event_router.sync_text_focus(text_focus);
        if let Some(tree) = ws.render_tree.as_mut() {
            tree.sync_keyboard_focus(&mut windowed_ctx.event_router);
        }
    }

    // Apply CSS state styles (:hover, :active, :focus) from stylesheet
    // This also detects property changes and starts new transitions.
    //
    // Gate on event-router state fingerprint: only run the
    // O(N) pass when the set of hovered / pressed / focused
    // nodes actually differs from the previous frame.
    // Animation-only ticks (the spinner-only steady state)
    // share the previous frame's router state, so we skip
    // the entire registered-IDs walk. First frame's
    // `last_router_state_fp = None` forces the pass.
    if let Some(ref mut tree) = ws.render_tree {
        if tree.stylesheet().is_some() {
            let current_fp = windowed_ctx.event_router.state_fingerprint();
            let should_apply = ws.last_router_state_fp != Some(current_fp);
            if should_apply {
                let state_changed = tree.apply_stylesheet_state_styles(&windowed_ctx.event_router);
                ws.last_router_state_fp = Some(current_fp);
                // Recompute layout if state styles affected layout properties
                // (e.g. visibility: hidden → display: none, or height changes on hover)
                if state_changed {
                    tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                    tree.update_flip_bounds();
                }
            }
        }
    }

    // Apply CSS animation/transition values AFTER state styles
    // (state styles reset to base, animations must override)
    if css_active
        || !ws
            .render_tree
            .as_ref()
            .is_none_or(|t| t.css_transitions_empty())
    {
        if let Some(ref mut tree) = ws.render_tree {
            tree.apply_all_css_animation_props();
            tree.apply_all_css_transition_props();
            tree.apply_flip_animation_props();
            if tree.apply_animated_layout_props() {
                tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                tree.update_flip_bounds();
            }
        }
    }

    // Update continuous pointer query state
    if !windowed_ctx.pointer_query.is_empty() {
        let (mx, my) = windowed_ctx.event_router.mouse_position();
        let is_pressed = windowed_ctx.event_router.has_pressed_target();
        let dt_sec = dt_ms / 1000.0;
        let time_sec = current_time as f64 / 1000.0;
        // Use event router's hit test results for hover detection.
        // The router already handles scroll offsets, transforms, and occlusion
        // correctly, so bounds from get_node_bounds match the rendering pipeline.
        windowed_ctx
            .pointer_query
            .update(mx, my, is_pressed, dt_sec, time_sec, |id| {
                let node = element_registry.get(id)?;
                // is_hovered is stable-id-keyed
                // now (see event_router doc);
                // use the layout-id shim that
                // resolves internally.
                if let Some(ref tree) = ws.render_tree {
                    if windowed_ctx.event_router.is_hovered_layout(tree, node) {
                        return windowed_ctx.event_router.get_node_bounds(node);
                    }
                }
                None
            });
        // Evaluate dynamic calc(env(...)) properties with current pointer state
        if let Some(ref mut tree) = ws.render_tree {
            tree.apply_pointer_styles(&windowed_ctx.pointer_query, &windowed_ctx.event_router);
        }
    }

    if let Some(ref tree) = ws.render_tree {
        // Set blend target for mix-blend-mode support
        blinc_app.set_blend_target(render_tex);

        // Pass cursor position for @flow pointer input
        let (mx, my) = windowed_ctx.event_router.mouse_position();
        let sf = windowed_ctx.scale_factor as f32;
        blinc_app.set_cursor_position(mx * sf, my * sf);

        // Drain any custom passes queued via BlincContextState
        // (e.g. SceneKit3D registering a GridPass from a closure)
        {
            let ctx_state = blinc_core::BlincContextState::get();
            for pass in ctx_state.drain_custom_passes() {
                if let Ok(typed) =
                    pass.downcast::<Box<dyn blinc_gpu::custom_pass::CustomRenderPass>>()
                {
                    blinc_app.context().register_custom_pass(*typed);
                }
            }
        }

        // Clear alpha tracks per-window transparency so a
        // mix of opaque and transparent windows can share
        // the same BlincApp.
        blinc_app.set_clear_alpha(if ws.transparent { 0.0 } else { 1.0 });

        // Compositor fast-path eligibility.
        //
        // Cache invalidation is now wired through
        // `Event::Input` (any input event clears
        // the cache), so scroll / hover / focus /
        // IME changes can't smuggle stale state
        // into the fast path — the very next
        // paint after any input takes the full
        // walker route and repopulates the cache.
        // The fast path engages between input
        // events, which is exactly when idle
        // animations (spinner / skeleton /
        // progress fill) need it.
        //
        // Per-frame gate stays conservative — any
        // frame-loop signal that the fast path's
        // delta-apply can't reproduce trips the
        // full path:
        //  - structural change (`did_rebuild`,
        //    `ws.needs_relayout`)
        //  - CSS animation / transition tick
        //    (`css_active`)
        //  - scroll physics decay
        //    (`scroll_animating`)
        //  - cold start (no cache yet,
        //    `last_paint_time_ms == 0`)
        // Per-frame gate. The walker now records
        // `composite_bindings` for every painted
        // motion-bound node (animating or not),
        // so a newly-active spring on a visible
        // node finds its target entry already in
        // the map on the very next fast-path
        // frame. Off-screen nodes are still
        // culled and unrecorded — when they
        // scroll into view, the input event
        // invalidates the cache and the next
        // full paint records them.
        // Canvas presence no longer bails the fast
        // path — `BlincApp::redraw_canvases`
        // re-invokes each recorded canvas's
        // `render_fn` into a scratch context and
        // splices the fresh primitives into the
        // cached batch, so the surrounding tree
        // stays cached and the walker doesn't run.
        // See the split-paint flow in
        // `render_tree_with_motion_opt`.
        // CSS-only animation frames take the fast path
        // when every playing animation / transition is
        // composite-promotable (opacity / 2D translate /
        // 2D scale): the walker doesn't need to run
        // because the layer textures were rasterized on
        // the last full paint and `composite_frame`
        // already calls `composite_css_layers_overlay`
        // to blit them with the current animated
        // dest_pos / dest_size / opacity. Non-promotable
        // CSS work (colour / layout / 3D / rotate-z)
        // still trips the slow path through `css_active`.
        let css_blocks_fast = css_active && !css_only_composite_promotable;
        // Visual / FLIP / layout animations resize / reposition
        // bounds each frame — the cached primitive batch
        // can't reflect the new clip rect, so we must
        // take the walker route while any of them are
        // mid-flight. Pre-fix, the tree-view expand
        // animation froze at whatever partial state the
        // first slow-path frame painted into the cache;
        // subsequent fast-path frames just blitted that
        // stale partial state until an input event
        // invalidated the cache. Same posture as
        // `scroll_animating` above.
        let bounds_anim_active = ws.render_tree.as_ref().is_some_and(|t| {
            t.has_active_visual_animations()
                || t.has_active_layout_animations()
                || t.has_active_flip_animations()
        });
        // Block the fast path while an overlay /
        // toast is structurally changing or animating.
        //
        //  - `is_dirty()`: a new entry was just pushed
        //    (or the layer was rebuilt). The cached
        //    bg-batch from the previous frame doesn't
        //    include the new layer content yet, so we
        //    need a full slow-walker pass to repopulate
        //    the static cache.
        //  - `has_animating_overlays()` / `has_animating()`:
        //    motion springs are still ticking. Animation
        //    state is per-frame; the cached batch would
        //    freeze at the moment the spring last
        //    crossed a frame boundary, so we walk every
        //    frame to re-emit primitives at the current
        //    spring state.
        //
        // Settled, idle overlays do NOT block fast
        // path. Pre-fix this branch also OR-d
        // `has_visible_overlays()` / `!t.is_empty()`,
        // which forced slow walker every frame for the
        // entire duration any overlay was open. Combined
        // with the cursor-blink continuous_redraw an
        // auto-focused input emits, that's a slow-walker
        // pass per vsync from the moment a popover with
        // an input appears until it closes — visible as
        // "canvas re-paints in a rapid zoom-flicker" on
        // a canvas-backed host (node_editor_demo) because
        // each pass re-bakes the static cache and re-
        // collects the canvas overlay back-to-back.
        // `is_dirty()` + `has_animating_overlays()` cover
        // the real correctness needs (cache-stale-after-
        // push, motion-needs-re-emit); the visible-only
        // case is now allowed to fast-path.
        let new_overlay_active = {
            use blinc_layout::overlay_state::{overlay_stack, toast_tray};
            let s = overlay_stack()
                .lock()
                .map(|s| s.is_dirty() || s.has_animating_overlays())
                .unwrap_or(false);
            let t = toast_tray()
                .lock()
                .map(|t| t.is_dirty() || t.has_animating())
                .unwrap_or(false);
            s || t
        };
        let try_fast_paint = !did_rebuild
            && !ws.needs_relayout
            && !css_blocks_fast
            && !scroll_animating
            && !bounds_anim_active
            && !new_overlay_active
            && ws.last_paint_time_ms != 0
            && blinc_app.has_render_cache();
        crate::motion_trace::note("fast", try_fast_paint);

        // Render with motion animations
        // Use physical pixel dimensions for the render surface
        let result = blinc_app.render_tree_with_motion_opt(
            tree,
            rs,
            &view,
            Some(render_tex),
            windowed_ctx.physical_width as u32,
            windowed_ctx.physical_height as u32,
            try_fast_paint,
        );
        if let Err(e) = result {
            tracing::error!("Render error: {}", e);
        }

        blinc_app.clear_blend_target();
    }

    // =========================================================
    // PHASE 4b: Overlay state management (overlays now in main tree)
    // Overlays are composed into the main tree via build_overlay_layer()
    // so they share the same event routing and incremental update path.
    // =========================================================

    // Animation dirtiness is render-only and can be consumed after paint.
    // Structural dirty flags must be preserved for the pre-render subtree
    // rebuild phase: canvas-hosted immediate-mode widgets can call
    // `.show()` while `render_tree_with_motion_opt` is painting, after the
    // rebuild phase has already run for this frame. Clearing content dirty
    // here would strand that new overlay in its manager without ever
    // mounting it into the render tree.
    let _animation_dirty = windowed_ctx.overlay_manager.take_animation_dirty();

    // Track overlay visibility for triggering rebuilds
    let has_visible_overlays = windowed_ctx.overlay_manager.has_visible_overlays();
    windowed_ctx.had_visible_overlays = has_visible_overlays;

    // Tell winit we're about to present. On Wayland this
    // arms the `wl_surface::frame()` callback that gates
    // the next `RedrawRequested` on the compositor's
    // `wl_callback::Done`. Without this, winit emits the
    // next redraw immediately and our `get_current_texture()`
    // blocks for ~1 s per acquire while the compositor
    // hasn't released a swapchain image — the documented
    // pathology behind the Linux "frozen UI" reports.
    // No-op on other platforms.
    //
    // When the experimental `wayland-frame-gate` feature
    // is active AND the hand-rolled gate constructed
    // successfully, WE own the frame-callback registration
    // — skip winit's gating so the two queues don't both
    // arm a `wl_surface::frame()` on each present.
    #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
    let _gate_active = ws.wayland_gate.is_some();
    #[cfg(not(all(feature = "wayland-frame-gate", target_os = "linux")))]
    let _gate_active = false;
    if !_gate_active {
        window.pre_present_notify();
    }
    // Arm BEFORE present. `frame.present()`
    // calls wgpu's internal `wl_surface::commit()`;
    // we need our `wl_surface::frame()` request to
    // hit the wire first so the callback bundles
    // with this commit rather than the next one.
    // The "after a few seconds it freezes" symptom
    // was caused by arming after present: callbacks
    // were buffered for a next commit that never
    // arrived when the render loop went quiet.
    #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
    if let Some(gate) = ws.wayland_gate.as_ref() {
        gate.arm_before_present();
    }
    frame.present();
    // Wayland keep-alive: ask for the next frame right after presenting. `pre_present_notify`
    // armed the frame callback, so winit delivers it at vsync; no-op off Wayland.
    if keep_alive {
        window.request_redraw();
    }
    // Frame-gate: keep presents flowing at the compositor's Done cadence.
    #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
    if _gate_active {
        window.request_redraw();
    }
    t_phase4 = phase4_start.elapsed();

    // =========================================================
    // PHASE 5: Request next frame if animations are active
    // This ensures smooth animation without waiting for events
    // =========================================================
    let phase5_start = std::time::Instant::now();

    // Check if background animation thread signaled that redraw is needed
    // The background thread runs at 120fps and sets this flag when
    // there are active animations (springs, keyframes, timelines)
    let scheduler = windowed_ctx.animations.lock().unwrap();
    let needs_animation_redraw_raw = scheduler.take_needs_redraw();
    dirty_spring_count = scheduler.dirty_spring_count();
    drop(scheduler); // Release lock before request_redraw

    // Check if stateful elements have active spring animations
    // and re-run their callbacks to get updated animation values.
    //
    // CRUCIAL: drive this off the *raw* scheduler signal, not
    // the visibility-gated one below. `check_stateful_animations`
    // is what unregisters settled statefuls. If we skip it
    // when the gate suppresses rendering, the registry never
    // shrinks — which makes `has_animating_statefuls()` return
    // a sticky `true`, which then keeps re-asserting the gate
    // (because we OR it into `visible_anim`), and we never
    // recover. The bookkeeping has to run on every animation
    // tick regardless of whether we'll actually paint.
    if needs_animation_redraw_raw && blinc_layout::has_animating_statefuls() {
        blinc_layout::check_stateful_animations();
    }

    // Gate the animation signal on visibility. The scheduler
    // ticks unconditionally for any active spring / keyframe /
    // timeline — including ones tied to off-screen nodes. The
    // paint walker sets `visible_anim_active` when it paints
    // a node that drives a per-frame redraw (Canvas, motion
    // bindings, active motion state).
    //
    // Stateful-driven animations (springs that mutate state and
    // trigger rebuilds — e.g. cn_demo's spinner / accordion's
    // `animated_progress`) bypass the per-node motion-binding
    // check, so we additionally OR in the global "any animating
    // stateful?" signal — but **filtered to those whose node
    // was painted this frame**. The paint walker records every
    // node it actually rendered into `painted_node_ids`;
    // `has_visible_animating_statefuls` intersects that with
    // the registry. Without this intersection a spinner
    // scrolled off-screen pinned the redraw chain forever
    // (cn_demo regression). Brand-new Statefuls whose node
    // hasn't been bound yet are conservatively counted as
    // visible by the predicate, so the very first frame still
    // renders.
    let visible_anim_paint = ws
        .render_tree
        .as_ref()
        .is_none_or(|t| t.visible_anim_active());
    let visible_anim_stateful = ws
        .render_tree
        .as_ref()
        .is_some_and(|t| blinc_layout::has_visible_animating_statefuls(&t.painted_node_ids()));
    let visible_anim = visible_anim_paint || visible_anim_stateful;
    // Mirror the flag to the scheduler-side atomic so the
    // wake callback (bg thread) skips waking the main
    // thread when the only active animations are off-screen.
    visible_anim_for_wake.store(visible_anim, Ordering::Release);
    let needs_animation_redraw = needs_animation_redraw_raw && visible_anim;

    // Cursor blink: a focused text input wants the
    // cursor visibility flipped every ~400 ms. A
    // sticky `has_focused_text_input()` read is the
    // right signal — the previous consume-on-read
    // flag forced a re-arm every frame which pinned
    // CPU at vsync. The cursor-only redraw branch
    // below paces this signal via `wake_at` so we
    // only paint at blink interval, not vsync.
    let needs_cursor_redraw = blinc_layout::widgets::has_focused_text_input();
    let _ = blinc_layout::widgets::take_needs_continuous_redraw();

    // Check if motion animations are active (enter/exit
    // animations). Painted-gated for the same reason as
    // `needs_vsync`: this term feeds `any_redraw_signal`,
    // so an off-screen motion re-arms the redraw chain
    // every frame and the loop never parks.
    let needs_motion_redraw = match (&ws.render_state, &ws.render_tree) {
        (Some(rs), Some(tree)) => rs.has_active_motions_visible(&tree.painted_node_ids()),
        _ => false,
    };

    // Check if overlays changed (modal opened/closed, toast
    // appeared, etc.) or are mid-animation. The presence of a
    // *visible* overlay is NOT a redraw signal — a static
    // popover should sit quiet between input events. Use
    // `has_animating_overlays` (enter/exit motion) instead;
    // any overlay-internal redraws (hover css, contained
    // motion) flow through their own signals below.
    let needs_overlay_redraw = {
        let mgr = windowed_ctx.overlay_manager.lock().unwrap();
        mgr.is_dirty() || mgr.take_animation_dirty() || mgr.has_animating_overlays()
    };
    // Phase 3 transition: same gate for the new stack +
    // tray. Either dirtying source schedules the next paint.
    let needs_overlay_stack_redraw = {
        use blinc_layout::overlay_state::{overlay_stack, toast_tray};
        let s_signal = overlay_stack()
            .lock()
            .map(|s| s.is_dirty() || s.take_animation_dirty() || s.has_animating_overlays())
            .unwrap_or(false);
        let t_signal = toast_tray()
            .lock()
            .map(|t| t.is_dirty() || t.has_animating())
            .unwrap_or(false);
        s_signal || t_signal
    };
    let needs_overlay_redraw = needs_overlay_redraw || needs_overlay_stack_redraw;

    // Check if CSS animations/transitions/FLIP/visual-animations need
    // continued redraws. Both `flip_animations` (older `animate_layout`)
    // and `visual_animations` (newer `animate_bounds`, used by the cn
    // accordion among others) drive bounds animation but live in
    // separate maps. Missing the visual_animations check here was the
    // cause of accordion jank: once the scheduler stopped waking the
    // main thread on every tick, the only thing keeping the chain
    // alive during an accordion expand was *no* signal at all, so the
    // animation only progressed when some other event (scroll, hover)
    // happened to fire `frame_dirty`.
    // Visibility-gated CSS-redraw signal. Same shape as the
    // four-way OR above used to be, but every term is now
    // intersected with `painted_node_ids`. Off-screen
    // `infinite` keyframes (the styling_demo had ~25 of
    // them, pinning ~73 % CPU at idle even with the cursor
    // parked) no longer keep the chain alive — they
    // continue ticking so progress stays in sync, but the
    // signal that drives request_redraw stops.
    //
    // The unfiltered `css_active`/`has_active_*` calls
    // are still made above (we want to advance every
    // animation regardless) — what changed is the GATE
    // that triggers another frame.
    let _ = css_active; // keep tick side-effects, drop signal
    let css_needs_redraw = ws.render_tree.as_ref().is_some_and(|t| {
        // CSS store keyed by `StableNodeId` (Phase 5);
        // FLIP / visual-animation visibility checks
        // still take `LayoutNodeId`, so we keep both.
        let painted_stable = t.painted_stable_ids();
        let painted = t.painted_node_ids();
        let store = t.css_anim_store();
        let store_guard = store.lock().unwrap();
        let store_visible = store_guard.has_visible_active(&painted_stable);
        drop(store_guard);
        store_visible
            || t.css_has_visible_transitions(&painted)
            || t.has_active_visible_flip_animations(&painted)
            || t.has_active_visible_visual_animations(&painted)
    });

    // Check if pointer query elements need continuous redraws
    let pointer_query_active = !windowed_ctx.pointer_query.is_empty();

    // @flow shaders using time/animation builtins need continuous redraws
    let flow_needs_redraw = blinc_app.has_active_flows();

    // Image load-time fade-ins tick a per-image
    // `fade_factor` based on elapsed wall-time;
    // without firing redraw each frame the fade
    // sits frozen until the user wiggles the
    // mouse. Read the dedicated flag set in
    // `RenderContext` so flows-flag overwrites
    // can't clobber the signal mid-dispatch.
    let image_fade_needs_redraw = blinc_app.context().has_pending_image_fade();

    // Log which signal(s) kept the redraw chain alive at trace
    // level. Run with `RUST_LOG=blinc_app=trace` to see what's
    // pinning a stuck-busy frame loop. Writes nothing in normal
    // builds — the format args aren't even evaluated when the
    // trace target is disabled.
    tracing::trace!(
        target: "blinc_app::redraw_signals",
        animation = needs_animation_redraw,
        cursor = needs_cursor_redraw,
        motion = needs_motion_redraw,
        scroll = scroll_animating,
        overlay = needs_overlay_redraw,
        theme = theme_animating,
        css = css_needs_redraw,
        pointer_query = pointer_query_active,
        flow = flow_needs_redraw,
        image_fade = image_fade_needs_redraw,
        "redraw chain"
    );

    // External animation tick — set by code that
    // drives its own per-frame work outside the
    // scheduler / motion / stateful registries
    // (canvas-closure animations like the node
    // editor's edge-state shimmer). Take-and-clear
    // so the source closure must re-request next
    // frame; that way the chain stops cleanly the
    // moment the source goes quiet.
    let external_anim_tick = blinc_layout::take_animation_tick_request();

    // cn dropdown/menu/select/popover fade in via CSS
    // @keyframes, not the motion FSM, so
    // `needs_overlay_redraw` (has_animating_overlays) never
    // covers them; and the composite-promoted overlay
    // container isn't reliably in `painted_node_ids`, so the
    // painted-gated `css_needs_redraw` misses it too. Result:
    // once the push frame's dirty flags clear, no term keeps
    // the chain alive and the fade freezes until input.
    // Mirror the image_fade pattern: a dedicated,
    // non-painted-gated signal scoped to "an overlay is on
    // screen AND a CSS anim/transition is playing". Bounded —
    // `css_has_active()` clears when `is_playing` goes false
    // and `has_visible_overlays()` clears when the overlay
    // closes — so off-screen infinite keyframes elsewhere
    // still die (the reason `css_needs_redraw` is painted-gated).
    let overlay_css_needs_redraw = {
        // Cover BOTH overlay systems: cn menubar/dropdown
        // may live on the legacy `overlay_manager`, popover/
        // select on the newer `overlay_stack`.
        let mgr_present = windowed_ctx.overlay_manager.has_visible_overlays();
        let stack_present = blinc_layout::overlay_state::overlay_stack()
            .lock()
            .map(|s| s.has_visible_overlays())
            .unwrap_or(false);
        let overlay_present = mgr_present || stack_present;
        let css_active = ws.render_tree.as_ref().is_some_and(|t| t.css_has_active());
        overlay_present && css_active
    };

    // CSS-activity edge repaints — one render-cache
    // invalidation on EITHER transition of
    // `css_has_active()`:
    //
    // * Rising edge (animations just started): the
    //   overlay's content may have been collected on
    //   a frame BEFORE `start_all_css_animations` ran
    //   (the known first-open ordering race), so the
    //   cached text pool has no composite-patch
    //   records and would render at full alpha over
    //   the fading panel. One slow walk re-collects
    //   with `is_playing == true`: promotion, patch
    //   records and baselines all captured.
    //
    // * Falling edge (animations settled): nothing on
    //   the fast path ever demotes the promoted
    //   region — the stale blit + base-alpha text
    //   pool would persist until the next input
    //   event. One slow walk demotes and re-emits the
    //   subtree at its exact final state.
    {
        let css_active_now = ws.render_tree.as_ref().is_some_and(|t| t.css_has_active());
        if ws.prev_css_active != css_active_now {
            blinc_app.invalidate_render_cache_tagged("css_activity_edge");
            frame_dirty.store(true, Ordering::Release);
            window.request_redraw();
        }
        ws.prev_css_active = css_active_now;
    }

    let any_redraw_signal = needs_animation_redraw
        || external_anim_tick
        || needs_cursor_redraw
        || needs_motion_redraw
        || scroll_animating
        || needs_overlay_redraw
        || theme_animating
        || css_needs_redraw
        || overlay_css_needs_redraw
        || pointer_query_active
        || flow_needs_redraw
        || image_fade_needs_redraw;
    if crate::motion_trace::enabled() {
        crate::motion_trace::note(
            "ask",
            serde_json::json!({
                "scheduler": needs_animation_redraw,
                "scheduler_raw": needs_animation_redraw_raw,
                "external": external_anim_tick,
                "motion": needs_motion_redraw,
                "scroll": scroll_animating,
                "overlay": needs_overlay_redraw,
                "theme": theme_animating,
                "css": css_needs_redraw,
                "overlay_css": overlay_css_needs_redraw,
                "pointer_query": pointer_query_active,
                "flow": flow_needs_redraw,
                "image_fade": image_fade_needs_redraw,
                "cursor": needs_cursor_redraw,
            }),
        );
        crate::motion_trace::note("another_frame", any_redraw_signal);
        crate::motion_trace::note("fps_cap", animation_fps_cap_atomic.load(Ordering::Relaxed));
    }
    if any_redraw_signal {
        // Cursor-only: a focused text input is the
        // only redraw signal. Pace at the blink
        // interval instead of vsync — the cursor
        // toggles twice per second, painting every
        // frame burns 15–25% CPU on a non-trivial
        // UI tree for nothing visible.
        let cursor_only = needs_cursor_redraw
            && !needs_animation_redraw
            && !needs_motion_redraw
            && !scroll_animating
            && !needs_overlay_redraw
            && !theme_animating
            && !css_needs_redraw
            && !overlay_css_needs_redraw
            && !pointer_query_active
            && !flow_needs_redraw;

        // Which signal is actually keeping the frame
        // pacer alive. Every one of these ORs into the
        // decision to schedule another wake, so exactly
        // one line here ends the guessing about why an
        // idle window never parks.
        {
            static T: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if T.fetch_add(1, Ordering::Relaxed) % 120 == 0 {
                tracing::debug!(
                    needs_animation_redraw,
                    needs_motion_redraw,
                    scroll_animating,
                    needs_overlay_redraw,
                    theme_animating,
                    css_needs_redraw,
                    overlay_css_needs_redraw,
                    pointer_query_active,
                    flow_needs_redraw,
                    needs_cursor_redraw,
                    "frame pacing: which signal is live"
                );
            }
        }

        if cursor_only {
            // Cursor blink toggles every ~400 ms
            // (see `RenderState::cursor_blink_interval`).
            // Schedule the next paint at the next
            // toggle, not at vsync.
            let delay = std::time::Duration::from_millis(400);
            frame_dirty.store(true, Ordering::Release);
            wake_proxy_for_pacing.wake_at(delay);
        } else {
            // Read the live cap from the atomic (the
            // adaptive FPS adapter may have changed it
            // since startup). 0 ⇒ no cap → vsync pace.
            let live_cap_chain = animation_fps_cap_atomic.load(Ordering::Relaxed);
            if live_cap_chain > 0 && !keep_alive {
                // Capped, non-keep-alive (macOS / X11):
                // pace the next Frame at exactly the cap
                // interval. Otherwise (`request_redraw`)
                // winit on macOS would deliver Frames
                // back-to-back at microsecond cadence —
                // Phase 4 would skip them via the cap-
                // elapsed check but the loop still spun
                // 3000+ times per second, pinning CPU at
                // 100 % under continuous click + active
                // animation. `wake_at` routes through the
                // platform shim's timer thread so the next
                // Frame arrives at the exact moment Phase 4
                // is allowed to render.
                let delay = std::time::Duration::from_millis(1000 / live_cap_chain as u64);
                crate::motion_trace::note("wake_in_ms", delay.as_millis() as u64);
                frame_dirty.store(true, Ordering::Release);
                wake_proxy_for_pacing.wake_at(delay);
            } else {
                // Wayland keep-alive, or no cap → render at
                // vsync via `request_redraw`. Under keep-
                // alive this is ALREADY vsync-paced by the
                // compositor frame callback (same self-drive
                // as after present), so it never spins — and
                // it must NOT be a cap-interval `wake_at`
                // timer: that timer (e.g. 16 ms) beats
                // against the compositor Done cadence
                // (~16.67 ms), delivering scroll-decel and
                // spinner frames on an uneven 2:3 rhythm =
                // the "laggy on Wayland, smooth on Mac"
                // judder. `request_redraw` coalesces with
                // the keep-alive self-drive onto one vsync
                // cadence.
                frame_dirty.store(true, Ordering::Release);
                window.request_redraw();
            }
        }
    }
    // Parked: no frame follows this one. A wake after `IDLE_RELEASE` frees the app's
    // between-frame GPU memory if nothing has drawn by then (see the `Frame` gate).
    ws.parked_at = (!any_redraw_signal && !keep_alive).then(std::time::Instant::now);
    if ws.parked_at.is_some() {
        wake_proxy_for_pacing.wake_at(crate::windowed::IDLE_RELEASE);
    }
    t_phase5 = phase5_start.elapsed();
    let t_total = frame_start.elapsed();
    // Feed the rendered-frame wall-clock into the
    // adaptive FPS adapter. Only runs when an
    // `Adaptive` policy was configured; `Fixed` /
    // `Refresh` / legacy `animation_fps_cap`
    // leave `fps_adapter == None`. The adapter
    // returns the cap it wants going forward; if
    // it differs from the atomic's current value
    // we publish the new cap and notify the
    // scheduler so spring / keyframe / timeline
    // ticking matches the new paint cadence.
    if let Some(adapter_arc) = fps_adapter.as_ref() {
        if let Ok(mut adapter) = adapter_arc.lock() {
            let new_cap = adapter.record(t_total.as_micros() as u64);
            let prev_cap = animation_fps_cap_atomic.load(Ordering::Relaxed);
            if new_cap != prev_cap {
                animation_fps_cap_atomic.store(new_cap, Ordering::Relaxed);
                if let Ok(mut sched) = animations.lock() {
                    sched.set_target_fps(new_cap);
                }
            }
        }
    }
    // Per-phase breakdown. Disabled by default;
    // RUST_LOG=blinc_app::frame_timing=trace surfaces it.
    // The four phase totals will not exactly sum to
    // `total` — the small gaps cover surface acquire +
    // scheduling work that happens between the labelled
    // sections (e.g. cursor sync, animation policy
    // decisions). `did_rebuild` + `dirty_springs`
    // contextualize the timings: if `did_rebuild=false`
    // and `dirty_springs=1` then any time spent in
    // phase 4 is paint walker overhead for what should
    // be a one-binding update — a candidate for the
    // upcoming fast Phase 4.
    tracing::trace!(
        target: "blinc_app::frame_timing",
        total_us = t_total.as_micros() as u64,
        p1_rebuild_check_us = t_phase1.as_micros() as u64,
        p2_rebuild_us = t_phase2.as_micros() as u64,
        p3_tick_us = t_phase3.as_micros() as u64,
        p4_render_us = t_phase4.as_micros() as u64,
        p5_chain_us = t_phase5.as_micros() as u64,
        did_rebuild,
        dirty_springs = dirty_spring_count,
        "frame"
    );
    if crate::motion_trace::enabled() {
        crate::motion_trace::note(
            "cost_us",
            serde_json::json!({
                "total": t_total.as_micros() as u64,
                "p1": t_phase1.as_micros() as u64,
                "p2": t_phase2.as_micros() as u64,
                "p3": t_phase3.as_micros() as u64,
                "p4": t_phase4.as_micros() as u64,
                "p5": t_phase5.as_micros() as u64,
            }),
        );
    }
    crate::motion_trace::end();
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
