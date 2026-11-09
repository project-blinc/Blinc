//! Scroll mechanics on `RenderTree`.
//!
//! Six concerns live here:
//!
//! - **Content sizing**: `update_scroll_content_dimensions` syncs
//!   each `ScrollPhysics` instance with the latest viewport + content
//!   sizes from taffy after every layout pass.
//! - **`ScrollRef` binding**: `scroll_ref`, `register_scroll_ref`,
//!   `process_pending_scroll_refs` connect user-facing `ScrollRef`
//!   handles to layout nodes and apply queued imperative scroll
//!   commands (`scroll_to`, `scroll_to_top`, `scroll_to_element`,
//!   etc.) before each frame.
//! - **Chained dispatch**: `dispatch_scroll_chain`,
//!   `dispatch_scroll_chain_with_time`, `dispatch_pinch_chain` walk
//!   the ancestor chain and let each scroll container consume the
//!   delta along its scrollable axis before passing the remainder
//!   outward.
//! - **Offset reads/writes**: `apply_scroll_delta`,
//!   `apply_scroll_delta_with_bounds`, `set_scroll_offset`,
//!   `get_scroll_offset` (rounded to whole device pixels to avoid jitter),
//!   `is_scroll_container`, `get_scroll_direction`,
//!   `can_consume_scroll`. `scroll_focused_text_input_above_keyboard`
//!   is the mobile-keyboard companion that scrolls a focused input
//!   into view.
//! - **Physics ticking**: `tick_scroll_physics` advances momentum +
//!   bounce springs once per frame. `on_scroll_end` /
//!   `on_gesture_end` settle the FSMs after wheel events end without
//!   an explicit phase, and `cancel_scroll_animation_in_chain`
//!   stops a momentum chain when a fresh user gesture starts.
//!   `has_bouncing_scroll` / `has_overscrolling_scroll` are redraw
//!   gates that short-circuit when nothing's moving.
//! - **Scrollbar overlay**: `render_scrollbar` paints the thumb +
//!   track when the renderer is walking a scroll container.

use std::sync::Arc;

use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect};

use crate::selector::ScrollRef;
use crate::tree::LayoutNodeId;

use super::RenderTree;

impl RenderTree {
    /// Update scroll physics with content dimensions from layout
    pub(crate) fn update_scroll_content_dimensions(&mut self) {
        // Collect node_ids to avoid borrowing issues
        let node_ids: Vec<_> = self.scroll_physics.keys().copied().collect();

        for node_id in node_ids {
            // Get viewport bounds (the scroll container's own size)
            let bounds = self.layout_tree.get_bounds(node_id, (0.0, 0.0));
            let viewport_width = bounds.map(|b| b.width).unwrap_or(0.0);
            let viewport_height = bounds.map(|b| b.height).unwrap_or(0.0);

            // Get content size from Taffy's content_size (enabled via feature)
            // This tells us the total size of all content that may overflow
            let (content_width, content_height) = self
                .layout_tree
                .get_content_size(node_id)
                .unwrap_or((viewport_width, viewport_height));

            // Update physics with dimensions
            if let Some(physics) = self.scroll_physics.get(&node_id)
                && let Ok(mut p) = physics.lock()
            {
                p.viewport_width = viewport_width;
                p.viewport_height = viewport_height;
                p.content_width = content_width;
                p.content_height = content_height;
            }
        }
    }

    /// Get a bound ScrollRef by node ID
    pub fn scroll_ref(&self, node_id: LayoutNodeId) -> Option<&ScrollRef> {
        self.scroll_refs.get(&node_id)
    }

    /// Register a ScrollRef for a scroll container node
    ///
    /// This binds the ScrollRef to the node and adds it to both the node-keyed
    /// HashMap (for quick lookup) and the active_scroll_refs Vec (for persistence
    /// across rebuilds).
    pub(crate) fn register_scroll_ref(&mut self, node_id: LayoutNodeId, scroll_ref: &ScrollRef) {
        scroll_ref.bind_to_node(node_id, Arc::downgrade(&self.element_registry));
        self.scroll_refs.insert(node_id, scroll_ref.clone());
        // Also track in active_scroll_refs for persistence across rebuilds
        // Check if already present by comparing inner pointer
        let inner_ptr = Arc::as_ptr(&scroll_ref.inner());
        if !self
            .active_scroll_refs
            .iter()
            .any(|sr| Arc::as_ptr(&sr.inner()) == inner_ptr)
        {
            self.active_scroll_refs.push(scroll_ref.clone());
        }
    }

    /// Process all pending scroll operations from bound ScrollRefs
    ///
    /// This should be called each frame before rendering to apply any
    /// programmatic scroll commands (scroll_to, scroll_to_bottom, etc.).
    ///
    /// Returns true if any scroll state was modified.
    pub fn process_pending_scroll_refs(&mut self) -> bool {
        use crate::selector::PendingScroll;

        let mut any_modified = false;

        // Collect scroll refs that have pending operations from active_scroll_refs
        // (active_scroll_refs persists across rebuilds, unlike scroll_refs HashMap)
        let pending: Vec<_> = self
            .active_scroll_refs
            .iter()
            .filter_map(|scroll_ref| {
                let node_id = scroll_ref.node_id()?;
                scroll_ref
                    .take_pending_scroll()
                    .map(|pending| (node_id, pending))
            })
            .collect();
        for (node_id, pending_scroll) in pending {
            let Some(physics) = self.scroll_physics.get(&node_id) else {
                continue;
            };

            let mut physics = physics.lock().unwrap();
            any_modified = true;

            match pending_scroll {
                PendingScroll::ToOffset { x, y, smooth: _ } => {
                    // For now, instant scroll (smooth animation TBD)
                    physics.cancel_active_animation();
                    physics.offset_x = -x;
                    physics.offset_y = -y;
                }
                PendingScroll::ByAmount { dx, dy, smooth: _ } => {
                    physics.apply_scroll_delta(dx, dy);
                }
                PendingScroll::ToTop { smooth: _ } => {
                    physics.offset_y = 0.0;
                }
                PendingScroll::ToBottom { smooth: _ } => {
                    physics.offset_y = physics.max_offset_y();
                }
                PendingScroll::ToElement {
                    element_id,
                    options,
                } => {
                    // Look up element bounds and scroll to make it visible
                    if let Some(target_node) = self.element_registry.get(&element_id) {
                        // Get target element's bounds
                        if let Some(target_bounds) = self.get_bounds(target_node) {
                            // Get scroll container's bounds
                            if let Some(container_bounds) = self.get_bounds(node_id) {
                                // Calculate scroll offset to bring element into view
                                // Element's position relative to scroll container
                                let relative_y = target_bounds.y - container_bounds.y;
                                let relative_x = target_bounds.x - container_bounds.x;

                                // Scroll to center the element (or just make it visible)
                                let viewport_height = physics.viewport_height;
                                let viewport_width = physics.viewport_width;

                                // Calculate target offsets
                                // Center vertically
                                let target_center_y =
                                    relative_y + target_bounds.height / 2.0 - viewport_height / 2.0;
                                let target_offset_y = (-target_center_y)
                                    .clamp(physics.max_offset_y(), physics.min_offset_y());

                                // Center horizontally
                                let target_center_x =
                                    relative_x + target_bounds.width / 2.0 - viewport_width / 2.0;
                                let target_offset_x = (-target_center_x)
                                    .clamp(physics.max_offset_x(), physics.min_offset_x());

                                // Use smooth animation if requested
                                if options.behavior == crate::selector::ScrollBehavior::Smooth {
                                    physics.scroll_to_animated(target_offset_x, target_offset_y);
                                } else {
                                    // Instant scroll
                                    physics.offset_y = target_offset_y;
                                    if matches!(
                                        physics.config.direction,
                                        crate::scroll::ScrollDirection::Horizontal
                                            | crate::scroll::ScrollDirection::Both
                                    ) {
                                        physics.offset_x = target_offset_x;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Update ScrollRef with current state
            if let Some(scroll_ref) = self.scroll_refs.get(&node_id) {
                scroll_ref.update_state(
                    (physics.offset_x.abs(), physics.offset_y.abs()),
                    (physics.content_width, physics.content_height),
                    (physics.viewport_width, physics.viewport_height),
                );
            }
        }

        any_modified
    }

    /// A SCROLL event for `node_id`, with where over it the wheel turned, as
    /// pointer events carry it.
    fn scroll_context(
        &self,
        node_id: LayoutNodeId,
        (mouse_x, mouse_y): (f32, f32),
        (dx, dy): (f32, f32),
    ) -> crate::event_handler::EventContext {
        let ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::SCROLL,
            node_id,
        )
        .with_mouse_pos(mouse_x, mouse_y)
        .with_scroll_delta(dx, dy);
        match self.get_absolute_bounds(node_id) {
            Some(b) => ctx
                .with_bounds_pos(b.x, b.y)
                .with_bounds(b.width, b.height)
                .with_local_pos(mouse_x - b.x, mouse_y - b.y),
            None => ctx,
        }
    }

    /// Dispatch scroll event through ancestor chain with consumption tracking
    ///
    /// For nested scrolls, inner scrolls consume delta for their direction,
    /// and outer scrolls only receive the remaining delta.
    ///
    /// - `hit_node`: The innermost node under the cursor
    /// - `ancestors`: The ancestor chain from root to hit_node
    /// - Returns the remaining delta after all consumption
    pub fn dispatch_scroll_chain(
        &mut self,
        hit_node: LayoutNodeId,
        ancestors: &[LayoutNodeId],
        mouse_x: f32,
        mouse_y: f32,
        mut delta_x: f32,
        mut delta_y: f32,
    ) -> (f32, f32) {
        // Routing rule: the scroll goes to whichever scrollable the
        // cursor is *over*. No chaining to ancestors when the inner
        // container reaches its edge — that behaviour (CSS-style scroll
        // chaining) reads as the parent "stealing" the gesture
        // mid-scroll, especially with high-rate wheel events where the
        // handoff happens in a single tick. If the user wants to scroll
        // the parent, they move the cursor off the inner container.
        //
        // Find the first node in the hit chain (leaf → root) that has a
        // scroll handler or physics. That's the sole target.
        let mut chain: Vec<LayoutNodeId> = vec![hit_node];
        for &ancestor in ancestors.iter().rev() {
            if ancestor != hit_node {
                chain.push(ancestor);
            }
        }
        let now_ms = crate::widgets::text_input::elapsed_ms() as f64;

        // While a text input owns focus, route scroll ONLY to real
        // scroll-physics containers (pages, textareas), never to a bare
        // SCROLL handler. A handler-only target is an app-level scroll
        // gesture — canvas zoom/pan is the motivating case — that must
        // NOT fire from stray trackpad inertia or wheel ticks while the
        // user is typing. Physics containers still scroll, and an inner
        // textarea still steals from the outer page because it is the
        // first physics node in the leaf→root chain. This replaces the
        // old blunt "skip all scroll when an input is focused" gate in
        // windowed.rs, which starved normal scroll containers too.
        let text_input_focused = crate::widgets::text_input::has_focused_text_input();

        // A gesture that a handler took stays with it.
        let latched = self.latched_scroll_target();
        let mut target: Option<LayoutNodeId> = latched;
        let unlatched: &[LayoutNodeId] = if latched.is_some() { &[] } else { &chain };
        for &node_id in unlatched {
            let has_handler = self.stable_id(node_id).is_some_and(|sid| {
                self.handler_registry
                    .has_handler(sid, blinc_core::events::event_types::SCROLL)
            });
            // A container that scrolls one axis lets a scroll along the other by, handler and
            // all, so a horizontal scroller inside a vertical one passes vertical wheels on.
            let takes_axis = self
                .get_scroll_direction(node_id)
                .is_none_or(|axis| Self::delta_along(axis, delta_x, delta_y));
            if !takes_axis {
                continue;
            }
            let has_registered_physics = self.scroll_physics.contains_key(&node_id);
            // A handler that takes one axis lets a scroll along the other by.
            let has_handler = has_handler
                && self
                    .handler_axis(node_id)
                    .is_none_or(|axis| Self::delta_along(axis, delta_x, delta_y));
            let eligible = if text_input_focused {
                has_registered_physics
            } else {
                has_handler || has_registered_physics
            };
            if eligible {
                target = Some(node_id);
                break;
            }
        }

        let Some(node_id) = target else {
            return (delta_x, delta_y);
        };
        self.latch_scroll_target(node_id);

        let has_scroll_physics = self.get_scroll_direction(node_id).is_some();
        let direction = self
            .get_scroll_direction(node_id)
            .or_else(|| self.handler_axis(node_id));
        let handles_x = direction.is_none_or(|d| {
            matches!(
                d,
                crate::scroll::ScrollDirection::Horizontal | crate::scroll::ScrollDirection::Both
            )
        });
        let handles_y = direction.is_none_or(|d| {
            matches!(
                d,
                crate::scroll::ScrollDirection::Vertical | crate::scroll::ScrollDirection::Both
            )
        });

        let dispatch_x = if handles_x { delta_x } else { 0.0 };
        let dispatch_y = if handles_y { delta_y } else { 0.0 };

        tracing::trace!(
            "scroll_disp node={:?} dir={:?} handles=({},{}) dispatch=({:.1},{:.1})",
            node_id,
            direction,
            handles_x,
            handles_y,
            dispatch_x,
            dispatch_y
        );

        if dispatch_x.abs() > 0.001 || dispatch_y.abs() > 0.001 {
            if has_scroll_physics {
                if let Some(physics) = self.scroll_physics.get(&node_id) {
                    let mut p = physics.lock().unwrap();
                    p.apply_touch_scroll_delta(dispatch_x, dispatch_y, now_ms);
                    p.on_scroll_activity();
                }
                self.sync_scroll_ref(node_id);
                self.last_scroll_target = Some((node_id, now_ms));
            } else {
                let mut ctx =
                    self.scroll_context(node_id, (mouse_x, mouse_y), (dispatch_x, dispatch_y));
                (ctx.scroll_gesture, ctx.scroll_momentum) = self.scroll_gesture_now();
                ctx.stable_id = self
                    .stable_id(node_id)
                    .unwrap_or(crate::tree::StableNodeId::ROOT);
                self.handler_registry.dispatch(&ctx);
                self.last_scroll_target = Some((node_id, now_ms));
            }
        }

        // If the target didn't handle an axis (direction mismatch), let
        // that axis return as unconsumed so a caller that cares about
        // "nothing handled this wheel event" (e.g. a horizontal-only
        // inner over a vertical parent — classic case) can still do
        // something with it. We *don't* apply to anyone else in the
        // chain; cross-axis passthrough is the only allowed handoff.
        if handles_x {
            delta_x = 0.0;
        }
        if handles_y {
            delta_y = 0.0;
        }

        (delta_x, delta_y)
    }

    /// Dispatch scroll with time for touch velocity tracking (mobile)
    ///
    /// Same as dispatch_scroll_chain but includes time for momentum scrolling.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_scroll_chain_with_time(
        &mut self,
        hit_node: LayoutNodeId,
        ancestors: &[LayoutNodeId],
        mouse_x: f32,
        mouse_y: f32,
        delta_x: f32,
        delta_y: f32,
        scroll_time: f64,
    ) -> (f32, f32) {
        // Build the chain from leaf to root
        let mut chain: Vec<LayoutNodeId> = vec![hit_node];
        for &ancestor in ancestors.iter().rev() {
            if ancestor != hit_node {
                chain.push(ancestor);
            }
        }

        // See `dispatch_scroll_chain` for the routing rationale: the
        // cursor's current scrollable gets the delta, with no chaining.
        // Mirror its focus rule too — while a text input is focused,
        // route only to physics containers, not bare SCROLL handlers.
        let text_input_focused = crate::widgets::text_input::has_focused_text_input();
        // A gesture that a handler took stays with it.
        let latched = self.latched_scroll_target();
        let mut target: Option<LayoutNodeId> = latched;
        let unlatched: &[LayoutNodeId] = if latched.is_some() { &[] } else { &chain };
        for &node_id in unlatched {
            let has_handler = self.stable_id(node_id).is_some_and(|sid| {
                self.handler_registry
                    .has_handler(sid, blinc_core::events::event_types::SCROLL)
            });
            // A container that scrolls one axis lets a scroll along the other by, handler and
            // all, so a horizontal scroller inside a vertical one passes vertical wheels on.
            let takes_axis = self
                .get_scroll_direction(node_id)
                .is_none_or(|axis| Self::delta_along(axis, delta_x, delta_y));
            if !takes_axis {
                continue;
            }
            let has_registered_physics = self.scroll_physics.contains_key(&node_id);
            // A handler that takes one axis lets a scroll along the other by.
            let has_handler = has_handler
                && self
                    .handler_axis(node_id)
                    .is_none_or(|axis| Self::delta_along(axis, delta_x, delta_y));
            let eligible = if text_input_focused {
                has_registered_physics
            } else {
                has_handler || has_registered_physics
            };
            if eligible {
                target = Some(node_id);
                break;
            }
        }

        let Some(node_id) = target else {
            return (delta_x, delta_y);
        };
        self.latch_scroll_target(node_id);

        let has_scroll_physics = self.get_scroll_direction(node_id).is_some();
        let direction = self
            .get_scroll_direction(node_id)
            .or_else(|| self.handler_axis(node_id));
        let handles_x = direction.is_none_or(|d| {
            matches!(
                d,
                crate::scroll::ScrollDirection::Horizontal | crate::scroll::ScrollDirection::Both
            )
        });
        let handles_y = direction.is_none_or(|d| {
            matches!(
                d,
                crate::scroll::ScrollDirection::Vertical | crate::scroll::ScrollDirection::Both
            )
        });

        let dispatch_x = if handles_x { delta_x } else { 0.0 };
        let dispatch_y = if handles_y { delta_y } else { 0.0 };

        let mut remaining_dx = delta_x;
        let mut remaining_dy = delta_y;

        if dispatch_x.abs() > 0.001 || dispatch_y.abs() > 0.001 {
            if has_scroll_physics {
                if let Some(physics) = self.scroll_physics.get(&node_id) {
                    let mut p = physics.lock().unwrap();
                    p.apply_touch_scroll_delta(dispatch_x, dispatch_y, scroll_time);
                    p.on_scroll_activity();
                }
                self.sync_scroll_ref(node_id);
                self.last_scroll_target = Some((node_id, scroll_time));
            } else {
                let mut ctx = self
                    .scroll_context(node_id, (mouse_x, mouse_y), (dispatch_x, dispatch_y))
                    .with_scroll_time(scroll_time);
                (ctx.scroll_gesture, ctx.scroll_momentum) = self.scroll_gesture_now();
                ctx.stable_id = self
                    .stable_id(node_id)
                    .unwrap_or(crate::tree::StableNodeId::ROOT);
                self.handler_registry.dispatch(&ctx);
                self.last_scroll_target = Some((node_id, scroll_time));
            }
        }

        // Mark handled axes consumed; cross-axis falls through unchanged.
        if handles_x {
            remaining_dx = 0.0;
        }
        if handles_y {
            remaining_dy = 0.0;
        }

        (remaining_dx, remaining_dy)
    }

    /// Dispatch a pinch event to the first handler in the hit chain (leaf -> root)
    pub fn dispatch_pinch_chain(
        &mut self,
        hit: &crate::event_router::HitTestResult,
        center_x: f32,
        center_y: f32,
        scale: f32,
    ) {
        let event_type = blinc_core::events::event_types::PINCH;

        let chain = std::iter::once(hit.node).chain(
            hit.ancestors
                .iter()
                .rev()
                .copied()
                .filter(|ancestor| *ancestor != hit.node),
        );

        for node_id in chain {
            let Some(stable_id) = self.stable_id(node_id) else {
                continue;
            };
            if !self.handler_registry.has_handler(stable_id, event_type) {
                continue;
            }

            let (bounds_x, bounds_y, bounds_width, bounds_height, local_x, local_y) =
                if node_id == hit.node {
                    (
                        hit.bounds_x,
                        hit.bounds_y,
                        hit.bounds_width,
                        hit.bounds_height,
                        hit.local_x,
                        hit.local_y,
                    )
                } else if let Some((bx, by, bw, bh)) = hit.ancestor_bounds.get(&node_id.to_raw()) {
                    (*bx, *by, *bw, *bh, center_x - *bx, center_y - *by)
                } else {
                    continue;
                };

            self.dispatch_event_full(
                node_id,
                event_type,
                center_x,
                center_y,
                local_x,
                local_y,
                bounds_x,
                bounds_y,
                bounds_width,
                bounds_height,
                0.0,
                0.0,
                scale,
                false,
                false,
                false,
                false,
            );
            return;
        }
    }

    // =========================================================================
    // Scroll Offset Management
    // =========================================================================

    /// Apply a scroll delta to a node's scroll offset (without bounds checking)
    pub fn apply_scroll_delta(&mut self, node_id: LayoutNodeId, delta_x: f32, delta_y: f32) {
        let (current_x, current_y) = self
            .scroll_offsets
            .get(&node_id)
            .copied()
            .unwrap_or((0.0, 0.0));
        self.scroll_offsets
            .insert(node_id, (current_x + delta_x, current_y + delta_y));
    }

    /// Apply a scroll delta with bounds checking based on viewport and content size
    pub fn apply_scroll_delta_with_bounds(
        &mut self,
        node_id: LayoutNodeId,
        delta_x: f32,
        delta_y: f32,
    ) {
        let (current_x, current_y) = self
            .scroll_offsets
            .get(&node_id)
            .copied()
            .unwrap_or((0.0, 0.0));

        // Get the viewport bounds for this node (parent offset doesn't matter for size)
        let bounds = self.layout_tree.get_bounds(node_id, (0.0, 0.0));
        let viewport_width = bounds.map(|b| b.width).unwrap_or(0.0);
        let viewport_height = bounds.map(|b| b.height).unwrap_or(0.0);

        // Get content size from Taffy's content_size
        let (content_width, content_height) = self
            .layout_tree
            .get_content_size(node_id)
            .unwrap_or((viewport_width, viewport_height));

        // Calculate scroll limits
        let min_offset_x = 0.0;
        let max_offset_x = if content_width > viewport_width {
            -(content_width - viewport_width)
        } else {
            0.0
        };
        let min_offset_y = 0.0;
        let max_offset_y = if content_height > viewport_height {
            -(content_height - viewport_height)
        } else {
            0.0
        };

        // Apply delta with clamping
        let new_x = (current_x + delta_x).clamp(max_offset_x, min_offset_x);
        let new_y = (current_y + delta_y).clamp(max_offset_y, min_offset_y);

        tracing::debug!(
            "Scroll bounds: viewport=({:.0}, {:.0}) content=({:.0}, {:.0}) limits_y=({:.0}, {:.0}) delta_y={:.1} current={:.1} new={:.1}",
            viewport_width,
            viewport_height,
            content_width,
            content_height,
            max_offset_y,
            min_offset_y,
            delta_y,
            current_y,
            new_y
        );

        self.scroll_offsets.insert(node_id, (new_x, new_y));
    }

    /// Set the scroll offset for a node
    pub fn set_scroll_offset(&mut self, node_id: LayoutNodeId, offset_x: f32, offset_y: f32) {
        self.scroll_offsets.insert(node_id, (offset_x, offset_y));
    }

    /// Scroll the currently-focused text input (or text area) into view above
    /// the soft keyboard.
    ///
    /// Called by mobile platform runners (`blinc_app::android`,
    /// `blinc_app::ios`) whenever the soft-keyboard inset changes — usually
    /// in response to `UIKeyboardWillChangeFrameNotification` (iOS) or a
    /// `WindowInsets.Type.ime()` callback (Android).
    ///
    /// Behavior:
    ///
    /// 1. Look up the currently focused text input via the global focus
    ///    tracker in `widgets::text_input` (or `widgets::text_area`). If
    ///    nothing is focused, return without doing anything.
    /// 2. Walk the focused node's ancestors looking for the nearest enclosing
    ///    scroll container. If none is found, return — there's no scroll
    ///    surface to adjust.
    /// 3. Compute how much the input is currently obscured by the keyboard:
    ///    `obstruction = max(0, input.bottom + margin - (viewport.height - keyboard_inset))`
    ///    where `viewport.height` is the full window logical height. The
    ///    margin (default 16 px) keeps a comfortable gap between the input
    ///    and the keyboard top edge.
    /// 4. If `obstruction > 0`, scroll the container up by that amount,
    ///    clamping to the container's content size so we don't over-scroll.
    ///    Scroll offsets in Blinc are negative for "content moved up" so
    ///    we subtract from the current y offset.
    /// 5. If the keyboard hides (`keyboard_inset == 0`), do not auto-scroll
    ///    back — the user can keep their current position. The original
    ///    position would require remembering pre-keyboard scroll state per
    ///    container, which is fragile across rebuilds.
    ///
    /// `viewport_height` is the **logical** window height in the same units
    /// the layout tree uses (UIKit points on iOS, density-independent
    /// pixels on Android). `keyboard_inset` is the keyboard's height in
    /// the same units. Both come from `WindowedContext`.
    ///
    /// Returns `true` if any scroll offset was updated (so the caller knows
    /// to request a redraw); `false` otherwise.
    pub fn scroll_focused_text_input_above_keyboard(
        &mut self,
        viewport_height: f32,
        keyboard_inset: f32,
    ) -> bool {
        if keyboard_inset <= 0.0 {
            // Nothing to scroll above — the keyboard is hidden.
            return false;
        }

        // Find the focused text-editable node.
        //
        // The generic `focused_editable_node_id` is the modern lookup —
        // every text-editable widget (`text_input`, `text_area`,
        // `code_editor`, `rich_text_editor`) writes its layout node id
        // there on focus, so a single lookup covers all of them. The
        // typed `focused_text_input_node_id` / `focused_text_area_node_id`
        // calls are kept as fallbacks in case any widget grows a focus
        // path that bypasses the generic atomic (or for older code that
        // sets the typed trackers but not the generic one).
        let focused_node = crate::widgets::text_input::focused_editable_node_id()
            .or_else(crate::widgets::text_input::focused_text_input_node_id)
            .or_else(crate::widgets::text_input::focused_text_area_node_id);

        let Some(focused_node) = focused_node else {
            return false;
        };

        // Walk ancestors to find the nearest scroll container.
        let scroll_container = self
            .layout_tree
            .ancestors(focused_node)
            .into_iter()
            .find(|&ancestor| self.is_scroll_container(ancestor));

        let Some(scroll_container) = scroll_container else {
            // The focused input isn't inside any scroll container — there's
            // no surface to scroll. Caller falls back to other strategies
            // (e.g. shrinking the safe area or letting the keyboard cover
            // the input).
            return false;
        };

        // Get absolute bounds for the focused input. `get_absolute_bounds`
        // already accounts for ancestor scroll offsets, so the returned
        // y is the input's actual on-screen position right now.
        let Some(input_bounds) = self.get_absolute_bounds(focused_node) else {
            return false;
        };

        // Visible bottom edge of the screen — anything below this is
        // covered by the soft keyboard.
        const MARGIN: f32 = 16.0;
        let visible_bottom = viewport_height - keyboard_inset;
        let input_bottom = input_bounds.y + input_bounds.height;
        let obstruction = (input_bottom + MARGIN) - visible_bottom;

        if obstruction <= 0.0 {
            // Already fully visible above the keyboard.
            return false;
        }

        // Apply the scroll. Blinc scroll offsets are negative for
        // "content moved up", so we subtract `obstruction` from the
        // current Y offset.
        let (current_x, current_y) = self.get_scroll_offset(scroll_container);
        let target_y = current_y - obstruction;

        // Clamp to the container's max scroll. The viewport / content
        // sizes come from the layout tree directly so the calculation
        // matches `dispatch_scroll_chain_with_time`'s clamping logic.
        let scroll_bounds = self.layout_tree.get_bounds(scroll_container, (0.0, 0.0));
        let scroll_viewport_h = scroll_bounds.map(|b| b.height).unwrap_or(viewport_height);
        let (_content_w, content_h) = self
            .layout_tree
            .get_content_size(scroll_container)
            .unwrap_or((0.0, scroll_viewport_h));
        let max_offset_y = if content_h > scroll_viewport_h {
            -(content_h - scroll_viewport_h)
        } else {
            0.0
        };
        let clamped_y = target_y.clamp(max_offset_y, 0.0);

        if (clamped_y - current_y).abs() < 0.5 {
            // Effectively unchanged.
            return false;
        }

        tracing::debug!(
            "scroll_focused_text_input_above_keyboard: container={:?} \
             input_bottom={:.1} visible_bottom={:.1} obstruction={:.1} \
             current_y={:.1} -> {:.1}",
            scroll_container,
            input_bottom,
            visible_bottom,
            obstruction,
            current_y,
            clamped_y,
        );

        // Write through both the legacy `scroll_offsets` map AND the
        // physics state if it exists, so the next frame samples the
        // updated value via `get_scroll_offset` regardless of which
        // path is active.
        self.scroll_offsets
            .insert(scroll_container, (current_x, clamped_y));
        if let Some(physics) = self.scroll_physics.get(&scroll_container)
            && let Ok(mut p) = physics.try_lock()
        {
            p.offset_x = current_x;
            p.offset_y = clamped_y;
            // Snap velocity to zero so the scroll doesn't keep
            // drifting after we set the offset programmatically.
            p.velocity_x = 0.0;
            p.velocity_y = 0.0;
        }

        true
    }

    /// Get the scroll offset for a node
    ///
    /// Reads from scroll physics if available (has direction-aware bounds),
    /// falls back to legacy scroll_offsets.
    ///
    /// Note: Returns rounded values to prevent subpixel jitter during scrolling.
    /// Fractional scroll offsets cause content to shift between pixel boundaries,
    /// resulting in wobbling text and lines.
    /// Check if a node is a scroll container
    pub fn is_scroll_container(&self, node_id: LayoutNodeId) -> bool {
        self.scroll_physics.contains_key(&node_id)
    }

    /// Check if a scroll container opted into viewport-culling descendants.
    pub(crate) fn is_viewport_cull_scroll(&self, node_id: LayoutNodeId) -> bool {
        self.viewport_cull_scrolls.contains(&node_id)
    }

    pub fn get_scroll_offset(&self, node_id: LayoutNodeId) -> (f32, f32) {
        // Check scroll physics first (has direction-aware scroll from element)
        let (x, y) = if let Some(physics) = self.scroll_physics.get(&node_id) {
            if let Ok(p) = physics.try_lock() {
                (p.offset_x, p.offset_y)
            } else {
                self.scroll_offsets
                    .get(&node_id)
                    .copied()
                    .unwrap_or((0.0, 0.0))
            }
        } else {
            // Fallback to legacy scroll_offsets
            self.scroll_offsets
                .get(&node_id)
                .copied()
                .unwrap_or((0.0, 0.0))
        };

        // Round to whole device pixels: no subpixel jitter, and a coasting scroll advances in the
        // finest steps the screen can show. Whole points were three device pixels at 3x, so a
        // slowing glide stepped unevenly, two points a frame then one.
        let scale = self.scale_factor.max(1.0);
        ((x * scale).round() / scale, (y * scale).round() / scale)
    }

    /// Render scrollbar overlay for a scroll container
    pub(crate) fn render_scrollbar(
        &self,
        ctx: &mut dyn DrawContext,
        viewport_width: f32,
        viewport_height: f32,
        info: &crate::scroll::ScrollbarRenderInfo,
    ) {
        let config = &info.config;
        if config.visibility == crate::scroll::ScrollbarVisibility::Never {
            return;
        }
        let scrollbar_width = config.width();
        let edge_padding = config.edge_padding;

        // Apply opacity to colors
        let opacity = info.opacity;
        let thumb_color = Color::rgba(
            config.thumb_color[0],
            config.thumb_color[1],
            config.thumb_color[2],
            config.thumb_color[3] * opacity,
        );
        let track_color = Color::rgba(
            config.track_color[0],
            config.track_color[1],
            config.track_color[2],
            config.track_color[3] * opacity,
        );

        // Calculate corner radius for thumb
        let thumb_radius = CornerRadius::uniform(scrollbar_width * config.corner_radius);

        // Render vertical scrollbar
        if info.show_vertical {
            // Track position (right edge)
            let track_x = viewport_width - scrollbar_width - edge_padding;
            let track_y = edge_padding;
            let track_height = viewport_height - edge_padding * 2.0;

            // Draw track
            let track_rect = Rect::new(track_x, track_y, scrollbar_width, track_height);
            ctx.fill_rect(track_rect, thumb_radius, Brush::Solid(track_color));

            // Draw thumb
            let thumb_rect = Rect::new(
                track_x,
                track_y + info.vertical_thumb_y - edge_padding,
                scrollbar_width,
                info.vertical_thumb_height,
            );
            ctx.fill_rect(thumb_rect, thumb_radius, Brush::Solid(thumb_color));
        }

        // Render horizontal scrollbar
        if info.show_horizontal {
            // Track position (bottom edge)
            let track_x = edge_padding;
            let track_y = viewport_height - scrollbar_width - edge_padding;
            let track_width = viewport_width - edge_padding * 2.0;

            // Adjust for vertical scrollbar if present
            let track_width = if info.show_vertical {
                track_width - scrollbar_width - edge_padding
            } else {
                track_width
            };

            // Draw track
            let track_rect = Rect::new(track_x, track_y, track_width, scrollbar_width);
            ctx.fill_rect(track_rect, thumb_radius, Brush::Solid(track_color));

            // Draw thumb
            let thumb_rect = Rect::new(
                track_x + info.horizontal_thumb_x - edge_padding,
                track_y,
                info.horizontal_thumb_width,
                scrollbar_width,
            );
            ctx.fill_rect(thumb_rect, thumb_radius, Brush::Solid(thumb_color));
        }
    }

    /// Get the scroll direction for a node (if it's a scroll container)
    ///
    /// Returns None if the node is not a scroll container.
    /// The one axis a node's scroll handler takes, if it takes only one.
    fn handler_axis(&self, node_id: LayoutNodeId) -> Option<crate::scroll::ScrollDirection> {
        self.get_render_node(node_id)
            .and_then(|n| n.props.scroll_handler_axis)
    }

    /// Whether a scroll moves mostly along `axis`.
    fn delta_along(axis: crate::scroll::ScrollDirection, dx: f32, dy: f32) -> bool {
        match axis {
            crate::scroll::ScrollDirection::Horizontal => dx.abs() > dy.abs(),
            crate::scroll::ScrollDirection::Vertical => dy.abs() >= dx.abs(),
            crate::scroll::ScrollDirection::Both => true,
        }
    }

    pub fn get_scroll_direction(
        &self,
        node_id: LayoutNodeId,
    ) -> Option<crate::scroll::ScrollDirection> {
        self.scroll_physics
            .get(&node_id)
            .and_then(|physics| physics.try_lock().ok().map(|p| p.config.direction))
    }

    /// Check if a scroll container can scroll in the given delta direction
    ///
    /// Returns true if the scroll container handles that axis.
    /// Used for nested scroll event handling.
    ///
    /// A scroll container consumes scroll for its direction(s) unless:
    /// - It has no scrollable content (content fits within viewport)
    /// - It's at an edge AND scrolling further into that edge AND bounce is disabled
    pub fn can_consume_scroll(
        &self,
        node_id: LayoutNodeId,
        delta_x: f32,
        delta_y: f32,
    ) -> (bool, bool) {
        let Some(physics) = self.scroll_physics.get(&node_id) else {
            return (false, false);
        };

        let Ok(p) = physics.try_lock() else {
            return (false, false);
        };

        let can_x = match p.config.direction {
            crate::scroll::ScrollDirection::Horizontal | crate::scroll::ScrollDirection::Both => {
                // Check if there's any scrollable content
                let scrollable_x = p.content_width - p.viewport_width;
                if scrollable_x <= 0.0 {
                    // No scrollable content - don't consume
                    false
                } else if delta_x.abs() < 0.001 {
                    // No horizontal delta to consume
                    false
                } else if delta_x < 0.0 {
                    // Scrolling left - can consume if not at left edge
                    // With bounce: only consume if we can still scroll OR are bouncing back
                    // Without bounce: only consume if not at edge
                    let at_left_edge = p.offset_x <= p.max_offset_x();
                    !at_left_edge || p.is_overscrolling_x()
                } else {
                    // Scrolling right - can consume if not at right edge
                    let at_right_edge = p.offset_x >= p.min_offset_x();
                    !at_right_edge || p.is_overscrolling_x()
                }
            }
            _ => false,
        };

        let can_y = match p.config.direction {
            crate::scroll::ScrollDirection::Vertical | crate::scroll::ScrollDirection::Both => {
                // Check if there's any scrollable content
                let scrollable_y = p.content_height - p.viewport_height;
                if scrollable_y <= 0.0 {
                    // No scrollable content - don't consume
                    false
                } else if delta_y.abs() < 0.001 {
                    // No vertical delta to consume
                    false
                } else if delta_y < 0.0 {
                    // Scrolling up (content moves down) - can consume if not at bottom edge
                    // With bounce: only consume if we can still scroll OR are bouncing back
                    // Without bounce: only consume if not at edge
                    let at_bottom_edge = p.offset_y <= p.max_offset_y();
                    !at_bottom_edge || p.is_overscrolling_y()
                } else {
                    // Scrolling down (content moves up) - can consume if not at top edge
                    let at_top_edge = p.offset_y >= p.min_offset_y();
                    !at_top_edge || p.is_overscrolling_y()
                }
            }
            _ => false,
        };

        (can_x, can_y)
    }

    // `transfer_scroll_offsets_from` and `transfer_scroll_physics_from`
    // moved to `renderer/transfers.rs`.

    /// Cancel any running scroll animation (momentum deceleration,
    /// bounce spring, rebound) on the first scrollable in the hit
    /// chain. Intended for the pointer-down / touch-down path so a tap
    /// on a coasting list halts it immediately, matching the native
    /// "grab-to-stop" affordance on every major toolkit.
    ///
    /// Walks leaf → root and cancels the first scroll container found
    /// that is actively animating. No-op if nothing under the cursor
    /// is animating.
    pub fn cancel_scroll_animation_in_chain(
        &mut self,
        hit_node: LayoutNodeId,
        ancestors: &[LayoutNodeId],
    ) {
        let mut chain: Vec<LayoutNodeId> = vec![hit_node];
        for &ancestor in ancestors.iter().rev() {
            if ancestor != hit_node {
                chain.push(ancestor);
            }
        }
        for node_id in chain {
            if let Some(physics) = self.scroll_physics.get(&node_id) {
                let mut p = physics.lock().unwrap();
                if p.is_animating() {
                    p.cancel_active_animation();
                    // Clear capture so the halted container doesn't
                    // keep absorbing subsequent scrolls as the "active"
                    // target after the tap cancelled its animation.
                    self.last_scroll_target = None;
                    return;
                }
            }
        }
    }

    /// Notify the most recently scrolled container that scrolling has
    /// ended.
    ///
    /// Fires only on the last scroll target (stored by `dispatch_scroll_chain`
    /// on each wheel/touch event) rather than every registered scroll
    /// physics in the tree. Broadcasting to all physics was the old
    /// behaviour and meant every scroll container in the app got its
    /// rebound spring kicked the instant the user released the mouse,
    /// which made untouched siblings / ancestors visibly spring from
    /// their offset — the "it springs on as soon as I release" bug.
    /// Clears the stored target after firing so subsequent gestures
    /// start fresh.
    pub fn on_scroll_end(&mut self) {
        let Some((node_id, _)) = self.last_scroll_target.take() else {
            return;
        };
        match self.scroll_physics.get(&node_id) {
            Some(physics) => physics.lock().unwrap().on_scroll_end(),
            None => self.dispatch_scroll_end(node_id),
        }
    }

    /// Tell a scroll handler with no physics of its own that its gesture
    /// ended, as `SCROLL_END`.
    fn dispatch_scroll_end(&mut self, node_id: LayoutNodeId) {
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        let mut ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::SCROLL_END,
            node_id,
        );
        ctx.stable_id = stable_id;
        self.handler_registry.dispatch(&ctx);
    }

    /// Notify the most recently scrolled container that the scroll
    /// gesture has ended (finger lifted).
    ///
    /// Same target-scoped behaviour as [`Self::on_scroll_end`] — this
    /// used to iterate over every physics in the tree, which fired
    /// rebound springs on scrolls the user never touched.
    pub fn on_gesture_end(&mut self) {
        let Some((node_id, _)) = self.last_scroll_target.take() else {
            return;
        };
        match self.scroll_physics.get(&node_id) {
            Some(physics) => physics.lock().unwrap().on_gesture_end(),
            None => self.dispatch_scroll_end(node_id),
        }
    }

    /// Returns `true` if any registered scroll physics is currently in
    /// the `Bouncing` state — i.e. an edge bounce-back spring is
    /// actively animating.
    ///
    /// Used by the web runner to absorb the macOS trackpad's ~800ms
    /// of OS-level momentum-scroll wheel events that arrive *after*
    /// a bounce has started: instead of letting them re-trigger
    /// `start_bounce` (which restarts the spring with a new initial
    /// position and produces a wobble), the runner drops the wheel
    /// event entirely while this returns true.
    pub fn has_bouncing_scroll(&self) -> bool {
        self.scroll_physics
            .values()
            .any(|p| p.lock().unwrap().state == crate::stateful::ScrollState::Bouncing)
    }

    /// Returns `true` if any registered scroll physics is in a state
    /// whose per-frame `tick()` advances the offset off any input —
    /// i.e. `Bouncing` (edge spring or programmatic `scroll_to_animated`)
    /// or `Decelerating` (post-flick momentum).
    ///
    /// Used by the compositor to keep the static-cache walker honest
    /// during these states: the cache primitives were emitted at the
    /// pre-tick offset, so a frame whose only animation source is
    /// scroll physics must still re-walk to reflect the new offset.
    /// `Scrolling` (active drag / wheel) is excluded — the input
    /// event that drove the offset change already tags the frame as
    /// `had_scroll` and invalidates the cache through that path.
    pub fn has_animating_scroll_physics(&self) -> bool {
        use crate::stateful::ScrollState;
        self.scroll_physics.values().any(|p| {
            let s = p.lock().unwrap().state;
            matches!(s, ScrollState::Bouncing | ScrollState::Decelerating)
        })
    }

    /// Returns `true` if any registered scroll physics is currently
    /// past its scroll bounds (rubber-band overscroll).
    ///
    /// Used by the web runner to shorten the wheel-end debounce when
    /// the user is staring at a stuck rubber-band stretch — there is
    /// nothing more to scroll past the edge, so the bounce-back can
    /// fire after only a couple of frames of wheel-event silence
    /// instead of the full debounce window the runner uses to absorb
    /// gaps between adjacent in-bounds wheel events.
    pub fn has_overscrolling_scroll(&self) -> bool {
        self.scroll_physics
            .values()
            .any(|p| p.lock().unwrap().is_overscrolling())
    }

    /// Tick all scroll physics and return true if any are animating
    ///
    /// Call this each frame with the current time in milliseconds.
    /// Uses actual time delta for smooth, frame-rate independent animation.
    /// Bring a container's `ScrollRef` up to date with its physics, so what
    /// reads the ref, or watches its offset, sees a scroll as it lands.
    fn sync_scroll_ref(&self, node_id: LayoutNodeId) {
        let (Some(scroll_ref), Some(physics)) = (
            self.scroll_refs.get(&node_id),
            self.scroll_physics.get(&node_id),
        ) else {
            return;
        };
        let (offset, content, viewport) = {
            let p = physics.lock().unwrap();
            (
                (p.offset_x.abs(), p.offset_y.abs()),
                (p.content_width, p.content_height),
                (p.viewport_width, p.viewport_height),
            )
        };
        scroll_ref.update_state(offset, content, viewport);
    }

    pub fn tick_scroll_physics(&mut self, current_time_ms: u64) -> bool {
        // Delta time from a MICROSECOND clock, not the millisecond
        // `current_time_ms` arg: integer-ms `dt` quantises a 16.67ms
        // vsync into alternating 16/17ms steps (~6% jitter), which the
        // momentum-coast integrator turns into visible micro-judder.
        // Input-driven active scrolling is immune (it doesn't integrate
        // `dt`), which is why only deceleration looked rough. `current_
        // time_ms` is still used below for the idle-bounce wall clock.
        let now_us = crate::widgets::text_input::elapsed_micros();
        let dt_secs = if let Some(last_us) = self.last_scroll_tick_us {
            (now_us.saturating_sub(last_us)) as f32 / 1_000_000.0
        } else {
            1.0 / 60.0 // Assume ~60fps for first frame
        };
        self.last_scroll_tick_us = Some(now_us);

        // Clamp dt to prevent huge jumps if app was paused
        let dt_secs = dt_secs.min(0.1);

        // Collect node_ids to iterate (avoid borrow conflicts)
        let node_ids: Vec<_> = self.scroll_physics.keys().copied().collect();

        let mut any_animating = false;
        for node_id in node_ids {
            let Some(physics_arc) = self.scroll_physics.get(&node_id) else {
                continue;
            };

            let mut physics = physics_arc.lock().unwrap();

            // Detect "scroll ended" for inputs without an explicit end phase
            // (mouse wheel, Windows/Linux trackpad drivers). If the user has
            // been idle past the threshold while overscrolled, synthesise an
            // `on_scroll_end` so the spring rebounds.
            physics.check_idle_bounce(current_time_ms as f64);

            // Tick the physics
            if physics.tick(dt_secs) {
                any_animating = true;
            }

            // Tick scrollbar animations (opacity fade in/out)
            if physics.tick_scrollbar(dt_secs) {
                any_animating = true;
            }

            // Sync ScrollRef state with current physics (for scrollbar position updates)
            if let Some(scroll_ref) = self.scroll_refs.get(&node_id) {
                scroll_ref.update_state(
                    (physics.offset_x.abs(), physics.offset_y.abs()),
                    (physics.content_width, physics.content_height),
                    (physics.viewport_width, physics.viewport_height),
                );
            }
        }

        any_animating
    }
}
