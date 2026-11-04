//! Event-loop handler for a frame: rebuild, layout, animate, paint, present.

#![allow(unused_variables)]

use super::desktop_loop::DesktopLoop;
use super::*;

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

                    // =========================================================
                    // PHASE 1: Check if tree structure needs rebuild
                    // Only structural changes require tree rebuild
                    // =========================================================
                    let phase1_start = std::time::Instant::now();

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
                            let overlay_content =
                                windowed_ctx.overlay_manager.build_overlay_layer();
                            blinc_layout::queue_subtree_rebuild(overlay_node_id, overlay_content);
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
                            prop_effects =
                                prop_effects.or(tree.apply_partial_property_updates(prop_updates));
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
                            let rebuilt = tree.process_pending_subtree_rebuilds_routed(Some(
                                &windowed_ctx.event_router,
                            ));
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
                                prop_effects = prop_effects
                                    .or(tree.apply_partial_property_updates(post_focus_updates));
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
                            let text_focus =
                                blinc_layout::widgets::text_input::focused_text_input_node_id()
                                    .or_else(
                                    blinc_layout::widgets::text_input::focused_text_area_node_id,
                                );
                            windowed_ctx.event_router.sync_text_focus(text_focus);
                            if let Some(ref mut tree) = ws.render_tree {
                                if tree.process_pending_subtree_rebuilds_routed(Some(
                                    &windowed_ctx.event_router,
                                )) {
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
                                tracing::debug!(
                                    "Window resize: full tree rebuild (bypassing incremental update)"
                                );

                                // Clear layout bounds storages before rebuild
                                existing_tree.clear_layout_bounds_storages();

                                // Full rebuild: create new tree from element with shared registry
                                // Pass registry to from_element_with_registry so IDs are registered during build
                                let mut tree = RenderTree::from_element_with_registry(
                                    &ui,
                                    Arc::clone(&element_registry),
                                );

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
                                    existing_tree
                                        .compute_layout(windowed_ctx.width, windowed_ctx.height);
                                    existing_tree.update_flip_bounds();
                                }

                                match update_result {
                                    UpdateResult::NoChanges => {
                                        tracing::debug!(
                                            "Incremental update: NoChanges - skipping rebuild"
                                        );
                                    }
                                    UpdateResult::VisualOnly => {
                                        tracing::debug!(
                                            "Incremental update: VisualOnly - skipping layout"
                                        );
                                        // Props already updated in-place by incremental_update
                                    }
                                    UpdateResult::LayoutChanged => {
                                        // Layout changed - recompute layout
                                        tracing::debug!(
                                            "Incremental update: LayoutChanged - recomputing layout"
                                        );
                                        existing_tree.apply_stylesheet_layout_overrides();
                                        existing_tree.compute_layout(
                                            windowed_ctx.width,
                                            windowed_ctx.height,
                                        );
                                        existing_tree.update_flip_bounds();
                                    }
                                    UpdateResult::ChildrenChanged => {
                                        // Children changed - subtrees were rebuilt in place
                                        tracing::debug!(
                                            "Incremental update: ChildrenChanged - subtrees rebuilt"
                                        );

                                        // Apply CSS styles to new nodes from rebuilt subtrees
                                        // (collect_render_props only applies ID-based CSS;
                                        // class selectors need apply_stylesheet_base_styles)
                                        existing_tree.apply_stylesheet_base_styles();
                                        // Recompute layout since structure changed
                                        existing_tree.apply_stylesheet_layout_overrides();
                                        existing_tree.compute_layout(
                                            windowed_ctx.width,
                                            windowed_ctx.height,
                                        );
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
                            let mut tree = RenderTree::from_element_with_registry(
                                &ui,
                                Arc::clone(&element_registry),
                            );

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
                    let motion_just_settled =
                        motion_was_active_pre_tick && !rs.has_active_motions();
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
                    let (css_active, css_only_composite_promotable) =
                        if let Some(ref mut tree) = ws.render_tree {
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
                        static T: std::sync::atomic::AtomicUsize =
                            std::sync::atomic::AtomicUsize::new(0);
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
                            wake_proxy_for_pacing
                                .wake_at(std::time::Duration::from_millis(remaining));
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
                        return ControlFlow::Continue;
                    }
                    ws.last_paint_time_ms = current_time;

                    // =========================================================
                    // PHASE 4: Render
                    // Combines stable tree structure with dynamic render state
                    // =========================================================
                    let phase4_start = std::time::Instant::now();

                    // Sync text input/textarea focus to EventRouter so CSS :focus matching works
                    {
                        let text_focus =
                            blinc_layout::widgets::text_input::focused_text_input_node_id()
                                .or_else(
                                    blinc_layout::widgets::text_input::focused_text_area_node_id,
                                );
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
                                let state_changed =
                                    tree.apply_stylesheet_state_styles(&windowed_ctx.event_router);
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
                        windowed_ctx.pointer_query.update(
                            mx,
                            my,
                            is_pressed,
                            dt_sec,
                            time_sec,
                            |id| {
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
                            },
                        );
                        // Evaluate dynamic calc(env(...)) properties with current pointer state
                        if let Some(ref mut tree) = ws.render_tree {
                            tree.apply_pointer_styles(
                                &windowed_ctx.pointer_query,
                                &windowed_ctx.event_router,
                            );
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
                                if let Ok(typed) = pass
                                    .downcast::<Box<dyn blinc_gpu::custom_pass::CustomRenderPass>>()
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
                    // Wayland keep-alive self-drive: request the next frame
                    // so the swapchain keeps cycling while the scene is
                    // static. `pre_present_notify` (called above) armed the
                    // compositor's frame callback, so winit paces this to
                    // vsync (Done) — steady ~7% CPU, not a busy spin.
                    // `keep_alive` is false off Wayland, so this is a no-op
                    // on X11 / macOS / Windows.
                    if keep_alive {
                        window.request_redraw();
                    }
                    // Frame-gate self-drive: keep presents flowing at the
                    // compositor's Done cadence so the swapchain never goes
                    // idle (see the keep-alive note above). The gate's Done
                    // wait throttles this to vsync, so it is not a spin.
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
                    let visible_anim_stateful = ws.render_tree.as_ref().is_some_and(|t| {
                        blinc_layout::has_visible_animating_statefuls(&t.painted_node_ids())
                    });
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
                        (Some(rs), Some(tree)) => {
                            rs.has_active_motions_visible(&tree.painted_node_ids())
                        }
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
                            .map(|s| {
                                s.is_dirty()
                                    || s.take_animation_dirty()
                                    || s.has_animating_overlays()
                            })
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
                        let css_active =
                            ws.render_tree.as_ref().is_some_and(|t| t.css_has_active());
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
                        let css_active_now =
                            ws.render_tree.as_ref().is_some_and(|t| t.css_has_active());
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
                        crate::motion_trace::note(
                            "fps_cap",
                            animation_fps_cap_atomic.load(Ordering::Relaxed),
                        );
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
                            static T: std::sync::atomic::AtomicUsize =
                                std::sync::atomic::AtomicUsize::new(0);
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
                                let delay =
                                    std::time::Duration::from_millis(1000 / live_cap_chain as u64);
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
                }
            }
            _ => {}
        }
        ControlFlow::Continue
    }
}
