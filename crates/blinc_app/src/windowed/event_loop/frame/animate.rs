//! Frame phase 3: advance animations and decide whether another frame is needed.

#![allow(unused_variables, unused_mut, unused_assignments)]

use super::super::frame::FrameVars;
use super::super::*;

/// Phase 3: tick animations and dynamic render state.
pub(super) fn tick_animations(
    ws: &mut WindowState,
    window: &blinc_platform_desktop::DesktopWindow,
    platform: &mut DesktopPlatform,
    animation_fps_cap: &mut Option<u32>,
    animation_fps_cap_atomic: &mut Arc<std::sync::atomic::AtomicU32>,
    wake_proxy_for_pacing: &mut blinc_platform_desktop::WakeProxy,
    frame_dirty: &mut Arc<AtomicBool>,
    animations: &mut SharedAnimationScheduler,
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
    // PHASE 3: Tick animations and dynamic render state
    // This must happen AFTER tree rebuild so motions are initialized
    // =========================================================
    let phase3_start = std::time::Instant::now();

    // Process any pending motion exit cancellations
    // This must happen before tick() so cancelled motions don't continue exiting
    rs.process_global_motion_exit_cancels();

    // Process any pending motion exit starts (explicit exit triggers)
    rs.process_global_motion_exit_starts();

    // Process suspended motion starts queued via query_motion(key).start()
    rs.process_global_motion_starts();

    // Capture pre-tick motion liveness so the `should_render`
    // gate below can recognise a "this frame's tick settled
    // the motion" transition. Without this, the final tick
    // (Entering → Visible) would leave `has_active_motions()`
    // false at the should_render check, the cap gate could
    // skip the paint, and the user would see the animation
    // freeze at the penultimate state — the "animation is
    // not allowed to settle" symptom. Capturing pre-tick
    // ensures the paint of the settled state still ships.
    let motion_was_active_pre_tick = rs.has_active_motions();
    // Same snapshot, restricted to nodes the last paint
    // actually walked. `motion_was_active_pre_tick` has to
    // stay unfiltered for the settle detection below (it is
    // compared against an equally unfiltered post-tick
    // read), but the cap-bypass gate wants the visible form
    // — an enter/exit motion on a node scrolled out of view
    // has nothing to stair-step, so paying vsync for it is
    // pure idle burn.
    let motion_was_visible_pre_tick = ws
        .render_tree
        .as_ref()
        .is_some_and(|t| rs.has_active_motions_visible(&t.painted_node_ids()));

    // Tick render state (handles cursor blink, color animations, etc.)
    // This updates dynamic properties without touching tree structure
    let _animations_active = rs.tick(current_time);

    // Detect "motion just settled this frame": pre-tick the
    // FSM had at least one motion mid-flight, post-tick it
    // has none. The current paint will display the settled
    // state — but we still arm one more redraw via
    // `stateful::request_redraw()` so the next Frame fires
    // unconditionally. Belt-and-suspenders against the
    // "final frame doesn't ship until mouse-move" symptom:
    // even if the current frame somehow skipped its paint
    // (cap-interval edge, surface contention, etc.), the
    // follow-up frame still paints the final state without
    // requiring external input to wake the loop.
    let motion_just_settled = motion_was_active_pre_tick && !rs.has_active_motions();
    if motion_just_settled {
        blinc_layout::stateful::request_redraw();
        // Force the next paint to re-walk the tree from
        // scratch. The compositor's static-cache
        // invalidation gate (`other_animations_active`
        // in `try_render_with_compositor`) reads
        // `has_active_motions()` POST-tick — on the
        // settle frame that's already `false`, so the
        // gate keeps the cached primitives from the
        // penultimate paint (which were rasterised with
        // `motion.current` in its lerped state, not the
        // settled `MotionKeyframe::default()`). User
        // sees the animation freeze one frame short of
        // the final state. Invalidating here forces
        // the walker to repopulate the cache with
        // post-tick `motion.current = default` values.
        blinc_app.invalidate_render_cache_tagged("motion_just_settled");
    }

    // Tick CSS animations/transitions synchronously on the main thread.
    // The scheduler's bg thread drives 120fps redraws via wake_callback,
    // but actual ticking is done here to stay in phase with rendering.
    let dt_ms = if ws.last_frame_time_ms > 0 {
        (current_time - ws.last_frame_time_ms) as f32
    } else {
        16.0
    };
    let (css_active, css_only_composite_promotable) = if let Some(ref mut tree) = ws.render_tree {
        let store = tree.css_anim_store();
        let mut s = store.lock().unwrap();
        let (anim, trans) = s.tick(dt_ms);
        drop(s);
        let flip = tree.tick_flip_animations(dt_ms);
        let active = anim || trans || flip || tree.css_has_active();
        // Composite-promotable predicate ignores FLIP
        // (FLIP animations re-layout under the hood,
        // so they always need the slow path).
        let promotable = !flip && tree.css_active_all_composite_promotable();
        (active, promotable)
    } else {
        (false, true)
    };
    ws.last_frame_time_ms = current_time;

    // Sync motion states to shared store for query_motion API
    rs.sync_shared_motion_states();

    if crate::motion_trace::enabled() {
        crate::motion_trace::note("dt_ms", dt_ms);
        crate::motion_trace::note("css_active", css_active);
        crate::motion_trace::note("css_promotable", css_only_composite_promotable);
        crate::motion_trace::note("motion_active_pre", motion_was_active_pre_tick);
        crate::motion_trace::note("motion_settled", motion_just_settled);
        if let Some(ref tree) = ws.render_tree {
            crate::motion_trace::note_animations(tree, rs);
        }
    }

    // Tick theme animation (handles color interpolation during theme transitions)
    let theme_animating = blinc_theme::ThemeState::get().tick();

    // Note: scroll physics tick moved to before PHASE 1 (before any rebuilds)
    // so that ScrollRef has up-to-date values when stateful components rebuild

    t_phase3 = phase3_start.elapsed();

    // Animation-only frames (no rebuild, no relayout,
    // no stateful redraw, no input-driven dirty)
    // cost ~6 ms of CPU per rendered frame on
    // cn_demo — overwhelmingly in GPU dispatch (the
    // paint walker itself is only ~0.1 ms, see the
    // `p4_breakdown` trace). When the app set an
    // `animation_fps_cap`, throttle Phase 4 to that
    // rate without losing the scheduler's Phase 3
    // tick. The `wake_at` below schedules the next
    // Frame for exactly the moment the cap interval
    // elapses, so we don't get the spinning request
    // _redraw / immediate-Frame loop that pinned
    // CPU at 100 % (winit on macOS delivers
    // `request_redraw`'d frames back-to-back at
    // microsecond cadence, *not* vsync-locked, so
    // the previous "re-arm at vsync" implementation
    // burned 3000+ tick-only frames per second).
    //
    // Always render when: there's no cap, this is
    // the very first paint, a rebuild ran this
    // frame, or the cap interval has elapsed. Skip
    // Phase 4 + present otherwise.
    // Read the (possibly dynamic) cap from the atomic so
    // the adaptive FPS path can change it at runtime
    // without rewriting captured closure state. 0 ⇒ no cap.
    let live_cap = animation_fps_cap_atomic.load(Ordering::Relaxed);
    let cap_interval_ms = if live_cap == 0 {
        None
    } else {
        Some(1000u64 / live_cap as u64)
    };
    let elapsed_since_paint = current_time.saturating_sub(ws.last_paint_time_ms);

    // Bypass the FPS cap whenever a vsync-class
    // animation is mid-flight — transforms, layout
    // sizing, clip-path geometry, and motion-FSM
    // enter / exit animations all stair-step
    // visibly when capped to 30 fps, and (more
    // critically) the cap can swallow the final
    // settle-to-Visible paint of a motion FSM
    // animation, leaving the dialog / sheet stuck
    // at the penultimate frame until a mouse-move
    // wakes the loop. `has_visible_vsync_class`
    // covers CSS animations / transitions; motion
    // FSM (overlay enter / exit) is the
    // `has_active_motions()` term.
    //
    // Use the PRE-tick `motion_was_active_pre_tick`
    // snapshot — not the post-tick poll — so the
    // frame where the final tick transitions the
    // FSM from Entering → Visible still counts as
    // vsync-class and the cap gate doesn't skip
    // the paint that displays the settled state.
    //
    // Every term is painted-gated, matching
    // `has_any_active_animation_visible`. The ungated form
    // let a single off-screen looping motion hold
    // `should_render` true forever: the cap never applied,
    // the self-driving Frame loop rendered at vsync, and an
    // idle window sat at 25-40% CPU with nothing moving on
    // screen.
    let needs_vsync = motion_was_visible_pre_tick
        || ws
            .render_tree
            .as_ref()
            .is_some_and(|t| rs.has_active_motions_visible(&t.painted_node_ids()))
        || ws.render_tree.as_ref().is_some_and(|t| {
            let store = t.css_anim_store();
            let guard = store.lock().unwrap();
            guard.has_visible_vsync_class(&t.painted_stable_ids())
        });
    let should_render = match cap_interval_ms {
        None => true,
        Some(_) if did_rebuild => true,
        Some(_) if ws.needs_relayout => true,
        Some(_) if ws.last_paint_time_ms == 0 => true,
        Some(_) if needs_vsync => true,
        Some(interval) => elapsed_since_paint >= interval,
    };
    // Is layout being marked dirty every frame? Either
    // `did_rebuild` or `needs_relayout` makes
    // `should_render` true unconditionally, bypassing
    // the FPS cap — an idle window that keeps setting
    // one of them renders at vsync forever no matter
    // what the animation-visibility gate says.
    {
        static T: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if T.fetch_add(1, Ordering::Relaxed) % 120 == 0 {
            tracing::debug!(
                did_rebuild,
                needs_relayout = ws.needs_relayout,
                needs_vsync,
                should_render,
                cap_interval_ms = ?cap_interval_ms,
                elapsed_since_paint,
                "frame gate: why are we rendering"
            );
        }
    }

    crate::motion_trace::note("did_rebuild", did_rebuild);
    crate::motion_trace::note("needs_vsync", needs_vsync);
    crate::motion_trace::note("since_paint", elapsed_since_paint);
    crate::motion_trace::note("drawn", should_render);
    if !should_render {
        crate::motion_trace::end();
        // Schedule the next Frame for exactly the
        // moment the cap interval elapses. `wake_at`
        // routes through the platform shim's timer
        // thread (same path the cursor-blink pacing
        // and the old `cap_applies` branch use), so
        // we get one tick per cap interval instead
        // of thousands per second.
        if let Some(interval) = cap_interval_ms {
            let remaining = interval.saturating_sub(elapsed_since_paint).max(1);
            frame_dirty.store(true, Ordering::Release);
            wake_proxy_for_pacing.wake_at(std::time::Duration::from_millis(remaining));
        }
        let t_total = frame_start.elapsed();
        tracing::trace!(
            target: "blinc_app::frame_timing",
            total_us = t_total.as_micros() as u64,
            p1_rebuild_check_us = t_phase1.as_micros() as u64,
            p2_rebuild_us = t_phase2.as_micros() as u64,
            p3_tick_us = t_phase3.as_micros() as u64,
            p4_render_us = 0u64,
            p5_chain_us = 0u64,
            did_rebuild,
            dirty_springs = dirty_spring_count,
            skipped = true,
            "frame"
        );
        return Some(ControlFlow::Continue);
    }
    ws.last_paint_time_ms = current_time;

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
