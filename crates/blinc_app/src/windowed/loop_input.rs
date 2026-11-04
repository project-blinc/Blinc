//! Event-loop handler for input events.

#![allow(unused_variables)]

use super::desktop_loop::DesktopLoop;
use super::*;

impl<F, E> DesktopLoop<F, E>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    /// An input event: pointer, scroll, keyboard, touch.
    pub(super) fn on_input(
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
            Event::Input(_, input_event) => {
                // Classify the event up front — `input_event` is
                // consumed by the match below, so the
                // post-dispatch redraw gate at the bottom of
                // this arm can't peek at it. Bool flags are
                // cheap and survive the move.
                let is_button_event = matches!(
                    input_event,
                    InputEvent::Mouse(MouseEvent::ButtonPressed { .. })
                        | InputEvent::Mouse(MouseEvent::ButtonReleased { .. })
                        | InputEvent::Touch(_)
                );
                let is_move_event = matches!(
                    input_event,
                    InputEvent::Mouse(MouseEvent::Moved { .. })
                        | InputEvent::Mouse(MouseEvent::Entered)
                        | InputEvent::Mouse(MouseEvent::Left),
                );
                // Cache invalidation now happens AFTER dispatch
                // (see the post-dispatch `peek_needs_redraw`
                // block at the bottom of this arm), gated on a
                // signal that the dispatch actually changed
                // render state. Invalidating up front for every
                // input — including bare mouse-moves that just
                // hit-test and produce no state change — meant
                // the cache never lived more than a single
                // frame on a UI with hover-eligible elements
                // (cn_demo's buttons make `mouse_move_pipeline_
                // _needed()` return `true`, so every mouse-move
                // enters this arm). Fast path was rebuilding
                // constantly and CPU never dropped.
                // Pending event structure for deferred dispatch
                #[derive(Clone)]
                struct PendingEvent {
                    node_id: LayoutNodeId,
                    event_type: u32,
                    mouse_x: f32,
                    mouse_y: f32,
                    /// Local coordinates relative to element bounds
                    local_x: f32,
                    local_y: f32,
                    /// Absolute position of element bounds (top-left corner)
                    bounds_x: f32,
                    bounds_y: f32,
                    /// Computed bounds dimensions of the element
                    bounds_width: f32,
                    bounds_height: f32,
                    scroll_delta_x: f32,
                    scroll_delta_y: f32,
                    /// Drag delta for DRAG/DRAG_END events
                    drag_delta_x: f32,
                    drag_delta_y: f32,
                    key_char: Option<char>,
                    key_code: u32,
                    shift: bool,
                    ctrl: bool,
                    alt: bool,
                    meta: bool,
                    /// Pinch scale or rotation delta
                    pinch_scale: f32,
                }

                impl Default for PendingEvent {
                    fn default() -> Self {
                        Self {
                            node_id: LayoutNodeId::default(),
                            event_type: 0,
                            mouse_x: 0.0,
                            mouse_y: 0.0,
                            local_x: 0.0,
                            local_y: 0.0,
                            bounds_x: 0.0,
                            bounds_y: 0.0,
                            bounds_width: 0.0,
                            bounds_height: 0.0,
                            scroll_delta_x: 0.0,
                            scroll_delta_y: 0.0,
                            drag_delta_x: 0.0,
                            drag_delta_y: 0.0,
                            key_char: None,
                            key_code: 0,
                            shift: false,
                            ctrl: false,
                            alt: false,
                            meta: false,
                            pinch_scale: 1.0,
                        }
                    }
                }

                // First phase: collect events using immutable borrow
                let (
                    pending_events,
                    keyboard_events,
                    scroll_ended,
                    gesture_ended,
                    scroll_info,
                    scroll_cancel_hit,
                ) = if let (Some(windowed_ctx), Some(tree)) = (&mut ws.ctx, &ws.render_tree) {
                    let router = &mut windowed_ctx.event_router;

                    // Collect events from router
                    let mut pending_events: Vec<PendingEvent> = Vec::new();
                    // Separate collection for keyboard events (TEXT_INPUT)
                    let mut keyboard_events: Vec<PendingEvent> = Vec::new();
                    // Track if scroll ended (momentum finished)
                    let mut scroll_ended = false;
                    // Track if gesture ended (finger lifted - may still have momentum)
                    let mut gesture_ended = false;
                    // Track scroll info for nested scroll dispatch (mouse_x, mouse_y, delta_x, delta_y)
                    let mut scroll_info: Option<(f32, f32, f32, f32)> = None;
                    // Hit chain (leaf + ancestors) captured at mouse-down so the
                    // mutable phase can cancel any active scroll animation under
                    // the cursor — the "grab-to-stop" affordance.
                    let mut scroll_cancel_hit: Option<(
                        blinc_layout::LayoutNodeId,
                        Vec<blinc_layout::LayoutNodeId>,
                    )> = None;

                    // Set up callback to collect events
                    router.set_event_callback({
                        let events = &mut pending_events as *mut Vec<PendingEvent>;
                        move |node, event_type| {
                            // SAFETY: This callback is only used within this scope
                            unsafe {
                                (*events).push(PendingEvent {
                                    node_id: node,
                                    event_type,
                                    ..Default::default()
                                });
                            }
                        }
                    });

                    // Note: Overlays are now part of the main tree, so all events
                    // are routed through the single main event router.

                    // Convert physical coordinates to logical for hit testing
                    let scale = windowed_ctx.scale_factor as f32;

                    match input_event {
                        InputEvent::Mouse(mouse_event) => match mouse_event {
                            MouseEvent::Moved { x, y } => {
                                // Convert physical to logical coordinates
                                let lx = x / scale;
                                let ly = y / scale;

                                // Short-circuit: cursor stayed inside the deepest
                                // hit node's AABB from the previous move → the
                                // hover set can't have changed, so the entire
                                // dispatch pipeline (hit_test, hover diff, emit,
                                // cursor lookup) is wasted work. macOS / Linux
                                // gaming mice fire at hundreds of Hz; without
                                // this gate cn_demo's tree-walk per event ate
                                // ~20 % CPU on cursor wiggles.
                                //
                                // Conditions to take the short-circuit:
                                //  - No press in flight (drag handlers need
                                //    every move).
                                //  - No `pointer_query` (continuous coord
                                //    consumers — flow shaders, calc(env(...))).
                                //  - No POINTER_MOVE handler on the tree (real
                                //    continuous-tracking subscribers).
                                //  - Cursor still inside last leaf bounds.
                                let pressed = router.is_press_in_flight();
                                let pointer_query_active = !windowed_ctx.pointer_query.is_empty();
                                let has_move_subscriber =
                                    tree.handler_registry().has_any_pointer_move_subscriber();
                                // A node publishing `text_hit_spans`
                                // varies its cursor ALONG itself, so
                                // "still inside the same leaf" no
                                // longer means "nothing to re-resolve"
                                // — the early-out is why a link inside
                                // rich text never got its pointer while
                                // its click worked: a click always
                                // dispatches, the cursor only re-resolves
                                // when the hovered node changes.
                                // ANY node in the hovered chain, not
                                // just the leaf: the node publishing
                                // spans may be an ancestor of
                                // whatever hit-testing returned.
                                let leaf_varies_cursor = router.last_hit_chain().iter().any(|n| {
                                    tree.get_render_node(*n)
                                        .is_some_and(|n| n.props.text_hit_spans.is_some())
                                });
                                if !pressed
                                    && !pointer_query_active
                                    && !has_move_subscriber
                                    && !leaf_varies_cursor
                                    && router.cursor_inside_last_leaf(lx, ly)
                                {
                                    router.set_mouse_position(lx, ly);
                                    // Drop the event callback before
                                    // returning — it holds a raw ptr
                                    // to `pending_events`'s stack
                                    // slot, which would dangle after
                                    // this scope exits. A later
                                    // event (e.g. Resized broadcast)
                                    // firing the stale callback
                                    // writes to freed memory and
                                    // corrupts the (since-replaced)
                                    // Vec header, surfacing as
                                    // "capacity overflow" on the
                                    // next push.
                                    router.clear_event_callback();
                                    return ControlFlow::Continue;
                                }

                                // Skip the heavy mouse-move pipeline (hit_test_all,
                                // hover-set diff, POINTER_ENTER / LEAVE emission,
                                // drag-delta tracking) if nothing in the tree could
                                // react to it: no node with a registered pointer
                                // handler, no CSS rule keyed on `:hover` / `:active`,
                                // and no node carries a custom `cursor:` style that
                                // would need re-resolving when the pointer crosses
                                // an element boundary.
                                //
                                // `hello_blinc` and similar static views now stay
                                // at near-zero CPU even during a continuous drag.
                                // Per-move cost was previously: hit_test_all +
                                // hover diff + DRAG emission + cursor hit_test +
                                // OS `set_cursor` syscall. With nothing listening,
                                // all of that is wasted work.
                                let needs_pointer_dispatch =
                                    tree.handler_registry().has_any_pointer_handler()
                                        || tree
                                            .stylesheet()
                                            .is_some_and(|s| s.has_pointer_state_rules());
                                let needs_cursor_resolve = tree.has_any_cursor_style();
                                if !needs_pointer_dispatch && !needs_cursor_resolve {
                                    // Reset the OS cursor to Default (only if we
                                    // previously asked for something else — `Default`
                                    // is the OS's idle state). Caches the last
                                    // request so the syscall fires at most once
                                    // when the UI transitions from "had a styled
                                    // cursor" to "no longer does".
                                    let want = blinc_platform::Cursor::Default;
                                    if ws.last_cursor != Some(want) {
                                        window.set_cursor(want);
                                        ws.last_cursor = Some(want);
                                    }
                                    // See the matching clear above for why.
                                    router.clear_event_callback();
                                    return ControlFlow::Continue;
                                }
                                if !needs_pointer_dispatch {
                                    // Cursor-only path: do the cheap one-shot
                                    // `hit_test` to resolve `cursor:` styles, but
                                    // skip the full hover-diff machinery.
                                    let cursor = tree
                                        .get_cursor_at(router, lx, ly)
                                        .unwrap_or(CursorStyle::Default);
                                    let want = convert_cursor_style(cursor);
                                    if ws.last_cursor != Some(want) {
                                        window.set_cursor(want);
                                        ws.last_cursor = Some(want);
                                    }
                                    // See the matching clear above for why.
                                    router.clear_event_callback();
                                    return ControlFlow::Continue;
                                }

                                // Sub-frame coalescing — Phase 3.1 of
                                // [[project-reactive-architecture-v2]]. High-rate
                                // mice (1000 Hz Linux gaming mice in particular)
                                // fire `MouseEvent::Moved` at >> display refresh,
                                // and each event runs the full hit_test + hover
                                // diff + handler dispatch path below — overkill
                                // since at most one paint per vsync interval can
                                // surface a visible result. Skip the heavy path
                                // when the previous dispatch was very recent AND
                                // dropping this event is safe:
                                //   - No press in flight (drag handlers need
                                //     every move to track velocity / position).
                                //   - No `POINTER_MOVE` subscriber on the tree
                                //     (continuous-tracking handlers like
                                //     scroll-velocity tracking, sketch coords).
                                //   - No pointer_query consumer (flow shaders,
                                //     calc(env(...)) — already check this in
                                //     the short-circuit a few branches up).
                                // The window is conservative — half a vsync at
                                // 60 Hz. At 1000 Hz mouse rate this turns ~16
                                // dispatches/frame into 2; at 144 Hz display +
                                // 1000 Hz mouse, ~7 → 1. Cursor + frame_dirty
                                // are still updated inline so the next paint
                                // reflects the latest cursor position.
                                //
                                // The last move before a stop won't be lost
                                // because: any new event after the 8ms window
                                // dispatches with its own (current) (lx, ly).
                                // The "user moved fast then stopped exactly
                                // mid-window" edge case leaves hover state up
                                // to 8 ms stale until the next event — half a
                                // 60 Hz frame, imperceptible.
                                if let Some(last) = ws.last_pointer_dispatch {
                                    const COALESCE_WINDOW: std::time::Duration =
                                        std::time::Duration::from_millis(8);
                                    let elapsed = last.elapsed();
                                    if elapsed < COALESCE_WINDOW
                                        && !router.is_press_in_flight()
                                        && !has_move_subscriber
                                    {
                                        // Cursor resolution reuses the cached
                                        // hit chain from the previous full
                                        // dispatch (`get_cursor_for_last_hit`)
                                        // instead of a fresh `hit_test`. The
                                        // whole point of this gate is to skip
                                        // dispatch cost; running a fresh
                                        // hit_test for cursor would burn most
                                        // of the savings — see
                                        // `get_cursor_for_last_hit`'s docstring
                                        // ("ate ~10 % CPU on its own").
                                        // Tradeoff: cursor style is up to
                                        // 8 ms stale when the user crosses an
                                        // element boundary mid-skip-window —
                                        // half-frame at 60 Hz, imperceptible.
                                        let cursor = tree
                                            .get_cursor_for_last_hit(router, lx, ly)
                                            .unwrap_or(CursorStyle::Default);
                                        let want = convert_cursor_style(cursor);
                                        if ws.last_cursor != Some(want) {
                                            window.set_cursor(want);
                                            ws.last_cursor = Some(want);
                                        }
                                        // Router-internal cursor tracking
                                        // stays current so
                                        // `cursor_inside_last_leaf` and
                                        // other queries see the latest pos.
                                        router.set_mouse_position(lx, ly);
                                        router.clear_event_callback();
                                        return ControlFlow::Continue;
                                    }
                                }

                                // Get overlay bounds and layer ID for occlusion-aware hit testing
                                // This prevents background elements from receiving hover events
                                // when they are visually occluded by overlay content
                                let overlay_bounds =
                                    windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                                let overlay_layer_id = tree
                                    .query_by_id(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID);

                                // Drag fast path: while a press is in flight the
                                // pressed target is fixed at mouse-down, so hover
                                // diff / ENTER / LEAVE / cursor lookup are all
                                // unnecessary. Just emit DRAG to the pressed
                                // target + ancestors. Skips the full tree walk
                                // that `on_mouse_move_with_occlusion` does — at
                                // 100+ Hz move rates × cn_demo's tree depth, that
                                // walk was the dominant remaining cost during a
                                // drag (the gap between hello_blinc's 11 % drag
                                // CPU and cn_demo's 25 %).
                                if router.is_press_in_flight() {
                                    router.on_mouse_drag_fast(tree, lx, ly);
                                } else {
                                    // Route mouse move through main tree with overlay occlusion awareness
                                    router.on_mouse_move_with_occlusion(
                                        tree,
                                        lx,
                                        ly,
                                        &overlay_bounds,
                                        overlay_layer_id,
                                    );
                                }

                                // Stamp the dispatch timestamp so the
                                // sub-frame coalescing gate above can
                                // skip subsequent moves within the
                                // next 8 ms. Phase 3.1.
                                ws.last_pointer_dispatch = Some(std::time::Instant::now());

                                // Crossing an element boundary changes CSS `:hover`
                                // styling and may switch which Stateful is in
                                // its `Hover` state — flip dirty so the next
                                // Event::Frame paints the new look.
                                //
                                // We deliberately do NOT include `DRAG` /
                                // `DRAG_END` here: the router emits a `DRAG`
                                // event for every mouse move while a button is
                                // held, regardless of whether any handler is
                                // attached. Including them turned a bare
                                // mouse-down + drag in `hello_blinc` (no
                                // handlers anywhere) into a 60–120 Hz redraw
                                // loop pinning ~30 % CPU. Stateful-driven drag
                                // (sliders, sortable, splitter panes) is
                                // already covered by the post-dispatch
                                // peek-needs-redraw check below — the drag
                                // handler mutates `State`/`Stateful`, that
                                // sets `NEEDS_REDRAW`, and we honour it.
                                // Pre-fix: every POINTER_ENTER / POINTER_LEAVE
                                // unconditionally invalidated the cache, even when
                                // the entering / leaving element had no `:hover`
                                // styling at all — every mouse move that crossed
                                // any element boundary forced a full slow-path
                                // repaint. styling_demo logs showed 142 of these
                                // hover invalidations during a 13-second run.
                                //
                                // Now we walk the pending events and ask the tree
                                // whether each target participates in `:hover`
                                // styling (its id or one of its classes appears
                                // in a `:hover`-bearing rule). frame_dirty +
                                // request_redraw still fire unconditionally so
                                // pointer handlers and the cursor-style path see
                                // every move; only the cache-invalidation step
                                // is per-target.
                                let mut any_hover_changed = false;
                                let mut hoverable_changed = false;
                                for event in &pending_events {
                                    let is_hover_event = matches!(
                                        event.event_type,
                                        blinc_core::events::event_types::POINTER_ENTER
                                            | blinc_core::events::event_types::POINTER_LEAVE
                                    );
                                    if !is_hover_event {
                                        continue;
                                    }
                                    any_hover_changed = true;
                                    if tree.node_participates_in_hover(event.node_id) {
                                        hoverable_changed = true;
                                        break;
                                    }
                                }
                                if any_hover_changed {
                                    frame_dirty.store(true, Ordering::Release);
                                    // Under `ControlFlow::Wait` (Linux/Wayland/X11)
                                    // flipping `frame_dirty` alone doesn't schedule
                                    // anything — we need to actually ask winit to
                                    // deliver a `RedrawRequested` event. macOS happens
                                    // to render anyway because Poll's auto-redraw was
                                    // there; on Linux this is the only path.
                                    window.request_redraw();
                                    // Cache invalidation is the expensive step —
                                    // gate it on the target actually having
                                    // pointer-state styling. Hovering over an
                                    // unstyled element produces no visual change
                                    // to invalidate for.
                                    //
                                    // Symptom this gate originally fixed: cn_demo
                                    // accordion hover lagged visibly behind the
                                    // cursor enter because the fast path just
                                    // blitted the pre-hover cache. We preserve
                                    // that fix by still invalidating when the
                                    // hovered element actually has hover styling.
                                    if hoverable_changed {
                                        if let Some(ref mut app) = ws.app {
                                            app.invalidate_render_cache_tagged(
                                                "hover_state_changed",
                                            );
                                        }
                                    }
                                }

                                // Get drag delta from router (for DRAG events)
                                let (drag_dx, drag_dy) = router.drag_delta();

                                // Populate bounds for each event from the router's hit test results
                                // This is needed for POINTER_ENTER/POINTER_LEAVE/POINTER_MOVE events
                                for event in pending_events.iter_mut() {
                                    event.mouse_x = lx;
                                    event.mouse_y = ly;
                                    // Populate drag delta for DRAG events
                                    if event.event_type == blinc_core::events::event_types::DRAG
                                        || event.event_type
                                            == blinc_core::events::event_types::DRAG_END
                                    {
                                        event.drag_delta_x = drag_dx;
                                        event.drag_delta_y = drag_dy;
                                    }
                                    // Populate bounds from hit test results (stored in router)
                                    if let Some((bx, by, bw, bh)) =
                                        router.get_node_bounds(event.node_id)
                                    {
                                        event.bounds_x = bx;
                                        event.bounds_y = by;
                                        event.bounds_width = bw;
                                        event.bounds_height = bh;
                                        event.local_x = lx - bx;
                                        event.local_y = ly - by;
                                    }
                                }

                                // Update cursor based on hovered element. Reuse
                                // the ancestor chain just cached by
                                // `on_mouse_move_with_occlusion` instead of
                                // running a fresh hit_test — at a Magic Mouse's
                                // 120 Hz move rate × cn_demo's tree depth, the
                                // second tree walk + HashMap allocation was ~10 %
                                // CPU on its own. Cached against `last_cursor`
                                // so a long stable hover doesn't syscall every
                                // move.
                                let cursor = tree
                                    .get_cursor_for_last_hit(router, lx, ly)
                                    .unwrap_or(CursorStyle::Default);
                                let want = convert_cursor_style(cursor);
                                if ws.last_cursor != Some(want) {
                                    window.set_cursor(want);
                                    ws.last_cursor = Some(want);
                                }
                            }
                            MouseEvent::ButtonPressed { button, x, y } => {
                                let lx = x / scale;
                                let ly = y / scale;
                                let btn = convert_mouse_button(button);
                                windowed_ctx.pointer_query.set_pressure(1.0);

                                // Check for backdrop clicks (dismisses overlays)
                                // This still needs special handling because backdrop clicks should
                                // not propagate to elements behind the overlay
                                let overlay_dismissed =
                                    if windowed_ctx.overlay_manager.has_blocking_overlay()
                                        || windowed_ctx.overlay_manager.has_dismissable_overlay()
                                    {
                                        windowed_ctx.overlay_manager.handle_click_at(lx, ly)
                                    } else {
                                        false
                                    };

                                // If overlay was dismissed by backdrop click, don't process further
                                if !overlay_dismissed {
                                    // Blur any focused text inputs BEFORE processing mouse down
                                    // This mimics HTML behavior where clicking anywhere blurs inputs,
                                    // and clicking on an input then re-focuses it via its own handler
                                    blinc_layout::widgets::blur_all_text_inputs();

                                    // "Grab-to-stop" — record the hit chain so the
                                    // mutable phase below can cancel any scroll
                                    // animation under the cursor before the click
                                    // dispatches. Without this a coasting list keeps
                                    // decelerating past the tap.
                                    scroll_cancel_hit = router
                                        .hit_test(tree, lx, ly)
                                        .map(|h| (h.node, h.ancestors.clone()));

                                    // Route through main tree (includes overlay content)
                                    let _events = router.on_mouse_down(tree, lx, ly, btn);

                                    let (local_x, local_y) = router.last_hit_local();
                                    let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                    let (bounds_width, bounds_height) = router.last_hit_bounds();
                                    for event in pending_events.iter_mut() {
                                        event.mouse_x = lx;
                                        event.mouse_y = ly;
                                        event.local_x = local_x;
                                        event.local_y = local_y;
                                        event.bounds_x = bounds_x;
                                        event.bounds_y = bounds_y;
                                        event.bounds_width = bounds_width;
                                        event.bounds_height = bounds_height;
                                    }
                                }
                            }
                            MouseEvent::ButtonReleased { button, x, y } => {
                                let lx = x / scale;
                                let ly = y / scale;
                                let btn = convert_mouse_button(button);
                                windowed_ctx.pointer_query.set_pressure(0.0);

                                // Route through main tree (includes overlay content)
                                router.on_mouse_up(tree, lx, ly, btn);
                                // Use the local coordinates from when the press started
                                // (stored by on_mouse_down via last_hit_local)
                                let (local_x, local_y) = router.last_hit_local();
                                let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                let (bounds_width, bounds_height) = router.last_hit_bounds();
                                for event in pending_events.iter_mut() {
                                    event.mouse_x = lx;
                                    event.mouse_y = ly;
                                    event.local_x = local_x;
                                    event.local_y = local_y;
                                    event.bounds_x = bounds_x;
                                    event.bounds_y = bounds_y;
                                    event.bounds_width = bounds_width;
                                    event.bounds_height = bounds_height;
                                }
                            }
                            MouseEvent::Left => {
                                // on_mouse_leave now emits POINTER_UP if there was a pressed target
                                // This handles the case where mouse leaves window while dragging
                                router.on_mouse_leave(tree);
                                // Reset cursor to default when mouse leaves window
                                let want = blinc_platform::Cursor::Default;
                                if ws.last_cursor != Some(want) {
                                    window.set_cursor(want);
                                    ws.last_cursor = Some(want);
                                }
                                // Events are collected via the callback set above
                            }
                            MouseEvent::Entered => {
                                let (mx, my) = router.mouse_position();

                                // Use occlusion-aware hit testing when mouse enters window
                                let overlay_bounds =
                                    windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                                let overlay_layer_id = tree
                                    .query_by_id(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID);
                                router.on_mouse_move_with_occlusion(
                                    tree,
                                    mx,
                                    my,
                                    &overlay_bounds,
                                    overlay_layer_id,
                                );

                                for event in pending_events.iter_mut() {
                                    event.mouse_x = mx;
                                    event.mouse_y = my;
                                }

                                // Update cursor based on hovered element. See the
                                // `MouseEvent::Moved` branch for the cache rationale.
                                let cursor = tree
                                    .get_cursor_at(router, mx, my)
                                    .unwrap_or(CursorStyle::Default);
                                let want = convert_cursor_style(cursor);
                                if ws.last_cursor != Some(want) {
                                    window.set_cursor(want);
                                    ws.last_cursor = Some(want);
                                }
                            }
                        },
                        InputEvent::Keyboard(kb_event) => {
                            let mods = &kb_event.modifiers;
                            // Cache the live modifier state so subsequent
                            // pointer events can propagate it through to
                            // their EventContext.
                            //
                            // The cache uses TWO inputs to stay
                            // accurate across backend quirks:
                            //
                            // 1. Start from kb_event.modifiers (winit's
                            //    snapshot at the moment the event was
                            //    queued). Good when ModifiersChanged
                            //    fires before the key event.
                            // 2. Override explicitly for modifier-key
                            //    transitions: a Pressed Shift sets
                            //    cache.shift=true, a Released Shift
                            //    clears it. Handles backends where
                            //    ModifiersChanged queues AFTER the
                            //    KeyboardInput (so the snapshot reports
                            //    the pre-transition state).
                            let mut next = *mods;
                            let is_press =
                                matches!(kb_event.state, blinc_platform::KeyState::Pressed);
                            let is_release =
                                matches!(kb_event.state, blinc_platform::KeyState::Released);
                            match (&kb_event.key, is_press, is_release) {
                                (blinc_platform::Key::Shift, true, _) => next.shift = true,
                                (blinc_platform::Key::Shift, _, true) => next.shift = false,
                                (blinc_platform::Key::Ctrl, true, _) => next.ctrl = true,
                                (blinc_platform::Key::Ctrl, _, true) => next.ctrl = false,
                                (blinc_platform::Key::Alt, true, _) => next.alt = true,
                                (blinc_platform::Key::Alt, _, true) => next.alt = false,
                                (blinc_platform::Key::Meta, true, _) => next.meta = true,
                                (blinc_platform::Key::Meta, _, true) => next.meta = false,
                                _ => {}
                            }
                            if next != ws.cached_modifiers {
                                tracing::trace!(
                                    target: "blinc_app::modifiers",
                                    shift = next.shift,
                                    ctrl = next.ctrl,
                                    alt = next.alt,
                                    meta = next.meta,
                                    "cached modifiers updated"
                                );
                            }
                            ws.cached_modifiers = next;

                            // Extract character from key if applicable
                            let key_char = match &kb_event.key {
                                Key::Char(c) => Some(*c),
                                Key::Space => Some(' '),
                                Key::A => Some(if mods.shift { 'A' } else { 'a' }),
                                Key::B => Some(if mods.shift { 'B' } else { 'b' }),
                                Key::C => Some(if mods.shift { 'C' } else { 'c' }),
                                Key::D => Some(if mods.shift { 'D' } else { 'd' }),
                                Key::E => Some(if mods.shift { 'E' } else { 'e' }),
                                Key::F => Some(if mods.shift { 'F' } else { 'f' }),
                                Key::G => Some(if mods.shift { 'G' } else { 'g' }),
                                Key::H => Some(if mods.shift { 'H' } else { 'h' }),
                                Key::I => Some(if mods.shift { 'I' } else { 'i' }),
                                Key::J => Some(if mods.shift { 'J' } else { 'j' }),
                                Key::K => Some(if mods.shift { 'K' } else { 'k' }),
                                Key::L => Some(if mods.shift { 'L' } else { 'l' }),
                                Key::M => Some(if mods.shift { 'M' } else { 'm' }),
                                Key::N => Some(if mods.shift { 'N' } else { 'n' }),
                                Key::O => Some(if mods.shift { 'O' } else { 'o' }),
                                Key::P => Some(if mods.shift { 'P' } else { 'p' }),
                                Key::Q => Some(if mods.shift { 'Q' } else { 'q' }),
                                Key::R => Some(if mods.shift { 'R' } else { 'r' }),
                                Key::S => Some(if mods.shift { 'S' } else { 's' }),
                                Key::T => Some(if mods.shift { 'T' } else { 't' }),
                                Key::U => Some(if mods.shift { 'U' } else { 'u' }),
                                Key::V => Some(if mods.shift { 'V' } else { 'v' }),
                                Key::W => Some(if mods.shift { 'W' } else { 'w' }),
                                Key::X => Some(if mods.shift { 'X' } else { 'x' }),
                                Key::Y => Some(if mods.shift { 'Y' } else { 'y' }),
                                Key::Z => Some(if mods.shift { 'Z' } else { 'z' }),
                                Key::Num0 => Some(if mods.shift { ')' } else { '0' }),
                                Key::Num1 => Some(if mods.shift { '!' } else { '1' }),
                                Key::Num2 => Some(if mods.shift { '@' } else { '2' }),
                                Key::Num3 => Some(if mods.shift { '#' } else { '3' }),
                                Key::Num4 => Some(if mods.shift { '$' } else { '4' }),
                                Key::Num5 => Some(if mods.shift { '%' } else { '5' }),
                                Key::Num6 => Some(if mods.shift { '^' } else { '6' }),
                                Key::Num7 => Some(if mods.shift { '&' } else { '7' }),
                                Key::Num8 => Some(if mods.shift { '*' } else { '8' }),
                                Key::Num9 => Some(if mods.shift { '(' } else { '9' }),
                                Key::Minus => Some(if mods.shift { '_' } else { '-' }),
                                Key::Equals => Some(if mods.shift { '+' } else { '=' }),
                                Key::LeftBracket => Some(if mods.shift { '{' } else { '[' }),
                                Key::RightBracket => Some(if mods.shift { '}' } else { ']' }),
                                Key::Backslash => Some(if mods.shift { '|' } else { '\\' }),
                                Key::Semicolon => Some(if mods.shift { ':' } else { ';' }),
                                Key::Quote => Some(if mods.shift { '"' } else { '\'' }),
                                Key::Comma => Some(if mods.shift { '<' } else { ',' }),
                                Key::Period => Some(if mods.shift { '>' } else { '.' }),
                                Key::Slash => Some(if mods.shift { '?' } else { '/' }),
                                Key::Grave => Some(if mods.shift { '~' } else { '`' }),
                                _ => None,
                            };

                            // Key code for special key handling (backspace, arrows, etc)
                            // Letter keys use ASCII uppercase (65=A, 90=Z) for Cmd+key shortcuts
                            let key_code = match &kb_event.key {
                                Key::Backspace => 8,
                                Key::Delete => 127,
                                Key::Enter => 13,
                                Key::Tab => 9,
                                Key::F10 => 121,
                                Key::ContextMenu => 93,
                                Key::Escape => 27,
                                Key::Space => 32,
                                Key::Left => 37,
                                Key::Right => 39,
                                Key::Up => 38,
                                Key::Down => 40,
                                Key::Home => 36,
                                Key::End => 35,
                                Key::PageUp => 33,
                                Key::PageDown => 34,
                                // Map letter keys to ASCII uppercase for Cmd+key shortcuts
                                Key::A => 65,
                                Key::B => 66,
                                Key::C => 67,
                                Key::D => 68,
                                Key::E => 69,
                                Key::F => 70,
                                Key::G => 71,
                                Key::H => 72,
                                Key::I => 73,
                                Key::J => 74,
                                Key::K => 75,
                                Key::L => 76,
                                Key::M => 77,
                                Key::N => 78,
                                Key::O => 79,
                                Key::P => 80,
                                Key::Q => 81,
                                Key::R => 82,
                                Key::S => 83,
                                Key::T => 84,
                                Key::U => 85,
                                Key::V => 86,
                                Key::W => 87,
                                Key::X => 88,
                                Key::Y => 89,
                                Key::Z => 90,
                                // Digit row — match standard JS
                                // KeyboardEvent.keyCode for parity
                                // with web-convention chord tables.
                                Key::Num0 => 48,
                                Key::Num1 => 49,
                                Key::Num2 => 50,
                                Key::Num3 => 51,
                                Key::Num4 => 52,
                                Key::Num5 => 53,
                                Key::Num6 => 54,
                                Key::Num7 => 55,
                                Key::Num8 => 56,
                                Key::Num9 => 57,
                                // Punctuation / symbol keys — JS
                                // keyCode values so chord tables
                                // that bind `,` / `=` / `-` etc
                                // resolve cleanly.
                                Key::Semicolon => 186,
                                Key::Equals => 187,
                                Key::Comma => 188,
                                Key::Minus => 189,
                                Key::Period => 190,
                                Key::Slash => 191,
                                Key::Grave => 192,
                                Key::LeftBracket => 219,
                                Key::Backslash => 220,
                                Key::RightBracket => 221,
                                Key::Quote => 222,
                                Key::Back => {
                                    // System back button — dispatch through back handler
                                    if blinc_layout::back_handler::dispatch_back() {
                                        // See the matching clear above for why.
                                        router.clear_event_callback();
                                        return ControlFlow::Continue;
                                    }
                                    // Not consumed — let default handling proceed
                                    0
                                }
                                _ => 0,
                            };

                            match kb_event.state {
                                KeyState::Pressed => {
                                    // Tab traversal and Enter/Space activation
                                    // take their keys from the widgets.
                                    let navigated = ws.render_tree.as_mut().is_some_and(|t| {
                                        t.handle_navigation_key(router, key_code, mods.shift)
                                    });

                                    // Dispatch KEY_DOWN for all keys
                                    if !navigated {
                                        router.on_key_down(key_code);
                                    }

                                    // For character-producing keys, dispatch TEXT_INPUT
                                    // We use broadcast dispatch so any focused text input can receive it
                                    if let Some(c) = key_char.filter(|_| !navigated) {
                                        // Don't send text input if ctrl/cmd is held (shortcuts)
                                        if !mods.ctrl && !mods.meta {
                                            keyboard_events.push(PendingEvent {
                                                event_type:
                                                    blinc_core::events::event_types::TEXT_INPUT,
                                                key_char: Some(c),
                                                key_code,
                                                shift: mods.shift,
                                                ctrl: mods.ctrl,
                                                alt: mods.alt,
                                                meta: mods.meta,
                                                ..Default::default()
                                            });
                                        }
                                    }

                                    // For KEY_DOWN events with special keys (backspace, arrows)
                                    if key_code != 0 && !navigated {
                                        keyboard_events.push(PendingEvent {
                                            event_type: blinc_core::events::event_types::KEY_DOWN,
                                            key_char: None,
                                            key_code,
                                            shift: mods.shift,
                                            ctrl: mods.ctrl,
                                            alt: mods.alt,
                                            meta: mods.meta,
                                            ..Default::default()
                                        });
                                    }
                                }
                                KeyState::Released => {
                                    router.on_key_up(key_code);

                                    // Also broadcast KEY_UP through the
                                    // `keyboard_events` path. Without this the
                                    // focus-targeted dispatch only fires on the
                                    // focused leaf node, so ancestor handlers
                                    // (e.g. `blinc_input::DivInputExt::capture_input`
                                    // attached to a viewport Div) never see
                                    // releases and their internal
                                    // `keys_down`-tracking sets never clear —
                                    // which in turn makes polling consumers see
                                    // every key as permanently-held after the
                                    // first press. Matches the broadcast path
                                    // KEY_DOWN already uses below.
                                    if key_code != 0 {
                                        keyboard_events.push(PendingEvent {
                                            event_type: blinc_core::events::event_types::KEY_UP,
                                            key_char: None,
                                            key_code,
                                            shift: mods.shift,
                                            ctrl: mods.ctrl,
                                            alt: mods.alt,
                                            meta: mods.meta,
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        }
                        InputEvent::Touch(touch_event) => {
                            // Track active touch IDs for touch count
                            match &touch_event {
                                TouchEvent::Started { .. } => {
                                    ws.active_touch_ids.insert(touch_event.id());
                                    windowed_ctx
                                        .pointer_query
                                        .set_touch_count(ws.active_touch_ids.len() as u32);
                                }
                                TouchEvent::Ended { .. } => {
                                    ws.active_touch_ids.remove(&touch_event.id());
                                    windowed_ctx
                                        .pointer_query
                                        .set_touch_count(ws.active_touch_ids.len() as u32);
                                }
                                TouchEvent::Cancelled { .. } => {
                                    ws.active_touch_ids.remove(&touch_event.id());
                                    windowed_ctx
                                        .pointer_query
                                        .set_touch_count(ws.active_touch_ids.len() as u32);
                                }
                                _ => {}
                            }
                            match touch_event {
                                TouchEvent::Started { x, y, pressure, .. } => {
                                    let lx = x / scale;
                                    let ly = y / scale;
                                    windowed_ctx.pointer_query.set_pressure(pressure);
                                    router.on_mouse_down(tree, lx, ly, MouseButton::Left);
                                    let (local_x, local_y) = router.last_hit_local();
                                    let (bounds_x, bounds_y) = router.last_hit_bounds_pos();
                                    let (bounds_width, bounds_height) = router.last_hit_bounds();
                                    for event in pending_events.iter_mut() {
                                        event.mouse_x = lx;
                                        event.mouse_y = ly;
                                        event.local_x = local_x;
                                        event.local_y = local_y;
                                        event.bounds_x = bounds_x;
                                        event.bounds_y = bounds_y;
                                        event.bounds_width = bounds_width;
                                        event.bounds_height = bounds_height;
                                    }
                                }
                                TouchEvent::Moved { x, y, pressure, .. } => {
                                    let lx = x / scale;
                                    let ly = y / scale;
                                    windowed_ctx.pointer_query.set_pressure(pressure);

                                    // Use occlusion-aware hit testing for touch move as well
                                    let overlay_bounds =
                                        windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                                    let overlay_layer_id = tree.query_by_id(
                                        blinc_layout::widgets::overlay::OVERLAY_LAYER_ID,
                                    );
                                    router.on_mouse_move_with_occlusion(
                                        tree,
                                        lx,
                                        ly,
                                        &overlay_bounds,
                                        overlay_layer_id,
                                    );

                                    for event in pending_events.iter_mut() {
                                        event.mouse_x = lx;
                                        event.mouse_y = ly;
                                    }
                                }
                                TouchEvent::Ended { x, y, .. } => {
                                    let lx = x / scale;
                                    let ly = y / scale;
                                    windowed_ctx.pointer_query.set_pressure(0.0);
                                    router.on_mouse_up(tree, lx, ly, MouseButton::Left);
                                    for event in pending_events.iter_mut() {
                                        event.mouse_x = lx;
                                        event.mouse_y = ly;
                                    }
                                }
                                TouchEvent::Cancelled { .. } => {
                                    // Touch cancelled - treat like mouse leave
                                    // This will emit POINTER_UP if there was a pressed target
                                    windowed_ctx.pointer_query.set_pressure(0.0);
                                    windowed_ctx.pointer_query.set_touch_count(0);
                                    router.on_mouse_leave(tree);
                                }
                            }
                        }
                        InputEvent::Scroll {
                            delta_x,
                            delta_y,
                            phase,
                        } => {
                            let (mx, my) = router.mouse_position();
                            // Scroll deltas are also in physical pixels, convert to logical
                            let ldx = delta_x;
                            let ldy = delta_y;

                            tracing::trace!(
                                "InputEvent::Scroll received: pos=({:.1}, {:.1}) delta=({:.1}, {:.1}) phase={:?}",
                                mx,
                                my,
                                ldx,
                                ldy,
                                phase
                            );

                            // Check if gesture ended (finger lifted from trackpad)
                            // This happens before momentum ends
                            if phase == blinc_platform::ScrollPhase::Ended {
                                gesture_ended = true;
                            }
                            // Which scrolls make one gesture, and which
                            // are momentum, for the handlers.
                            use blinc_layout::ScrollGesturePhase as G;
                            match phase {
                                blinc_platform::ScrollPhase::Started => {
                                    tree.on_scroll_gesture(G::Touch)
                                }
                                blinc_platform::ScrollPhase::Ended => {
                                    tree.on_scroll_gesture(G::Lift)
                                }
                                blinc_platform::ScrollPhase::MomentumEnded => {
                                    tree.on_scroll_gesture(G::MomentumEnd)
                                }
                                blinc_platform::ScrollPhase::Moved => {}
                            }

                            // Use nested scroll support - get hit result for smart dispatch
                            // Store mouse position and delta for dispatch phase
                            // We'll re-do hit test in dispatch phase since we need mutable borrow
                            scroll_info = Some((mx, my, ldx, ldy));
                        }
                        InputEvent::ScrollEnd => {
                            // Scroll momentum ended - full stop
                            scroll_ended = true;
                        }
                        InputEvent::Pinch { scale, .. } => {
                            let (mx, my) = router.mouse_position();
                            pending_events.push(PendingEvent {
                                event_type: blinc_core::events::event_types::PINCH,
                                mouse_x: mx,
                                mouse_y: my,
                                pinch_scale: scale,
                                ..Default::default()
                            });
                        }
                        InputEvent::Rotation { angle, .. } => {
                            let (mx, my) = router.mouse_position();
                            pending_events.push(PendingEvent {
                                event_type: blinc_core::events::event_types::ROTATE,
                                mouse_x: mx,
                                mouse_y: my,
                                pinch_scale: angle,
                                ..Default::default()
                            });
                        }
                        InputEvent::ModifiersChanged(mods) => {
                            // Authoritative modifier-state
                            // update from the platform layer
                            // (winit's ModifiersChanged). Some
                            // backends don't fire KeyboardInput
                            // for the modifier keys themselves
                            // when focus is on a non-text
                            // element (e.g. an open overlay /
                            // popover) — without this update
                            // path the cache would latch at
                            // `shift: true` after a Shift +
                            // marquee gesture even after the
                            // user lifted Shift.
                            if ws.cached_modifiers != mods {
                                tracing::trace!(
                                    target: "blinc_app::modifiers",
                                    shift = mods.shift,
                                    ctrl = mods.ctrl,
                                    alt = mods.alt,
                                    meta = mods.meta,
                                    "cached modifiers updated (ModifiersChanged)"
                                );
                                ws.cached_modifiers = mods;
                            }
                        }
                    }

                    router.clear_event_callback();
                    (
                        pending_events,
                        keyboard_events,
                        scroll_ended,
                        gesture_ended,
                        scroll_info,
                        scroll_cancel_hit,
                    )
                } else {
                    (Vec::new(), Vec::new(), false, false, None, None)
                };

                // Snapshot the cached modifier state before the
                // dispatch loop's mutable borrow on `ws.render_tree`
                // shadows access to `ws.cached_modifiers`. The
                // pointer dispatch path reads from this snapshot to
                // stamp Shift / Cmd / Ctrl / Alt onto the
                // EventContext (the platform layer doesn't carry
                // modifier state on mouse events).
                let cached_modifiers_for_dispatch = ws.cached_modifiers;

                // Second phase: dispatch events with mutable borrow
                // This automatically marks the tree dirty when handlers fire
                if let Some(ref mut tree) = ws.render_tree {
                    // "Grab-to-stop": if mouse-down landed on an
                    // animating scroll container, stop its
                    // momentum/rebound before any other handler
                    // runs. The target was captured in phase 1;
                    // we apply here where the tree is mutable.
                    if let Some((hit, ancestors)) = scroll_cancel_hit {
                        tree.cancel_scroll_animation_in_chain(hit, &ancestors);
                    }

                    // IMPORTANT: Process gesture_ended BEFORE scroll delta dispatch
                    // When gesture ends while overscrolling, we start bounce which
                    // sets state to Bouncing. Then apply_scroll_delta will early-return
                    // and ignore the momentum delta that came with this same event.
                    if gesture_ended {
                        tree.on_gesture_end();
                        // Request redraw to animate bounce-back
                        window.request_redraw();
                    }

                    // Handle scroll with nested scroll support
                    // Skip scroll delta entirely if gesture just ended - the delta
                    // from the same event as gesture_ended is the last finger movement,
                    // not momentum, but we still want to ignore it for instant snap-back
                    //
                    // Also skip scroll when an overlay with an actual backdrop is open to prevent
                    // background content from scrolling while dropdown/modal is visible.
                    // Note: We only check has_blocking_overlay(), not has_dismissable_overlay(),
                    // because overlays with dismiss_on_click_outside (like popovers) should allow
                    // scroll events to pass through to content behind them.
                    let has_overlay_backdrop = ws
                        .ctx
                        .as_ref()
                        .map(|c| c.overlay_manager.has_blocking_overlay())
                        .unwrap_or(false);

                    if let Some((mouse_x, mouse_y, delta_x, delta_y)) = scroll_info {
                        // Skip if gesture ended in this same event - go straight to bounce
                        if gesture_ended {
                            tracing::trace!("Skipping scroll delta - gesture ended, bouncing");
                        } else if delta_x == 0.0 && delta_y == 0.0 {
                            // No-op scroll events fire from the platform layer with
                            // zero deltas as gesture momentum decays past the floor.
                            // canvas-kit's SCROLL branch still calls zoom_at with
                            // factor=1.000, which is a no-op for zoom magnitude but
                            // does write the viewport state through `update`,
                            // counting as a state mutation and (on canvas-backed
                            // hosts) feeding the dot-grid render. With the popover-
                            // open continuous_redraw firing frames every vsync,
                            // even noop scrolls compound visible cost. Drop them
                            // at the dispatch boundary.
                            tracing::trace!("Skipping zero-delta scroll");
                        } else if has_overlay_backdrop {
                            // Skip scroll when overlay is visible to prevent background scrolling
                            tracing::trace!(
                                "Skipping scroll delta - overlay with backdrop is visible"
                            );
                        } else {
                            tracing::trace!(
                                "Scroll dispatch: pos=({:.1}, {:.1}) delta=({:.1}, {:.1})",
                                mouse_x,
                                mouse_y,
                                delta_x,
                                delta_y
                            );

                            // Update overlay positions for overlays with follows_scroll enabled
                            // Use the singleton overlay manager since components use get_overlay_manager()
                            if OverlayContext::is_initialized() {
                                let mgr = get_overlay_manager();
                                if mgr.handle_scroll(delta_y) {
                                    // Apply scroll offsets to render tree for visual movement
                                    for (element_id, offset_y) in mgr.get_scroll_offsets() {
                                        if let Some(node_id) = tree.query_by_id(&element_id) {
                                            tree.set_scroll_offset(node_id, 0.0, offset_y);
                                        }
                                    }
                                    window.request_redraw();
                                }
                            }

                            // Re-do hit test with mutable borrow to get ancestor chain
                            // Then use dispatch_scroll_chain for proper nested scroll handling
                            if let Some(ref mut windowed_ctx) = ws.ctx {
                                let router = &mut windowed_ctx.event_router;
                                if let Some(hit) = router.hit_test(tree, mouse_x, mouse_y) {
                                    tree.dispatch_scroll_chain(
                                        hit.node,
                                        &hit.ancestors,
                                        mouse_x,
                                        mouse_y,
                                        delta_x,
                                        delta_y,
                                    );
                                }
                            }
                        }
                    }

                    // Per-hit cache-invalidation gate. Scan `pending_events`
                    // for any pointer event (POINTER_MOVE / DOWN / UP /
                    // DRAG / DRAG_END / FILE_DRAG_OVER) whose target node
                    // actually has a registered handler. The router's
                    // mouse-move path filters POINTER_MOVE emit to handler-
                    // bearing nodes; mouse-button / drag-fast paths emit
                    // unfiltered, so we re-check via `has_handler` here.
                    // Used below so the cache only invalidates when a
                    // real subscriber was under the cursor — bare cursor
                    // wiggle / click over empty space no longer pays the
                    // full re-render.
                    let pointer_event_to_handler = {
                        use blinc_core::events::event_types;
                        let registry = tree.handler_registry();
                        pending_events.iter().any(|e| {
                            // DRAG / DRAG_END deliberately excluded.
                            // Widgets that need a redraw on drag (cn
                            // sliders / sortables, scroll-bar drag)
                            // either go through Stateful (caught by
                            // `state_changed` above) or call
                            // `stateful::request_redraw()` themselves
                            // when they actually move. Including DRAG
                            // here invalidated the cache every frame
                            // when the user dragged through an empty
                            // area of a scroll container — the scroll
                            // widget registers a permanent on_drag
                            // handler that no-ops unless the scrollbar
                            // itself is being dragged, so the gate saw
                            // a handler and triggered the full slow
                            // path for nothing.
                            let is_pointer_event = matches!(
                                e.event_type,
                                event_types::POINTER_MOVE
                                    | event_types::POINTER_DOWN
                                    | event_types::POINTER_UP
                                    | event_types::POINTER_ENTER
                                    | event_types::POINTER_LEAVE
                                    | event_types::DOUBLE_TAP
                                    | event_types::FILE_DRAG_OVER
                            );
                            if !is_pointer_event {
                                return false;
                            }
                            tree.stable_id(e.node_id)
                                .map(|sid| registry.has_handler(sid, e.event_type))
                                .unwrap_or(false)
                        })
                    };

                    // Dispatch mouse/touch events (scroll is handled above with nested support)
                    if let Some(ref mut windowed_ctx) = ws.ctx {
                        let router = &windowed_ctx.event_router;
                        for mut event in pending_events {
                            // Skip scroll events - already handled with nested scroll support
                            if event.event_type == blinc_core::events::event_types::SCROLL {
                                continue;
                            }
                            // Gesture events (PINCH/ROTATE) need hit testing since
                            // they were collected without a node target
                            if (event.event_type == blinc_core::events::event_types::PINCH
                                || event.event_type == blinc_core::events::event_types::ROTATE)
                                && event.node_id == LayoutNodeId::default()
                            {
                                if let Some(hit) =
                                    router.hit_test(tree, event.mouse_x, event.mouse_y)
                                {
                                    event.node_id = hit.node;
                                    event.local_x = hit.local_x;
                                    event.local_y = hit.local_y;
                                    event.bounds_x = hit.bounds_x;
                                    event.bounds_y = hit.bounds_y;
                                    event.bounds_width = hit.bounds_width;
                                    event.bounds_height = hit.bounds_height;
                                } else {
                                    continue; // No element under cursor
                                }
                            }
                            // Look up the correct bounds for this specific node.
                            // When events bubble from a child to a parent handler,
                            // we need the parent's bounds, not the original hit target's bounds.
                            let (bounds_x, bounds_y, bounds_width, bounds_height) =
                                router.get_node_bounds(event.node_id).unwrap_or((
                                    event.bounds_x,
                                    event.bounds_y,
                                    event.bounds_width,
                                    event.bounds_height,
                                ));
                            let local_x = event.mouse_x - bounds_x;
                            let local_y = event.mouse_y - bounds_y;
                            // Pointer events arrive without modifier
                            // state attached — the platform layer
                            // doesn't propagate it on mouse events.
                            // Fall back to the per-event payload (set
                            // by the keyboard arm above for actual
                            // keyboard PendingEvents); if zero,
                            // substitute the cached modifier snapshot
                            // updated on every KeyboardInput.
                            let mods_snapshot = cached_modifiers_for_dispatch;
                            let shift = event.shift || mods_snapshot.shift;
                            let ctrl = event.ctrl || mods_snapshot.ctrl;
                            let alt = event.alt || mods_snapshot.alt;
                            let meta = event.meta || mods_snapshot.meta;
                            tree.dispatch_event_full(
                                event.node_id,
                                event.event_type,
                                event.mouse_x,
                                event.mouse_y,
                                local_x,
                                local_y,
                                bounds_x,
                                bounds_y,
                                bounds_width,
                                bounds_height,
                                event.drag_delta_x,
                                event.drag_delta_y,
                                event.pinch_scale,
                                shift,
                                ctrl,
                                alt,
                                meta,
                            );
                        }
                    }

                    // Note: Overlay events are now dispatched through the main tree
                    // since overlays are composed into the main tree via build_overlay_layer()

                    // Dispatch keyboard events
                    // Use broadcast instead of bubbling to handle focus correctly after tree rebuilds.
                    // Text inputs track their own focus state internally via `s.visual.is_focused()`,
                    // so broadcasting to all handlers is safe - only the focused one will process.
                    for event in keyboard_events {
                        if event.event_type == blinc_core::events::event_types::TEXT_INPUT {
                            if let Some(c) = event.key_char {
                                // Broadcast to all text input handlers
                                // Each handler checks its own focus state internally
                                tree.broadcast_text_input_event(
                                    c,
                                    event.shift,
                                    event.ctrl,
                                    event.alt,
                                    event.meta,
                                );
                            }
                        } else {
                            // Broadcast KEY_DOWN to all key handlers
                            tree.broadcast_key_event(
                                event.event_type,
                                event.key_code,
                                event.shift,
                                event.ctrl,
                                event.alt,
                                event.meta,
                            );
                        }
                    }

                    // Fire the rebound on `TouchPhase::Ended`. macOS
                    // trackpads deliver Ended twice per gesture (once
                    // at finger-lift, once at OS-momentum end); the
                    // physics' `on_scroll_end` is idempotent — the
                    // second call is a no-op because `is_overscrolling`
                    // is false after the first spring has already
                    // clamped the content to the edge.
                    if scroll_ended {
                        tree.on_scroll_end();
                        window.request_redraw();
                    }

                    // After every input dispatch, check whether any
                    // handler set `NEEDS_REDRAW` (via
                    // `stateful::request_redraw()` from a `dispatch`
                    // / state-change path) or queued a subtree
                    // rebuild. On Linux's `ControlFlow::Wait` the
                    // event loop doesn't deliver `Event::Frame` on
                    // its own; we must explicitly request a
                    // redraw so the queued work actually runs.
                    // Sliders, sortable lists, splitter panes — any
                    // Stateful-driven drag — flow through here.
                    // A bound value a handler wrote (a signal a prop
                    // follows) is a change too, from a scroll or drag
                    // as much as a click.
                    let state_changed = blinc_layout::peek_needs_redraw()
                        || blinc_layout::has_pending_subtree_rebuilds()
                        || blinc_layout::has_pending_partial_prop_updates();
                    if state_changed {
                        frame_dirty.store(true, Ordering::Release);
                        window.request_redraw();
                        // Dispatch produced a real state change —
                        // drop the compositor cache so the next
                        // paint repopulates it with the new
                        // hover / focus / scroll / state.
                        if let Some(ref mut app) = ws.app {
                            app.invalidate_render_cache_tagged("state_changed");
                        }
                    }
                    // Scroll events update `tree.scroll_offsets`
                    // through `dispatch_scroll`, which doesn't
                    // route through `stateful::request_redraw` —
                    // force invalidation any frame a scroll
                    // input landed.
                    let had_scroll = scroll_info.is_some() || scroll_ended;
                    if had_scroll {
                        if let Some(ref mut app) = ws.app {
                            app.invalidate_render_cache_tagged("had_scroll");
                        }
                    }
                    // Interactive elements that don't route through
                    // `stateful::request_redraw` — canvas closures
                    // that read mouse position to draw hover state
                    // (`blinc_canvas_kit::kit.element(...)`), the
                    // `pointer_query` calc(env(...)) cursor
                    // subscribers, custom `on_pointer_*` handlers
                    // that update private state — all need a frame
                    // to paint their response to the event, but
                    // pre-fix the `state_changed` gate above
                    // missed them. The visible symptom was the
                    // user's "I have to scroll for the next frame
                    // to render" report: scroll was the only path
                    // that unconditionally invalidated.
                    //
                    // For mouse-button events (down / up) we
                    // unconditionally invalidate + redraw because
                    // a click is always a candidate state change.
                    // For mouse moves we only redraw when
                    // interactive subscribers exist on the tree —
                    // a static UI with no pointer-aware nodes
                    // shouldn't pay redraw cost for every cursor
                    // wiggle. Same gate the windowed redraw chain
                    // already uses: pointer_query elements or
                    // any node with an attached pointer handler.
                    if !state_changed && !had_scroll {
                        // Per-hit invalidation for ALL pointer events
                        // (moves, button events, drags). The previous
                        // gate invalidated unconditionally on button
                        // events and globally on moves — together that
                        // meant every click on empty space and every
                        // cursor wiggle near hoverable elements forced
                        // the slow path.
                        //
                        // `pointer_event_to_handler` (computed above
                        // before pending_events is consumed) is true
                        // only when an emitted pointer event actually
                        // targets a node with a registered handler for
                        // that event type. Stateful elements that
                        // mutate state on POINTER_DOWN still get
                        // caught by the `state_changed` gate above;
                        // this branch covers non-stateful handlers
                        // (custom on_click closures, drag handlers,
                        // etc.).
                        //
                        // Pointer-query elements (calc(env(--mouse...)) /
                        // canvas-kit cursor subscribers) read mouse
                        // position every frame regardless of hit chain
                        // and need the global invalidate; we OR them
                        // in for move events.
                        let has_pointer_query =
                            ws.ctx.as_ref().is_some_and(|c| !c.pointer_query.is_empty());
                        let need_invalidate =
                            pointer_event_to_handler || (is_move_event && has_pointer_query);
                        if need_invalidate {
                            frame_dirty.store(true, Ordering::Release);
                            window.request_redraw();
                            if let Some(ref mut app) = ws.app {
                                app.invalidate_render_cache_tagged(
                                    "pointer_event_with_subscribers",
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue
    }
}
