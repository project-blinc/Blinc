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

#[cfg(feature = "recorder")]
use crate::recorder_bridge::{self, RecorderEventData, RecorderMouseButton};

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
    /// Absolute position of the element bounds (top-left corner)
    pub bounds_x: f32,
    pub bounds_y: f32,
    /// The bounds width of the hit element
    pub bounds_width: f32,
    /// The bounds height of the hit element
    pub bounds_height: f32,
    /// Bounds for each ancestor node (for correct bounds when bubbling)
    /// Maps node_id.to_raw() to (x, y, width, height)
    pub ancestor_bounds: std::collections::HashMap<u64, (f32, f32, f32, f32)>,
    /// Whether this hit is within a foreground-layer subtree.
    /// Used to prioritize foreground elements over normal elements.
    pub is_foreground: bool,
    /// Absolute bounds of every direct child of `node`, captured at
    /// hit-test time. `node` is the topmost (deepest) hit, so by
    /// construction none of these children contained the cursor at
    /// hit time. Used by the cursor-inside-last-leaf short-circuit
    /// to detect when the cursor moves into one of them — at which
    /// point the deepest hit would change and a full re-test is
    /// needed.
    pub leaf_children_bounds: Vec<(f32, f32, f32, f32)>,
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

    /// Bounds position from the last hit test (absolute position)
    last_hit_bounds_x: f32,
    last_hit_bounds_y: f32,

    /// Bounds from the last hit test (element dimensions)
    last_hit_bounds_width: f32,
    last_hit_bounds_height: f32,

    /// Elements currently under the pointer, keyed by `StableNodeId`
    /// so a subtree rebuild between mouse-moves doesn't invalidate
    /// the set. Pre-fix this was `HashSet<LayoutNodeId>` and any
    /// rebuild (the OverlayStack rebuilds the overlay layer when a
    /// submenu mounts, for instance) replaced layout ids for the
    /// same logical elements → the next mouse-move's hit test saw
    /// "all new" ids → diff fired POINTER_LEAVE on the (now-dead)
    /// old ids AND POINTER_ENTER on the new ones for unchanged
    /// hover state → cn::context_menu's submenu trigger ran its
    /// `on_hover_enter` close-and-reopen logic on every mouse move,
    /// re-spawning the submenu and restarting its enter animation =
    /// constant flicker. Mirrors `pressed_target`'s `StableNodeId`
    /// keying — see `gotcha_event_router_layout_id_staleness`.
    hovered: HashSet<crate::tree::StableNodeId>,

    /// Element where mouse button was pressed (for proper release
    /// targeting). Stored as `StableNodeId` so rebuilds between
    /// mouse-down and mouse-up don't invalidate the target; the
    /// release path resolves back to the current LayoutNodeId via
    /// `tree.layout_id()`.
    pressed_target: Option<crate::tree::StableNodeId>,

    /// Ancestors of pressed target (for event bubbling on release).
    /// Same stable-id treatment as `pressed_target`.
    pressed_ancestors: Vec<crate::tree::StableNodeId>,

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

    /// Bounds for each ancestor from the last hit test
    /// Maps node_id.to_raw() to (x, y, width, height)
    last_hit_ancestor_bounds: std::collections::HashMap<u64, (f32, f32, f32, f32)>,

    /// Ancestor chain (root → leaf) from the last successful hit
    /// test. Reused by `RenderTree::get_cursor_for_last_hit` so a
    /// mouse-move only walks the tree once per event instead of
    /// twice (once for hover dispatch + once for cursor resolution
    /// on a UI with `cursor:` styles set).
    last_hit_chain: Vec<LayoutNodeId>,
    /// `true` iff the deepest hit from the last test has no children
    /// in the layout tree. Gates `cursor_inside_last_leaf` — a leaf
    /// with children could see the cursor cross into a child without
    /// the parent's bounds being exited, which would change the
    /// hover set without our short-circuit detecting it.
    last_leaf_has_no_children: bool,
    /// Absolute bounds of every direct child of the last hit's leaf
    /// node. Used by `cursor_inside_last_leaf` to detect when the
    /// cursor moves into a child (which would change the deepest
    /// hit and require a full re-test) while staying inside the
    /// parent's bounds. Empty when the leaf is childless; in that
    /// case `last_leaf_has_no_children = true` and this check is
    /// vacuous.
    last_leaf_children_bounds: Vec<(f32, f32, f32, f32)>,
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
            last_hit_bounds_x: 0.0,
            last_hit_bounds_y: 0.0,
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
            last_hit_ancestor_bounds: std::collections::HashMap::new(),
            last_hit_chain: Vec::new(),
            last_leaf_has_no_children: false,
            last_leaf_children_bounds: Vec::new(),
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

    /// Get the last hit test bounds position (absolute top-left corner)
    ///
    /// These are updated whenever a hit test is performed (mouse move, click, etc.)
    pub fn last_hit_bounds_pos(&self) -> (f32, f32) {
        (self.last_hit_bounds_x, self.last_hit_bounds_y)
    }

    /// Get bounds for a specific node from the last hit test
    ///
    /// Returns the absolute bounds (x, y, width, height) for a node that was in
    /// the hit chain. This is used for event bubbling to ensure each handler
    /// receives the correct bounds for its own node, not the original hit target.
    ///
    /// Returns None if the node wasn't in the last hit chain.
    pub fn get_node_bounds(&self, node: LayoutNodeId) -> Option<(f32, f32, f32, f32)> {
        self.last_hit_ancestor_bounds.get(&node.to_raw()).copied()
    }

    /// `true` if a mouse-button press is currently in flight. Drag
    /// detection needs every move event, so the
    /// cursor-still-inside-last-leaf short-circuit defers to this.
    pub fn is_press_in_flight(&self) -> bool {
        self.pressed_target.is_some()
    }

    /// `true` when `(x, y)` lies within the AABB of the deepest hit
    /// node from the most recent hit test. When this returns true,
    /// the hover set can't have changed and the entire dispatch
    /// pipeline (hit_test, hover-set diff, ENTER/LEAVE/MOVE emit,
    /// cursor lookup) is safe to skip — only the stored cursor
    /// position needs updating.
    ///
    /// Returns `false` when no prior hit is recorded (zero bounds)
    /// so the very first mouse-move after window init always runs
    /// the full pipeline.
    pub fn cursor_inside_last_leaf(&self, x: f32, y: f32) -> bool {
        if self.last_hit_bounds_width <= 0.0 || self.last_hit_bounds_height <= 0.0 {
            return false;
        }
        let inside_leaf = x >= self.last_hit_bounds_x
            && x < self.last_hit_bounds_x + self.last_hit_bounds_width
            && y >= self.last_hit_bounds_y
            && y < self.last_hit_bounds_y + self.last_hit_bounds_height;
        if !inside_leaf {
            return false;
        }
        // Cursor is inside the previous leaf. The hover set is
        // unchanged ONLY if no direct child of the leaf now contains
        // the cursor — otherwise the child would become the new
        // deepest hit and a POINTER_ENTER would be due.
        //
        // Leaves with no children (`last_leaf_has_no_children`) skip
        // this loop entirely. Leaves with children pay an O(children)
        // bounds check per move; in cn_demo's deepest leaves that's
        // a handful of containers — orders of magnitude cheaper than
        // the recursive hit_test we'd otherwise run.
        if self.last_leaf_has_no_children {
            return true;
        }
        for (cx, cy, cw, ch) in self.last_leaf_children_bounds.iter().copied() {
            if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
                return false;
            }
        }
        true
    }

    /// Update stored cursor coordinates without running the hit-
    /// test pipeline. Paired with `cursor_inside_last_leaf` so the
    /// short-circuit path still keeps `mouse_position()` current
    /// for any handler / shader reading it.
    pub fn set_mouse_position(&mut self, x: f32, y: f32) {
        self.mouse_x = x;
        self.mouse_y = y;
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

    /// Check if a specific node is currently hovered.
    ///
    /// Callers holding a `LayoutNodeId` should resolve to the stable
    /// id via `tree.stable_id(node)` first — `self.hovered` is
    /// stable-id-keyed so a subtree rebuild between mouse-move
    /// frames doesn't drop the hover state. Convenience shim that
    /// takes a tree + layout id is below.
    pub fn is_hovered(&self, stable: crate::tree::StableNodeId) -> bool {
        self.hovered.contains(&stable)
    }

    /// `is_hovered` for callers that only have a `LayoutNodeId` and
    /// the tree — resolves the stable id internally.
    pub fn is_hovered_layout(&self, tree: &RenderTree, node_id: LayoutNodeId) -> bool {
        match tree.stable_id(node_id) {
            Some(s) => self.hovered.contains(&s),
            None => false,
        }
    }

    /// Check if a specific node is currently pressed, by its
    /// stable id (the source-of-truth for the router's pressed
    /// state). Callers holding a `LayoutNodeId` should look up its
    /// stable id via `tree.stable_id(node)` first.
    pub fn is_pressed(&self, stable: crate::tree::StableNodeId) -> bool {
        self.pressed_target == Some(stable)
    }

    /// Check if a specific node is currently focused
    pub fn is_focused(&self, node_id: LayoutNodeId) -> bool {
        self.focused == Some(node_id)
    }

    /// Get all currently hovered node IDs (stable). Callers wanting
    /// live `LayoutNodeId`s for dispatch should resolve via
    /// `tree.layout_id(stable)`; this iterator returns the
    /// stable-keyed canonical set.
    pub fn hovered_nodes(&self) -> impl Iterator<Item = crate::tree::StableNodeId> + '_ {
        self.hovered.iter().copied()
    }

    /// Root → leaf ancestor chain from the most recent hit test.
    /// Empty when the last test missed (cursor outside any node).
    /// Used by `RenderTree::get_cursor_for_last_hit` to resolve
    /// `cursor:` styles without a second tree walk.
    pub fn last_hit_chain(&self) -> &[LayoutNodeId] {
        &self.last_hit_chain
    }

    /// Get the pressed target as a `StableNodeId`, if any.
    ///
    /// Callers needing a `LayoutNodeId` for the current frame should
    /// resolve via `tree.layout_id(stable)` (which returns `None`
    /// when the press happened on a subtree that has since been
    /// removed by a rebuild).
    pub fn pressed_target_stable(&self) -> Option<crate::tree::StableNodeId> {
        self.pressed_target
    }

    /// Resolve the pressed target to a live `LayoutNodeId` for the
    /// given tree, or `None` if no press is in flight or the press
    /// target was removed by an intervening rebuild.
    pub fn pressed_target(&self, tree: &RenderTree) -> Option<LayoutNodeId> {
        self.pressed_target.and_then(|s| tree.layout_id(s))
    }

    /// `true` if a press is currently in flight (cheap; no tree
    /// lookup needed).
    pub fn has_pressed_target(&self) -> bool {
        self.pressed_target.is_some()
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
    /// Compute a small fingerprint of the routing state that
    /// `apply_stylesheet_state_styles` actually consults: the set of
    /// currently-hovered nodes, the pressed target, and the focused
    /// node. Callers cache the previous-frame fingerprint and skip
    /// the state-style application entirely when it's unchanged — a
    /// noticeable CPU drop on `cn_demo` where the steady-state
    /// (spinners rotating, no input) was iterating all ~hundreds of
    /// registered IDs every frame to detect zero state changes.
    ///
    /// The encoding XOR-folds the hovered set into a single `u64`
    /// (order-independent, cheap), then mixes in the pressed /
    /// focused identities. Different orderings of the same hovered
    /// set produce the same fingerprint, which is exactly what we
    /// want — `apply_stylesheet_state_styles` is itself
    /// order-insensitive.
    pub fn state_fingerprint(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hovered_xor: u64 = 0;
        for n in self.hovered.iter() {
            let mut h = DefaultHasher::new();
            n.hash(&mut h);
            hovered_xor ^= h.finish();
        }

        let mut h = DefaultHasher::new();
        hovered_xor.hash(&mut h);
        match self.pressed_target {
            Some(s) => {
                1u8.hash(&mut h);
                s.hash(&mut h);
            }
            None => 0u8.hash(&mut h),
        }
        match self.focused {
            Some(n) => {
                1u8.hash(&mut h);
                n.hash(&mut h);
            }
            None => 0u8.hash(&mut h),
        }
        h.finish()
    }

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
        let old_focused = self.focused;

        // Send BLUR to old focused element AND bubble to its ancestors
        if let Some(old) = old_focused {
            if Some(old) != node {
                tracing::debug!(
                    "EventRouter: sending BLUR to old_focused {:?}, new focus will be {:?}",
                    old,
                    node
                );
                // Use the stored focused_ancestors for bubbling BLUR
                let old_ancestors = std::mem::take(&mut self.focused_ancestors);
                self.emit_event(old, event_types::BLUR);
                // Bubble BLUR to ancestors (container elements with blur handlers)
                for ancestor in old_ancestors {
                    if ancestor != old {
                        self.emit_event(ancestor, event_types::BLUR);
                    }
                }
            } else {
                tracing::debug!("EventRouter: focus unchanged at {:?}", node);
            }
        } else {
            tracing::debug!(
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

        // Record focus change event if focus actually changed (only if recording is enabled)
        #[cfg(feature = "recorder")]
        if old_focused != node && recorder_bridge::is_recording() {
            recorder_bridge::record_event(RecorderEventData::FocusChange {
                from: old_focused.map(|n| format!("{:?}", n)),
                to: node.map(|n| format!("{:?}", n)),
            });
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
        // Delegate to the internal implementation with no occlusion
        self.on_mouse_move_internal(tree, x, y, &[], None)
    }

    /// Handle mouse move event with overlay occlusion awareness
    ///
    /// Drag-only fast path: emit DRAG events to the pressed target +
    /// its ancestors without running the hit-test pipeline. While a
    /// press is in flight the pressed target is fixed (set at
    /// mouse-down), so hover-set updates / ENTER / LEAVE / cursor
    /// lookup don't apply — those all care about which node is
    /// currently under the cursor, but during a drag the user is
    /// interacting with the pressed element regardless of cursor
    /// position. Returns the emitted events for the dispatch loop.
    ///
    /// Caller MUST verify `is_press_in_flight()` before invoking.
    /// Mouse-move events without a press should still go through
    /// `on_mouse_move_with_occlusion` for the full hover pipeline.
    pub fn on_mouse_drag_fast(
        &mut self,
        tree: &RenderTree,
        x: f32,
        y: f32,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;
        let mut events = Vec::new();
        let Some(stable_target) = self.pressed_target else {
            return events;
        };

        self.drag_delta_x = x - self.drag_start_x;
        self.drag_delta_y = y - self.drag_start_y;

        const DRAG_THRESHOLD: f32 = 3.0;
        let delta_exceeds =
            self.drag_delta_x.abs() > DRAG_THRESHOLD || self.drag_delta_y.abs() > DRAG_THRESHOLD;

        if !self.is_dragging && delta_exceeds {
            self.is_dragging = true;
        }

        if self.is_dragging {
            if let Some(target) = tree.layout_id(stable_target) {
                self.emit_event(target, event_types::DRAG);
                events.push((target, event_types::DRAG));
            }
            // Bubble to ancestors of the pressed target. Collect first
            // to avoid borrow conflict with `emit_event`.
            let ancestors: Vec<_> = self
                .pressed_ancestors
                .iter()
                .rev()
                .skip(1)
                .copied()
                .collect();
            for stable_ancestor in ancestors {
                if let Some(ancestor) = tree.layout_id(stable_ancestor) {
                    self.emit_event(ancestor, event_types::DRAG);
                    events.push((ancestor, event_types::DRAG));
                }
            }
        }

        // Also emit POINTER_MOVE to the pressed target + ancestors. The
        // slow `on_mouse_move_with_occlusion` path emits POINTER_MOVE
        // on every hovered node (filtered to those with handlers); we
        // mirror that for the drag fast path so an `on_mouse_move`
        // handler on a pressed widget still receives per-move ticks
        // during a drag. Without this, `motion_demo`'s pull-to-refresh
        // (which uses `on_mouse_move` to read mouse_y and drive
        // `set_immediate` on its translate binding) sat frozen during
        // a drag because POINTER_MOVE never reached the handler —
        // only DRAG did, and the demo doesn't subscribe to DRAG.
        // Filtered to nodes with handlers so the common no-subscriber
        // case stays cheap.
        let registry = tree.handler_registry();
        if let Some(target) = tree.layout_id(stable_target) {
            if let Some(stable) = tree.stable_id(target) {
                if registry.has_handler(stable, event_types::POINTER_MOVE) {
                    self.emit_event(target, event_types::POINTER_MOVE);
                    events.push((target, event_types::POINTER_MOVE));
                }
            }
        }
        let ancestors: Vec<_> = self
            .pressed_ancestors
            .iter()
            .rev()
            .skip(1)
            .copied()
            .collect();
        for stable_ancestor in ancestors {
            if registry.has_handler(stable_ancestor, event_types::POINTER_MOVE) {
                if let Some(ancestor) = tree.layout_id(stable_ancestor) {
                    self.emit_event(ancestor, event_types::POINTER_MOVE);
                    events.push((ancestor, event_types::POINTER_MOVE));
                }
            }
        }
        events
    }

    /// Same as `on_mouse_move`, but also checks for overlay occlusion.
    /// Elements that are visually occluded by overlays will not receive hover events.
    ///
    /// # Arguments
    /// - `tree` - The render tree
    /// - `x`, `y` - Mouse position
    /// - `overlay_bounds` - Bounds of visible overlays as (x, y, width, height)
    /// - `overlay_layer_id` - LayoutNodeId of the overlay layer container
    pub fn on_mouse_move_with_occlusion(
        &mut self,
        tree: &RenderTree,
        x: f32,
        y: f32,
        overlay_bounds: &[(f32, f32, f32, f32)],
        overlay_layer_id: Option<LayoutNodeId>,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.on_mouse_move_internal(tree, x, y, overlay_bounds, overlay_layer_id)
    }

    /// Internal mouse move handler (shared implementation)
    fn on_mouse_move_internal(
        &mut self,
        tree: &RenderTree,
        x: f32,
        y: f32,
        overlay_bounds: &[(f32, f32, f32, f32)],
        overlay_layer_id: Option<LayoutNodeId>,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        let mut events = Vec::new();

        // Hit test to find elements under pointer. The common no-overlay case
        // only needs the topmost hit chain (root -> leaf) for hover enter/leave
        // and event bubbling, so avoid `hit_test_all`'s sibling walk on every
        // mouse move in large scroll views.
        let (hits, current_hovered): (Vec<HitTestResult>, HashSet<LayoutNodeId>) = if overlay_bounds
            .is_empty()
        {
            if let Some(hit) = self.hit_test(tree, x, y) {
                let hovered = hit
                    .ancestors
                    .iter()
                    .copied()
                    .filter(|node| {
                        let in_bounds = hit.ancestor_bounds.get(&node.to_raw()).is_some_and(
                            |&(bx, by, bw, bh)| x >= bx && x < bx + bw && y >= by && y < by + bh,
                        );
                        let pointer_events_none = tree
                            .get_render_node(*node)
                            .map(|n| n.props.pointer_events_none)
                            .unwrap_or(false);
                        in_bounds && !pointer_events_none
                    })
                    .collect();
                (vec![hit], hovered)
            } else {
                (Vec::new(), HashSet::new())
            }
        } else {
            let hits =
                self.hit_test_all_with_occlusion(tree, x, y, overlay_bounds, overlay_layer_id);
            let hovered = hits.iter().map(|h| h.node).collect();
            (hits, hovered)
        };

        // Store bounds for all hit nodes (for event handlers to access via get_node_bounds)
        // Clear previous bounds and populate with current hit test results
        self.last_hit_ancestor_bounds.clear();
        for hit in &hits {
            self.last_hit_ancestor_bounds.insert(
                hit.node.to_raw(),
                (
                    hit.bounds_x,
                    hit.bounds_y,
                    hit.bounds_width,
                    hit.bounds_height,
                ),
            );
            // Also store ancestor bounds from the hit
            for (key, value) in &hit.ancestor_bounds {
                self.last_hit_ancestor_bounds.insert(*key, *value);
            }
        }

        // Update last_hit values from topmost hit (if any) for backwards compatibility
        if let Some(topmost) = hits.last() {
            self.last_hit_local_x = topmost.local_x;
            self.last_hit_local_y = topmost.local_y;
            self.last_hit_bounds_x = topmost.bounds_x;
            self.last_hit_bounds_y = topmost.bounds_y;
            self.last_hit_bounds_width = topmost.bounds_width;
            self.last_hit_bounds_height = topmost.bounds_height;
            // Cache the root → leaf ancestor chain so a subsequent
            // cursor-style lookup can reuse it instead of re-walking
            // the tree.
            self.last_hit_chain.clear();
            self.last_hit_chain.extend_from_slice(&topmost.ancestors);
            // Whether the deepest hit truly has no children — only
            // then is the cursor-inside-last-leaf short-circuit safe.
            // A leaf with children (e.g. button-div with a text
            // child) could see the cursor cross into a child without
            // leaving the parent's bounds; the child would become a
            // new deepest hit needing POINTER_ENTER. We capture each
            // child's absolute bounds (populated by hit_test_node at
            // the leaf level) so the short-circuit can additionally
            // require the cursor not be in any child.
            self.last_leaf_has_no_children = topmost.leaf_children_bounds.is_empty();
            self.last_leaf_children_bounds.clear();
            self.last_leaf_children_bounds
                .extend_from_slice(&topmost.leaf_children_bounds);
        } else {
            self.last_hit_chain.clear();
            self.last_leaf_has_no_children = false;
            self.last_leaf_children_bounds.clear();
        }

        // Resolve current hover set to stable ids so the diff against
        // `self.hovered` survives any subtree rebuild that fired
        // between this mouse-move and the previous. Build a side-map
        // back to layout ids so we can dispatch to the right node.
        // Nodes without a stable id (anonymous layout-only nodes
        // inserted by the pipeline) drop out of hover tracking — they
        // wouldn't have stable handlers anyway.
        let mut current_hovered_layout: std::collections::HashMap<
            crate::tree::StableNodeId,
            LayoutNodeId,
        > = std::collections::HashMap::with_capacity(current_hovered.len());
        for &lid in &current_hovered {
            if let Some(stable) = tree.stable_id(lid) {
                current_hovered_layout.insert(stable, lid);
            }
        }
        let current_hovered_stable: HashSet<crate::tree::StableNodeId> =
            current_hovered_layout.keys().copied().collect();

        // Elements that were hovered but no longer are -> POINTER_LEAVE
        let left: Vec<_> = self
            .hovered
            .difference(&current_hovered_stable)
            .copied()
            .collect();
        for stable in left {
            // Resolve the (now potentially recycled) stable id to a
            // live layout id for dispatch. None means the node was
            // removed entirely by a rebuild — LEAVE has no live target
            // to fire on, so skip.
            if let Some(node) = tree.layout_id(stable) {
                self.emit_event(node, event_types::POINTER_LEAVE);
                events.push((node, event_types::POINTER_LEAVE));

                // Record hover leave event (only if recording is enabled)
                #[cfg(feature = "recorder")]
                if recorder_bridge::is_recording() {
                    recorder_bridge::record_event(RecorderEventData::HoverLeave {
                        element_id: format!("{:?}", node),
                        x,
                        y,
                    });
                }
            }
        }

        // Elements that are newly hovered -> POINTER_ENTER
        let entered: Vec<_> = current_hovered_stable
            .difference(&self.hovered)
            .copied()
            .collect();
        for stable in entered {
            let node = current_hovered_layout[&stable];
            self.emit_event(node, event_types::POINTER_ENTER);
            events.push((node, event_types::POINTER_ENTER));

            // Record hover enter event (only if recording is enabled)
            #[cfg(feature = "recorder")]
            if recorder_bridge::is_recording() {
                recorder_bridge::record_event(RecorderEventData::HoverEnter {
                    element_id: format!("{:?}", node),
                    x,
                    y,
                });
            }
        }

        // POINTER_MOVE on every hovered node — but only emit to
        // nodes that actually have a `POINTER_MOVE` handler. The
        // previous unconditional emit pushed ~5-10 events per
        // mouse-move (one per ancestor in the hover chain), each
        // round-tripping through `pending_events` and
        // `dispatch_event_full` to ultimately find no handler.
        // At a Magic Mouse's ~120 Hz move rate × cn_demo's hover
        // chain, that ate ~20 % CPU during cursor wiggles for no
        // visible work — `state_changed` and pointer_query handle
        // the legitimate cases.
        //
        // Per-node check is cheap: `has_handler` is a single
        // HashMap probe per node, and we already walked the
        // hover chain to compute the set.
        let registry = tree.handler_registry();
        for node in &current_hovered {
            if let Some(stable) = tree.stable_id(*node) {
                if registry.has_handler(stable, event_types::POINTER_MOVE) {
                    self.emit_event(*node, event_types::POINTER_MOVE);
                    events.push((*node, event_types::POINTER_MOVE));
                }
            }
        }

        // Record mouse move event (only if recording is enabled)
        #[cfg(feature = "recorder")]
        if recorder_bridge::is_recording() {
            let hover_element = hits.last().map(|h| format!("{:?}", h.node));
            recorder_bridge::record_event(RecorderEventData::MouseMove {
                x,
                y,
                hover_element,
            });
        }

        self.hovered = current_hovered_stable;

        // Drag detection: if we have a pressed target and moved, emit DRAG.
        // `pressed_target` is a stable id; resolve to the live layout id
        // for this frame before emitting so a rebuild between mouse-down
        // and the move doesn't drop the drag.
        if let Some(stable_target) = self.pressed_target {
            // Update drag delta
            self.drag_delta_x = x - self.drag_start_x;
            self.drag_delta_y = y - self.drag_start_y;

            // Start dragging if we've moved more than a small threshold
            const DRAG_THRESHOLD: f32 = 3.0;
            let delta_exceeds = self.drag_delta_x.abs() > DRAG_THRESHOLD
                || self.drag_delta_y.abs() > DRAG_THRESHOLD;

            tracing::debug!(
                "Drag check: stable_target={:?}, delta=({:.1}, {:.1}), threshold_exceeded={}, is_dragging={}",
                stable_target,
                self.drag_delta_x,
                self.drag_delta_y,
                delta_exceeds,
                self.is_dragging
            );

            if !self.is_dragging && delta_exceeds {
                self.is_dragging = true;
                tracing::debug!(
                    "DRAG started: stable_target={:?}, delta=({:.1}, {:.1})",
                    stable_target,
                    self.drag_delta_x,
                    self.drag_delta_y
                );
            }

            // Emit DRAG event to the pressed target (resolved per-frame)
            if self.is_dragging {
                if let Some(target) = tree.layout_id(stable_target) {
                    tracing::debug!(
                        "Emitting DRAG to {:?}, delta=({:.1}, {:.1})",
                        target,
                        self.drag_delta_x,
                        self.drag_delta_y
                    );
                    self.emit_event(target, event_types::DRAG);
                    events.push((target, event_types::DRAG));
                }

                // Bubble DRAG to ancestors (skip the target itself —
                // last entry of ancestors). Collect first to avoid
                // borrow conflict.
                let ancestors: Vec<_> = self
                    .pressed_ancestors
                    .iter()
                    .rev()
                    .skip(1)
                    .copied()
                    .collect();
                for stable_ancestor in ancestors {
                    if let Some(ancestor) = tree.layout_id(stable_ancestor) {
                        self.emit_event(ancestor, event_types::DRAG);
                        events.push((ancestor, event_types::DRAG));
                    }
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
        button: MouseButton,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        // Store button info for EventContext population
        let button_code = match button {
            MouseButton::Left => 0u8,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(_) => 5,
        };
        crate::event_handler::set_current_mouse_button(button_code);

        // Initialize drag tracking
        self.drag_start_x = x;
        self.drag_start_y = y;
        self.drag_delta_x = 0.0;
        self.drag_delta_y = 0.0;
        self.is_dragging = false;

        let mut events = Vec::new();

        // Hit test for the topmost element
        if let Some(hit) = self.hit_test(tree, x, y) {
            // Record the press as a stable id so a rebuild between
            // mouse-down and mouse-up (or a mid-drag rebuild) doesn't
            // strand the release on a recycled slotmap key.
            self.pressed_target = tree.stable_id(hit.node);
            // Store ancestors for bubbling on release, also stable.
            self.pressed_ancestors = hit
                .ancestors
                .iter()
                .filter_map(|&n| tree.stable_id(n))
                .collect();
            // Store local coordinates and bounds for event handlers
            self.last_hit_local_x = hit.local_x;
            self.last_hit_local_y = hit.local_y;
            self.last_hit_bounds_x = hit.bounds_x;
            self.last_hit_bounds_y = hit.bounds_y;
            self.last_hit_bounds_width = hit.bounds_width;
            self.last_hit_bounds_height = hit.bounds_height;
            // Store ancestor bounds for proper bounds lookup during event bubbling
            self.last_hit_ancestor_bounds = hit.ancestor_bounds.clone();

            // Fire click-outside handlers (dismiss dropdowns etc. when clicking elsewhere)
            // Resolve ancestor node IDs to element IDs for matching
            let ancestor_ids: Vec<String> = hit
                .ancestors
                .iter()
                .filter_map(|&node| tree.element_registry().get_id(node))
                .collect();
            crate::click_outside::fire_click_outside(&ancestor_ids);

            // Record mouse down event (only if recording is enabled)
            #[cfg(feature = "recorder")]
            if recorder_bridge::is_recording() {
                recorder_bridge::record_event(RecorderEventData::MouseDown {
                    x,
                    y,
                    button: RecorderMouseButton::from(button),
                    target_element: Some(format!("{:?}", hit.node)),
                });
            }

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
            // Clicked outside any element - fire all click-outside handlers
            crate::click_outside::fire_click_outside(&[] as &[String]);
            // Clear focus
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
        tree: &RenderTree,
        x: f32,
        y: f32,
        button: MouseButton,
    ) -> Vec<(LayoutNodeId, u32)> {
        self.mouse_x = x;
        self.mouse_y = y;

        let mut events = Vec::new();

        // Check if we were dragging
        let was_dragging = self.is_dragging;

        tracing::debug!(
            "on_mouse_up: pressed_target_stable={:?}, was_dragging={}, pos=({:.1}, {:.1})",
            self.pressed_target,
            was_dragging,
            x,
            y
        );

        // Release goes to the element where press started. Resolve
        // the stable id to the live layout id for this frame — if
        // the press target's subtree was removed by an intervening
        // rebuild, `layout_id` returns None and we silently drop
        // the release (the unmount-time UNMOUNT event was already
        // emitted by `on_unmount`).
        if let Some(stable_target) = self.pressed_target.take() {
            let target = tree.layout_id(stable_target);

            if let Some(target) = target {
                // If we were dragging, emit DRAG_END before POINTER_UP
                if was_dragging {
                    self.emit_event(target, event_types::DRAG_END);
                    events.push((target, event_types::DRAG_END));
                }

                // Record mouse up event (only if recording is enabled)
                #[cfg(feature = "recorder")]
                if recorder_bridge::is_recording() {
                    recorder_bridge::record_event(RecorderEventData::MouseUp {
                        x,
                        y,
                        button: RecorderMouseButton::from(button),
                        target_element: Some(format!("{:?}", target)),
                    });

                    // If not dragging, also record a click event
                    if !was_dragging {
                        recorder_bridge::record_event(RecorderEventData::Click {
                            x,
                            y,
                            button: RecorderMouseButton::from(button),
                            target_element: Some(format!("{:?}", target)),
                        });
                    }
                }

                // Emit to the target first
                tracing::debug!("on_mouse_up: emitting POINTER_UP to target {:?}", target);
                self.emit_event(target, event_types::POINTER_UP);
                events.push((target, event_types::POINTER_UP));
            } else {
                tracing::debug!(
                    "on_mouse_up: press target {:?} no longer in tree (removed mid-press) — dropping release",
                    stable_target
                );
            }

            // Bubble through ancestors (stored from on_mouse_down).
            // Each ancestor is resolved independently — some may
            // have survived the rebuild even if the press target
            // didn't, so we still want to deliver POINTER_UP to
            // those.
            let ancestors = std::mem::take(&mut self.pressed_ancestors);
            for &stable_ancestor in ancestors.iter().rev().skip(1) {
                if let Some(ancestor) = tree.layout_id(stable_ancestor) {
                    if was_dragging {
                        self.emit_event(ancestor, event_types::DRAG_END);
                        events.push((ancestor, event_types::DRAG_END));
                    }
                    self.emit_event(ancestor, event_types::POINTER_UP);
                    events.push((ancestor, event_types::POINTER_UP));
                }
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
    pub fn on_mouse_leave(&mut self, tree: &RenderTree) -> Vec<(LayoutNodeId, u32)> {
        let mut events = Vec::new();

        // If we were pressing/dragging, emit POINTER_UP to clean up
        // state. `pressed_target` is a stable id; resolve to the
        // current frame's layout id (None if the target's subtree
        // was removed by a rebuild — we still clear the press).
        if let Some(stable_target) = self.pressed_target.take() {
            if let Some(target) = tree.layout_id(stable_target) {
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
            } else {
                tracing::debug!(
                    "on_mouse_leave: press target {:?} no longer in tree — dropping POINTER_UP",
                    stable_target
                );
            }

            // Bubble through ancestors — resolve each independently
            let ancestors = std::mem::take(&mut self.pressed_ancestors);
            for &stable_ancestor in ancestors.iter().rev().skip(1) {
                if let Some(ancestor) = tree.layout_id(stable_ancestor) {
                    if self.is_dragging {
                        self.emit_event(ancestor, event_types::DRAG_END);
                        events.push((ancestor, event_types::DRAG_END));
                    }
                    self.emit_event(ancestor, event_types::POINTER_UP);
                    events.push((ancestor, event_types::POINTER_UP));
                }
            }

            // Reset drag state
            self.is_dragging = false;
            self.drag_delta_x = 0.0;
            self.drag_delta_y = 0.0;
        }

        // Emit POINTER_LEAVE to all hovered elements (resolved from
        // their stable ids to current LayoutNodeIds; nodes that
        // disappeared since the last mouse-move have no live target
        // and are silently skipped).
        let stable_nodes: Vec<_> = self.hovered.iter().copied().collect();
        for stable in stable_nodes {
            if let Some(node) = tree.layout_id(stable) {
                self.emit_event(node, event_types::POINTER_LEAVE);
                events.push((node, event_types::POINTER_LEAVE));
            }
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
    pub fn on_key_down(&mut self, key_code: u32) -> Option<(LayoutNodeId, u32)> {
        if let Some(focused) = self.focused {
            // Record key down event (only if recording is enabled)
            #[cfg(feature = "recorder")]
            if recorder_bridge::is_recording() {
                recorder_bridge::record_event(RecorderEventData::KeyDown {
                    key_code,
                    focused_element: Some(format!("{:?}", focused)),
                });
            }

            self.emit_event(focused, event_types::KEY_DOWN);
            Some((focused, event_types::KEY_DOWN))
        } else {
            None
        }
    }

    /// Handle key release
    ///
    /// Emits KEY_UP to the focused element.
    pub fn on_key_up(&mut self, key_code: u32) -> Option<(LayoutNodeId, u32)> {
        if let Some(focused) = self.focused {
            // Record key up event (only if recording is enabled)
            #[cfg(feature = "recorder")]
            if recorder_bridge::is_recording() {
                recorder_bridge::record_event(RecorderEventData::KeyUp {
                    key_code,
                    focused_element: Some(format!("{:?}", focused)),
                });
            }

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
    pub fn on_text_input(&mut self, ch: char) -> Option<(LayoutNodeId, u32)> {
        if let Some(focused) = self.focused {
            // Record text input event (only if recording is enabled)
            #[cfg(feature = "recorder")]
            if recorder_bridge::is_recording() {
                recorder_bridge::record_event(RecorderEventData::TextInput {
                    text: ch.to_string(),
                    focused_element: Some(format!("{:?}", focused)),
                });
            }

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
            // Record scroll event (only if recording is enabled)
            #[cfg(feature = "recorder")]
            if recorder_bridge::is_recording() {
                recorder_bridge::record_event(RecorderEventData::Scroll {
                    x: self.mouse_x,
                    y: self.mouse_y,
                    delta_x,
                    delta_y,
                    target_element: Some(format!("{:?}", hit.node)),
                });
            }

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
    pub fn on_unmount(&mut self, tree: &RenderTree, node: LayoutNodeId) {
        self.emit_event(node, event_types::UNMOUNT);

        // Clear any state associated with this node (hover + pressed
        // both stable-id-keyed; resolve once).
        if let Some(stable) = tree.stable_id(node) {
            self.hovered.remove(&stable);
            if self.pressed_target == Some(stable) {
                self.pressed_target = None;
                self.pressed_ancestors.clear();
            }
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
            self.on_unmount(new_tree, *node);
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
        let mut ancestors = Vec::new();
        let mut ancestor_bounds = std::collections::HashMap::new();
        self.hit_test_node(
            tree,
            root,
            x,
            y,
            (0.0, 0.0),
            &mut ancestors,
            &mut ancestor_bounds,
            (0.0, 0.0),
            None,
        )
    }

    /// Hit test to find all elements at a point
    ///
    /// Returns all elements that contain the point, from root to leaf.
    pub fn hit_test_all(&self, tree: &RenderTree, x: f32, y: f32) -> Vec<HitTestResult> {
        let mut results = Vec::new();
        if let Some(root) = tree.root() {
            self.hit_test_node_all(
                tree,
                root,
                x,
                y,
                (0.0, 0.0),
                Vec::new(),
                std::collections::HashMap::new(),
                &mut results,
                (0.0, 0.0),
                None,
            );
        }
        results
    }

    /// Recursive hit test for a single node
    #[allow(clippy::too_many_arguments)]
    fn hit_test_node(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        x: f32,
        y: f32,
        parent_offset: (f32, f32),
        ancestors: &mut Vec<LayoutNodeId>,
        ancestor_bounds: &mut std::collections::HashMap<u64, (f32, f32, f32, f32)>,
        cumulative_scroll: (f32, f32),
        active_cull_viewport: Option<(f32, f32, f32, f32)>,
    ) -> Option<HitTestResult> {
        let bounds = tree.layout().get_bounds(node, parent_offset)?;

        // Check if point is within bounds
        let in_bounds = self.point_in_bounds(x, y, &bounds);

        // If point is outside bounds, check whether this node clips content.
        // If it does NOT clip (overflow: visible), children may extend beyond
        // the parent bounds and still be interactive — so we must test them.
        // If it DOES clip, children outside bounds are invisible → skip.
        // Track whether we're outside a clipping parent — if so, only foreground hits count
        let mut clipped_non_foreground = false;
        if !in_bounds {
            let clips = tree
                .get_render_node(node)
                .map(|n| n.props.clips_content)
                .unwrap_or(true); // default to clipping if no render node
            if clips {
                // Don't return None immediately — foreground descendants render
                // outside the clip and should still be hittable. We'll only
                // accept foreground results from the children walk below.
                clipped_non_foreground = true;
            }
            // overflow: visible — fall through to test children
        }

        // Debug log for nodes in hit path
        tracing::debug!(
            "hit_test_node: HIT node={:?}, bounds=({:.1}, {:.1}, {:.1}x{:.1}), point=({:.1}, {:.1}), in_bounds={}",
            node,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            x,
            y,
            in_bounds
        );

        ancestors.push(node);
        let bounds_key = node.to_raw();
        // Store this node's bounds for event bubbling
        ancestor_bounds.insert(
            bounds_key,
            (bounds.x, bounds.y, bounds.width, bounds.height),
        );

        // Get scroll offset for this node (if it's a scroll container)
        // Children are rendered at bounds + scroll_offset, so we need to
        // include the scroll offset when hit testing children
        let scroll_offset = tree.get_scroll_offset(node);
        let base_child_offset = (bounds.x + scroll_offset.0, bounds.y + scroll_offset.1);
        let is_scroll_container = tree.is_scroll_container(node);
        let new_cumulative_scroll = if is_scroll_container {
            (scroll_offset.0, scroll_offset.1)
        } else {
            (
                cumulative_scroll.0 + scroll_offset.0,
                cumulative_scroll.1 + scroll_offset.1,
            )
        };
        let cull_viewport = if tree.is_viewport_cull_scroll(node) {
            Some((bounds.x, bounds.y, bounds.width, bounds.height))
        } else {
            active_cull_viewport
        };

        // Check children in reverse order (last child is on top).
        // Foreground-aware: if a child subtree yields a foreground hit,
        // it takes priority over non-foreground hits from siblings that
        // appear later in tree order but visually behind the foreground.
        let children = tree.layout().children(node);
        tracing::trace!(
            "hit_test_node: node={:?}, bounds=({:.1}, {:.1}, {:.1}x{:.1}), children={:?}",
            node,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            children
        );

        let mut best_hit: Option<HitTestResult> = None;

        for child in children.into_iter().rev() {
            let child_render = tree.get_render_node(child);
            let child_is_fixed = child_render.map(|n| n.props.is_fixed).unwrap_or(false);
            let child_is_sticky = child_render.map(|n| n.props.is_sticky).unwrap_or(false);
            let child_is_fg = child_render
                .map(|n| n.props.layer == crate::element::RenderLayer::Foreground)
                .unwrap_or(false);

            let mut child_offset = base_child_offset;

            let child_cumulative = if child_is_fixed {
                // Cancel all accumulated scroll for fixed elements
                child_offset.0 -= new_cumulative_scroll.0;
                child_offset.1 -= new_cumulative_scroll.1;
                (0.0, 0.0)
            } else if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = tree.layout().get_bounds(child, (0.0, 0.0)) {
                        let visual_y = cb.y + new_cumulative_scroll.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            child_offset.1 += correction;
                        }
                    }
                }
                new_cumulative_scroll
            } else {
                new_cumulative_scroll
            };

            // Match the paint path for `scroll().viewport_cull(true)`: once
            // a culling scroll is active, ordinary off-viewport child subtrees
            // are neither painted nor hit-testable. Visible nested scrolls are
            // still walked and remain the first scroll target in the hit chain.
            if let Some((cx, cy, cw, ch)) = cull_viewport {
                if !child_is_fixed && !child_is_sticky && !child_is_fg {
                    if let Some(cb) = tree.layout().get_bounds(child, child_offset) {
                        let intersects = cb.x + cb.width > cx
                            && cb.x < cx + cw
                            && cb.y + cb.height > cy
                            && cb.y < cy + ch;
                        if !intersects {
                            continue;
                        }
                    }
                }
            }

            if let Some(mut result) = self.hit_test_node(
                tree,
                child,
                x,
                y,
                child_offset,
                ancestors,
                ancestor_bounds,
                child_cumulative,
                cull_viewport,
            ) {
                // Mark result as foreground if this child or the result itself is foreground
                if child_is_fg {
                    result.is_foreground = true;
                }

                // When parent clips and point is outside its bounds, only accept
                // foreground hits — non-foreground children are invisible outside
                // the clip rect, but foreground children render above the clip.
                if clipped_non_foreground && !result.is_foreground {
                    continue;
                }

                match &best_hit {
                    None => {
                        best_hit = Some(result);
                    }
                    Some(existing) => {
                        // Foreground results always take priority
                        if result.is_foreground && !existing.is_foreground {
                            best_hit = Some(result);
                        }
                        // Otherwise keep the first hit (topmost in reverse order)
                    }
                }

                // If we already have a foreground result, no need to check more
                if best_hit.as_ref().is_some_and(|h| h.is_foreground) {
                    break;
                }
            }
        }

        let result = if best_hit.is_some() {
            best_hit
        } else if !in_bounds {
            // If the point was outside this node's own bounds, don't return
            // this node as a target (only its overflow children could match).
            None
        } else {
            // No child hit - check if this node has pointer_events_none
            // If so, return None to let the hit fall through to siblings
            let pointer_events_none = tree
                .get_render_node(node)
                .map(|n| n.props.pointer_events_none)
                .unwrap_or(false);

            if pointer_events_none {
                tracing::trace!(
                    "hit_test_node: node={:?} has pointer_events_none, passing through",
                    node
                );
                None
            } else {
                // This node is the target. Clone the traversal state only
                // once, when we actually have a hit to return; the old code
                // cloned it for every sibling probe during mouse movement.
                // Capture direct children's absolute bounds. By
                // construction (this branch fires only when no
                // child's recursion returned `Some`), none of them
                // contained the cursor at hit time. The router uses
                // these for the cursor-inside-last-leaf short-circuit
                // — a wiggle that stays inside this node AND outside
                // every child bound can't have changed the hover
                // set, so the entire dispatch pipeline is safe to
                // skip.
                let mut leaf_children_bounds = Vec::new();
                for child in tree.layout().children(node) {
                    if let Some(cb) = tree.layout().get_bounds(child, base_child_offset) {
                        leaf_children_bounds.push((cb.x, cb.y, cb.width, cb.height));
                    }
                }
                Some(HitTestResult {
                    node,
                    local_x: x - bounds.x,
                    local_y: y - bounds.y,
                    ancestors: ancestors.clone(),
                    bounds_x: bounds.x,
                    bounds_y: bounds.y,
                    bounds_width: bounds.width,
                    bounds_height: bounds.height,
                    ancestor_bounds: ancestor_bounds.clone(),
                    is_foreground: false,
                    leaf_children_bounds,
                })
            }
        };

        ancestor_bounds.remove(&bounds_key);
        ancestors.pop();
        result
    }

    /// Recursive hit test collecting all hits
    #[allow(clippy::too_many_arguments)]
    fn hit_test_node_all(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        x: f32,
        y: f32,
        parent_offset: (f32, f32),
        mut ancestors: Vec<LayoutNodeId>,
        mut ancestor_bounds: std::collections::HashMap<u64, (f32, f32, f32, f32)>,
        results: &mut Vec<HitTestResult>,
        cumulative_scroll: (f32, f32),
        active_cull_viewport: Option<(f32, f32, f32, f32)>,
    ) {
        let Some(bounds) = tree.layout().get_bounds(node, parent_offset) else {
            return;
        };

        // Check if point is within bounds
        let in_bounds = self.point_in_bounds(x, y, &bounds);

        if !in_bounds {
            // If this node clips content, children outside bounds are invisible
            let clips = tree
                .get_render_node(node)
                .map(|n| n.props.clips_content)
                .unwrap_or(true);
            if clips {
                return;
            }
            // overflow: visible — fall through to test children
        }

        ancestors.push(node);
        // Store this node's bounds
        ancestor_bounds.insert(
            node.to_raw(),
            (bounds.x, bounds.y, bounds.width, bounds.height),
        );

        // Check if this node has pointer_events_none - if so, skip adding it to results
        // but still recurse into children (they may capture events)
        let pointer_events_none = tree
            .get_render_node(node)
            .map(|n| n.props.pointer_events_none)
            .unwrap_or(false);

        // Only add this node to results if point is within its own bounds
        if in_bounds && !pointer_events_none {
            results.push(HitTestResult {
                node,
                local_x: x - bounds.x,
                local_y: y - bounds.y,
                ancestors: ancestors.clone(),
                bounds_x: bounds.x,
                bounds_y: bounds.y,
                bounds_width: bounds.width,
                bounds_height: bounds.height,
                ancestor_bounds: ancestor_bounds.clone(),
                is_foreground: false,
                // `hit_test_all` collects every container in the
                // chain, not just the leaf — short-circuit lives on
                // the single-hit path, so the empty Vec is fine.
                leaf_children_bounds: Vec::new(),
            });
        }

        // Get scroll offset for this node (if it's a scroll container)
        // Children are rendered at bounds + scroll_offset, so we need to
        // include the scroll offset when hit testing children
        let scroll_offset = tree.get_scroll_offset(node);
        let base_child_offset = (bounds.x + scroll_offset.0, bounds.y + scroll_offset.1);
        let is_scroll_container = tree.is_scroll_container(node);
        let new_cumulative_scroll = if is_scroll_container {
            (scroll_offset.0, scroll_offset.1)
        } else {
            (
                cumulative_scroll.0 + scroll_offset.0,
                cumulative_scroll.1 + scroll_offset.1,
            )
        };
        let cull_viewport = if tree.is_viewport_cull_scroll(node) {
            Some((bounds.x, bounds.y, bounds.width, bounds.height))
        } else {
            active_cull_viewport
        };

        // Check children
        let children = tree.layout().children(node);

        for child in children {
            let child_render = tree.get_render_node(child);
            let child_is_fixed = child_render.map(|n| n.props.is_fixed).unwrap_or(false);
            let child_is_sticky = child_render.map(|n| n.props.is_sticky).unwrap_or(false);

            let mut child_offset = base_child_offset;

            let child_cumulative = if child_is_fixed {
                child_offset.0 -= new_cumulative_scroll.0;
                child_offset.1 -= new_cumulative_scroll.1;
                (0.0, 0.0)
            } else if child_is_sticky {
                if let Some(threshold) = child_render.and_then(|n| n.props.sticky_top) {
                    if let Some(cb) = tree.layout().get_bounds(child, (0.0, 0.0)) {
                        let visual_y = cb.y + new_cumulative_scroll.1;
                        if visual_y < threshold {
                            let correction = threshold - visual_y;
                            child_offset.1 += correction;
                        }
                    }
                }
                new_cumulative_scroll
            } else {
                new_cumulative_scroll
            };

            if let Some((cx, cy, cw, ch)) = cull_viewport {
                let child_is_fg = child_render
                    .map(|n| n.props.layer == crate::element::RenderLayer::Foreground)
                    .unwrap_or(false);
                if !child_is_fixed && !child_is_sticky && !child_is_fg {
                    if let Some(cb) = tree.layout().get_bounds(child, child_offset) {
                        let intersects = cb.x + cb.width > cx
                            && cb.x < cx + cw
                            && cb.y + cb.height > cy
                            && cb.y < cy + ch;
                        if !intersects {
                            continue;
                        }
                    }
                }
            }

            self.hit_test_node_all(
                tree,
                child,
                x,
                y,
                child_offset,
                ancestors.clone(),
                ancestor_bounds.clone(),
                results,
                child_cumulative,
                cull_viewport,
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

    /// Hit test with overlay occlusion awareness
    ///
    /// This method performs a standard hit test, but also checks if the hit point
    /// is occluded by any visible overlay. If the point is within an overlay's bounds
    /// but the hit target is NOT part of the overlay tree (i.e., it's a background element),
    /// the hit is blocked and returns None.
    ///
    /// This prevents background triggers from receiving hover events when an overlay
    /// (like a hover card) is covering them.
    ///
    /// # Arguments
    /// - `tree` - The render tree to hit test against
    /// - `x`, `y` - The point to test
    /// - `overlay_bounds` - List of visible overlay bounds as (x, y, width, height)
    /// - `overlay_layer_id` - The LayoutNodeId of the overlay layer in the tree
    ///
    /// # Returns
    /// - `Some(HitTestResult)` if a valid hit is found (either in overlay or not occluded)
    /// - `None` if no hit or if hit is occluded by overlay
    pub fn hit_test_with_occlusion(
        &self,
        tree: &RenderTree,
        x: f32,
        y: f32,
        overlay_bounds: &[(f32, f32, f32, f32)],
        overlay_layer_id: Option<LayoutNodeId>,
    ) -> Option<HitTestResult> {
        // Perform standard hit test first
        let hit = self.hit_test(tree, x, y)?;

        // Check if the point is inside any overlay bounds
        let in_overlay_bounds = overlay_bounds
            .iter()
            .any(|&(ox, oy, ow, oh)| x >= ox && x < ox + ow && y >= oy && y < oy + oh);

        // If point is not in any overlay bounds, the hit is valid
        if !in_overlay_bounds {
            return Some(hit);
        }

        // Point is in overlay bounds - check if hit target is part of the overlay layer
        if let Some(overlay_id) = overlay_layer_id {
            // If the hit ancestors include the overlay layer, the hit is valid
            // (the user clicked on something inside the overlay)
            if hit.ancestors.contains(&overlay_id) {
                return Some(hit);
            }

            // Hit target is NOT in overlay layer, but point is in overlay bounds
            // This means the background element is being hovered through the overlay
            // Block the hit
            tracing::debug!(
                "hit_test_with_occlusion: blocking hit on {:?} - occluded by overlay at ({:.1}, {:.1})",
                hit.node,
                x,
                y
            );
            return None;
        }

        // No overlay layer specified, return the hit as-is
        Some(hit)
    }

    /// Hit test all elements with overlay occlusion awareness
    ///
    /// Similar to `hit_test_with_occlusion`, but returns all hits that pass
    /// the occlusion check. Elements that are occluded by overlays are filtered out.
    pub fn hit_test_all_with_occlusion(
        &self,
        tree: &RenderTree,
        x: f32,
        y: f32,
        overlay_bounds: &[(f32, f32, f32, f32)],
        overlay_layer_id: Option<LayoutNodeId>,
    ) -> Vec<HitTestResult> {
        let all_hits = self.hit_test_all(tree, x, y);

        // Check if the point is inside any overlay bounds
        let in_overlay_bounds = overlay_bounds
            .iter()
            .any(|&(ox, oy, ow, oh)| x >= ox && x < ox + ow && y >= oy && y < oy + oh);

        // If point is not in any overlay bounds, all hits are valid
        if !in_overlay_bounds {
            return all_hits;
        }

        // Point is in overlay bounds - filter out hits that are NOT in the overlay layer
        // BUT also keep foreground elements: they render on top of everything
        // (including overlays) and should receive hover events regardless.
        if let Some(overlay_id) = overlay_layer_id {
            let result: Vec<_> = all_hits
                .into_iter()
                .filter(|hit| {
                    // Keep nodes in the overlay subtree
                    if hit.ancestors.contains(&overlay_id) {
                        return true;
                    }
                    // Keep foreground elements (e.g., absolutely positioned dropdowns
                    // rendered in foreground pass)
                    if let Some(render_node) = tree.get_render_node(hit.node) {
                        if render_node.props.layer == crate::element::RenderLayer::Foreground {
                            return true;
                        }
                    }
                    // Also keep nodes whose ancestors include a foreground element
                    // (children of foreground containers like dropdown items)
                    for &ancestor in &hit.ancestors {
                        if let Some(render_node) = tree.get_render_node(ancestor) {
                            if render_node.props.layer == crate::element::RenderLayer::Foreground {
                                return true;
                            }
                        }
                    }
                    false
                })
                .collect();

            // Log when we're in overlay bounds but filtering
            if result.is_empty() {
                tracing::debug!(
                    "hit_test_with_occlusion: in overlay bounds at ({:.1}, {:.1}), overlay_id={:?}, but no hits pass filter",
                    x,
                    y,
                    overlay_id
                );
            }
            result
        } else {
            all_hits
        }
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
    fn test_hit_test_nested_scroll_after_outer_scroll_offset() {
        let ui = scroll()
            .id("outer")
            .w(200.0)
            .h(100.0)
            .viewport_cull(true)
            .child(
                div()
                    .w_full()
                    .flex_col()
                    .child(div().id("spacer").w_full().h(120.0))
                    .child(
                        scroll()
                            .id("inner")
                            .w_full()
                            .h(80.0)
                            .child(div().id("inner-content").w_full().h(160.0)),
                    ),
            );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 100.0);

        let outer = tree.query_by_id("outer").expect("outer scroll");
        let inner = tree.query_by_id("inner").expect("inner scroll");
        tree.dispatch_scroll_chain(outer, &[outer], 50.0, 50.0, 0.0, -120.0);

        let router = EventRouter::new();
        let hit = router
            .hit_test(&tree, 50.0, 40.0)
            .expect("visible inner scroll should be hit");

        assert!(hit.ancestors.contains(&inner));

        let first_scroll = std::iter::once(hit.node)
            .chain(
                hit.ancestors
                    .iter()
                    .rev()
                    .copied()
                    .filter(|ancestor| *ancestor != hit.node),
            )
            .find(|node| tree.is_scroll_container(*node));
        assert_eq!(first_scroll, Some(inner));
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
        assert!(
            captured
                .iter()
                .any(|(_, e)| *e == event_types::POINTER_ENTER)
        );
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

    /// Regression: pressed_target survives a tree rebuild between
    /// POINTER_DOWN and POINTER_UP. Pre-fix `pressed_target` was a
    /// `LayoutNodeId` — the slotmap version bumped during the rebuild,
    /// the cached id resolved to None in `dispatch_event_full`, and the
    /// release event was silently dropped. Symptom on the user side
    /// was `on_click` closures never firing on `cn::button`,
    /// `cn::sidebar`, and the cn_demo Reset Animation button after the
    /// POINTER_DOWN handler ran any state mutation. The fix switched
    /// `pressed_target` to `StableNodeId`, which survives the rebuild
    /// and resolves to the new LayoutNodeId at release time.
    ///
    /// Setup: root → middle → leaf. Press lands on `leaf` (the
    /// innermost hit). Queue a rebuild on `middle` so its children
    /// (including `leaf`) get fresh LayoutNodeIds. Then release —
    /// POINTER_UP must still reach the press target's *new* id, via
    /// stable_id resolution.
    #[test]
    fn pressed_target_survives_rebuild_between_down_and_up() {
        // Serialize against other tests that touch the global
        // PENDING_SUBTREE_REBUILDS queue (see
        // `PENDING_QUEUE_TEST_LOCK` docs). Parallel slotmap
        // `LayoutNodeId` collisions otherwise let an unrelated
        // test's rebuild supersede ours, and
        // `process_pending_subtree_rebuilds` returns false.
        let _guard = crate::stateful::PENDING_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Clear any leftover pending rebuilds from prior tests so we
        // only process the one this test queues.
        let _ = crate::stateful::take_pending_subtree_rebuilds();

        let ui = div()
            .w(400.0)
            .h(300.0)
            .child(div().w(200.0).h(200.0).child(div().w(100.0).h(100.0)));
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 300.0);

        let middle_id = tree.layout_tree.children(tree.root().unwrap())[0];
        let leaf_id_before = tree.layout_tree.children(middle_id)[0];
        let leaf_stable = tree.stable_id(leaf_id_before).unwrap();

        let events: Rc<RefCell<Vec<(LayoutNodeId, u32)>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);

        let mut router = EventRouter::new();
        router.set_event_callback(move |node, event| {
            events_clone.borrow_mut().push((node, event));
        });

        // Press on the leaf (innermost hit at 50,50).
        router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);
        assert_eq!(router.pressed_target_stable(), Some(leaf_stable));

        // Rebuild `middle`'s subtree. `process_pending_subtree_rebuilds`
        // removes and re-creates middle's children, so the slotmap
        // version bumps and `leaf_id_before` is now stale. Note: we
        // queue against `middle`, not the leaf — the rebuild root
        // itself keeps its id; only its descendants get re-keyed.
        crate::stateful::queue_subtree_rebuild(
            middle_id,
            div().w(200.0).h(200.0).child(div().w(100.0).h(100.0)),
        );
        assert!(tree.process_pending_subtree_rebuilds());
        assert!(
            tree.stable_id(leaf_id_before).is_none(),
            "old leaf LayoutNodeId should be stale after rebuild — if this fails, the slotmap key didn't bump and the regression test no longer exercises the staleness path"
        );

        // The press target's stable id should still resolve, to a new
        // LayoutNodeId. That's what `dispatch_event_full` does
        // internally on POINTER_UP.
        let leaf_id_after = tree
            .layout_id(leaf_stable)
            .expect("press target's stable id must survive the rebuild");
        assert_ne!(
            leaf_id_after, leaf_id_before,
            "post-rebuild LayoutNodeId must differ from pre-rebuild id"
        );

        // Release — POINTER_UP must reach the new layout id, not the stale one.
        router.on_mouse_up(&tree, 55.0, 55.0, MouseButton::Left);

        let captured = events.borrow();
        let up_events: Vec<_> = captured
            .iter()
            .filter(|(_, ty)| *ty == event_types::POINTER_UP)
            .collect();
        assert!(
            !up_events.is_empty(),
            "POINTER_UP must fire across the rebuild"
        );
        assert!(
            up_events.iter().any(|(n, _)| *n == leaf_id_after),
            "POINTER_UP must reach the press target's post-rebuild LayoutNodeId, not the stale one"
        );

        let _ = crate::stateful::take_pending_subtree_rebuilds();
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
            assert!(
                captured
                    .iter()
                    .any(|(n, e)| *n == first_focused && *e == event_types::FOCUS)
            );
        }

        // Click second child - should blur first and focus second
        router.on_mouse_down(&tree, 50.0, 150.0, MouseButton::Left);

        {
            let captured = events.borrow();
            // Should have BLUR for first element
            assert!(
                captured
                    .iter()
                    .any(|(n, e)| *n == first_focused && *e == event_types::BLUR)
            );
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
        router.on_unmount(&tree, focused.unwrap());

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
