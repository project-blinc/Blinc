//! Event-loop handlers for the lifecycle and for window events.

#![allow(unused_variables)]

use super::DesktopLoop;
use crate::app::BlincApp;
use crate::windowed::{
    OPEN_WINDOW_FN, WindowState, WindowedApp, WindowedContext, pending_builders,
    preferred_present_mode,
};
use crate::{ElementBuilder, EventRouter};
use blinc_core::context_state::BlincContextState;
use blinc_layout::prelude::*;
use blinc_platform::{ControlFlow, Event, LifecycleEvent, Window, WindowConfig, WindowEvent};
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl<F, E> DesktopLoop<F, E>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    /// The application resumed: create the primary window state and its surface.
    pub(super) fn on_resumed(
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
            Event::Lifecycle(LifecycleEvent::Resumed) => {
                let wid = window.id();
                // Initialize GPU if not already done (primary window)
                if ws.app.is_none() {
                    *primary_wid = Some(wid);
                    let winit_window = window.winit_window_arc();

                    match BlincApp::with_window(winit_window, None) {
                        Ok((mut blinc_app, surf)) => {
                            let (width, height) = window.size();
                            // Use the same texture format that the renderer's pipelines use
                            let format = blinc_app.texture_format();
                            let alpha_mode = WindowedApp::pick_alpha_mode(
                                &surf,
                                blinc_app.adapter(),
                                ws.transparent,
                            );
                            if ws.transparent {
                                blinc_app.set_clear_alpha(0.0);
                            }
                            let config = wgpu::SurfaceConfiguration {
                                // COPY_DST is required by the layer
                                // compositor's `composite_frame` step —
                                // it `copy_texture_to_texture`s the
                                // cached static layer into the surface
                                // before drawing canvas overlays on top.
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                    | wgpu::TextureUsages::COPY_SRC
                                    | wgpu::TextureUsages::COPY_DST,
                                format,
                                width,
                                height,
                                present_mode: preferred_present_mode(&surf, blinc_app.adapter()),
                                alpha_mode,
                                view_formats: vec![],
                                desired_maximum_frame_latency: *primary_max_frame_latency,
                            };
                            surf.configure(blinc_app.device(), &config);

                            // Update text measurer with shared font registry for accurate measurement
                            blinc_layout::init_text_measurer_with_registry(
                                blinc_app.font_registry(),
                            );

                            // Adapt the scheduler's tick rate to the display's
                            // refresh rate, capped further if the app set
                            // `animation_fps_cap`. The scheduler ticking
                            // faster than the paint cap just produces
                            // intermediate values that get overwritten
                            // before the next paint reads them — wasted
                            // CPU on every spring / keyframe / timeline
                            // step. Match the effective paint cadence so
                            // each scheduler tick corresponds to one paint
                            // attempt.
                            //
                            // Winit returns refresh in millihertz; clamp to a
                            // sane range so a 240/360 Hz display doesn't pin
                            // a CPU and a missing/zero report doesn't drop us
                            // to 0 fps.
                            {
                                let refresh = window
                                    .winit_window()
                                    .current_monitor()
                                    .and_then(|m| m.refresh_rate_millihertz())
                                    .map(|mhz| (mhz / 1000).clamp(30, 120))
                                    .unwrap_or(60);
                                let effective_fps = match *animation_fps_cap {
                                    Some(cap) => refresh.min(cap),
                                    None => refresh,
                                };
                                // Cap the adaptive ladder at the panel
                                // refresh so it never climbs past vsync
                                // (e.g. 60→90→120 on a 60 Hz NUC panel,
                                // which beat-judders the spinner + scroll
                                // decel). On Wayland `refresh_rate_*`
                                // often reports None → 60, the correct
                                // default ceiling.
                                if let Some(adapter) = fps_adapter.as_ref() {
                                    if let Ok(mut a) = adapter.lock() {
                                        a.set_ceiling(refresh);
                                    }
                                }
                                if let Ok(mut sched) = animations.lock() {
                                    sched.set_target_fps(effective_fps);
                                    tracing::debug!(
                                        "Scheduler target_fps set to {} Hz (refresh={}, cap={:?})",
                                        effective_fps,
                                        refresh,
                                        animation_fps_cap
                                    );
                                }
                            }

                            #[cfg(target_os = "linux")]
                            {
                                // Off by default: a static scene presents only when something changes.
                                // BLINC_KEEP_ALIVE=1 turns on presents at vsync while the scene is static;
                                // it applies on a Wayland backend only (not X11/XWayland).
                                let on_wayland = {
                                    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
                                    window
                                        .winit_window()
                                        .display_handle()
                                        .map(|dh| {
                                            matches!(dh.as_raw(), RawDisplayHandle::Wayland(_))
                                        })
                                        .unwrap_or(false)
                                };
                                ws.wayland_keep_alive = on_wayland
                                    && matches!(
                                        std::env::var("BLINC_KEEP_ALIVE").ok().as_deref(),
                                        Some("1" | "true" | "on")
                                    );
                                if ws.wayland_keep_alive {
                                    tracing::info!("wayland keep-alive active");
                                }
                                ws.surface = Some(surf);
                            }
                            #[cfg(not(target_os = "linux"))]
                            {
                                ws.surface = Some(surf);
                            }
                            ws.surface_config = Some(config);
                            ws.app = Some(blinc_app);

                            // Bring up the experimental Wayland
                            // frame-callback gate iff the feature
                            // is enabled AND raw_window_handle
                            // reports a Wayland surface. Anything
                            // else (X11, macOS, Windows): the
                            // factory returns None and the gate
                            // stays inert, falling back to
                            // winit's native `pre_present_notify`
                            // gating path elsewhere in this loop.
                            #[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
                            {
                                use raw_window_handle::{
                                    HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
                                    RawWindowHandle,
                                };
                                let winit_win = window.winit_window();
                                if let (Ok(dh), Ok(wh)) =
                                    (winit_win.display_handle(), winit_win.window_handle())
                                {
                                    if let (
                                        RawDisplayHandle::Wayland(d),
                                        RawWindowHandle::Wayland(w),
                                    ) = (dh.as_raw(), wh.as_raw())
                                    {
                                        ws.wayland_gate =
                                                    crate::wayland_frame_gate::WaylandFrameGate::try_new_from_raw(
                                                        d.display.as_ptr(),
                                                        w.surface.as_ptr(),
                                                    );
                                        tracing::info!(
                                            enabled = ws.wayland_gate.is_some(),
                                            "wayland-frame-gate experimental: \
                                                     hand-rolled wl_surface::frame() callbacks \
                                                     {}",
                                            if ws.wayland_gate.is_some() {
                                                "active"
                                            } else {
                                                "construction failed — falling back to winit"
                                            }
                                        );
                                    }
                                }
                            }

                            // Initialize context with event router, animations, dirty flag, reactive graph, hooks, overlay manager, registry, and ready callbacks
                            ws.ctx = Some(WindowedContext::from_window(
                                window,
                                EventRouter::new(),
                                Arc::clone(&animations),
                                Arc::clone(&ref_dirty_flag),
                                Arc::clone(&reactive),
                                Arc::clone(&hooks),
                                Arc::clone(&overlays),
                                Arc::clone(&element_registry),
                                Arc::clone(&ready_callbacks),
                            ));

                            // Wire open_window callback using the event loop's wake proxy
                            let wp_for_ctx = wake_proxy_for_windows.clone();
                            let open_fn: Arc<dyn Fn(WindowConfig) + Send + Sync> =
                                Arc::new(move |config| {
                                    wp_for_ctx.create_window(config);
                                });
                            if let Some(ref mut windowed_ctx) = ws.ctx {
                                windowed_ctx.set_open_window_fn(Arc::clone(&open_fn));
                                // Per-window action callbacks
                                let win_actions = WindowedApp::make_window_actions(
                                    window.winit_window_arc(),
                                    wake_proxy_for_windows.clone(),
                                );
                                windowed_ctx.set_window_actions(
                                    win_actions.0,
                                    win_actions.1,
                                    win_actions.2,
                                    win_actions.3,
                                );
                            }
                            // Register globally so open_window() works from anywhere
                            let _ = OPEN_WINDOW_FN.set(open_fn);

                            // Register global window action callbacks (for drag_region() on Div)
                            WindowedApp::register_window_actions_static(
                                window.winit_window_arc(),
                                wake_proxy_for_windows.clone(),
                            );

                            // Set initial viewport size in BlincContextState
                            if let Some(ref windowed_ctx) = ws.ctx {
                                BlincContextState::get()
                                    .set_viewport_size(windowed_ctx.width, windowed_ctx.height);
                            }

                            // Initialize render state with the shared animation scheduler
                            // RenderState handles dynamic properties (cursor blink, animations)
                            // independently from tree structure changes
                            let mut rs = blinc_layout::RenderState::new(Arc::clone(&animations));
                            rs.set_shared_motion_states(Arc::clone(&shared_motion_states));
                            ws.render_state = Some(rs);

                            tracing::debug!("Blinc windowed ws.app initialized");
                        }
                        Err(e) => {
                            tracing::error!("Failed to initialize Blinc: {}", e);
                            return ControlFlow::Exit;
                        }
                    }
                } else {
                    // Resumed for a secondary window
                    let wid = window.id();
                    #[allow(clippy::map_entry)]
                    if !secondary_windows.contains_key(&wid) {
                        if let Some(ref blinc_app) = ws.app {
                            // Pop the matching pending request up front so we can use
                            // its `max_frame_latency` on the surface config below.
                            // Match by title — winit's `WindowId` isn't known to the
                            // pending side, and titles are practically unique per
                            // open_window call.
                            let pending_req = {
                                let winit_title = window.winit_window().title();
                                pending_builders().lock().ok().and_then(|mut p| {
                                    p.iter()
                                        .position(|r| r.config.title == winit_title)
                                        .map(|idx| p.remove(idx))
                                })
                            };
                            let secondary_latency = pending_req
                                .as_ref()
                                .map(|r| r.config.max_frame_latency.clamp(1, 3))
                                .unwrap_or(2);
                            let winit_window = window.winit_window_arc();
                            match blinc_app.create_surface_for_window(winit_window) {
                                Ok(surf) => {
                                    let (w, h) = window.size();
                                    let format = blinc_app.texture_format();
                                    let window_transparent = window.is_transparent();
                                    let alpha_mode = WindowedApp::pick_alpha_mode(
                                        &surf,
                                        blinc_app.adapter(),
                                        window_transparent,
                                    );
                                    let config = wgpu::SurfaceConfiguration {
                                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                            | wgpu::TextureUsages::COPY_SRC
                                            | wgpu::TextureUsages::COPY_DST,
                                        format,
                                        width: w,
                                        height: h,
                                        present_mode: preferred_present_mode(
                                            &surf,
                                            blinc_app.adapter(),
                                        ),
                                        alpha_mode,
                                        view_formats: vec![],
                                        desired_maximum_frame_latency: secondary_latency,
                                    };
                                    surf.configure(blinc_app.device(), &config);

                                    let mut sws = WindowState::new(
                                        Arc::clone(&css_anim_store),
                                        Arc::clone(&shared_motion_states),
                                    );
                                    sws.transparent = window_transparent;
                                    sws.surface = Some(surf);
                                    sws.surface_config = Some(config);

                                    sws.ctx = Some(WindowedContext::from_window(
                                        window,
                                        EventRouter::new(),
                                        Arc::clone(&animations),
                                        Arc::clone(&ref_dirty_flag),
                                        Arc::clone(&reactive),
                                        Arc::clone(&hooks),
                                        Arc::clone(&overlays),
                                        Arc::clone(&element_registry),
                                        Arc::clone(&ready_callbacks),
                                    ));

                                    if let Some(ref mut ctx) = sws.ctx {
                                        let wp = wake_proxy_for_windows.clone();
                                        ctx.set_open_window_fn(Arc::new(move |c| {
                                            wp.create_window(c);
                                        }));
                                        // Per-window actions
                                        let win_actions = WindowedApp::make_window_actions(
                                            window.winit_window_arc(),
                                            wake_proxy_for_windows.clone(),
                                        );
                                        ctx.set_window_actions(
                                            win_actions.0,
                                            win_actions.1,
                                            win_actions.2,
                                            win_actions.3,
                                        );
                                    }

                                    let mut rs =
                                        blinc_layout::RenderState::new(Arc::clone(&animations));
                                    rs.set_shared_motion_states(Arc::clone(&shared_motion_states));
                                    sws.render_state = Some(rs);

                                    // Adopt the UI builder from the pending request
                                    // that we already popped above (along with the
                                    // surface frame latency).
                                    if let Some(req) = pending_req {
                                        sws.ui_builder = req.builder;
                                    }

                                    // Per-window callbacks are set via set_window_actions above.
                                    // Global window_actions is NOT set — secondary windows
                                    // use ctx.close_callback() etc. instead.

                                    secondary_windows.insert(wid, sws);
                                    tracing::info!("Secondary window initialized (wid={:?})", wid);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to create surface for window: {}", e);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue
    }

    /// A window event: resize, scale change, focus, close, file drops.
    pub(super) fn on_window_event(
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
            Event::Window(_, WindowEvent::Resized { width, height }) => {
                if let (Some(blinc_app), Some(surf), Some(config)) =
                    (&mut ws.app, &ws.surface, &mut ws.surface_config)
                {
                    // winit fires a spurious Resized event when the window is first
                    // mapped, with the same dimensions used to configure the surface.
                    // Rebuilding on that no-op resize triggers a double initial build
                    // (visible as duplicated `build_ui` side effects) and — more
                    // critically — clobbers Stateful-handle state from the first build
                    // that downstream canvases depend on, so sketches wired up during
                    // the initial build stop painting after the phantom rebuild.
                    // Short-circuit when neither axis actually changed.
                    let dims_changed = config.width != width || config.height != height;
                    if width > 0 && height > 0 && dims_changed {
                        config.width = width;
                        config.height = height;
                        surf.configure(blinc_app.device(), config);
                        ws.needs_rebuild = true;
                        ws.needs_relayout = true;
                        // Resize invalidates every cached primitive
                        // position. Without this, the slow-path
                        // rebuild repopulates the cache but
                        // canvas-backed subtrees can still get
                        // their bounds resolved against the old
                        // cached layout because the static-cache
                        // texture itself isn't dropped. Match the
                        // ScaleFactorChanged handler's posture by
                        // tagging the invalidation explicitly.
                        blinc_app.invalidate_render_cache_tagged("window_resized");
                        frame_dirty.store(true, Ordering::Release);

                        // Dispatch RESIZE event to elements (use logical dimensions)
                        if let (Some(windowed_ctx), Some(tree)) = (&mut ws.ctx, &ws.render_tree) {
                            let logical_width = width as f32 / windowed_ctx.scale_factor as f32;
                            let logical_height = height as f32 / windowed_ctx.scale_factor as f32;

                            // Update windowed context dimensions - CRITICAL for layout computation
                            // Without this, compute_layout uses stale dimensions
                            windowed_ctx.width = logical_width;
                            windowed_ctx.height = logical_height;
                            windowed_ctx.physical_width = width as f32;
                            windowed_ctx.physical_height = height as f32;

                            // Update viewport size in BlincContextState for ElementHandle.is_visible()
                            BlincContextState::get()
                                .set_viewport_size(logical_width, logical_height);

                            windowed_ctx.event_router.on_window_resize(
                                tree,
                                logical_width,
                                logical_height,
                            );

                            // Clear layout bounds storages to force fresh calculations
                            // This prevents stale cached bounds from influencing the new layout
                            tree.clear_layout_bounds_storages();
                        }

                        // Request redraw to trigger relayout with new dimensions
                        window.request_redraw();
                    }
                }
            }

            // Multi-monitor: window dragged onto a monitor with a different
            // DPI / scale factor. Without this handler `windowed_ctx.scale_factor`
            // stays stale, the next `Resized` computes logical dims against the
            // old DPI, and the surface (which winit may have invalidated during
            // the monitor handoff) goes un-reconfigured — observed as a crash
            // or wrong-sized render the first time the window crosses monitors.
            //
            // Guard against the no-op case: macOS fires `ScaleFactorChanged`
            // early in window setup to report the real backing scale (1.0 → 2.0
            // on a Retina display). When the reported factor matches what the
            // context already has, the rebuild this handler used to force was
            // pure waste — observed as "build_ui called twice on launch" on
            // cn_demo. Only fire the rebuild path when scale, physical dims,
            // or surface config actually changed.
            Event::Window(_, WindowEvent::ScaleFactorChanged { scale_factor }) => {
                let (phys_w, phys_h) = window.size();
                let prev_scale = ws.ctx.as_ref().map(|c| c.scale_factor).unwrap_or(0.0);
                let prev_phys_w = ws.ctx.as_ref().map(|c| c.physical_width).unwrap_or(0.0);
                let prev_phys_h = ws.ctx.as_ref().map(|c| c.physical_height).unwrap_or(0.0);
                let scale_changed = (scale_factor - prev_scale).abs() > f64::EPSILON;
                let dims_changed = (phys_w as f32 - prev_phys_w).abs() > f32::EPSILON
                    || (phys_h as f32 - prev_phys_h).abs() > f32::EPSILON;
                if !scale_changed && !dims_changed {
                    // No-op event — common at startup on macOS Retina.
                    return ControlFlow::Continue;
                }

                if let (Some(blinc_app), Some(surf), Some(config)) =
                    (&ws.app, &ws.surface, &mut ws.surface_config)
                {
                    if phys_w > 0
                        && phys_h > 0
                        && (config.width != phys_w || config.height != phys_h)
                    {
                        config.width = phys_w;
                        config.height = phys_h;
                        surf.configure(blinc_app.device(), config);
                    }
                }
                if let Some(ref mut windowed_ctx) = ws.ctx {
                    windowed_ctx.scale_factor = scale_factor;
                    windowed_ctx.physical_width = phys_w as f32;
                    windowed_ctx.physical_height = phys_h as f32;
                    let logical_width = phys_w as f32 / scale_factor as f32;
                    let logical_height = phys_h as f32 / scale_factor as f32;
                    windowed_ctx.width = logical_width;
                    windowed_ctx.height = logical_height;
                    BlincContextState::get().set_viewport_size(logical_width, logical_height);
                    if let Some(ref tree) = ws.render_tree {
                        windowed_ctx.event_router.on_window_resize(
                            tree,
                            logical_width,
                            logical_height,
                        );
                        tree.clear_layout_bounds_storages();
                    }
                }
                ws.needs_rebuild = true;
                ws.needs_relayout = true;
                frame_dirty.store(true, Ordering::Release);
                window.request_redraw();
            }

            Event::Window(_, WindowEvent::Focused(focused)) => {
                // Update context focus state
                if let Some(ref mut windowed_ctx) = ws.ctx {
                    windowed_ctx.focused = focused;
                    windowed_ctx.event_router.on_window_focus(focused);

                    if !focused {
                        blinc_layout::widgets::blur_all_text_inputs();
                    }
                }
            }

            Event::Window(_, WindowEvent::CloseRequested) => {
                return ControlFlow::Exit;
            }

            // File drop events — dispatch to drop handler and render tree
            Event::Window(_, WindowEvent::DroppedFile { paths }) => {
                crate::dnd::dispatch_drop_event(crate::dnd::DropEvent::Dropped(paths));
            }
            Event::Window(_, WindowEvent::DroppedFileHovered { paths }) => {
                crate::dnd::dispatch_drop_event(crate::dnd::DropEvent::Hovered(paths));
            }
            Event::Window(_, WindowEvent::DroppedFileCancelled) => {
                crate::dnd::dispatch_drop_event(crate::dnd::DropEvent::Cancelled);
            }

            // Handle input events
            _ => {}
        }
        ControlFlow::Continue
    }
}
