//! Event handler storage for layout elements
//!
//! This module provides the infrastructure for storing event handlers on
//! layout elements and dispatching events to them via the EventRouter.
//!
//! # Architecture
//!
//! ```text
//! Element (Div/Stateful)
//!     ↓ .on_click(|e| ...)
//! EventHandlers (stored on element)
//!     ↓ built into RenderTree
//! RenderTree (handlers indexed by LayoutNodeId)
//!     ↓ EventRouter routes event
//! Handler callback invoked
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! let ui = div()
//!     .w(100.0).h(50.0)
//!     .bg(Color::BLUE)
//!     .on_click(|_| {
//!         println!("Clicked!");
//!     })
//!     .on_hover_enter(|_| {
//!         println!("Hovered!");
//!     });
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use blinc_core::events::{event_types, EventType};

use crate::tree::LayoutNodeId;

/// Callback for handling events
///
/// The callback receives an `EventContext` with information about the event.
/// Uses Rc since UI is single-threaded.
pub type EventCallback = Rc<dyn Fn(&EventContext)>;

/// Context passed to event handlers
#[derive(Clone, Debug)]
pub struct EventContext {
    /// The type of event that occurred
    pub event_type: EventType,
    /// The node that received the event
    pub node_id: LayoutNodeId,
    /// Mouse position at time of event (if applicable)
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Position relative to element bounds
    pub local_x: f32,
    pub local_y: f32,
    /// Scroll delta for SCROLL events (pixels scrolled)
    pub scroll_delta_x: f32,
    pub scroll_delta_y: f32,
    /// Drag delta for DRAG/DRAG_END events (offset from drag start)
    pub drag_delta_x: f32,
    pub drag_delta_y: f32,
    /// Character for TEXT_INPUT events
    pub key_char: Option<char>,
    /// Key code for KEY_DOWN/KEY_UP events (platform-specific)
    pub key_code: u32,
    /// Whether shift modifier is held
    pub shift: bool,
    /// Whether ctrl modifier is held
    pub ctrl: bool,
    /// Whether alt modifier is held
    pub alt: bool,
    /// Whether meta modifier is held (Cmd on macOS, Win on Windows)
    pub meta: bool,
}

impl EventContext {
    /// Create a new event context
    pub fn new(event_type: EventType, node_id: LayoutNodeId) -> Self {
        Self {
            event_type,
            node_id,
            mouse_x: 0.0,
            mouse_y: 0.0,
            local_x: 0.0,
            local_y: 0.0,
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
        }
    }

    /// Set mouse position
    pub fn with_mouse_pos(mut self, x: f32, y: f32) -> Self {
        self.mouse_x = x;
        self.mouse_y = y;
        self
    }

    /// Set local position
    pub fn with_local_pos(mut self, x: f32, y: f32) -> Self {
        self.local_x = x;
        self.local_y = y;
        self
    }

    /// Set scroll delta (for SCROLL events)
    pub fn with_scroll_delta(mut self, dx: f32, dy: f32) -> Self {
        self.scroll_delta_x = dx;
        self.scroll_delta_y = dy;
        self
    }

    /// Set drag delta (for DRAG/DRAG_END events)
    pub fn with_drag_delta(mut self, dx: f32, dy: f32) -> Self {
        self.drag_delta_x = dx;
        self.drag_delta_y = dy;
        self
    }

    /// Set key character (for TEXT_INPUT events)
    pub fn with_key_char(mut self, c: char) -> Self {
        self.key_char = Some(c);
        self
    }

    /// Set key code (for KEY_DOWN/KEY_UP events)
    pub fn with_key_code(mut self, code: u32) -> Self {
        self.key_code = code;
        self
    }

    /// Set modifier keys
    pub fn with_modifiers(mut self, shift: bool, ctrl: bool, alt: bool, meta: bool) -> Self {
        self.shift = shift;
        self.ctrl = ctrl;
        self.alt = alt;
        self.meta = meta;
        self
    }
}

/// Storage for event handlers on an element
#[derive(Default, Clone)]
pub struct EventHandlers {
    /// Handlers keyed by event type
    handlers: HashMap<EventType, Vec<EventCallback>>,
}

impl EventHandlers {
    /// Create a new empty event handlers storage
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any handlers registered
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Check if a handler is registered for a specific event type
    pub fn has_handler(&self, event_type: EventType) -> bool {
        self.handlers.contains_key(&event_type)
    }

    /// Register a handler for an event type
    pub fn on<F>(&mut self, event_type: EventType, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.handlers
            .entry(event_type)
            .or_default()
            .push(Rc::new(handler));
    }

    /// Get handlers for an event type
    pub fn get(&self, event_type: EventType) -> Option<&[EventCallback]> {
        self.handlers.get(&event_type).map(|v| v.as_slice())
    }

    /// Get all registered event types
    pub fn event_types(&self) -> impl Iterator<Item = EventType> + '_ {
        self.handlers.keys().copied()
    }

    /// Dispatch an event to all registered handlers for that type
    pub fn dispatch(&self, ctx: &EventContext) {
        if let Some(handlers) = self.handlers.get(&ctx.event_type) {
            for handler in handlers {
                handler(ctx);
            }
        }
    }

    /// Merge another set of handlers into this one
    pub fn merge(&mut self, other: EventHandlers) {
        for (event_type, handlers) in other.handlers {
            self.handlers
                .entry(event_type)
                .or_default()
                .extend(handlers);
        }
    }

    // =========================================================================
    // Convenience registration methods
    // =========================================================================

    /// Register a click handler (POINTER_DOWN followed by POINTER_UP on same element)
    ///
    /// Note: This registers for POINTER_UP, which fires after press+release.
    pub fn on_click<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::POINTER_UP, handler);
    }

    /// Register a mouse down handler
    pub fn on_mouse_down<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::POINTER_DOWN, handler);
    }

    /// Register a mouse up handler
    pub fn on_mouse_up<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::POINTER_UP, handler);
    }

    /// Register a hover enter handler
    pub fn on_hover_enter<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::POINTER_ENTER, handler);
    }

    /// Register a hover leave handler
    pub fn on_hover_leave<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::POINTER_LEAVE, handler);
    }

    /// Register a focus handler
    pub fn on_focus<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::FOCUS, handler);
    }

    /// Register a blur handler
    pub fn on_blur<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::BLUR, handler);
    }

    /// Register a mount handler (element added to tree)
    pub fn on_mount<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::MOUNT, handler);
    }

    /// Register an unmount handler (element removed from tree)
    pub fn on_unmount<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::UNMOUNT, handler);
    }

    /// Register a key down handler
    pub fn on_key_down<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::KEY_DOWN, handler);
    }

    /// Register a key up handler
    pub fn on_key_up<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::KEY_UP, handler);
    }

    /// Register a scroll handler
    pub fn on_scroll<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::SCROLL, handler);
    }

    /// Register a resize handler
    pub fn on_resize<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::RESIZE, handler);
    }

    /// Register a text input handler (for character input)
    pub fn on_text_input<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::TEXT_INPUT, handler);
    }

    /// Register a drag handler (mouse down + move)
    ///
    /// Drag events are emitted when the mouse moves while a button is pressed.
    /// Use `EventContext::drag_delta_x/y` to get the drag offset from the start.
    pub fn on_drag<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::DRAG, handler);
    }

    /// Register a drag end handler (mouse up after dragging)
    ///
    /// Called when the mouse button is released after a drag operation.
    pub fn on_drag_end<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::DRAG_END, handler);
    }
}

/// Global handler registry for the render tree
///
/// This stores handlers indexed by LayoutNodeId so the EventRouter can
/// dispatch events to the correct handlers.
#[derive(Default)]
pub struct HandlerRegistry {
    /// Handlers keyed by node ID
    nodes: HashMap<LayoutNodeId, EventHandlers>,
}

impl HandlerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register handlers for a node
    pub fn register(&mut self, node_id: LayoutNodeId, handlers: EventHandlers) {
        if !handlers.is_empty() {
            self.nodes.insert(node_id, handlers);
        }
    }

    /// Get handlers for a node
    pub fn get(&self, node_id: LayoutNodeId) -> Option<&EventHandlers> {
        self.nodes.get(&node_id)
    }

    /// Dispatch an event to a node's handlers
    pub fn dispatch(&self, ctx: &EventContext) {
        if let Some(handlers) = self.nodes.get(&ctx.node_id) {
            handlers.dispatch(ctx);
        }
    }

    /// Check if a node has handlers for a specific event type
    pub fn has_handler(&self, node_id: LayoutNodeId, event_type: EventType) -> bool {
        self.nodes
            .get(&node_id)
            .map(|h| h.get(event_type).is_some())
            .unwrap_or(false)
    }

    /// Remove handlers for a node
    pub fn remove(&mut self, node_id: LayoutNodeId) {
        self.nodes.remove(&node_id);
    }

    /// Clear all handlers
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Broadcast an event to ALL nodes that have handlers for the given event type
    ///
    /// This is used for keyboard events (TEXT_INPUT, KEY_DOWN) after a tree rebuild,
    /// when the router's focused node ID may be stale. Each handler can check its own
    /// internal focus state to determine if it should process the event.
    pub fn broadcast(&self, event_type: EventType, base_ctx: &EventContext) {
        for (node_id, handlers) in &self.nodes {
            if handlers.get(event_type).is_some() {
                let ctx = EventContext {
                    event_type,
                    node_id: *node_id,
                    ..base_ctx.clone()
                };
                handlers.dispatch(&ctx);
            }
        }
    }

    /// Get all node IDs that have handlers for a specific event type
    pub fn nodes_with_handler(&self, event_type: EventType) -> Vec<LayoutNodeId> {
        self.nodes
            .iter()
            .filter(|(_, handlers)| handlers.get(event_type).is_some())
            .map(|(node_id, _)| *node_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn create_node_id() -> LayoutNodeId {
        let mut sm: SlotMap<LayoutNodeId, ()> = SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn test_event_handlers_registration() {
        let mut handlers = EventHandlers::new();
        let call_count = Arc::new(AtomicU32::new(0));

        let count = Arc::clone(&call_count);
        handlers.on_click(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        });

        assert!(!handlers.is_empty());
        assert!(handlers.get(event_types::POINTER_UP).is_some());
    }

    #[test]
    fn test_event_dispatch() {
        let mut handlers = EventHandlers::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let node_id = create_node_id();

        let count = Arc::clone(&call_count);
        handlers.on_click(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        });

        // Dispatch POINTER_UP (click)
        let ctx = EventContext::new(event_types::POINTER_UP, node_id);
        handlers.dispatch(&ctx);

        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Dispatch again
        handlers.dispatch(&ctx);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_multiple_handlers() {
        let mut handlers = EventHandlers::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let node_id = create_node_id();

        // Register multiple handlers for same event
        let count1 = Arc::clone(&call_count);
        handlers.on_click(move |_| {
            count1.fetch_add(1, Ordering::SeqCst);
        });

        let count2 = Arc::clone(&call_count);
        handlers.on_click(move |_| {
            count2.fetch_add(10, Ordering::SeqCst);
        });

        let ctx = EventContext::new(event_types::POINTER_UP, node_id);
        handlers.dispatch(&ctx);

        // Both handlers should be called
        assert_eq!(call_count.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn test_handler_registry() {
        let mut registry = HandlerRegistry::new();
        let node_id = create_node_id();
        let call_count = Arc::new(AtomicU32::new(0));

        let mut handlers = EventHandlers::new();
        let count = Arc::clone(&call_count);
        handlers.on_hover_enter(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        });

        registry.register(node_id, handlers);

        assert!(registry.has_handler(node_id, event_types::POINTER_ENTER));
        assert!(!registry.has_handler(node_id, event_types::POINTER_DOWN));

        let ctx = EventContext::new(event_types::POINTER_ENTER, node_id);
        registry.dispatch(&ctx);

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
