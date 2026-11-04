//! Collecting pointer events for the router.

#![allow(unused_variables)]

use super::collect::{Collect, PendingEvent};
use super::*;

/// A mouse event: move, press, release, enter, leave.
pub(super) fn collect_mouse(
    ws: &mut WindowState,
    window: &blinc_platform_desktop::DesktopWindow,
    input_event: InputEvent,
    c: Collect<'_>,
    platform: &DesktopPlatform,
    frame_dirty: &Arc<AtomicBool>,
) -> Option<ControlFlow> {
    let Collect {
        pending_events,
        keyboard_events,
        scroll_ended,
        gesture_ended,
        scroll_info,
        scroll_cancel_hit,
    } = c;
    let (Some(windowed_ctx), Some(tree)) = (&mut ws.ctx, &ws.render_tree) else {
        return None;
    };
    let router = &mut windowed_ctx.event_router;
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
                let has_move_subscriber = tree.handler_registry().has_any_pointer_move_subscriber();
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
                    return Some(ControlFlow::Continue);
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
                let needs_pointer_dispatch = tree.handler_registry().has_any_pointer_handler()
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
                    return Some(ControlFlow::Continue);
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
                    return Some(ControlFlow::Continue);
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
                        return Some(ControlFlow::Continue);
                    }
                }

                // Get overlay bounds and layer ID for occlusion-aware hit testing
                // This prevents background elements from receiving hover events
                // when they are visually occluded by overlay content
                let overlay_bounds = windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                let overlay_layer_id =
                    tree.query_by_id(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID);

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
                for event in pending_events.iter() {
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
                            app.invalidate_render_cache_tagged("hover_state_changed");
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
                        || event.event_type == blinc_core::events::event_types::DRAG_END
                    {
                        event.drag_delta_x = drag_dx;
                        event.drag_delta_y = drag_dy;
                    }
                    // Populate bounds from hit test results (stored in router)
                    if let Some((bx, by, bw, bh)) = router.get_node_bounds(event.node_id) {
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
                let overlay_dismissed = if windowed_ctx.overlay_manager.has_blocking_overlay()
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
                    *scroll_cancel_hit = router
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
                let overlay_bounds = windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                let overlay_layer_id =
                    tree.query_by_id(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID);
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
        _ => {}
    }
    None
}
