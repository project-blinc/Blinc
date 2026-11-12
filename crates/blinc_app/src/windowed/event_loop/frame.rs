//! Event-loop handler for a frame: rebuild, layout, animate, paint, present.

#![allow(unused_variables)]

use super::DesktopLoop;
use crate::windowed::{WindowState, WindowedContext};
use crate::{ElementBuilder, elapsed_ms};
use blinc_layout::prelude::*;
use blinc_platform::{ControlFlow, Event, Window};
use std::sync::atomic::Ordering;

mod animate;
mod present;
mod rebuild;

impl<F, E> DesktopLoop<F, E>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    /// The OS asks for a frame.
    pub(super) fn on_frame(
        &mut self,
        event: Event,
        window: &blinc_platform_desktop::DesktopWindow,
    ) -> ControlFlow {
        let Self {
            platform,
            primary_max_frame_latency,
            animation_fps_cap,
            animation_fps_cap_atomic,
            fps_adapter,
            wake_proxy_for_windows,
            wake_proxy_for_pacing,
            frame_dirty,
            visible_anim_for_wake,
            ref_dirty_flag,
            reactive,
            hooks,
            animations,
            css_anim_store,
            element_registry,
            ready_callbacks,
            shared_motion_states,
            overlays,
            ws,
            primary_wid,
            secondary_windows,
            ui_builder,
            ..
        } = self;
        match event {
            Event::Frame(_) => {
                // Skip the frame entirely if nothing has changed since
                // the last render. The OS sends `Event::Frame` at the
                // display refresh rate to focused windows whether we
                // asked for it or not; without this gate a static
                // focused UI burns CPU re-rendering an identical scene
                // every vsync interval. `frame_dirty` is flipped back
                // to `true` by any input event (in the prelude above,
                // bare mouse-moves excluded), by the scheduler wake
                // callback (set during init), and by the end-of-frame
                // redraw chain when any animation / cursor / transition
                // / etc. signal indicates ongoing work.
                //
                // We also honour the layout-side stateful redraw
                // signals here — a hover handler firing
                // `stateful::request_redraw()` mid-mouse-move would
                // otherwise be dropped now that bare moves don't
                // flip `frame_dirty`. Peek-without-clear so the
                // start-of-frame `take_needs_redraw()` still fires
                // its normal prop-update / subtree-rebuild path.
                let dirty = frame_dirty.swap(false, Ordering::AcqRel);
                let stateful_dirty = blinc_layout::peek_needs_redraw()
                    || blinc_layout::has_pending_subtree_rebuilds()
                    || blinc_layout::has_pending_partial_prop_updates();
                // Wayland keep-alive: when the frame-gate is active we must
                // keep presenting even when the scene is static. Once
                // animations finish and the redraw chain goes quiet,
                // skipping here stops all presents; Mutter then holds every
                // swapchain buffer and the next get_current_texture()
                // starves for the full ~1s acquire timeout (freeze). The
                // gate throttles us to the compositor's Done cadence, so
                // this stays vsync-paced, not a spin.
                // Wayland keep-alive: keep presenting even when the
                // scene is static (frame-gate or present-thread mode),
                // else the swapchain goes idle and the compositor starves
                // the next acquire.
                #[allow(unused_mut)]
                let mut keep_alive = false;
                #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                {
                    keep_alive |= ws.wayland_gate.is_some();
                }
                #[cfg(target_os = "linux")]
                {
                    keep_alive |= ws.wayland_keep_alive;
                }
                if !dirty && !stateful_dirty && !keep_alive {
                    release_when_idle(ws, wake_proxy_for_pacing);
                    return ControlFlow::Continue;
                }

                if let (Some(blinc_app), Some(config), Some(windowed_ctx), Some(rs)) = (
                    &mut ws.app,
                    &ws.surface_config,
                    &mut ws.ctx,
                    &mut ws.render_state,
                ) {
                    // Per-phase frame timing. Cheap when the trace target
                    // is disabled (Instant::now is ~10 ns on macOS); gated
                    // behind a single check at frame end so format args
                    // aren't evaluated in production builds. Enable with
                    // `RUST_LOG=blinc_app::frame_timing=trace` to see
                    // where the frame budget is actually going.
                    let frame_start = std::time::Instant::now();
                    blinc_core::FrameClock::begin_frame();
                    let mut t_phase1 = std::time::Duration::ZERO;
                    let mut t_phase2 = std::time::Duration::ZERO;
                    let mut t_phase3 = std::time::Duration::ZERO;
                    let mut t_phase4 = std::time::Duration::ZERO;
                    let mut t_phase5 = std::time::Duration::ZERO;
                    let mut did_rebuild = false;
                    let mut dirty_spring_count = 0usize;

                    // Bind the surface handle up front: the
                    // (feature-gated) frame-gate stall recovery below may
                    // reconfigure it, and the frame acquire further down
                    // uses it. Binding here keeps `surf` in scope for both
                    // regardless of which Wayland features are enabled.
                    let surf = match ws.surface.as_ref() {
                        Some(s) => s,
                        None => return ControlFlow::Continue,
                    };

                    // Experimental Wayland frame-callback gate: when
                    // active, drain our wl_callback Done events from
                    // the connection's per-queue inbox and bail out
                    // of this frame entirely if the compositor hasn't
                    // delivered Done for the last armed callback.
                    // Re-arming `request_redraw` keeps us re-checking
                    // without ever attempting a blocking acquire.
                    //
                    // The motivating bug: some Mesa + Wayland
                    // compositors don't deliver Done to winit's
                    // internal queue reliably; winit's own gating
                    // either fires too eagerly (and we hit the wgpu
                    // 1s acquire timeout) or stalls forever. Our
                    // parallel gate has its own queue independent
                    // of winit's, so it's not vulnerable to the
                    // same stale-source race.
                    #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                    if let Some(gate) = ws.wayland_gate.as_ref() {
                        gate.dispatch_pending();
                        let ready = gate.is_frame_ready();
                        // 100 ms safety-valve so a stopped Done
                        // stream caps user-perceived freeze at one
                        // tenth of a second instead of forever.
                        let proceed =
                            gate.is_frame_ready_or_timeout(std::time::Duration::from_millis(100));

                        // Periodic diagnostic: every 60 frames,
                        // print how many Done events have actually
                        // routed into our queue, and how many of
                        // those frames hit the safety valve instead
                        // of receiving Done in time. If
                        // `callbacks_received` stays at 0 the
                        // routing bridge between winit's connection
                        // read loop and our parallel queue is
                        // broken — every frame would be relying on
                        // the 100ms safety valve, which is exactly
                        // the "responsive then sluggish then
                        // freezes" pathology under continuous
                        // input.
                        use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
                        static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
                        static SAFETY_VALVE_HITS: AtomicU64 = AtomicU64::new(0);
                        let fc = FRAME_COUNT.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                        if !ready && proceed {
                            SAFETY_VALVE_HITS.fetch_add(1, AtomicOrdering::Relaxed);
                        }
                        if fc % 60 == 0 {
                            tracing::info!(
                                target: "blinc_app::wayland_gate",
                                frame = fc,
                                done_events = gate.callbacks_received(),
                                safety_valve_hits = SAFETY_VALVE_HITS
                                    .load(AtomicOrdering::Relaxed),
                                "wayland-frame-gate heartbeat",
                            );
                        }

                        if !proceed {
                            // Not ready and the safety valve hasn't
                            // elapsed: re-check next frame without
                            // busy-spinning. request_redraw() re-dispatches
                            // RedrawRequested immediately, so pace with a
                            // sub-ms sleep (Done cadence is ~16ms; catching
                            // it within ~0.5ms adds no perceptible latency).
                            std::thread::sleep(std::time::Duration::from_micros(500));
                            window.request_redraw();
                            return ControlFlow::Continue;
                        }
                        // Proceeding WITHOUT Done means the safety valve
                        // fired — the compositor stalled its frame
                        // callbacks and the swapchain has starved (Mutter
                        // holds every buffer until the surface is
                        // reconfigured). Recreate it, exactly as a manual
                        // window resize does, so the acquire below doesn't
                        // block the full ~1s timeout.
                        if !ready {
                            tracing::debug!(
                                target: "blinc_app::wayland_gate",
                                "Done stalled; recreating swapchain to un-starve acquire",
                            );
                            surf.configure(blinc_app.device(), config);
                        }
                    }

                    // Get current frame (surf bound above)
                    let frame = match surf.get_current_texture() {
                        Ok(f) => f,
                        Err(wgpu::SurfaceError::Lost) => {
                            surf.configure(blinc_app.device(), config);
                            return ControlFlow::Continue;
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            tracing::error!("Out of GPU memory");
                            return ControlFlow::Exit;
                        }
                        Err(wgpu::SurfaceError::Timeout) => {
                            // The compositor stopped releasing swapchain
                            // buffers, so the acquire starved. Reconfigure
                            // to un-stick it — the same recovery a manual
                            // window resize produces. Without this the UI
                            // thread re-enters the acquire and parks on the
                            // GPU fence again ("responsive for a while,
                            // then froze"). The keep-alive self-drive keeps
                            // presents flowing afterward so it stays live.
                            tracing::debug!("Surface acquire timeout — reconfiguring to un-stick");
                            surf.configure(blinc_app.device(), config);
                            window.request_redraw();
                            return ControlFlow::Continue;
                        }
                        Err(e) => {
                            tracing::warn!("Surface error: {:?}", e);
                            return ControlFlow::Continue;
                        }
                    };

                    let render_tex: &wgpu::Texture = &frame.texture;
                    let view = render_tex.create_view(&wgpu::TextureViewDescriptor::default());

                    // Update context from window
                    windowed_ctx.update_from_window(window);

                    // Update viewport for lazy loading visibility checks
                    // Uses logical pixels (width/height) as that's what layout uses
                    rs.set_viewport_size(windowed_ctx.width, windowed_ctx.height);

                    // Get current time for animation updates (used in multiple phases)
                    let current_time = elapsed_ms();
                    crate::motion_trace::begin(current_time);

                    // Clear overlays from previous frame (cursor, selection, focus ring)
                    // These are re-added during rendering if still active
                    rs.clear_overlays();

                    // Tick scroll physics and sync ScrollRef state BEFORE any rebuilds
                    // This ensures ScrollRef has up-to-date values when stateful components
                    // query scroll position during rebuild
                    //
                    // `process_pending_scroll_refs` returns true when it just
                    // promoted a ScrollRef command (e.g. a click handler's
                    // `scroll_to_with_options`) into a live spring. Without
                    // OR-ing that into `scroll_animating`, the frame that
                    // started the spring sees `tick_scroll_physics`'s pre-
                    // process Idle result and the end-of-frame redraw chain
                    // closes — the spring then sits in `Bouncing` until the
                    // next stray input wakes the loop. Symptom: smooth-scroll
                    // commands in carousel_demo "jump" to the target only
                    // after the next mouse move.
                    let scroll_animating = if let Some(ref mut tree) = ws.render_tree {
                        let ticking = tree.tick_scroll_physics(current_time);
                        let just_started = tree.process_pending_scroll_refs();
                        ticking || just_started
                    } else {
                        false
                    };

                    let mut vars = FrameVars {
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
                        dt_ms: Default::default(),
                        theme_animating: Default::default(),
                        needs_vsync: Default::default(),
                        css_active: Default::default(),
                        css_only_composite_promotable: Default::default(),
                        has_visible_overlays: Default::default(),
                        keep_alive,
                    };
                    if let Some(flow) = rebuild::check_rebuild(
                        ws,
                        window,
                        frame_dirty,
                        ref_dirty_flag,
                        reactive,
                        animations,
                        element_registry,
                        &mut vars,
                    ) {
                        return flow;
                    }
                    if let Some(flow) = rebuild::build_tree(
                        ws,
                        window,
                        animations,
                        css_anim_store,
                        element_registry,
                        ready_callbacks,
                        ui_builder,
                        &mut vars,
                    ) {
                        return flow;
                    }
                    if let Some(flow) = animate::tick_animations(
                        ws,
                        window,
                        platform,
                        animation_fps_cap,
                        animation_fps_cap_atomic,
                        wake_proxy_for_pacing,
                        frame_dirty,
                        animations,
                        &mut vars,
                    ) {
                        return flow;
                    }
                    if let Some(flow) = present::render_and_schedule(
                        ws,
                        window,
                        platform,
                        animation_fps_cap,
                        animation_fps_cap_atomic,
                        fps_adapter,
                        wake_proxy_for_pacing,
                        frame_dirty,
                        visible_anim_for_wake,
                        animations,
                        element_registry,
                        overlays,
                        frame,
                        view,
                        &mut vars,
                    ) {
                        return flow;
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue
    }
}

/// What the frame phases hand one another.
#[derive(Clone, Copy)]
pub(super) struct FrameVars {
    pub(super) frame_start: std::time::Instant,
    pub(super) t_phase1: std::time::Duration,
    pub(super) t_phase2: std::time::Duration,
    pub(super) t_phase3: std::time::Duration,
    pub(super) t_phase4: std::time::Duration,
    pub(super) t_phase5: std::time::Duration,
    pub(super) did_rebuild: bool,
    pub(super) dirty_spring_count: usize,
    pub(super) current_time: u64,
    pub(super) scroll_animating: bool,
    pub(super) dt_ms: f32,
    pub(super) theme_animating: bool,
    pub(super) needs_vsync: bool,
    pub(super) css_active: bool,
    pub(super) css_only_composite_promotable: bool,
    pub(super) has_visible_overlays: bool,
    pub(super) keep_alive: bool,
}

/// A frame with nothing to draw: once the window has been parked for `IDLE_RELEASE`, free the
/// app's between-frame GPU memory. A wake that comes early (the timer keeps the earliest
/// deadline) asks again for the rest of the wait.
fn release_when_idle(ws: &mut WindowState, wake: &blinc_platform_desktop::WakeProxy) {
    let Some(parked_at) = ws.parked_at else {
        return;
    };
    let waited = parked_at.elapsed();
    if waited < crate::windowed::IDLE_RELEASE {
        wake.wake_at(crate::windowed::IDLE_RELEASE - waited);
        return;
    }
    ws.parked_at = None;
    if let Some(app) = ws.app.as_mut() {
        tracing::debug!("idle: releasing between-frame GPU memory");
        app.release_idle_memory();
    }
}
