//! Event-loop handler for input events.

#![allow(unused_variables)]

use self::collect::{Collect, PendingEvent};
use self::keyboard::collect_keyboard;
use self::mouse::collect_mouse;
use self::touch::collect_touch_scroll;
use super::DesktopLoop;
use super::*;

mod collect;
mod keyboard;
mod mouse;
mod touch;

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

                    let mut collect = Collect {
                        pending_events: &mut pending_events,
                        keyboard_events: &mut keyboard_events,
                        scroll_ended: &mut scroll_ended,
                        gesture_ended: &mut gesture_ended,
                        scroll_info: &mut scroll_info,
                        scroll_cancel_hit: &mut scroll_cancel_hit,
                    };
                    let flow = match input_event {
                        InputEvent::Mouse(..) => collect_mouse(
                            ws,
                            window,
                            input_event,
                            collect.reborrow(),
                            platform,
                            frame_dirty,
                        ),
                        InputEvent::Keyboard(..) => collect_keyboard(
                            ws,
                            window,
                            input_event,
                            collect.reborrow(),
                            platform,
                            frame_dirty,
                        ),
                        _ => collect_touch_scroll(
                            ws,
                            window,
                            input_event,
                            collect.reborrow(),
                            platform,
                            frame_dirty,
                        ),
                    };
                    if let Some(flow) = flow {
                        return flow;
                    }

                    if let Some(ctx) = ws.ctx.as_mut() {
                        ctx.event_router.clear_event_callback();
                    }
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
