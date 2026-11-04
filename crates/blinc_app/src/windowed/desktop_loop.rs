//! The desktop event loop: its state and event handler.

use std::marker::PhantomData;

use super::*;

/// Everything the event handler keeps between events.
pub(super) struct DesktopLoop<F, E> {
    pub(super) platform: DesktopPlatform,
    pub(super) primary_max_frame_latency: u32,
    pub(super) animation_fps_cap: Option<u32>,
    pub(super) animation_fps_cap_atomic: Arc<std::sync::atomic::AtomicU32>,
    pub(super) fps_adapter: Option<Arc<Mutex<FpsAdapter>>>,
    pub(super) wake_proxy_for_windows: blinc_platform_desktop::WakeProxy,
    pub(super) wake_proxy_for_pacing: blinc_platform_desktop::WakeProxy,
    pub(super) frame_dirty: Arc<AtomicBool>,
    pub(super) visible_anim_for_wake: Arc<AtomicBool>,
    pub(super) ref_dirty_flag: RefDirtyFlag,
    pub(super) reactive: SharedReactiveGraph,
    pub(super) hooks: SharedHookState,
    pub(super) animations: SharedAnimationScheduler,
    pub(super) css_anim_store: Arc<Mutex<blinc_layout::CssAnimationStore>>,
    pub(super) element_registry: SharedElementRegistry,
    pub(super) ready_callbacks: SharedReadyCallbacks,
    pub(super) shared_motion_states: blinc_layout::render_state::SharedMotionStates,
    pub(super) overlays: OverlayManager,
    pub(super) ws: WindowState,
    pub(super) primary_wid: Option<blinc_platform::WindowId>,
    pub(super) secondary_windows: std::collections::HashMap<blinc_platform::WindowId, WindowState>,
    pub(super) ui_builder: F,
    pub(super) _builder_output: PhantomData<fn() -> E>,
}

impl<F, E> DesktopLoop<F, E>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    /// Handle one event from the platform event loop.
    pub(super) fn handle(
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
        // Mark the next frame dirty for any non-Frame event. Input,
        // lifecycle changes, drag/drop, etc. are all "something
        // happened" signals — the next OS frame should actually
        // render rather than skip. Frame events are the OS asking
        // us to render; whether we should is decided below by the
        // `frame_dirty` swap at the top of `Event::Frame`.
        //
        // Exception: bare mouse moves are too frequent to flip
        // unconditionally (60–120 events/s during drag and hover).
        // For those we let the input handler decide whether anything
        // visible changed; if a hover handler / Stateful dispatch
        // fires, it sets `NEEDS_REDRAW`, which the `Event::Frame`
        // gate also honours. Skipping the blanket flip here keeps
        // a static UI from re-rendering at vsync just because the
        // pointer is in motion.
        //
        // We pair the dirty flip with a `request_redraw()` because
        // under `ControlFlow::Wait` (set by the desktop platform
        // shim — Linux/Wayland/X11 had no other pacing and burned
        // 25% CPU just spinning the loop in Poll) `frame_dirty`
        // alone does nothing; winit only delivers the next
        // `RedrawRequested → Event::Frame` if someone actually
        // asks for it. macOS used to coast on Poll's
        // request_redraw spam, which we removed at the same time.
        // Per-event trace for debugging idle-CPU / mouse-move
        // hot paths. Silent in normal builds (`tracing::trace!`
        // doesn't evaluate args when the target is disabled).
        // Run with
        //   RUST_LOG=blinc_app::events=trace
        // and the output gets one line per event; pipe through
        //   `grep -oE 'kind=[^ ]+' | sort | uniq -c`
        // to see counts per event-kind over a sampling window.
        tracing::trace!(
            target: "blinc_app::events",
            kind = match &event {
                Event::Frame(_) => "frame",
                Event::Input(_, InputEvent::Mouse(MouseEvent::Moved { .. })) =>
                    "input.mouse.moved",
                Event::Input(_, InputEvent::Mouse(MouseEvent::ButtonPressed { .. })) =>
                    "input.mouse.pressed",
                Event::Input(_, InputEvent::Mouse(MouseEvent::ButtonReleased { .. })) =>
                    "input.mouse.released",
                Event::Input(_, InputEvent::Mouse(MouseEvent::Entered)) =>
                    "input.mouse.entered",
                Event::Input(_, InputEvent::Mouse(MouseEvent::Left)) =>
                    "input.mouse.left",
                Event::Input(_, InputEvent::Scroll { .. }) => "input.scroll",
                Event::Input(_, InputEvent::ScrollEnd) => "input.scroll_end",
                Event::Input(_, InputEvent::Keyboard(_)) => "input.keyboard",
                Event::Input(_, InputEvent::Touch(_)) => "input.touch",
                Event::Input(_, InputEvent::Pinch { .. }) => "input.pinch",
                Event::Input(_, InputEvent::Rotation { .. }) => "input.rotation",
                Event::Input(_, _) => "input.other",
                Event::Window(_, WindowEvent::Resized { .. }) => "window.resized",
                Event::Window(_, WindowEvent::Moved { .. }) => "window.moved",
                Event::Window(_, WindowEvent::CloseRequested) => "window.close_requested",
                Event::Window(_, WindowEvent::Focused(_)) => "window.focused",
                Event::Window(_, WindowEvent::ScaleFactorChanged { .. }) =>
                    "window.scale_factor_changed",
                Event::Window(_, WindowEvent::DroppedFileHovered { .. }) =>
                    "window.dropped_file_hovered",
                Event::Window(_, WindowEvent::DroppedFile { .. }) =>
                    "window.dropped_file",
                Event::Window(_, WindowEvent::DroppedFileCancelled) =>
                    "window.dropped_file_cancelled",
                Event::Lifecycle(LifecycleEvent::Resumed) => "lifecycle.resumed",
                Event::Lifecycle(LifecycleEvent::Suspended) => "lifecycle.suspended",
                Event::Lifecycle(LifecycleEvent::LowMemory) => "lifecycle.low_memory",
            },
            "blinc event"
        );

        let is_bare_mouse_move = matches!(
            event,
            Event::Input(_, InputEvent::Mouse(MouseEvent::Moved { .. }))
        );
        let event_wid = match &event {
            Event::Window(wid, _) | Event::Input(wid, _) | Event::Frame(wid) => Some(*wid),
            _ => None,
        };
        let is_secondary = event_wid
            .map(|wid| primary_wid.is_some_and(|p| wid != p))
            .unwrap_or(false);
        // Mouse button press/release on a non-interactive area
        // doesn't need to schedule a redraw — if the click fires
        // a handler that mutates state, the post-dispatch check
        // (`peek_needs_redraw || has_pending_subtree_rebuilds`)
        // at the bottom of the input branch will arm one. Without
        // this gate, every click on empty space ran two full
        // renders (press + release) on cn_demo's complex tree,
        // burning ~30 % CPU for rapid clicks where nothing
        // visible changes. Scroll/keyboard/touch events kept
        // unconditional because their handlers commonly update
        // physics or input state via paths that don't go through
        // `NEEDS_REDRAW` (scroll offset, IME composition).
        let is_mouse_button = matches!(
            event,
            Event::Input(
                _,
                InputEvent::Mouse(
                    MouseEvent::ButtonPressed { .. } | MouseEvent::ButtonReleased { .. }
                )
            )
        );
        if !matches!(event, Event::Frame(_)) && !is_bare_mouse_move && !is_mouse_button {
            frame_dirty.store(true, Ordering::Release);
            window.request_redraw();
        }

        // Bare-mouse-move fast path. Linux high-rate mice
        // (gaming mice on Hyprland in particular) deliver
        // 1 kHz `CursorMoved` events; running the full input
        // pipeline (Vec scratch alloc, `Box<dyn FnMut>`
        // event-callback alloc, hit_test, hover diff, cursor
        // resolve) once per move puts the process at ~60% of
        // a CPU on `hello_blinc` even though no element in
        // the tree could react. The Moved branch already
        // had an early-return guard with the same predicate,
        // but the prelude that gets us there allocated and
        // destructed unconditionally — so the branch ran
        // 1000 times/sec on a UI that needs zero pointer
        // work.
        //
        // The cached predicate on `RenderTree` is one
        // relaxed atomic load; recomputed lazily on next
        // read after a tree mutation invalidates it. For a
        // static UI with no handlers / no `:hover` / no
        // `cursor:` styles it stays `false` from the first
        // call after build until the next rebuild, so this
        // branch returns ~immediately and the closure exits
        // without touching `ws.ctx`, `ws.render_tree`, or
        // any allocator.
        if is_bare_mouse_move && !is_secondary {
            let pipeline_needed = ws
                .render_tree
                .as_ref()
                .is_some_and(|tree| tree.mouse_move_pipeline_needed());
            if !pipeline_needed {
                return ControlFlow::Continue;
            }
            // Cursor-only fast path. When the tree has cursor
            // styles but no pointer handlers / hover rules, the
            // entire input branch below ends up allocating two
            // `Vec<PendingEvent>`s + boxing a `FnMut` event
            // callback per move just to fall into the cursor-
            // resolve early-return at the very top. For
            // `hello_blinc` (a single `text()` with the default
            // I-beam cursor style) that allocation churn was the
            // entire 11 % CPU baseline during continuous mouse
            // movement. Resolving cursor inline up here keeps
            // the bare-move-only-needs-cursor case allocation-
            // free and reuses the `last_cursor` cache so the
            // OS `set_cursor` syscall only fires on transitions.
            let needs_pointer_dispatch = ws.render_tree.as_ref().is_some_and(|tree| {
                tree.handler_registry().has_any_pointer_handler()
                    || tree
                        .stylesheet()
                        .is_some_and(|s| s.has_pointer_state_rules())
            });
            if !needs_pointer_dispatch {
                if let (Some(blinc_ctx), Some(tree)) = (&mut ws.ctx, &ws.render_tree) {
                    if let Event::Input(_, InputEvent::Mouse(MouseEvent::Moved { x, y })) = &event {
                        let scale = blinc_ctx.scale_factor as f32;
                        let lx = *x / scale;
                        let ly = *y / scale;
                        let cursor = tree
                            .get_cursor_at(&blinc_ctx.event_router, lx, ly)
                            .unwrap_or(CursorStyle::Default);
                        let want = convert_cursor_style(cursor);
                        if ws.last_cursor != Some(want) {
                            window.set_cursor(want);
                            ws.last_cursor = Some(want);
                        }
                    }
                }
                return ControlFlow::Continue;
            }
        }

        // Handle secondary window events
        if is_secondary {
            let wid = event_wid.unwrap();
            match event {
                Event::Window(_, WindowEvent::Resized { width, height }) => {
                    if let Some(sws) = secondary_windows.get_mut(&wid) {
                        if let (Some(surf), Some(config)) = (&sws.surface, &mut sws.surface_config)
                        {
                            if width > 0 && height > 0 {
                                config.width = width;
                                config.height = height;
                                if let Some(ref blinc_app) = ws.app {
                                    surf.configure(blinc_app.device(), config);
                                }
                                sws.needs_rebuild = true;
                                if let Some(ref mut ctx) = sws.ctx {
                                    let sf = window.scale_factor();
                                    ctx.width = width as f32 / sf as f32;
                                    ctx.height = height as f32 / sf as f32;
                                    ctx.physical_width = width as f32;
                                    ctx.physical_height = height as f32;
                                    ctx.scale_factor = sf;
                                }
                            }
                        }
                    }
                }
                Event::Window(_, WindowEvent::CloseRequested) => {
                    secondary_windows.remove(&wid);
                    tracing::info!("Secondary window closed (wid={:?})", wid);
                }
                Event::Window(_, WindowEvent::Focused(_focused)) => {}
                Event::Input(_, ref input_event) => {
                    if let Some(sws) = secondary_windows.get_mut(&wid) {
                        if let (Some(ctx), Some(tree)) = (&mut sws.ctx, &mut sws.render_tree) {
                            let sf = ctx.scale_factor as f32;

                            // Collect events from the router
                            let mut pending: Vec<(blinc_layout::tree::LayoutNodeId, u32)> =
                                Vec::new();
                            ctx.event_router.set_event_callback({
                                let events = &mut pending
                                    as *mut Vec<(blinc_layout::tree::LayoutNodeId, u32)>;
                                move |node, event_type| unsafe {
                                    (*events).push((node, event_type));
                                }
                            });

                            let convert_button = |b: &blinc_platform::MouseButton| match b {
                                blinc_platform::MouseButton::Left => {
                                    blinc_layout::prelude::MouseButton::Left
                                }
                                blinc_platform::MouseButton::Right => {
                                    blinc_layout::prelude::MouseButton::Right
                                }
                                blinc_platform::MouseButton::Middle => {
                                    blinc_layout::prelude::MouseButton::Middle
                                }
                                _ => blinc_layout::prelude::MouseButton::Left,
                            };

                            match input_event {
                                InputEvent::Mouse(MouseEvent::Moved { x, y }) => {
                                    ctx.event_router.on_mouse_move(tree, *x / sf, *y / sf);
                                }
                                InputEvent::Mouse(MouseEvent::ButtonPressed { button, x, y }) => {
                                    // Mark this event as mouse (not
                                    // touch) input so editable widgets
                                    // restore desktop semantics
                                    // (drag = extend selection). The
                                    // flag is sticky between events
                                    // and gets flipped back to true
                                    // by the touch path on
                                    // touchscreens — desktop runners
                                    // don't see touch events at all,
                                    // but a docked tablet running
                                    // the desktop runner could mix
                                    // both, so we set this on every
                                    // mouse press to be safe.
                                    blinc_layout::widgets::text_input::set_touch_input(false);
                                    ctx.event_router.on_mouse_down(
                                        tree,
                                        *x / sf,
                                        *y / sf,
                                        convert_button(button),
                                    );
                                }
                                InputEvent::Mouse(MouseEvent::ButtonReleased { button, x, y }) => {
                                    ctx.event_router.on_mouse_up(
                                        tree,
                                        *x / sf,
                                        *y / sf,
                                        convert_button(button),
                                    );
                                }
                                _ => {}
                            }

                            ctx.event_router.clear_event_callback();

                            // Dispatch collected events through render tree handlers
                            for (node_id, event_type) in &pending {
                                tree.dispatch_event(
                                    *node_id,
                                    *event_type,
                                    ctx.event_router.mouse_position().0,
                                    ctx.event_router.mouse_position().1,
                                );
                            }
                        }
                    }
                }
                Event::Frame(_) => {
                    if let Some(sws) = secondary_windows.get_mut(&wid) {
                        if let (Some(blinc_app), Some(surf), Some(config)) =
                            (&mut ws.app, &sws.surface, &sws.surface_config)
                        {
                            // Build render tree on first frame or after resize
                            if sws.render_tree.is_none() || sws.needs_rebuild {
                                let build_pass = sws.build_scope.begin();
                                let (w, h) = sws
                                    .ctx
                                    .as_ref()
                                    .map(|c| (c.width, c.height))
                                    .unwrap_or((400.0, 300.0));

                                let ui: Div = if let Some(ref mut builder) = sws.ui_builder {
                                    if let Some(ref mut sctx) = sws.ctx {
                                        invoke_window_builder(builder, sctx)
                                    } else {
                                        div().w(w).h(h)
                                    }
                                } else {
                                    let title = window.winit_window().title();
                                    div()
                                        .w(w)
                                        .h(h)
                                        .bg(blinc_core::Color::rgba(0.06, 0.06, 0.09, 1.0))
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .gap_px(12.0)
                                        .child(
                                            text(&title)
                                                .size(24.0)
                                                .color(blinc_core::Color::WHITE)
                                                .bold(),
                                        )
                                        .child(
                                            text(format!("{:.0} x {:.0}", w, h))
                                                .size(14.0)
                                                .color(blinc_core::Color::rgba(0.5, 0.5, 0.6, 1.0)),
                                        )
                                };

                                let sf = sws
                                    .ctx
                                    .as_ref()
                                    .map(|c| c.scale_factor as f32)
                                    .unwrap_or(1.0);
                                let mut tree = RenderTree::from_element(&ui);
                                tree.set_scale_factor(sf);
                                tree.compute_layout(w, h);
                                sws.render_tree = Some(tree);
                                sws.needs_rebuild = false;
                                sws.build_scope.finish(build_pass);
                            }

                            if let Some(tree) = sws.render_tree.as_mut() {
                                let updates = tree.take_pending_property_updates();
                                let effects = tree.apply_partial_property_updates(updates);
                                let rebuilt = tree.process_pending_subtree_rebuilds_routed(
                                    sws.ctx.as_ref().map(|ctx| &ctx.event_router),
                                );
                                if effects.needs_layout || rebuilt {
                                    let (w, h) = sws
                                        .ctx
                                        .as_ref()
                                        .map(|ctx| (ctx.width, ctx.height))
                                        .unwrap_or((400.0, 300.0));
                                    tree.apply_stylesheet_layout_overrides();
                                    tree.compute_layout(w, h);
                                    tree.apply_flip_transitions();
                                    tree.update_flip_bounds();
                                }
                            }

                            // Render the tree (skip if minimized / zero size)
                            if config.width > 0 && config.height > 0 {
                                if let (Some(tree), Some(rs)) =
                                    (&sws.render_tree, &sws.render_state)
                                {
                                    match surf.get_current_texture() {
                                        Ok(frame) => {
                                            let view = frame.texture.create_view(
                                                &wgpu::TextureViewDescriptor::default(),
                                            );
                                            blinc_app.set_clear_alpha(if sws.transparent {
                                                0.0
                                            } else {
                                                1.0
                                            });
                                            let _ = blinc_app.render_tree_with_motion(
                                                tree,
                                                rs,
                                                &view,
                                                config.width,
                                                config.height,
                                            );
                                            window.pre_present_notify();
                                            frame.present();
                                        }
                                        Err(
                                            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated,
                                        ) => {
                                            surf.configure(blinc_app.device(), config);
                                        }
                                        Err(_) => {}
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            return ControlFlow::Continue;
        }

        match event {
            Event::Lifecycle(LifecycleEvent::Resumed) => self.on_resumed(event, window),
            Event::Window(..) => self.on_window_event(event, window),
            Event::Input(..) => self.on_input(event, window),
            Event::Frame(_) => self.on_frame(event, window),
            _ => ControlFlow::Continue,
        }
    }
}
