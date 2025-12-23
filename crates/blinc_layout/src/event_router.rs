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
pub struct EventRouter {
    /// Current mouse position
    mouse_x: f32,
    mouse_y: f32,

    /// Elements currently under the pointer (for enter/leave tracking)
    hovered: HashSet<LayoutNodeId>,

    /// Element where mouse button was pressed (for proper release targeting)
    pressed_target: Option<LayoutNodeId>,

    /// Currently focused element (receives keyboard events)
    focused: Option<LayoutNodeId>,

    /// Callback for routing events to elements
    event_callback: Option<EventCallback>,
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
            hovered: HashSet::new(),
            pressed_target: None,
            focused: None,
            event_callback: None,
        }
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

    /// Get the currently focused element
    pub fn focused(&self) -> Option<LayoutNodeId> {
        self.focused
    }

    /// Set focus to an element (or None to clear focus)
    pub fn set_focus(&mut self, node: Option<LayoutNodeId>) {
        // Send BLUR to old focused element
        if let Some(old_focused) = self.focused {
            if Some(old_focused) != node {
                self.emit_event(old_focused, event_types::BLUR);
            }
        }

        // Send FOCUS to new focused element
        if let Some(new_focused) = node {
            if self.focused != Some(new_focused) {
                self.emit_event(new_focused, event_types::FOCUS);
            }
        }

        self.focused = node;
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

        events
    }

    /// Handle mouse button press
    ///
    /// Emits POINTER_DOWN to the topmost hit element.
    /// Also sets focus to the clicked element.
    pub fn on_mouse_down(
        &mut self,
        tree: &RenderTree,
        x: f32,
        y: f32,
        _button: MouseButton,
    ) -> Option<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        // Hit test for the topmost element
        if let Some(hit) = self.hit_test(tree, x, y) {
            self.pressed_target = Some(hit.node);

            // Set focus to the clicked element
            self.set_focus(Some(hit.node));

            self.emit_event(hit.node, event_types::POINTER_DOWN);
            Some((hit.node, event_types::POINTER_DOWN))
        } else {
            // Clicked outside any element - clear focus
            self.set_focus(None);
            self.pressed_target = None;
            None
        }
    }

    /// Handle mouse button release
    ///
    /// Emits POINTER_UP to the element where the press started
    /// (ensures proper button release even if cursor moved).
    pub fn on_mouse_up(
        &mut self,
        _tree: &RenderTree,
        x: f32,
        y: f32,
        _button: MouseButton,
    ) -> Option<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        // Release goes to the element where press started
        if let Some(target) = self.pressed_target.take() {
            self.emit_event(target, event_types::POINTER_UP);
            Some((target, event_types::POINTER_UP))
        } else {
            None
        }
    }

    /// Handle mouse leaving the window
    ///
    /// Emits POINTER_LEAVE to all currently hovered elements.
    pub fn on_mouse_leave(&mut self) -> Vec<(LayoutNodeId, u32)> {
        // Collect nodes first to avoid borrow conflict
        let nodes: Vec<_> = self.hovered.iter().copied().collect();
        let mut events = Vec::with_capacity(nodes.len());

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

    // =========================================================================
    // Scroll Events
    // =========================================================================

    /// Handle scroll event
    ///
    /// Emits SCROLL to the element under the pointer.
    pub fn on_scroll(
        &mut self,
        tree: &RenderTree,
        _delta_x: f32,
        _delta_y: f32,
    ) -> Option<(LayoutNodeId, u32)> {
        if let Some(hit) = self.hit_test(tree, self.mouse_x, self.mouse_y) {
            self.emit_event(hit.node, event_types::SCROLL);
            Some((hit.node, event_types::SCROLL))
        } else {
            None
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

        // Check children in reverse order (last child is on top)
        let children = tree.layout().children(node);
        for child in children.into_iter().rev() {
            if let Some(result) = self.hit_test_node(
                tree,
                child,
                x,
                y,
                (bounds.x, bounds.y),
                ancestors.clone(),
            ) {
                return Some(result);
            }
        }

        // No child hit, this node is the target
        Some(HitTestResult {
            node,
            local_x: x - bounds.x,
            local_y: y - bounds.y,
            ancestors,
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
        });

        // Check children
        let children = tree.layout().children(node);
        for child in children {
            self.hit_test_node_all(
                tree,
                child,
                x,
                y,
                (bounds.x, bounds.y),
                ancestors.clone(),
                results,
            );
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
        let ui = div()
            .w(400.0)
            .h(300.0)
            .child(div().w(100.0).h(100.0));

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
        assert!(captured.iter().any(|(_, e)| *e == event_types::POINTER_ENTER));
    }

    #[test]
    fn test_mouse_down_up() {
        let ui = div()
            .w(400.0)
            .h(300.0)
            .child(div().w(100.0).h(100.0));

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
}
