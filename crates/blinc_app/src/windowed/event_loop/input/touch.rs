//! Collecting touch, scroll and gesture events for the router.

#![allow(unused_variables, clippy::too_many_arguments)]

use super::collect::{Collect, PendingEvent};
use crate::MouseButton;
use crate::windowed::WindowState;
use blinc_layout::prelude::*;
use blinc_platform::{ControlFlow, InputEvent, TouchEvent};
use blinc_platform_desktop::DesktopPlatform;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// A touch, scroll, pinch, rotation or modifier event.
pub(super) fn collect_touch_scroll(
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
                    let overlay_bounds = windowed_ctx.overlay_manager.get_visible_overlay_bounds();
                    let overlay_layer_id =
                        tree.query_by_id(blinc_layout::widgets::overlay::OVERLAY_LAYER_ID);
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
                *gesture_ended = true;
            }
            // Which scrolls make one gesture, and which
            // are momentum, for the handlers.
            use blinc_layout::ScrollGesturePhase as G;
            match phase {
                blinc_platform::ScrollPhase::Started => tree.on_scroll_gesture(G::Touch),
                blinc_platform::ScrollPhase::Ended => tree.on_scroll_gesture(G::Lift),
                blinc_platform::ScrollPhase::MomentumEnded => {
                    tree.on_scroll_gesture(G::MomentumEnd)
                }
                blinc_platform::ScrollPhase::Moved => {}
            }

            // Use nested scroll support - get hit result for smart dispatch
            // Store mouse position and delta for dispatch phase
            // We'll re-do hit test in dispatch phase since we need mutable borrow
            *scroll_info = Some((mx, my, ldx, ldy));
        }
        InputEvent::ScrollEnd => {
            // Scroll momentum ended - full stop
            *scroll_ended = true;
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
        _ => {}
    }
    None
}
