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
use std::sync::atomic::{AtomicU8, Ordering};

use blinc_core::events::{EventType, event_types};

use crate::tree::LayoutNodeId;

/// Current mouse button for POINTER_DOWN/UP events (0=left, 1=middle, 2=right).
/// Set by the event router before dispatching, read by EventContext construction.
static CURRENT_MOUSE_BUTTON: AtomicU8 = AtomicU8::new(0);

/// Set the current mouse button before dispatching pointer events.
pub fn set_current_mouse_button(button: u8) {
    CURRENT_MOUSE_BUTTON.store(button, Ordering::Relaxed);
}

/// Get the current mouse button (for EventContext population).
pub fn current_mouse_button() -> u8 {
    CURRENT_MOUSE_BUTTON.load(Ordering::Relaxed)
}

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
    /// The node that received the event (current frame's slotmap key)
    pub node_id: LayoutNodeId,
    /// Stable identity of the receiving node, survives rebuilds.
    /// Set by the renderer when constructing the context from a
    /// hit-test result. Handlers can capture this to dispatch
    /// programmatic events to the same logical element across
    /// rebuilds without an explicit `.id("...")`.
    pub stable_id: crate::tree::StableNodeId,
    /// Mouse position at time of event (if applicable)
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Position relative to element bounds
    pub local_x: f32,
    pub local_y: f32,
    /// Absolute position of element bounds (top-left corner)
    pub bounds_x: f32,
    pub bounds_y: f32,
    /// Computed bounds width (set after layout)
    pub bounds_width: f32,
    /// Computed bounds height (set after layout)
    pub bounds_height: f32,
    /// Scroll delta for SCROLL events (pixels scrolled)
    pub scroll_delta_x: f32,
    pub scroll_delta_y: f32,
    /// Scroll event time in milliseconds (for touch velocity tracking)
    /// If Some, indicates touch scroll that needs velocity tracking for momentum
    pub scroll_time: Option<f64>,
    /// Whether a SCROLL is momentum the platform runs on after the fingers
    /// lifted, rather than the fingers moving.
    pub scroll_momentum: bool,
    /// Which scroll gesture a SCROLL belongs to: a number that changes each
    /// time fingers touch a trackpad. Wheels report no gestures; theirs stays
    /// the same.
    pub scroll_gesture: u64,
    /// Drag delta for DRAG/DRAG_END events (offset from drag start)
    pub drag_delta_x: f32,
    pub drag_delta_y: f32,
    /// Pinch center in absolute coordinates (for PINCH events)
    pub pinch_center_x: f32,
    pub pinch_center_y: f32,
    /// Pinch scale ratio delta per update (1.0 = no change)
    pub pinch_scale: f32,
    /// Rotation angle delta in radians (for ROTATE events)
    pub rotation_delta: f32,
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
    /// Mouse button for POINTER_DOWN/POINTER_UP events (0=left, 1=middle, 2=right)
    pub mouse_button: u8,
}

impl EventContext {
    /// Create a new event context.
    ///
    /// `stable_id` defaults to `StableNodeId::ROOT` for callers that
    /// don't have a resolved stable id (mostly test code). The
    /// renderer's dispatch path always overrides this with the real
    /// resolved id before invoking handlers — see
    /// `RenderTree::dispatch_event_full` (renderer/events.rs).
    pub fn new(event_type: EventType, node_id: LayoutNodeId) -> Self {
        Self {
            event_type,
            node_id,
            stable_id: crate::tree::StableNodeId::ROOT,
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
            scroll_time: None,
            scroll_momentum: false,
            scroll_gesture: 0,
            drag_delta_x: 0.0,
            drag_delta_y: 0.0,
            pinch_center_x: 0.0,
            pinch_center_y: 0.0,
            pinch_scale: 1.0,
            rotation_delta: 0.0,
            key_char: None,
            key_code: 0,
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            mouse_button: current_mouse_button(),
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

    /// Set computed bounds (width and height from layout)
    pub fn with_bounds(mut self, width: f32, height: f32) -> Self {
        self.bounds_width = width;
        self.bounds_height = height;
        self
    }

    /// Set bounds position (absolute position of element top-left corner)
    pub fn with_bounds_pos(mut self, x: f32, y: f32) -> Self {
        self.bounds_x = x;
        self.bounds_y = y;
        self
    }

    /// Set scroll delta (for SCROLL events)
    pub fn with_scroll_delta(mut self, dx: f32, dy: f32) -> Self {
        self.scroll_delta_x = dx;
        self.scroll_delta_y = dy;
        self
    }

    /// Set scroll time for touch velocity tracking (milliseconds)
    pub fn with_scroll_time(mut self, time: f64) -> Self {
        self.scroll_time = Some(time);
        self
    }

    /// Set drag delta (for DRAG/DRAG_END events)
    pub fn with_drag_delta(mut self, dx: f32, dy: f32) -> Self {
        self.drag_delta_x = dx;
        self.drag_delta_y = dy;
        self
    }

    /// Set pinch data (for PINCH events)
    pub fn with_pinch(mut self, scale: f32, center_x: f32, center_y: f32) -> Self {
        self.pinch_scale = scale;
        self.pinch_center_x = center_x;
        self.pinch_center_y = center_y;
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

    /// Register a pinch handler (zoom gesture updates)
    pub fn on_pinch<F>(&mut self, handler: F)
    where
        F: Fn(&EventContext) + 'static,
    {
        self.on(event_types::PINCH, handler);
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

/// Global handler registry for the render tree.
///
/// Handlers are stored keyed by `StableNodeId` so they survive
/// rebuilds: a `Stateful` that re-runs `on_state`, a route swap, or
/// any other rebuild trigger overwrites the existing entry at the
/// same stable id with a fresh closure rather than going through a
/// destructive wipe-and-re-register dance. External systems that
/// capture a stable id can dispatch through the registry across
/// rebuilds without an explicit `.id("...")` workaround.
///
/// Callers translate `LayoutNodeId` → `StableNodeId` at the
/// renderer boundary (via `RenderTree::stable_id(node_id)`) before
/// hitting registry methods. `EventContext.stable_id` carries the
/// resolved id for the dispatch site to read.
#[derive(Default)]
pub struct HandlerRegistry {
    /// Handlers keyed by stable id. The same closure persists across
    /// rebuilds as long as the element's source location +
    /// tree position (and any `.id(...)` widget key) stay the same.
    nodes: HashMap<crate::tree::StableNodeId, EventHandlers>,
    /// Cached `has_any_pointer_move_subscriber()` result. Invalidated
    /// (set to `None`) on every mutation; recomputed on first read.
    /// Mouse-move events fire at hundreds of Hz; without the cache
    /// each event re-iterated every handler entry to answer the same
    /// invariant question.
    cached_has_move_subscriber: std::cell::Cell<Option<bool>>,
}

impl HandlerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register handlers for a stable node id.
    ///
    /// Overwrites any existing entry at the same stable id, so a
    /// rebuild emitting fresh closures replaces the previous entry
    /// in place without the registry ever being empty at that key
    /// during dispatch.
    pub fn register(&mut self, stable: crate::tree::StableNodeId, handlers: EventHandlers) {
        if !handlers.is_empty() {
            self.nodes.insert(stable, handlers);
            self.cached_has_move_subscriber.set(None);
        }
    }

    /// Get handlers for a stable node id.
    pub fn get(&self, stable: crate::tree::StableNodeId) -> Option<&EventHandlers> {
        self.nodes.get(&stable)
    }

    /// Dispatch an event to a node's handlers using the stable id
    /// already populated on the `EventContext`. Callers must set
    /// `ctx.stable_id` before invoking dispatch (the renderer does
    /// this when building the context from a hit-test result).
    pub fn dispatch(&self, ctx: &EventContext) {
        if let Some(handlers) = self.nodes.get(&ctx.stable_id) {
            handlers.dispatch(ctx);
        }
    }

    /// Check if a node has handlers for a specific event type, by
    /// stable id.
    pub fn has_handler(&self, stable: crate::tree::StableNodeId, event_type: EventType) -> bool {
        self.nodes
            .get(&stable)
            .map(|h| h.get(event_type).is_some())
            .unwrap_or(false)
    }

    /// Remove handlers for a stable node id. Called by the renderer
    /// when a node is genuinely removed from the tree (not on
    /// rebuild — rebuilt nodes preserve their stable id).
    pub fn remove(&mut self, stable: crate::tree::StableNodeId) {
        self.nodes.remove(&stable);
        self.cached_has_move_subscriber.set(None);
    }

    /// Clear all handlers
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.cached_has_move_subscriber.set(None);
    }

    /// Drop handler entries whose stable id is no longer in the live
    /// tree. Called by `RenderTree::sweep_stale_handlers` after
    /// every mint walk so subtrees removed mid-frame don't leak
    /// closures.
    pub fn retain<F: Fn(crate::tree::StableNodeId) -> bool>(&mut self, keep: F) {
        self.nodes.retain(|stable, _| keep(*stable));
        self.cached_has_move_subscriber.set(None);
    }

    /// Check whether any node has a pointer-related handler registered.
    ///
    /// Used to short-circuit mouse-move hit testing on UIs that don't
    /// listen for pointer interaction at all (most static views, splash
    /// screens, etc.). Walks every node's handler map but stops at the
    /// first match — bounded O(N) where N is the count of nodes that
    /// have *any* handler.
    pub fn has_any_pointer_handler(&self) -> bool {
        use blinc_core::events::event_types::*;
        const POINTER_EVENTS: &[EventType] = &[
            POINTER_DOWN,
            POINTER_UP,
            POINTER_MOVE,
            POINTER_ENTER,
            POINTER_LEAVE,
            DRAG,
            DRAG_END,
            SCROLL,
            SCROLL_END,
            PINCH,
            ROTATE,
            DOUBLE_TAP,
            FILE_DRAG_OVER,
            FILE_DROP,
        ];
        self.nodes
            .values()
            .any(|h| POINTER_EVENTS.iter().any(|t| h.has_handler(*t)))
    }

    /// `true` when any node listens for handlers that consume
    /// **bare** pointer motion (cursor moves with no button held):
    /// `POINTER_MOVE` and `FILE_DRAG_OVER`. Used by the windowed
    /// runner to decide whether a bare mouse-move should drive a
    /// redraw + cache invalidation.
    ///
    /// Intentionally **excludes** `DRAG` / `DRAG_END` — those only
    /// fire while a button is pressed, so a separate gate
    /// (`pressed_target_stable().is_some()` + `has_any_drag_subscriber()`)
    /// covers the drag-in-flight case without invalidating on every
    /// cursor wiggle over a slider thumb.
    ///
    /// Also excludes `POINTER_DOWN` / `POINTER_UP` / `POINTER_ENTER`
    /// / `POINTER_LEAVE` / click handlers — those fire on discrete
    /// events that dispatch their own redraws, and the hover-state
    /// change from ENTER/LEAVE flows through the stateful redraw
    /// flag (`peek_needs_redraw`) which the runner checks
    /// separately. Including them would make the gate fire for any
    /// UI with click handlers or `:hover` styles (i.e. nearly
    /// every UI), defeating the purpose.
    pub fn has_any_pointer_move_subscriber(&self) -> bool {
        if let Some(cached) = self.cached_has_move_subscriber.get() {
            return cached;
        }
        use blinc_core::events::event_types::*;
        const MOVE_EVENTS: &[EventType] = &[POINTER_MOVE, FILE_DRAG_OVER];
        let result = self
            .nodes
            .values()
            .any(|h| MOVE_EVENTS.iter().any(|t| h.has_handler(*t)));
        self.cached_has_move_subscriber.set(Some(result));
        result
    }

    /// `true` when any node listens for `DRAG` / `DRAG_END`. The
    /// drag callback only meaningfully fires when a press is
    /// already in flight, so callers should AND this with a
    /// "pressed_target_is_some" check before invalidating on every
    /// move.
    pub fn has_any_drag_subscriber(&self) -> bool {
        use blinc_core::events::event_types::*;
        const DRAG_EVENTS: &[EventType] = &[DRAG, DRAG_END];
        self.nodes
            .values()
            .any(|h| DRAG_EVENTS.iter().any(|t| h.has_handler(*t)))
    }

    /// Broadcast an event to ALL nodes that have handlers for the given event type
    ///
    /// This is used for keyboard events (TEXT_INPUT, KEY_DOWN) after a tree rebuild,
    /// when the router's focused node ID may be stale. Each handler can check its own
    /// internal focus state to determine if it should process the event.
    ///
    /// `node_id_for` resolves each stable id back to the current
    /// frame's `LayoutNodeId` so handlers reading `ctx.node_id` get
    /// a live key. The renderer passes a closure backed by its
    /// `layout_id` lookup; entries whose stable id no longer maps
    /// to a live layout node are skipped (an unmapped entry means
    /// the node was removed but the registry hasn't been swept
    /// yet).
    pub fn broadcast<F>(&self, event_type: EventType, base_ctx: &EventContext, node_id_for: F)
    where
        F: Fn(crate::tree::StableNodeId) -> Option<LayoutNodeId>,
    {
        for (stable, handlers) in &self.nodes {
            if handlers.get(event_type).is_some() {
                let Some(node_id) = node_id_for(*stable) else {
                    continue;
                };
                let ctx = EventContext {
                    event_type,
                    node_id,
                    stable_id: *stable,
                    ..base_ctx.clone()
                };
                handlers.dispatch(&ctx);
            }
        }
    }

    /// Get all stable ids that have handlers for a specific event type
    pub fn stable_ids_with_handler(&self, event_type: EventType) -> Vec<crate::tree::StableNodeId> {
        self.nodes
            .iter()
            .filter(|(_, handlers)| handlers.get(event_type).is_some())
            .map(|(stable, _)| *stable)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

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
        let stable_id = crate::tree::StableNodeId::ROOT.derive_child(0, None);
        let call_count = Arc::new(AtomicU32::new(0));

        let mut handlers = EventHandlers::new();
        let count = Arc::clone(&call_count);
        handlers.on_hover_enter(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        });

        registry.register(stable_id, handlers);

        assert!(registry.has_handler(stable_id, event_types::POINTER_ENTER));
        assert!(!registry.has_handler(stable_id, event_types::POINTER_DOWN));

        let mut ctx = EventContext::new(event_types::POINTER_ENTER, node_id);
        ctx.stable_id = stable_id;
        registry.dispatch(&ctx);

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
