//! Widget Context - manages widget state, FSM, and rendering
//!
//! The WidgetContext provides:
//! - Reactive state management via signals
//! - FSM-driven interaction states
//! - Event dispatch and handling
//! - Efficient re-rendering through dirty tracking
//!
//! This module wraps `blinc_layout::InteractiveContext` and adds
//! widget-specific functionality like reactive signals.

use std::sync::{Arc, Mutex};

use blinc_core::events::{Event, EventDispatcher};
use blinc_core::reactive::{ReactiveGraph, Signal};
use blinc_layout::interactive::InteractiveContext;
use blinc_layout::LayoutNodeId;
use slotmap::{Key, SlotMap};

use blinc_core::fsm::StateMachine;

// Re-export from blinc_layout for convenience
pub use blinc_layout::interactive::{DirtyTracker, NodeState as WidgetState};

use crate::widget::WidgetId;

/// Per-widget data stored in the context (widget-level data only)
struct WidgetData {
    /// Associated layout node (if any)
    layout_node: Option<LayoutNodeId>,
}

/// The main widget context that manages all widget state
///
/// This wraps `blinc_layout::InteractiveContext` for state and dirty tracking,
/// adding widget-specific features like reactive signals.
///
/// This is the central coordinator for:
/// - Reactive signals and effects
/// - State machines for each widget
/// - Event dispatch
/// - Dirty tracking for efficient re-renders
pub struct WidgetContext {
    /// Interactive context from layout layer (state, FSM, dirty tracking)
    interactive: InteractiveContext,
    /// Reactive graph for signals and effects
    pub reactive: ReactiveGraph,
    /// Event dispatcher
    pub events: EventDispatcher,
    /// Per-widget data (widget ID -> layout node mapping)
    widgets: SlotMap<WidgetId, WidgetData>,
    /// Shared context for callbacks (wrapped in Arc<Mutex> for thread safety)
    shared: Arc<Mutex<SharedContext>>,
    /// Counter for generating unique layout node IDs
    next_layout_id: u32,
}

/// Shared mutable state accessible from callbacks
struct SharedContext {
    /// Widgets to mark dirty
    pending_dirty: Vec<WidgetId>,
}

impl Default for WidgetContext {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetContext {
    /// Create a new widget context
    pub fn new() -> Self {
        Self {
            interactive: InteractiveContext::new(),
            reactive: ReactiveGraph::new(),
            events: EventDispatcher::new(),
            widgets: SlotMap::with_key(),
            shared: Arc::new(Mutex::new(SharedContext {
                pending_dirty: Vec::new(),
            })),
            next_layout_id: 0,
        }
    }

    // =========================================================================
    // Widget Registration
    // =========================================================================

    /// Register a new widget and get its ID
    pub fn register_widget(&mut self) -> WidgetId {
        // Create a layout node ID for this widget
        let layout_id = self.create_layout_node_id();
        self.interactive.register(layout_id, None);

        self.widgets.insert(WidgetData {
            layout_node: Some(layout_id),
        })
    }

    /// Register a widget with a state machine
    pub fn register_widget_with_fsm(&mut self, fsm: StateMachine) -> WidgetId {
        let layout_id = self.create_layout_node_id();
        self.interactive.register_with_fsm(layout_id, fsm);

        self.widgets.insert(WidgetData {
            layout_node: Some(layout_id),
        })
    }

    /// Create a layout node ID (internal helper)
    fn create_layout_node_id(&mut self) -> LayoutNodeId {
        // We need to create a valid LayoutNodeId
        // For now, we use a simple approach with a counter
        // In a real implementation, this would be coordinated with the layout tree
        use slotmap::KeyData;
        // Generate a unique ID using an incrementing counter
        // SlotMap FFI format: lower 32 bits = index, upper 32 bits = version
        let idx = self.next_layout_id;
        self.next_layout_id += 1;
        // Use index in lower bits and version 1 in upper bits
        LayoutNodeId::from(KeyData::from_ffi((1u64 << 32) | (idx as u64)))
    }

    /// Get the layout node ID for a widget
    pub(crate) fn get_layout_id(&self, widget_id: WidgetId) -> Option<LayoutNodeId> {
        self.widgets.get(widget_id).and_then(|d| d.layout_node)
    }

    /// Unregister a widget
    pub fn unregister_widget(&mut self, id: WidgetId) {
        if let Some(data) = self.widgets.remove(id) {
            if let Some(layout_id) = data.layout_node {
                self.interactive.unregister(layout_id);
            }
        }
    }

    /// Check if a widget is registered
    pub fn is_registered(&self, id: WidgetId) -> bool {
        self.widgets.contains_key(id)
    }

    // =========================================================================
    // State Machine Integration
    // =========================================================================

    /// Send an event to a widget's FSM
    pub fn send_fsm_event(&mut self, widget_id: WidgetId, event: u32) -> bool {
        if let Some(layout_id) = self.get_layout_id(widget_id) {
            self.interactive.send_event(layout_id, event)
        } else {
            false
        }
    }

    /// Get a widget's current FSM state
    pub fn get_fsm_state(&self, widget_id: WidgetId) -> Option<u32> {
        let layout_id = self.get_layout_id(widget_id)?;
        self.interactive.get_fsm_state(layout_id)
    }

    // =========================================================================
    // Reactive Signals
    // =========================================================================

    /// Create a signal and automatically mark widget dirty when it changes
    pub fn create_signal<T: Clone + Send + 'static>(
        &mut self,
        widget_id: WidgetId,
        initial: T,
    ) -> Signal<T> {
        let signal = self.reactive.create_signal(initial);

        // Create an effect that marks the widget dirty when the signal changes
        let shared = self.shared.clone();
        self.reactive.create_effect(move |graph| {
            // Read the signal to register as dependency
            let _ = graph.get(signal);
            // Mark widget dirty
            if let Ok(mut shared) = shared.lock() {
                shared.pending_dirty.push(widget_id);
            }
        });

        signal
    }

    /// Create a plain signal (without auto-dirty tracking)
    pub fn create_signal_plain<T: Clone + Send + 'static>(&mut self, initial: T) -> Signal<T> {
        self.reactive.create_signal(initial)
    }

    /// Get a signal's value
    pub fn get<T: Clone + 'static>(&self, signal: Signal<T>) -> Option<T> {
        self.reactive.get(signal)
    }

    /// Set a signal's value
    pub fn set<T: Send + 'static>(&mut self, signal: Signal<T>, value: T) {
        self.reactive.set(signal, value);
        self.process_pending();
    }

    /// Update a signal's value with a function
    pub fn update<T: Clone + Send + 'static, F: FnOnce(T) -> T>(
        &mut self,
        signal: Signal<T>,
        f: F,
    ) {
        self.reactive.update(signal, f);
        self.process_pending();
    }

    /// Batch multiple signal updates (effects run once at end)
    pub fn batch<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.reactive.batch_start();
        let result = f(self);
        self.reactive.batch_end();
        self.process_pending();
        result
    }

    // =========================================================================
    // Widget State
    // =========================================================================

    /// Set custom state for a widget
    pub fn set_widget_state<S: WidgetState>(&mut self, widget_id: WidgetId, state: S) {
        if let Some(layout_id) = self.get_layout_id(widget_id) {
            self.interactive.set_state(layout_id, state);
        }
    }

    /// Get custom state for a widget
    pub fn get_widget_state<S: 'static>(&self, widget_id: WidgetId) -> Option<&S> {
        let layout_id = self.get_layout_id(widget_id)?;
        self.interactive.get_state(layout_id)
    }

    /// Get mutable custom state for a widget
    pub fn get_widget_state_mut<S: 'static>(&mut self, widget_id: WidgetId) -> Option<&mut S> {
        let layout_id = self.get_layout_id(widget_id)?;
        self.interactive.get_state_mut(layout_id)
    }

    // =========================================================================
    // Dirty Tracking
    // =========================================================================

    /// Mark a widget as needing re-render
    pub fn mark_dirty(&mut self, widget_id: WidgetId) {
        if let Some(layout_id) = self.get_layout_id(widget_id) {
            self.interactive.mark_dirty(layout_id);
        }
    }

    /// Mark entire tree as needing rebuild
    pub fn mark_full_rebuild(&mut self) {
        self.interactive.mark_layout();
    }

    /// Check if any widgets need re-rendering
    pub fn has_dirty(&self) -> bool {
        self.interactive.has_dirty()
    }

    /// Check if a specific widget needs re-rendering
    pub fn is_dirty(&self, widget_id: WidgetId) -> bool {
        if let Some(layout_id) = self.get_layout_id(widget_id) {
            self.interactive.is_dirty(layout_id)
        } else {
            false
        }
    }

    /// Check if full rebuild is needed
    pub fn needs_full_rebuild(&self) -> bool {
        self.interactive.needs_layout()
    }

    /// Clear all dirty flags (call after rendering)
    pub fn clear_dirty(&mut self) {
        self.interactive.clear_all();
    }

    /// Get the dirty tracker
    pub fn dirty_tracker(&self) -> &DirtyTracker {
        self.interactive.dirty_tracker()
    }

    // =========================================================================
    // Event Handling
    // =========================================================================

    /// Dispatch an event to a widget
    pub fn dispatch_event(&mut self, widget_id: WidgetId, event: &Event) {
        if let Some(layout_id) = self.get_layout_id(widget_id) {
            self.interactive.dispatch_event(layout_id, event);
        }

        // Also dispatch through the event dispatcher
        let mut event_copy = event.clone();
        event_copy.target = widget_id.data().as_ffi() as u64;
        self.events.dispatch(&mut event_copy);
    }

    /// Register an event handler for a widget
    pub fn on_event<F>(&mut self, widget_id: WidgetId, event_type: u32, handler: F)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.events
            .register(widget_id.data().as_ffi() as u64, event_type, handler);
    }

    // =========================================================================
    // Internal
    // =========================================================================

    /// Process pending dirty markers from callbacks
    fn process_pending(&mut self) {
        // Collect pending dirty markers first to avoid borrow conflict
        let pending: Vec<WidgetId> = if let Ok(mut shared) = self.shared.lock() {
            shared.pending_dirty.drain(..).collect()
        } else {
            return;
        };

        for widget_id in pending {
            self.mark_dirty(widget_id);
        }
    }

    /// Get access to the underlying interactive context
    pub fn interactive(&self) -> &InteractiveContext {
        &self.interactive
    }

    /// Get mutable access to the underlying interactive context
    pub fn interactive_mut(&mut self) -> &mut InteractiveContext {
        &mut self.interactive
    }
}

/// Helper trait for creating widget-specific contexts
pub trait WidgetContextExt {
    /// Create a button FSM with standard states
    fn create_button_fsm(&mut self) -> StateMachine;

    /// Create a checkbox FSM
    fn create_checkbox_fsm(&mut self) -> StateMachine;

    /// Create a text field FSM
    fn create_text_field_fsm(&mut self) -> StateMachine;
}

impl WidgetContextExt for WidgetContext {
    fn create_button_fsm(&mut self) -> StateMachine {
        use blinc_core::events::event_types;
        use blinc_core::fsm::StateMachine;

        // Button states
        const IDLE: u32 = 0;
        const HOVERED: u32 = 1;
        const PRESSED: u32 = 2;
        #[allow(dead_code)]
        const DISABLED: u32 = 3;

        StateMachine::builder(IDLE)
            .on(IDLE, event_types::POINTER_ENTER, HOVERED)
            .on(HOVERED, event_types::POINTER_LEAVE, IDLE)
            .on(HOVERED, event_types::POINTER_DOWN, PRESSED)
            .on(PRESSED, event_types::POINTER_UP, HOVERED)
            .on(PRESSED, event_types::POINTER_LEAVE, IDLE)
            .build()
    }

    fn create_checkbox_fsm(&mut self) -> StateMachine {
        use blinc_core::events::event_types;
        use blinc_core::fsm::StateMachine;

        // Checkbox states (combines interaction + checked state)
        const UNCHECKED_IDLE: u32 = 0;
        const UNCHECKED_HOVERED: u32 = 1;
        const UNCHECKED_PRESSED: u32 = 2;
        const CHECKED_IDLE: u32 = 10;
        const CHECKED_HOVERED: u32 = 11;
        const CHECKED_PRESSED: u32 = 12;

        // Custom event for toggling
        const TOGGLE: u32 = 100;

        StateMachine::builder(UNCHECKED_IDLE)
            // Unchecked hover transitions
            .on(
                UNCHECKED_IDLE,
                event_types::POINTER_ENTER,
                UNCHECKED_HOVERED,
            )
            .on(
                UNCHECKED_HOVERED,
                event_types::POINTER_LEAVE,
                UNCHECKED_IDLE,
            )
            .on(
                UNCHECKED_HOVERED,
                event_types::POINTER_DOWN,
                UNCHECKED_PRESSED,
            )
            .on(UNCHECKED_PRESSED, event_types::POINTER_UP, CHECKED_HOVERED) // Toggle on click
            .on(
                UNCHECKED_PRESSED,
                event_types::POINTER_LEAVE,
                UNCHECKED_IDLE,
            )
            // Checked hover transitions
            .on(CHECKED_IDLE, event_types::POINTER_ENTER, CHECKED_HOVERED)
            .on(CHECKED_HOVERED, event_types::POINTER_LEAVE, CHECKED_IDLE)
            .on(CHECKED_HOVERED, event_types::POINTER_DOWN, CHECKED_PRESSED)
            .on(CHECKED_PRESSED, event_types::POINTER_UP, UNCHECKED_HOVERED) // Toggle on click
            .on(CHECKED_PRESSED, event_types::POINTER_LEAVE, CHECKED_IDLE)
            // Manual toggle
            .on(UNCHECKED_IDLE, TOGGLE, CHECKED_IDLE)
            .on(CHECKED_IDLE, TOGGLE, UNCHECKED_IDLE)
            .build()
    }

    fn create_text_field_fsm(&mut self) -> StateMachine {
        use blinc_core::events::event_types;
        use blinc_core::fsm::StateMachine;

        // TextField states
        const IDLE: u32 = 0;
        const HOVERED: u32 = 1;
        const FOCUSED: u32 = 2;
        const FOCUSED_HOVERED: u32 = 3;
        #[allow(dead_code)]
        const DISABLED: u32 = 4;

        StateMachine::builder(IDLE)
            // Idle state
            .on(IDLE, event_types::POINTER_ENTER, HOVERED)
            .on(IDLE, event_types::FOCUS, FOCUSED)
            // Hovered state
            .on(HOVERED, event_types::POINTER_LEAVE, IDLE)
            .on(HOVERED, event_types::POINTER_DOWN, FOCUSED)
            .on(HOVERED, event_types::FOCUS, FOCUSED_HOVERED)
            // Focused state
            .on(FOCUSED, event_types::BLUR, IDLE)
            .on(FOCUSED, event_types::POINTER_ENTER, FOCUSED_HOVERED)
            // Focused + Hovered state
            .on(FOCUSED_HOVERED, event_types::POINTER_LEAVE, FOCUSED)
            .on(FOCUSED_HOVERED, event_types::BLUR, HOVERED)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestState {
        value: i32,
    }

    #[test]
    fn test_widget_state_storage() {
        let mut ctx = WidgetContext::new();
        let id = ctx.register_widget();

        // Store state
        ctx.set_widget_state(id, TestState { value: 42 });

        // Retrieve state
        let state = ctx.get_widget_state::<TestState>(id);
        assert!(state.is_some(), "State should be retrievable");
        assert_eq!(state.unwrap().value, 42);

        // Retrieve mutable state
        let state_mut = ctx.get_widget_state_mut::<TestState>(id);
        assert!(state_mut.is_some(), "Mutable state should be retrievable");
        state_mut.unwrap().value = 100;

        // Verify mutation
        let state = ctx.get_widget_state::<TestState>(id);
        assert_eq!(state.unwrap().value, 100);
    }

    #[test]
    fn test_widget_registration() {
        let mut ctx = WidgetContext::new();
        let id1 = ctx.register_widget();
        let id2 = ctx.register_widget();

        assert!(ctx.is_registered(id1));
        assert!(ctx.is_registered(id2));
        assert_ne!(id1, id2);

        ctx.unregister_widget(id1);
        assert!(!ctx.is_registered(id1));
        assert!(ctx.is_registered(id2));
    }

    #[test]
    fn test_fsm_integration() {
        let mut ctx = WidgetContext::new();
        let fsm = ctx.create_button_fsm();
        let widget_id = ctx.register_widget_with_fsm(fsm);

        // Initial state is IDLE (0)
        assert_eq!(ctx.get_fsm_state(widget_id), Some(0));

        // Send POINTER_ENTER -> HOVERED (1)
        ctx.send_fsm_event(widget_id, blinc_core::events::event_types::POINTER_ENTER);
        assert_eq!(ctx.get_fsm_state(widget_id), Some(1));

        // Widget should be marked dirty
        assert!(ctx.is_dirty(widget_id));
    }

    #[test]
    fn test_dirty_tracking() {
        let mut ctx = WidgetContext::new();
        let id1 = ctx.register_widget();
        let id2 = ctx.register_widget();

        // Verify layout IDs are different
        let layout1 = ctx.get_layout_id(id1);
        let layout2 = ctx.get_layout_id(id2);
        assert_ne!(layout1, layout2, "Layout IDs should be different");

        // Clear initial dirty state from registration
        ctx.clear_dirty();
        assert!(!ctx.has_dirty());

        ctx.mark_dirty(id1);
        assert!(ctx.has_dirty());
        assert!(ctx.is_dirty(id1));
        assert!(!ctx.is_dirty(id2));

        ctx.clear_dirty();
        assert!(!ctx.has_dirty());
    }
}
