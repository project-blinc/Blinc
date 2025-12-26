//! Interactive state management for layout nodes
//!
//! This module provides:
//! - Node state storage (arbitrary typed state per node)
//! - Dirty tracking for incremental re-renders
//! - FSM integration for interaction states
//!
//! # Architecture
//!
//! The interactive system is integrated at the layout level because:
//! - State is tied to layout nodes, not abstract widgets
//! - Dirty tracking enables incremental re-rendering of the tree
//! - FSM state transitions affect rendering properties directly
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! // Create an interactive render tree
//! let mut tree = InteractiveTree::new();
//!
//! // Set state for a node
//! tree.set_state(node_id, ButtonState { scale: 1.0 });
//!
//! // Mark nodes as dirty
//! tree.mark_dirty(node_id);
//!
//! // Process only dirty nodes
//! for node_id in tree.take_dirty() {
//!     // Re-render this node
//! }
//! ```

use std::any::Any;
use std::collections::{HashMap, HashSet};

use blinc_core::events::Event;
use blinc_core::fsm::{EventId, StateMachine};

use crate::tree::LayoutNodeId;

/// Trait for node state types
///
/// Any type that can be stored as node state must implement this trait.
/// The `as_any` methods enable type-safe downcasting.
pub trait NodeState: Send + 'static {
    /// Get self as Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Get self as mutable Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Blanket implementation for all types
impl<T: Send + 'static> NodeState for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Data stored for each interactive node
struct NodeData {
    /// Optional FSM for interaction states
    fsm: Option<StateMachine>,
    /// Custom state (type-erased)
    state: Option<Box<dyn NodeState>>,
}

impl Default for NodeData {
    fn default() -> Self {
        Self {
            fsm: None,
            state: None,
        }
    }
}

/// Dirty tracking for incremental re-renders
#[derive(Default)]
pub struct DirtyTracker {
    /// Set of dirty node IDs
    dirty: HashSet<LayoutNodeId>,
    /// Whether the entire tree needs re-layout
    needs_layout: bool,
}

impl DirtyTracker {
    /// Create a new dirty tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a node as dirty (needs re-render)
    pub fn mark(&mut self, id: LayoutNodeId) {
        self.dirty.insert(id);
    }

    /// Mark the tree as needing full re-layout
    pub fn mark_layout(&mut self) {
        self.needs_layout = true;
    }

    /// Check if a node is dirty
    pub fn is_dirty(&self, id: LayoutNodeId) -> bool {
        self.dirty.contains(&id)
    }

    /// Check if any nodes are dirty
    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Check if layout is needed
    pub fn needs_layout(&self) -> bool {
        self.needs_layout
    }

    /// Take all dirty node IDs (clears the set)
    pub fn take_dirty(&mut self) -> Vec<LayoutNodeId> {
        self.dirty.drain().collect()
    }

    /// Clear the layout flag
    pub fn clear_layout(&mut self) {
        self.needs_layout = false;
    }

    /// Clear all dirty flags
    pub fn clear_all(&mut self) {
        self.dirty.clear();
        self.needs_layout = false;
    }
}

/// Interactive state manager for layout nodes
///
/// Manages FSMs, custom state, and dirty tracking for layout nodes.
/// This is the core infrastructure for interactive widgets.
pub struct InteractiveContext {
    /// Node data storage (keyed by LayoutNodeId's raw index for HashMap)
    nodes: HashMap<u64, NodeData>,
    /// Dirty tracker
    dirty: DirtyTracker,
}

impl Default for InteractiveContext {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractiveContext {
    /// Create a new interactive context
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            dirty: DirtyTracker::new(),
        }
    }

    /// Get the raw key for a LayoutNodeId
    fn key(id: LayoutNodeId) -> u64 {
        // SlotMap keys can be converted to u64 via their index
        use slotmap::Key;
        id.data().as_ffi()
    }

    /// Register a node with optional FSM
    pub fn register(&mut self, id: LayoutNodeId, fsm: Option<StateMachine>) {
        self.nodes
            .insert(Self::key(id), NodeData { fsm, state: None });
        self.dirty.mark(id);
    }

    /// Register a node with an FSM
    pub fn register_with_fsm(&mut self, id: LayoutNodeId, fsm: StateMachine) {
        self.register(id, Some(fsm));
    }

    /// Unregister a node
    pub fn unregister(&mut self, id: LayoutNodeId) {
        self.nodes.remove(&Self::key(id));
    }

    /// Check if a node is registered
    pub fn is_registered(&self, id: LayoutNodeId) -> bool {
        self.nodes.contains_key(&Self::key(id))
    }

    /// Get the FSM state for a node
    pub fn get_fsm_state(&self, id: LayoutNodeId) -> Option<u32> {
        self.nodes
            .get(&Self::key(id))
            .and_then(|d| d.fsm.as_ref())
            .map(|fsm| fsm.current_state())
    }

    /// Send an event to a node's FSM
    ///
    /// Returns true if the FSM transitioned to a new state.
    pub fn send_event(&mut self, id: LayoutNodeId, event_type: EventId) -> bool {
        let key = Self::key(id);
        if let Some(data) = self.nodes.get_mut(&key) {
            if let Some(ref mut fsm) = data.fsm {
                let old_state = fsm.current_state();
                fsm.send(event_type);
                let new_state = fsm.current_state();

                if old_state != new_state {
                    self.dirty.mark(id);
                    return true;
                }
            }
        }
        false
    }

    /// Dispatch an Event struct to a node's FSM
    ///
    /// Convenience method that extracts the event_type and calls send_event.
    /// Returns true if the FSM transitioned to a new state.
    pub fn dispatch_event(&mut self, id: LayoutNodeId, event: &Event) -> bool {
        self.send_event(id, event.event_type)
    }

    /// Set custom state for a node
    pub fn set_state<S: NodeState>(&mut self, id: LayoutNodeId, state: S) {
        let key = Self::key(id);
        if let Some(data) = self.nodes.get_mut(&key) {
            data.state = Some(Box::new(state));
            self.dirty.mark(id);
        } else {
            // Auto-register if not registered
            self.nodes.insert(
                key,
                NodeData {
                    fsm: None,
                    state: Some(Box::new(state)),
                },
            );
            self.dirty.mark(id);
        }
    }

    /// Get custom state for a node (immutable)
    pub fn get_state<S: 'static>(&self, id: LayoutNodeId) -> Option<&S> {
        self.nodes
            .get(&Self::key(id))
            .and_then(|d| d.state.as_ref())
            .and_then(|s| {
                // Double deref to get concrete type (Box -> dyn NodeState -> concrete)
                (**s).as_any().downcast_ref()
            })
    }

    /// Get custom state for a node (mutable)
    pub fn get_state_mut<S: 'static>(&mut self, id: LayoutNodeId) -> Option<&mut S> {
        self.nodes
            .get_mut(&Self::key(id))
            .and_then(|d| d.state.as_mut())
            .and_then(|s| {
                // Double deref to get concrete type
                (**s).as_any_mut().downcast_mut()
            })
    }

    /// Mark a node as dirty
    pub fn mark_dirty(&mut self, id: LayoutNodeId) {
        self.dirty.mark(id);
    }

    /// Mark the tree as needing full re-layout
    pub fn mark_layout(&mut self) {
        self.dirty.mark_layout();
    }

    /// Check if a node is dirty
    pub fn is_dirty(&self, id: LayoutNodeId) -> bool {
        self.dirty.is_dirty(id)
    }

    /// Check if any nodes are dirty
    pub fn has_dirty(&self) -> bool {
        self.dirty.has_dirty()
    }

    /// Check if layout is needed
    pub fn needs_layout(&self) -> bool {
        self.dirty.needs_layout()
    }

    /// Take all dirty node IDs (clears the set)
    pub fn take_dirty(&mut self) -> Vec<LayoutNodeId> {
        self.dirty.take_dirty()
    }

    /// Clear the layout flag
    pub fn clear_layout(&mut self) {
        self.dirty.clear_layout();
    }

    /// Clear all dirty flags
    pub fn clear_all(&mut self) {
        self.dirty.clear_all();
    }

    /// Get the dirty tracker (immutable)
    pub fn dirty_tracker(&self) -> &DirtyTracker {
        &self.dirty
    }

    /// Get the dirty tracker (mutable)
    pub fn dirty_tracker_mut(&mut self) -> &mut DirtyTracker {
        &mut self.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::events::{event_types, EventData};
    use blinc_core::fsm::StateMachine;
    use slotmap::SlotMap;

    // Create test node IDs
    fn create_node_id() -> LayoutNodeId {
        let mut sm: SlotMap<LayoutNodeId, ()> = SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn test_dirty_tracker() {
        let mut tracker = DirtyTracker::new();
        let id = create_node_id();

        assert!(!tracker.is_dirty(id));
        assert!(!tracker.has_dirty());

        tracker.mark(id);
        assert!(tracker.is_dirty(id));
        assert!(tracker.has_dirty());

        let dirty = tracker.take_dirty();
        assert_eq!(dirty.len(), 1);
        assert!(!tracker.has_dirty());
    }

    #[test]
    fn test_interactive_context_state() {
        let mut ctx = InteractiveContext::new();
        let id = create_node_id();

        // Set state
        ctx.set_state(id, 42u32);

        // Get state
        let state = ctx.get_state::<u32>(id);
        assert_eq!(state, Some(&42));

        // Modify state
        if let Some(s) = ctx.get_state_mut::<u32>(id) {
            *s = 100;
        }
        assert_eq!(ctx.get_state::<u32>(id), Some(&100));
    }

    #[test]
    fn test_interactive_context_fsm() {
        let mut ctx = InteractiveContext::new();
        let id = create_node_id();

        // Create FSM: IDLE --(POINTER_ENTER)--> HOVERED
        let fsm = StateMachine::builder(0)
            .on(0, event_types::POINTER_ENTER, 1)
            .on(1, event_types::POINTER_LEAVE, 0)
            .build();

        ctx.register_with_fsm(id, fsm);
        assert_eq!(ctx.get_fsm_state(id), Some(0));

        // Clear dirty flag from registration
        ctx.take_dirty();

        // Send event directly
        let transitioned = ctx.send_event(id, event_types::POINTER_ENTER);
        assert!(transitioned);
        assert_eq!(ctx.get_fsm_state(id), Some(1));
        assert!(ctx.is_dirty(id));

        // Also test dispatch_event with Event struct
        ctx.take_dirty();
        let event = Event {
            event_type: event_types::POINTER_LEAVE,
            target: 0,
            data: EventData::Pointer {
                x: 0.0,
                y: 0.0,
                button: 0,
                pressure: 1.0,
            },
            timestamp: 0,
            propagation_stopped: false,
        };

        let transitioned = ctx.dispatch_event(id, &event);
        assert!(transitioned);
        assert_eq!(ctx.get_fsm_state(id), Some(0));
    }

    #[test]
    fn test_complex_state_type() {
        #[derive(Debug, PartialEq)]
        struct ButtonState {
            scale: f32,
            clicked: bool,
        }

        let mut ctx = InteractiveContext::new();
        let id = create_node_id();

        ctx.set_state(
            id,
            ButtonState {
                scale: 1.0,
                clicked: false,
            },
        );

        if let Some(state) = ctx.get_state_mut::<ButtonState>(id) {
            state.scale = 0.95;
            state.clicked = true;
        }

        let state = ctx.get_state::<ButtonState>(id).unwrap();
        assert_eq!(state.scale, 0.95);
        assert!(state.clicked);
    }
}
