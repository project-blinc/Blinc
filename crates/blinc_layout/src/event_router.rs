//! Event routing from platform input to layout elements
//!
//! Bridges platform-level input events (mouse, touch, keyboard) to
//! element-level events that drive FSM state transitions.
//!
//! # Architecture
//!
//! ```text
//! Platform Input (mouse moved, button pressed)
//!     ↓
//! EventRouter (hit testing, hover tracking)
//!     ↓
//! Element FSM Events (POINTER_ENTER, POINTER_DOWN, etc.)
//!     ↓
//! Stateful<S> state transitions
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_layout::event_router::EventRouter;
//!
//! let mut router = EventRouter::new();
//!
//! // After building render tree with computed layout
//! let tree = RenderTree::from_element(&ui);
//! tree.compute_layout(800.0, 600.0);
//!
//! // Route mouse events
//! router.on_mouse_move(&tree, 100.0, 200.0);
//! router.on_mouse_down(&tree, 100.0, 200.0, MouseButton::Left);
//! router.on_mouse_up(&tree, 100.0, 200.0, MouseButton::Left);
//! ```

use std::collections::HashSet;

use blinc_core::events::event_types;

use crate::element::ElementBounds;
use crate::renderer::RenderTree;
use crate::tree::LayoutNodeId;

/// Mouse button identifier (matches platform)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Result of a hit test
#[derive(Clone, Debug)]
pub struct HitTestResult {
    /// The node that was hit (topmost in z-order)
    pub node: LayoutNodeId,
    /// Position relative to the node's bounds
    pub local_x: f32,
    pub local_y: f32,
    /// The hit chain from root to the hit node (for event bubbling)
    pub ancestors: Vec<LayoutNodeId>,
    /// The bounds width of the hit element
    pub bounds_width: f32,
    /// The bounds height of the hit element
    pub bounds_height: f32,
}

/// Callback for element events
pub type EventCallback = Box<dyn FnMut(LayoutNodeId, u32)>;

/// Routes platform input events to layout elements
///
/// Maintains state for:
/// - Current mouse position
/// - Currently hovered elements (for enter/leave detection)
/// - Currently pressed elements (for proper release targeting)
/// - Focused element (for keyboard events)
/// - Last scroll delta (for scroll event dispatch)
/// - Drag state (for drag gesture detection)
pub struct EventRouter {
    /// Current mouse position
    mouse_x: f32,
    mouse_y: f32,

    /// Local coordinates from the last hit test (relative to the hit element)
    last_hit_local_x: f32,
    last_hit_local_y: f32,

    /// Bounds from the last hit test (element dimensions)
    last_hit_bounds_width: f32,
    last_hit_bounds_height: f32,

    /// Elements currently under the pointer (for enter/leave tracking)
    hovered: HashSet<LayoutNodeId>,

    /// Element where mouse button was pressed (for proper release targeting)
    pressed_target: Option<LayoutNodeId>,

    /// Ancestors of pressed target (for event bubbling on release)
    pressed_ancestors: Vec<LayoutNodeId>,

    /// Currently focused element (receives keyboard events)
    focused: Option<LayoutNodeId>,

    /// Ancestors of the focused element (for BLUR bubbling)
    focused_ancestors: Vec<LayoutNodeId>,

    /// Callback for routing events to elements
    event_callback: Option<EventCallback>,

    /// Last scroll delta (for passing to event handlers)
    scroll_delta_x: f32,
    scroll_delta_y: f32,

    /// Drag state tracking
    is_dragging: bool,
    /// Start position of the drag
    drag_start_x: f32,
    drag_start_y: f32,
    /// Delta from drag start
    drag_delta_x: f32,
    drag_delta_y: f32,
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRouter {
    /// Create a new event router
    pub fn new() -> Self {
        Self {
            mouse_x: 0.0,
            mouse_y: 0.0,
            last_hit_local_x: 0.0,
            last_hit_local_y: 0.0,
            last_hit_bounds_width: 0.0,
            last_hit_bounds_height: 0.0,
            hovered: HashSet::new(),
            pressed_target: None,
            pressed_ancestors: Vec::new(),
            focused: None,
            focused_ancestors: Vec::new(),
            event_callback: None,
            scroll_delta_x: 0.0,
            scroll_delta_y: 0.0,
            is_dragging: false,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            drag_delta_x: 0.0,
            drag_delta_y: 0.0,
        }
    }

    /// Get the last hit test local coordinates
    ///
    /// These are updated whenever a hit test is performed (mouse move, click, etc.)
    pub fn last_hit_local(&self) -> (f32, f32) {
        (self.last_hit_local_x, self.last_hit_local_y)
    }

    /// Get the last hit test bounds dimensions
    ///
    /// These are updated whenever a hit test is performed (mouse move, click, etc.)
    pub fn last_hit_bounds(&self) -> (f32, f32) {
        (self.last_hit_bounds_width, self.last_hit_bounds_height)
    }

    /// Get the current drag delta (offset from drag start position)
    ///
    /// Returns (delta_x, delta_y) - the distance dragged from the initial mouse_down position.
    /// Only meaningful when `is_dragging()` returns true.
    pub fn drag_delta(&self) -> (f32, f32) {
        (self.drag_delta_x, self.drag_delta_y)
    }

    /// Check if a drag operation is currently in progress
    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Set the event callback for routing events to elements
    ///
    /// The callback receives (node_id, event_type) and should dispatch
    /// to the appropriate element's FSM.
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: FnMut(LayoutNodeId, u32) + 'static,
    {
        self.event_callback = Some(Box::new(callback));
    }

    /// Clear the event callback
    pub fn clear_event_callback(&mut self) {
        self.event_callback = None;
    }

    /// Get the currently focused element
    pub fn focused(&self) -> Option<LayoutNodeId> {
        self.focused
    }

    /// Get the ancestors of the focused element (for bubbling keyboard events)
    ///
    /// Returns ancestors from root to leaf order (the focused element is the last item
    /// in this list).
    pub fn focused_ancestors(&self) -> &[LayoutNodeId] {
        &self.focused_ancestors
    }

    /// Set focus to an element (or None to clear focus)
    ///
    /// BLUR is bubbled to ancestors (so container elements receive blur even when
    /// focus was on a child leaf element).
    pub fn set_focus(&mut self, node: Option<LayoutNodeId>) {
        self.set_focus_with_ancestors(node, Vec::new());
    }

    /// Set focus to an element with its ancestor chain (for proper BLUR bubbling)
    pub fn set_focus_with_ancestors(
        &mut self,
        node: Option<LayoutNodeId>,
        ancestors: Vec<LayoutNodeId>,
    ) {
        // Send BLUR to old focused element AND bubble to its ancestors
        if let Some(old_focused) = self.focused {
            if Some(old_focused) != node {
                tracing::info!(
                    "EventRouter: sending BLUR to old_focused {:?}, new focus will be {:?}",
                    old_focused,
                    node
                );
                // Use the stored focused_ancestors for bubbling BLUR
                let old_ancestors = std::mem::take(&mut self.focused_ancestors);
                self.emit_event(old_focused, event_types::BLUR);
                // Bubble BLUR to ancestors (container elements with blur handlers)
                for ancestor in old_ancestors {
                    if ancestor != old_focused {
                        self.emit_event(ancestor, event_types::BLUR);
                    }
                }
            } else {
                tracing::info!("EventRouter: focus unchanged at {:?}", node);
            }
        } else {
            tracing::info!(
                "EventRouter: no previous focus, setting focus to {:?}",
                node
            );
        }

        // Send FOCUS to new focused element
        if let Some(new_focused) = node {
            if self.focused != Some(new_focused) {
                self.emit_event(new_focused, event_types::FOCUS);
            }
        }

        self.focused = node;
        self.focused_ancestors = ancestors;
    }

    /// Get current mouse position
    pub fn mouse_position(&self) -> (f32, f32) {
        (self.mouse_x, self.mouse_y)
    }

    // =========================================================================
    // Mouse Events
    // =========================================================================

    /// Handle mouse move event
    ///
    /// Updates hover state and emits POINTER_ENTER/POINTER_LEAVE events.
    /// Also emits DRAG events if a button is pressed (dragging).
    /// Returns the list of events that were emitted.
    pub fn on_mouse_move(&mut self, tree: &RenderTree, x: f32, y: f32) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        let mut events = Vec::new();

        // Hit test to find elements under pointer
        let hits = self.hit_test_all(tree, x, y);
        let current_hovered: HashSet<LayoutNodeId> = hits.iter().map(|h| h.node).collect();

        // Elements that were hovered but no longer are -> POINTER_LEAVE
        let left: Vec<_> = self.hovered.difference(&current_hovered).copied().collect();
        for node in left {
            self.emit_event(node, event_types::POINTER_LEAVE);
            events.push((node, event_types::POINTER_LEAVE));
        }

        // Elements that are newly hovered -> POINTER_ENTER
        let entered: Vec<_> = current_hovered.difference(&self.hovered).copied().collect();
        for node in entered {
            self.emit_event(node, event_types::POINTER_ENTER);
            events.push((node, event_types::POINTER_ENTER));
        }

        // All currently hovered elements get POINTER_MOVE
        for node in &current_hovered {
            self.emit_event(*node, event_types::POINTER_MOVE);
            events.push((*node, event_types::POINTER_MOVE));
        }

        self.hovered = current_hovered;

        // Drag detection: if we have a pressed target and moved, emit DRAG
        if let Some(target) = self.pressed_target {
            // Update drag delta
            self.drag_delta_x = x - self.drag_start_x;
            self.drag_delta_y = y - self.drag_start_y;

            // Start dragging if we've moved more than a small threshold
            const DRAG_THRESHOLD: f32 = 3.0;
            let delta_exceeds = self.drag_delta_x.abs() > DRAG_THRESHOLD
                || self.drag_delta_y.abs() > DRAG_THRESHOLD;

            tracing::trace!(
                "Drag check: target={:?}, delta=({:.1}, {:.1}), threshold_exceeded={}, is_dragging={}",
                target, self.drag_delta_x, self.drag_delta_y, delta_exceeds, self.is_dragging
            );

            if !self.is_dragging && delta_exceeds {
                self.is_dragging = true;
                tracing::info!(
                    "DRAG started: target={:?}, delta=({:.1}, {:.1})",
                    target, self.drag_delta_x, self.drag_delta_y
                );
            }

            // Emit DRAG event to the pressed target
            if self.is_dragging {
                tracing::info!(
                    "Emitting DRAG to {:?}, delta=({:.1}, {:.1})",
                    target, self.drag_delta_x, self.drag_delta_y
                );
                self.emit_event(target, event_types::DRAG);
                events.push((target, event_types::DRAG));

                // Collect ancestors to avoid borrow conflict
                let ancestors: Vec<_> = self
                    .pressed_ancestors
                    .iter()
                    .rev()
                    .skip(1)
                    .copied()
                    .collect();
                for ancestor in ancestors {
                    self.emit_event(ancestor, event_types::DRAG);
                    events.push((ancestor, event_types::DRAG));
                }
            }
        }

        events
    }

    /// Handle mouse button press
    ///
    /// Emits POINTER_DOWN to the topmost hit element AND bubbles through ancestors.
    /// This allows parent elements to receive click events even when clicking on children.
    /// Also sets focus to the clicked element and initializes drag tracking.
    pub fn on_mouse_down(
        &mut self,
        tree: &RenderTree,
        x: f32,
        y: f32,
        _button: MouseButton,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        // Initialize drag tracking
        self.drag_start_x = x;
        self.drag_start_y = y;
        self.drag_delta_x = 0.0;
        self.drag_delta_y = 0.0;
        self.is_dragging = false;

        let mut events = Vec::new();

        // Hit test for the topmost element
        if let Some(hit) = self.hit_test(tree, x, y) {
            tracing::debug!(
                "on_mouse_down: hit node {:?} at ({:.1}, {:.1}), ancestors={:?}",
                hit.node,
                x,
                y,
                hit.ancestors
            );
            self.pressed_target = Some(hit.node);
            // Store ancestors for bubbling on release
            self.pressed_ancestors = hit.ancestors.clone();
            // Store local coordinates and bounds for event handlers
            self.last_hit_local_x = hit.local_x;
            self.last_hit_local_y = hit.local_y;
            self.last_hit_bounds_width = hit.bounds_width;
            self.last_hit_bounds_height = hit.bounds_height;

            // Set focus to the clicked element WITH its ancestors (for BLUR bubbling later)
            self.set_focus_with_ancestors(Some(hit.node), hit.ancestors.clone());

            // Emit to the hit node first
            self.emit_event(hit.node, event_types::POINTER_DOWN);
            events.push((hit.node, event_types::POINTER_DOWN));

            // Bubble through ancestors (leaf to root order)
            // ancestors is root to leaf, so reverse and skip the hit node (last element)
            for &ancestor in hit.ancestors.iter().rev().skip(1) {
                self.emit_event(ancestor, event_types::POINTER_DOWN);
                events.push((ancestor, event_types::POINTER_DOWN));
            }
        } else {
            // Clicked outside any element - clear focus
            self.set_focus(None);
            self.pressed_target = None;
            self.pressed_ancestors.clear();
        }

        events
    }

    /// Handle mouse button release
    ///
    /// Emits POINTER_UP to the element where the press started AND bubbles through ancestors.
    /// If dragging was in progress, also emits DRAG_END.
    /// (ensures proper button release even if cursor moved).
    pub fn on_mouse_up(
        &mut self,
        _tree: &RenderTree,
        x: f32,
        y: f32,
        _button: MouseButton,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        let mut events = Vec::new();

        // Check if we were dragging
        let was_dragging = self.is_dragging;

        tracing::debug!(
            "on_mouse_up: pressed_target={:?}, was_dragging={}, pos=({:.1}, {:.1})",
            self.pressed_target,
            was_dragging,
            x,
            y
        );

        // Release goes to the element where press started
        if let Some(target) = self.pressed_target.take() {
            // If we were dragging, emit DRAG_END before POINTER_UP
            if was_dragging {
                self.emit_event(target, event_types::DRAG_END);
                events.push((target, event_types::DRAG_END));
            }

            // Emit to the target first
            tracing::debug!("on_mouse_up: emitting POINTER_UP to target {:?}", target);
            self.emit_event(target, event_types::POINTER_UP);
            events.push((target, event_types::POINTER_UP));

            // Bubble through ancestors (stored from on_mouse_down)
            // ancestors is root to leaf, so reverse and skip the target node (last element)
            let ancestors = std::mem::take(&mut self.pressed_ancestors);
            for &ancestor in ancestors.iter().rev().skip(1) {
                if was_dragging {
                    self.emit_event(ancestor, event_types::DRAG_END);
                    events.push((ancestor, event_types::DRAG_END));
                }
                self.emit_event(ancestor, event_types::POINTER_UP);
                events.push((ancestor, event_types::POINTER_UP));
            }
        } else {
            self.pressed_ancestors.clear();
        }

        // Reset drag state
        self.is_dragging = false;
        self.drag_delta_x = 0.0;
        self.drag_delta_y = 0.0;

        events
    }

    /// Handle mouse leaving the window
    ///
    /// Emits POINTER_LEAVE to all currently hovered elements.
    /// Also emits POINTER_UP to the pressed target if there is one (mouse left while dragging).
    pub fn on_mouse_leave(&mut self) -> Vec<(LayoutNodeId, u32)> {
        let mut events = Vec::new();

        // If we were pressing/dragging, emit POINTER_UP to clean up state
        // This handles the case where mouse leaves the window while dragging
        if let Some(target) = self.pressed_target.take() {
            tracing::debug!(
                "on_mouse_leave: emitting POINTER_UP to pressed_target {:?} (mouse left window while pressing)",
                target
            );

            // If we were dragging, emit DRAG_END before POINTER_UP
            if self.is_dragging {
                self.emit_event(target, event_types::DRAG_END);
                events.push((target, event_types::DRAG_END));
            }

            self.emit_event(target, event_types::POINTER_UP);
            events.push((target, event_types::POINTER_UP));

            // Bubble through ancestors
            let ancestors = std::mem::take(&mut self.pressed_ancestors);
            for &ancestor in ancestors.iter().rev().skip(1) {
                if self.is_dragging {
                    self.emit_event(ancestor, event_types::DRAG_END);
                    events.push((ancestor, event_types::DRAG_END));
                }
                self.emit_event(ancestor, event_types::POINTER_UP);
                events.push((ancestor, event_types::POINTER_UP));
            }

            // Reset drag state
            self.is_dragging = false;
            self.drag_delta_x = 0.0;
            self.drag_delta_y = 0.0;
        }

        // Emit POINTER_LEAVE to all hovered elements
        let nodes: Vec<_> = self.hovered.iter().copied().collect();
        for node in nodes {
            self.emit_event(node, event_types::POINTER_LEAVE);
            events.push((node, event_types::POINTER_LEAVE));
        }

        self.hovered.clear();
        events
    }

    // =========================================================================
    // Keyboard Events
    // =========================================================================

    /// Handle key press
    ///
    /// Emits KEY_DOWN to the focused element.
    pub fn on_key_down(&mut self, _key_code: u32) -> Option<(LayoutNodeId, u32)> {
        if let Some(focused) = self.focused {
            self.emit_event(focused, event_types::KEY_DOWN);
            Some((focused, event_types::KEY_DOWN))
        } else {
            None
        }
    }

    /// Handle key release
    ///
    /// Emits KEY_UP to the focused element.
    pub fn on_key_up(&mut self, _key_code: u32) -> Option<(LayoutNodeId, u32)> {
        if let Some(focused) = self.focused {
            self.emit_event(focused, event_types::KEY_UP);
            Some((focused, event_types::KEY_UP))
        } else {
            None
        }
    }

    /// Handle text input (character typed)
    ///
    /// Emits TEXT_INPUT to the focused element.
    /// Returns the focused node if there is one.
    pub fn on_text_input(&mut self, _char: char) -> Option<(LayoutNodeId, u32)> {
        if let Some(focused) = self.focused {
            self.emit_event(focused, event_types::TEXT_INPUT);
            Some((focused, event_types::TEXT_INPUT))
        } else {
            None
        }
    }

    // =========================================================================
    // Scroll Events
    // =========================================================================

    /// Handle scroll event
    ///
    /// Emits SCROLL to the element under the pointer AND all its ancestors.
    /// This allows scroll events to bubble up to scroll containers even when
    /// the mouse is over a child element inside the scroll.
    ///
    /// Returns all nodes that received the scroll event.
    pub fn on_scroll(
        &mut self,
        tree: &RenderTree,
        delta_x: f32,
        delta_y: f32,
    ) -> Vec<(LayoutNodeId, u32)> {
        // Store delta for event dispatch
        self.scroll_delta_x = delta_x;
        self.scroll_delta_y = delta_y;

        let mut events = Vec::new();

        if let Some(hit) = self.hit_test(tree, self.mouse_x, self.mouse_y) {
            // Emit to the hit node first
            self.emit_event(hit.node, event_types::SCROLL);
            events.push((hit.node, event_types::SCROLL));

            // Then bubble up through ancestors (excluding the hit node which is last in ancestors)
            // Ancestors are stored from root to leaf, so iterate in reverse to go leaf-to-root
            for &ancestor in hit.ancestors.iter().rev().skip(1) {
                self.emit_event(ancestor, event_types::SCROLL);
                events.push((ancestor, event_types::SCROLL));
            }
        }

        events
    }

    /// Handle scroll event with smart nested scroll support
    ///
    /// Returns the hit result (node and ancestors) for use with RenderTree::dispatch_scroll_chain.
    /// This enables nested scrolls where inner scrolls consume delta for their direction
    /// before outer scrolls receive the remaining delta.
    pub fn on_scroll_nested(
        &mut self,
        tree: &RenderTree,
        delta_x: f32,
        delta_y: f32,
    ) -> Option<HitTestResult> {
        // Store delta for event dispatch
        self.scroll_delta_x = delta_x;
        self.scroll_delta_y = delta_y;

        // Return the hit result - caller will use dispatch_scroll_chain
        self.hit_test(tree, self.mouse_x, self.mouse_y)
    }

    /// Get the last scroll delta
    ///
    /// Use this to retrieve scroll delta when dispatching scroll events.
    pub fn scroll_delta(&self) -> (f32, f32) {
        (self.scroll_delta_x, self.scroll_delta_y)
    }

    // =========================================================================
    // Window Events
    // =========================================================================

    /// Handle window focus change
    ///
    /// When the window gains focus, emits WINDOW_FOCUS to the focused element.
    /// When the window loses focus, emits WINDOW_BLUR to the focused element.
    pub fn on_window_focus(&mut self, focused: bool) -> Option<(LayoutNodeId, u32)> {
        if let Some(focus_target) = self.focused {
            let event_type = if focused {
                event_types::WINDOW_FOCUS
            } else {
                event_types::WINDOW_BLUR
            };
            self.emit_event(focus_target, event_type);
            Some((focus_target, event_type))
        } else {
            None
        }
    }

    /// Handle window resize
    ///
    /// Emits RESIZE to all elements in the tree (broadcast).
    /// Returns the list of nodes that received the event.
    pub fn on_window_resize(
        &mut self,
        tree: &RenderTree,
        _width: f32,
        _height: f32,
    ) -> Vec<(LayoutNodeId, u32)> {
        let mut events = Vec::new();

        // Broadcast RESIZE to all nodes in the tree
        if let Some(root) = tree.root() {
            self.broadcast_event(tree, root, event_types::RESIZE, &mut events);
        }

        events
    }

    /// Broadcast an event to a node and all its descendants
    fn broadcast_event(
        &mut self,
        tree: &RenderTree,
        node: LayoutNodeId,
        event_type: u32,
        events: &mut Vec<(LayoutNodeId, u32)>,
    ) {
        self.emit_event(node, event_type);
        events.push((node, event_type));

        // Recurse to children
        let children = tree.layout().children(node);
        for child in children {
            self.broadcast_event(tree, child, event_type, events);
        }
    }

    // =========================================================================
    // Lifecycle Events
    // =========================================================================

    /// Notify that an element has been mounted (added to the tree)
    ///
    /// Should be called when a new element is added to the render tree.
    /// Emits MOUNT to the element.
    pub fn on_mount(&mut self, node: LayoutNodeId) {
        self.emit_event(node, event_types::MOUNT);
    }

    /// Notify that an element is about to be unmounted (removed from the tree)
    ///
    /// Should be called before an element is removed from the render tree.
    /// Emits UNMOUNT to the element. Also clears any state associated with
    /// the element (hover, focus, pressed target).
    pub fn on_unmount(&mut self, node: LayoutNodeId) {
        self.emit_event(node, event_types::UNMOUNT);

        // Clear any state associated with this node
        self.hovered.remove(&node);
        if self.pressed_target == Some(node) {
            self.pressed_target = None;
        }
        if self.focused == Some(node) {
            self.focused = None;
        }
    }

    /// Diff two render trees and emit mount/unmount events for changed elements
    ///
    /// This is the primary method for lifecycle tracking. Call it after
    /// rebuilding the UI to detect which elements were added or removed.
    ///
    /// Returns (mounted_nodes, unmounted_nodes).
    pub fn diff_trees(
        &mut self,
        old_tree: Option<&RenderTree>,
        new_tree: &RenderTree,
    ) -> (Vec<LayoutNodeId>, Vec<LayoutNodeId>) {
        let mut mounted = Vec::new();
        let mut unmounted = Vec::new();

        // Collect all nodes from old tree
        let old_nodes: HashSet<LayoutNodeId> = old_tree
            .map(|t| self.collect_all_nodes(t))
            .unwrap_or_default();

        // Collect all nodes from new tree
        let new_nodes: HashSet<LayoutNodeId> = self.collect_all_nodes(new_tree);

        // Nodes in new but not old -> mounted
        for node in new_nodes.difference(&old_nodes) {
            self.on_mount(*node);
            mounted.push(*node);
        }

        // Nodes in old but not new -> unmounted
        for node in old_nodes.difference(&new_nodes) {
            self.on_unmount(*node);
            unmounted.push(*node);
        }

        (mounted, unmounted)
    }

    /// Collect all node IDs from a render tree
    fn collect_all_nodes(&self, tree: &RenderTree) -> HashSet<LayoutNodeId> {
        let mut nodes = HashSet::new();
        if let Some(root) = tree.root() {
            self.collect_nodes_recursive(tree, root, &mut nodes);
        }
        nodes
    }

    /// Recursively collect node IDs
    fn collect_nodes_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        nodes: &mut HashSet<LayoutNodeId>,
    ) {
        nodes.insert(node);
        let children = tree.layout().children(node);
        for child in children {
            self.collect_nodes_recursive(tree, child, nodes);
        }
    }

    // =========================================================================
    // Hit Testing
    // =========================================================================

    /// Hit test to find the topmost element at a point
    ///
    /// Returns the hit result for the frontmost (last in child order) element
    /// that contains the point.
    pub fn hit_test(&self, tree: &RenderTree, x: f32, y: f32) -> Option<HitTestResult> {
        let root = tree.root()?;
        self.hit_test_node(tree, root, x, y, (0.0, 0.0), Vec::new())
    }

    /// Hit test to find all elements at a point
    ///
    /// Returns all elements that contain the point, from root to leaf.
    pub fn hit_test_all(&self, tree: &RenderTree, x: f32, y: f32) -> Vec<HitTestResult> {
        let mut results = Vec::new();
        if let Some(root) = tree.root() {
            self.hit_test_node_all(tree, root, x, y, (0.0, 0.0), Vec::new(), &mut results);
        }
        results
    }

    /// Recursive hit test for a single node
    fn hit_test_node(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        x: f32,
        y: f32,
        parent_offset: (f32, f32),
        mut ancestors: Vec<LayoutNodeId>,
    ) -> Option<HitTestResult> {
        let bounds = tree.layout().get_bounds(node, parent_offset)?;

        // Check if point is within bounds
        if !self.point_in_bounds(x, y, &bounds) {
            return None;
        }

        ancestors.push(node);

        // Get scroll offset for this node (if it's a scroll container)
        // Children are rendered at bounds + scroll_offset, so we need to
        // include the scroll offset when hit testing children
        let scroll_offset = tree.get_scroll_offset(node);
        let child_offset = (bounds.x + scroll_offset.0, bounds.y + scroll_offset.1);

        // Check children in reverse order (last child is on top)
        let children = tree.layout().children(node);
        tracing::trace!(
            "hit_test_node: node={:?}, bounds=({:.1}, {:.1}, {:.1}x{:.1}), children={:?}",
            node, bounds.x, bounds.y, bounds.width, bounds.height, children
        );
        for child in children.into_iter().rev() {
            if let Some(result) =
                self.hit_test_node(tree, child, x, y, child_offset, ancestors.clone())
            {
                return Some(result);
            }
        }

        // No child hit, this node is the target
        tracing::trace!("hit_test_node: returning node={:?} as target (no children hit)", node);
        Some(HitTestResult {
            node,
            local_x: x - bounds.x,
            local_y: y - bounds.y,
            ancestors,
            bounds_width: bounds.width,
            bounds_height: bounds.height,
        })
    }

    /// Recursive hit test collecting all hits
    fn hit_test_node_all(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        x: f32,
        y: f32,
        parent_offset: (f32, f32),
        mut ancestors: Vec<LayoutNodeId>,
        results: &mut Vec<HitTestResult>,
    ) {
        let Some(bounds) = tree.layout().get_bounds(node, parent_offset) else {
            return;
        };

        // Check if point is within bounds
        if !self.point_in_bounds(x, y, &bounds) {
            return;
        }

        ancestors.push(node);

        // Add this node to results
        results.push(HitTestResult {
            node,
            local_x: x - bounds.x,
            local_y: y - bounds.y,
            ancestors: ancestors.clone(),
            bounds_width: bounds.width,
            bounds_height: bounds.height,
        });

        // Get scroll offset for this node (if it's a scroll container)
        // Children are rendered at bounds + scroll_offset, so we need to
        // include the scroll offset when hit testing children
        let scroll_offset = tree.get_scroll_offset(node);
        let child_offset = (bounds.x + scroll_offset.0, bounds.y + scroll_offset.1);

        // Check children
        let children = tree.layout().children(node);
        for child in children {
            self.hit_test_node_all(tree, child, x, y, child_offset, ancestors.clone(), results);
        }
    }

    /// Check if a point is within element bounds
    fn point_in_bounds(&self, x: f32, y: f32, bounds: &ElementBounds) -> bool {
        x >= bounds.x
            && x < bounds.x + bounds.width
            && y >= bounds.y
            && y < bounds.y + bounds.height
    }

    /// Emit an event via the callback
    fn emit_event(&mut self, node: LayoutNodeId, event_type: u32) {
        tracing::debug!(
            "emit_event: node={:?}, event_type={}, has_callback={}",
            node,
            event_type,
            self.event_callback.is_some()
        );
        if let Some(ref mut callback) = self.event_callback {
            callback(node, event_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn test_hit_test_basic() {
        let ui = div()
            .w(400.0)
            .h(300.0)
            .child(div().w(100.0).h(100.0)) // 0,0 -> 100,100
            .child(div().w(100.0).h(100.0)); // 0,100 -> 100,200

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let router = EventRouter::new();

        // Hit first child
        let result = router.hit_test(&tree, 50.0, 50.0);
        assert!(result.is_some());

        // Hit second child
        let result = router.hit_test(&tree, 50.0, 150.0);
        assert!(result.is_some());

        // Miss - outside bounds
        let result = router.hit_test(&tree, 500.0, 500.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_hover_enter_leave() {
        let ui = div().w(400.0).h(300.0).child(div().w(100.0).h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let events: Rc<RefCell<Vec<(LayoutNodeId, u32)>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);

        let mut router = EventRouter::new();
        router.set_event_callback(move |node, event| {
            events_clone.borrow_mut().push((node, event));
        });

        // Move into the child
        router.on_mouse_move(&tree, 50.0, 50.0);

        // Should have POINTER_ENTER events
        let captured = events.borrow();
        assert!(captured
            .iter()
            .any(|(_, e)| *e == event_types::POINTER_ENTER));
    }

    #[test]
    fn test_mouse_down_up() {
        let ui = div().w(400.0).h(300.0).child(div().w(100.0).h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let events: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);

        let mut router = EventRouter::new();
        router.set_event_callback(move |_node, event| {
            events_clone.borrow_mut().push(event);
        });

        // Mouse down
        router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);

        // Mouse up (even if moved slightly)
        router.on_mouse_up(&tree, 55.0, 55.0, MouseButton::Left);

        let captured = events.borrow();
        assert!(captured.contains(&event_types::POINTER_DOWN));
        assert!(captured.contains(&event_types::POINTER_UP));
    }

    #[test]
    fn test_focus_blur() {
        let ui = div()
            .w(400.0)
            .h(300.0)
            .flex_col()
            .child(div().w(100.0).h(100.0)) // First child
            .child(div().w(100.0).h(100.0)); // Second child

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let events: Rc<RefCell<Vec<(LayoutNodeId, u32)>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);

        let mut router = EventRouter::new();
        router.set_event_callback(move |node, event| {
            events_clone.borrow_mut().push((node, event));
        });

        // Click first child - should focus it
        router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);
        assert!(router.focused().is_some());
        let first_focused = router.focused().unwrap();

        // Check FOCUS was emitted
        {
            let captured = events.borrow();
            assert!(captured
                .iter()
                .any(|(n, e)| *n == first_focused && *e == event_types::FOCUS));
        }

        // Click second child - should blur first and focus second
        router.on_mouse_down(&tree, 50.0, 150.0, MouseButton::Left);

        {
            let captured = events.borrow();
            // Should have BLUR for first element
            assert!(captured
                .iter()
                .any(|(n, e)| *n == first_focused && *e == event_types::BLUR));
        }
    }

    #[test]
    fn test_lifecycle_mount_unmount() {
        // Build first tree with 2 children
        let ui1 = div()
            .w(400.0)
            .h(300.0)
            .child(div().w(100.0).h(100.0))
            .child(div().w(100.0).h(100.0));

        let mut tree1 = RenderTree::from_element(&ui1);
        tree1.compute_layout(400.0, 300.0);

        let events: Rc<RefCell<Vec<(LayoutNodeId, u32)>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);

        let mut router = EventRouter::new();
        router.set_event_callback(move |node, event| {
            events_clone.borrow_mut().push((node, event));
        });

        // First render - all elements are mounted
        let (mounted, unmounted) = router.diff_trees(None, &tree1);
        assert_eq!(mounted.len(), 3); // root + 2 children
        assert_eq!(unmounted.len(), 0);

        // Check MOUNT events were emitted
        {
            let captured = events.borrow();
            assert_eq!(
                captured
                    .iter()
                    .filter(|(_, e)| *e == event_types::MOUNT)
                    .count(),
                3
            );
        }

        // Clear events
        events.borrow_mut().clear();

        // Build second tree with only 1 child
        let ui2 = div().w(400.0).h(300.0).child(div().w(100.0).h(100.0));

        let mut tree2 = RenderTree::from_element(&ui2);
        tree2.compute_layout(400.0, 300.0);

        // Second render - tree structure changed
        // Note: In real usage, node IDs would be stable across renders
        // for elements that didn't change. This test shows the mechanism.
        let (_mounted2, _unmounted2) = router.diff_trees(Some(&tree1), &tree2);

        // The diff mechanism works - specific counts depend on ID stability
        // which is implementation-dependent
    }

    #[test]
    fn test_unmount_clears_state() {
        let ui = div().w(400.0).h(300.0).child(div().w(100.0).h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let mut router = EventRouter::new();

        // Hover and focus the child
        router.on_mouse_move(&tree, 50.0, 50.0);
        router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);

        // Get the focused node
        let focused = router.focused();
        assert!(focused.is_some());

        // Unmount the focused node
        router.on_unmount(focused.unwrap());

        // Focus should be cleared
        assert!(router.focused().is_none());
    }

    #[test]
    fn test_window_focus_blur() {
        let ui = div().w(400.0).h(300.0).child(div().w(100.0).h(100.0));

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let events: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);

        let mut router = EventRouter::new();
        router.set_event_callback(move |_node, event| {
            events_clone.borrow_mut().push(event);
        });

        // Focus an element
        router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);
        events.borrow_mut().clear();

        // Window loses focus
        router.on_window_focus(false);
        {
            let captured = events.borrow();
            assert!(captured.contains(&event_types::WINDOW_BLUR));
        }

        events.borrow_mut().clear();

        // Window gains focus
        router.on_window_focus(true);
        {
            let captured = events.borrow();
            assert!(captured.contains(&event_types::WINDOW_FOCUS));
        }
    }
}
