//! Event-dispatch surface on `RenderTree`.
//!
//! Thin wrappers around `HandlerRegistry::dispatch` that build the
//! `EventContext` for each event type (mouse, key, text-input,
//! scroll). Two flavours per kind:
//!
//! - **Direct dispatch** to a specific node id (`dispatch_event*`,
//!   `dispatch_text_input_event`, `dispatch_key_event`,
//!   `dispatch_scroll_event`).
//! - **Bubbling** — walk the ancestor chain leaf→root until the
//!   first handler matches (`dispatch_text_input_event_bubbling`,
//!   `dispatch_key_event_bubbling`).
//! - **Broadcast** to every registered handler of a given type
//!   (`broadcast_text_input_event`, `broadcast_key_event`) — used
//!   when the focused node id may be stale after a tree rebuild and
//!   each handler can self-filter via its own focus state.
//!
//! Scroll-chain dispatch (`dispatch_scroll_chain*`,
//! `dispatch_pinch_chain`) lives with the rest of the scroll
//! subsystem rather than here, since it's tightly coupled to
//! `scroll_physics` consumption tracking.

use crate::tree::LayoutNodeId;

use super::RenderTree;

impl RenderTree {
    /// Dispatch an event to a node's handlers
    ///
    /// This automatically marks the tree as dirty after dispatching,
    /// signaling that the UI needs to be rebuilt.
    pub fn dispatch_event(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        // Stale hit-test result: node was removed by a rebuild
        // between hit-test and dispatch. Silently skip — handlers
        // for the new node will pick up the next event.
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        let mut ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_mouse_pos(mouse_x, mouse_y);
        ctx.stable_id = stable_id;

        // Check if this node has handlers for this event type
        if self.handler_registry.has_handler(stable_id, event_type) {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - handlers update values in place
        }
    }

    /// Dispatch an event with local coordinates
    ///
    /// Dispatches an event to a node's handler.
    ///
    /// Note: This does NOT automatically mark the tree as dirty.
    /// Handlers that need a rebuild should use EventContext::request_rebuild().
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_event_with_local(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
        local_x: f32,
        local_y: f32,
        bounds_x: f32,
        bounds_y: f32,
        bounds_width: f32,
        bounds_height: f32,
    ) {
        self.dispatch_event_full(
            node_id,
            event_type,
            mouse_x,
            mouse_y,
            local_x,
            local_y,
            bounds_x,
            bounds_y,
            bounds_width,
            bounds_height,
            0.0,
            0.0,
            1.0,
            false,
            false,
            false,
            false,
        );
    }

    /// Dispatch an event with all context data including drag delta
    ///
    /// This is the full dispatch method that includes drag_delta for DRAG events.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_event_full(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        mouse_x: f32,
        mouse_y: f32,
        local_x: f32,
        local_y: f32,
        bounds_x: f32,
        bounds_y: f32,
        bounds_width: f32,
        bounds_height: f32,
        drag_delta_x: f32,
        drag_delta_y: f32,
        pinch_scale: f32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        // Stale hit-test: bail if the node was removed by a recent
        // rebuild. The skip itself is normal (queued events for
        // nodes that just disappeared); only log the dropped-event
        // signal at debug — info-level on every dispatch dominated
        // the log.
        let Some(stable_id) = self.stable_id(node_id) else {
            tracing::debug!(
                target: "blinc_layout::event_dispatch",
                "dispatch_event_full: stale node {:?} (no stable id, dropping {})",
                node_id,
                event_type,
            );
            return;
        };
        let has_handler = self.handler_registry.has_handler(stable_id, event_type);
        tracing::debug!(
            target: "blinc_layout::event_dispatch",
            "dispatch_event_full: node={:?}, stable={:?}, event={}, has_handler={}",
            node_id,
            stable_id,
            event_type,
            has_handler,
        );

        let mut ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_mouse_pos(mouse_x, mouse_y)
            .with_local_pos(local_x, local_y)
            .with_bounds_pos(bounds_x, bounds_y)
            .with_bounds(bounds_width, bounds_height)
            .with_drag_delta(drag_delta_x, drag_delta_y)
            .with_modifiers(shift, ctrl, alt, meta);
        ctx.stable_id = stable_id;

        if event_type == blinc_core::events::event_types::PINCH {
            ctx = ctx.with_pinch(pinch_scale, mouse_x, mouse_y);
        }

        if has_handler {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - handlers update values in place
            // Rebuild only when explicitly requested via State::set() or structural changes
        }
    }

    /// Dispatch a text input event with character data
    ///
    /// This is used for character input in text fields.
    pub fn dispatch_text_input_event(
        &mut self,
        node_id: LayoutNodeId,
        key_char: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        let mut ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::TEXT_INPUT,
            node_id,
        )
        .with_key_char(key_char)
        .with_modifiers(shift, ctrl, alt, meta);
        ctx.stable_id = stable_id;

        if self
            .handler_registry
            .has_handler(stable_id, blinc_core::events::event_types::TEXT_INPUT)
        {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - text input handler updates values in place
            // and calls State::set() which marks dirty if structural change needed
        }
    }

    /// Dispatch a text input event with bubbling through ancestors
    ///
    /// This is used for character input in text fields. The event bubbles up
    /// through ancestors until a handler is found.
    pub fn dispatch_text_input_event_bubbling(
        &mut self,
        ancestors: &[LayoutNodeId],
        key_char: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        let event_type = blinc_core::events::event_types::TEXT_INPUT;

        // Try each node in reverse order (leaf to root) until we find a handler
        for &node_id in ancestors.iter().rev() {
            let Some(stable_id) = self.stable_id(node_id) else {
                continue;
            };
            if self.handler_registry.has_handler(stable_id, event_type) {
                let mut ctx = crate::event_handler::EventContext::new(event_type, node_id)
                    .with_key_char(key_char)
                    .with_modifiers(shift, ctrl, alt, meta);
                ctx.stable_id = stable_id;
                self.handler_registry.dispatch(&ctx);
                // Don't auto-mark dirty - handler updates state in place
                return; // Stop after first handler found
            }
        }
    }

    /// Dispatch a key event with key code and modifiers
    ///
    /// This is used for KEY_DOWN and KEY_UP events.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_key_event(
        &mut self,
        node_id: LayoutNodeId,
        event_type: blinc_core::events::EventType,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        let mut ctx = crate::event_handler::EventContext::new(event_type, node_id)
            .with_key_code(key_code)
            .with_modifiers(shift, ctrl, alt, meta);
        ctx.stable_id = stable_id;

        if self.handler_registry.has_handler(stable_id, event_type) {
            self.handler_registry.dispatch(&ctx);
            // Don't auto-mark dirty - handler updates state in place
        }
    }

    /// Dispatch a key event with bubbling through ancestors
    ///
    /// This is used for KEY_DOWN and KEY_UP events. The event bubbles up
    /// through ancestors until a handler is found.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_key_event_bubbling(
        &mut self,
        ancestors: &[LayoutNodeId],
        event_type: blinc_core::events::EventType,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        // Try each node in reverse order (leaf to root) until we find a handler
        for &node_id in ancestors.iter().rev() {
            let Some(stable_id) = self.stable_id(node_id) else {
                continue;
            };
            if self.handler_registry.has_handler(stable_id, event_type) {
                let mut ctx = crate::event_handler::EventContext::new(event_type, node_id)
                    .with_key_code(key_code)
                    .with_modifiers(shift, ctrl, alt, meta);
                ctx.stable_id = stable_id;
                self.handler_registry.dispatch(&ctx);
                // Don't auto-mark dirty - handler updates state in place
                return; // Stop after first handler found
            }
        }
    }

    /// Broadcast a text input event to ALL text input handlers
    ///
    /// This is used when the router's focused node ID may be stale after a tree rebuild.
    /// Each text input handler checks its own internal focus state (`s.visual.is_focused()`)
    /// to determine if it should process the event.
    pub fn broadcast_text_input_event(
        &mut self,
        key_char: char,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        let ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::TEXT_INPUT,
            crate::tree::LayoutNodeId::default(), // Will be overwritten per-node
        )
        .with_key_char(key_char)
        .with_modifiers(shift, ctrl, alt, meta);

        // Snapshot the stable→layout map so the broadcast resolver
        // doesn't borrow `self` (it's already borrowed mutably
        // through `self.handler_registry`).
        let resolver_map = self.stable_to_layout.clone();
        self.handler_registry.broadcast(
            blinc_core::events::event_types::TEXT_INPUT,
            &ctx,
            |stable| resolver_map.get(&stable).copied(),
        );
    }

    /// Broadcast a key event to ALL key handlers
    ///
    /// This is used when the router's focused node ID may be stale after a tree rebuild.
    /// Each handler checks its own internal focus state to determine if it should process.
    pub fn broadcast_key_event(
        &mut self,
        event_type: blinc_core::events::EventType,
        key_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        let ctx = crate::event_handler::EventContext::new(
            event_type,
            crate::tree::LayoutNodeId::default(), // Will be overwritten per-node
        )
        .with_key_code(key_code)
        .with_modifiers(shift, ctrl, alt, meta);

        let resolver_map = self.stable_to_layout.clone();
        self.handler_registry.broadcast(event_type, &ctx, |stable| {
            resolver_map.get(&stable).copied()
        });
    }

    /// Dispatch a scroll event with scroll delta
    ///
    /// Updates the scroll offset for this node and dispatches to handlers.
    /// Does NOT mark the tree as dirty since scroll only affects rendering,
    /// not the UI tree structure.
    pub fn dispatch_scroll_event(
        &mut self,
        node_id: LayoutNodeId,
        mouse_x: f32,
        mouse_y: f32,
        scroll_delta_x: f32,
        scroll_delta_y: f32,
    ) {
        let _tree_scope = self.layout_tree.enter_scope();
        let Some(stable_id) = self.stable_id(node_id) else {
            return;
        };
        let mut ctx = crate::event_handler::EventContext::new(
            blinc_core::events::event_types::SCROLL,
            node_id,
        )
        .with_mouse_pos(mouse_x, mouse_y)
        .with_scroll_delta(scroll_delta_x, scroll_delta_y);
        ctx.stable_id = stable_id;

        let has_handler = self
            .handler_registry
            .has_handler(stable_id, blinc_core::events::event_types::SCROLL);

        if has_handler {
            // Dispatch to handlers - the Scroll element's internal handler will update
            // ScrollPhysics with direction-aware bounds checking. We also update
            // scroll_offsets here for rendering, but the internal handler may clamp values.
            self.handler_registry.dispatch(&ctx);
            // Don't mark dirty - scroll doesn't require tree rebuild
        }
    }
}
