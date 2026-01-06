//! Element handle for programmatic element manipulation

use std::sync::Arc;

use blinc_core::context_state::MotionAnimationState;
use blinc_core::BlincContextState;

use crate::element::{ElementBounds, RenderProps};
use crate::tree::LayoutNodeId;

use super::registry::{ElementRegistry, OnReadyCallback};
use super::ScrollOptions;

/// Handle to a queried element for programmatic manipulation
///
/// Returned by `ctx.query("element-id")` for element manipulation.
/// The handle can be created even before the element exists in the tree,
/// allowing operations like `on_ready` to be registered early.
#[derive(Clone)]
pub struct ElementHandle<T = ()> {
    /// The string ID used to query this element
    string_id: String,
    /// Cached node_id (may be default if element doesn't exist yet)
    node_id: LayoutNodeId,
    registry: Arc<ElementRegistry>,
    /// Typed element data (if available)
    _marker: std::marker::PhantomData<T>,
}

impl<T> ElementHandle<T> {
    /// Create a new element handle from a string ID
    ///
    /// The handle is valid even if the element doesn't exist yet.
    /// Operations like `on_ready` will work and fire when the element is laid out.
    pub fn new(string_id: impl Into<String>, registry: Arc<ElementRegistry>) -> Self {
        let string_id = string_id.into();
        let node_id = registry.get(&string_id).unwrap_or_default();
        Self {
            string_id,
            node_id,
            registry,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the underlying layout node ID
    ///
    /// Returns a default ID if the element doesn't exist yet.
    pub fn node_id(&self) -> LayoutNodeId {
        // Refresh from registry in case element was created after handle
        self.registry.get(&self.string_id).unwrap_or(self.node_id)
    }

    /// Get the string ID of this element
    pub fn id(&self) -> &str {
        &self.string_id
    }

    /// Check if the element currently exists in the tree
    pub fn exists(&self) -> bool {
        self.registry.get(&self.string_id).is_some()
    }

    // =========================================================================
    // Layout & Visibility
    // =========================================================================

    /// Get the computed bounds of this element
    ///
    /// Returns None if layout hasn't been computed yet or the element doesn't exist.
    pub fn bounds(&self) -> Option<ElementBounds> {
        // Get bounds from the registry cache (populated by RenderTree after layout)
        let bounds = self.registry.get_bounds(&self.string_id)?;
        Some(ElementBounds::new(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        ))
    }

    /// Check if this element is visible in the viewport
    ///
    /// An element is visible if its bounds intersect with the window viewport.
    /// This is a simple viewport check - does not account for scroll container clipping.
    pub fn is_visible(&self) -> bool {
        let Some(bounds) = self.registry.get_bounds(&self.string_id) else {
            return false;
        };

        // Get viewport size from BlincContextState
        if let Some(ctx) = BlincContextState::try_get() {
            let (vw, vh) = ctx.viewport_size();
            // Check if element bounds intersect viewport
            bounds.x < vw
                && bounds.x + bounds.width > 0.0
                && bounds.y < vh
                && bounds.y + bounds.height > 0.0
        } else {
            // No context state, assume visible
            true
        }
    }

    // =========================================================================
    // Tree Traversal
    // =========================================================================

    /// Get the parent element handle
    pub fn parent(&self) -> Option<ElementHandle<()>> {
        let current_node_id = self.node_id();
        let parent_node_id = self.registry.get_parent(current_node_id)?;
        let parent_string_id = self.registry.get_id(parent_node_id)?;
        Some(ElementHandle::new(parent_string_id, self.registry.clone()))
    }

    /// Get all ancestors (immediate parent to root)
    pub fn ancestors(&self) -> impl Iterator<Item = ElementHandle<()>> {
        let current_node_id = self.node_id();
        let ancestors = self.registry.ancestors(current_node_id);
        let registry = self.registry.clone();
        ancestors.into_iter().filter_map(move |id| {
            let string_id = registry.get_id(id)?;
            Some(ElementHandle::new(string_id, registry.clone()))
        })
    }

    // =========================================================================
    // Navigation (scroll, focus)
    // =========================================================================

    /// Scroll this element into view using default options
    pub fn scroll_into_view(&self) {
        self.scroll_into_view_with(ScrollOptions::default());
    }

    /// Scroll this element into view with custom options
    pub fn scroll_into_view_with(&self, _options: ScrollOptions) {
        // Use BlincContextState callback to scroll the element
        if let Some(ctx) = BlincContextState::try_get() {
            ctx.scroll_element_into_view(&self.string_id);
        }
    }

    /// Focus this element
    ///
    /// For focusable elements like TextInput, this sets keyboard focus.
    /// For other elements, this updates the EventRouter's focus state.
    pub fn focus(&self) {
        if let Some(ctx) = BlincContextState::try_get() {
            ctx.set_focus(Some(&self.string_id));
        }
    }

    /// Remove focus from this element
    pub fn blur(&self) {
        if let Some(ctx) = BlincContextState::try_get() {
            // Only blur if this element is currently focused
            if ctx.is_focused(&self.string_id) {
                ctx.set_focus(None);
            }
        }
    }

    /// Check if this element is currently focused
    pub fn is_focused(&self) -> bool {
        BlincContextState::try_get()
            .map(|ctx| ctx.is_focused(&self.string_id))
            .unwrap_or(false)
    }

    // =========================================================================
    // Signal Operations
    // =========================================================================

    /// Emit a signal to trigger reactive updates
    ///
    /// This notifies stateful elements that depend on this signal, triggering
    /// only the affected subtree rebuilds through the reactive system.
    ///
    /// Note: For typed signal updates, use `State::set()` directly which
    /// automatically triggers dependent updates.
    pub fn emit_signal(&self, signal_id: blinc_core::SignalId) {
        if let Some(ctx) = BlincContextState::try_get() {
            // Notify stateful elements via the callback - this triggers
            // targeted subtree rebuilds, not a full UI rebuild
            ctx.notify_stateful_deps(&[signal_id]);
        }
    }

    /// Mark this element as dirty, forcing a rebuild
    ///
    /// This triggers a UI rebuild. The hash-based diffing system will
    /// determine what actually needs to be updated.
    pub fn mark_dirty(&self) {
        if let Some(ctx) = BlincContextState::try_get() {
            ctx.request_rebuild();
        }
    }

    /// Mark this element's subtree as dirty with new children
    ///
    /// This queues an explicit subtree rebuild with the provided new children.
    /// Use this for more efficient updates when you know exactly what the
    /// new children should be.
    pub fn mark_dirty_subtree(&self, new_children: crate::div::Div) {
        if let Some(node_id) = self.registry.get(&self.string_id) {
            crate::stateful::queue_subtree_rebuild(node_id, new_children);
        }
    }

    /// Mark this element as visually dirty with new render props
    ///
    /// This queues a visual-only update that **skips layout recomputation**.
    /// Use this for changes to background, opacity, shadows, transforms, etc.
    ///
    /// This is the most efficient update method when you only need to change
    /// visual properties without affecting layout.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Change background color without triggering layout
    /// ctx.query("my-button").mark_visual_dirty(
    ///     RenderProps::default().with_background(Color::RED.into())
    /// );
    /// ```
    pub fn mark_visual_dirty(&self, props: RenderProps) {
        if let Some(node_id) = self.registry.get(&self.string_id) {
            crate::stateful::queue_prop_update(node_id, props);
        }
    }

    // =========================================================================
    // Event Simulation
    // =========================================================================

    /// Simulate a click event on this element
    pub fn click(&self) {
        self.dispatch_event(ElementEvent::Click { x: 0.0, y: 0.0 });
    }

    /// Simulate a click at specific coordinates within the element
    pub fn click_at(&self, x: f32, y: f32) {
        self.dispatch_event(ElementEvent::Click { x, y });
    }

    /// Simulate hover enter or leave
    pub fn hover(&self, enter: bool) {
        if enter {
            self.dispatch_event(ElementEvent::MouseEnter);
        } else {
            self.dispatch_event(ElementEvent::MouseLeave);
        }
    }

    /// Dispatch a custom event to this element
    pub fn dispatch_event(&self, _event: ElementEvent) {
        // TODO: Wire up to EventRouter/HandlerRegistry
        // This needs access to the event dispatch system
    }

    // =========================================================================
    // On-Ready Callback
    // =========================================================================

    /// Register an on_ready callback for this element
    ///
    /// The callback will be invoked once after the element's first successful
    /// layout computation. The callback receives the element's computed bounds.
    ///
    /// This works even if the element doesn't exist yet - the callback will
    /// fire when the element is first laid out. If the element already exists
    /// and has been laid out, the callback fires on the next layout pass.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Query element and register callback
    /// ctx.query("progress-bar").on_ready(|bounds| {
    ///     progress_anim.lock().unwrap().set_target(bounds.width * 0.75);
    /// });
    /// ```
    pub fn on_ready<F>(&self, callback: F)
    where
        F: Fn(ElementBounds) + Send + Sync + 'static,
    {
        self.registry
            .register_on_ready_for_id(&self.string_id, Arc::new(callback));
    }

    /// Register an on_ready callback (Arc version for shared callbacks)
    pub fn on_ready_arc(&self, callback: OnReadyCallback) {
        self.registry
            .register_on_ready_for_id(&self.string_id, callback);
    }
}

/// Events that can be programmatically dispatched to elements
#[derive(Debug, Clone)]
pub enum ElementEvent {
    /// Mouse click at local coordinates
    Click { x: f32, y: f32 },
    /// Mouse entered element bounds
    MouseEnter,
    /// Mouse left element bounds
    MouseLeave,
    /// Element received focus
    Focus,
    /// Element lost focus
    Blur,
    /// Key pressed while focused
    KeyDown {
        key: u32,      // Key code
        modifiers: u8, // Modifier flags
    },
    /// Custom user-defined event
    Custom(u32),
}

/// Trait for elements that can be queried by type
///
/// Implement this for your element types to enable typed queries:
/// ```rust,ignore
/// ctx.query::<Image>("my-image")
/// ```
pub trait Queryable: Sized {
    /// Try to extract this type from an element handle
    fn from_handle(handle: &ElementHandle<()>) -> Option<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_creation() {
        let registry = Arc::new(ElementRegistry::new());
        let node_id = LayoutNodeId::default();

        registry.register("test", node_id);

        let handle: ElementHandle<()> = ElementHandle::new("test", registry);
        assert_eq!(handle.node_id(), node_id);
        assert_eq!(handle.id(), "test");
        assert!(handle.exists());
    }

    #[test]
    fn test_handle_for_nonexistent_element() {
        let registry = Arc::new(ElementRegistry::new());

        // Handle can be created for element that doesn't exist yet
        let handle: ElementHandle<()> = ElementHandle::new("future-element", registry);
        assert_eq!(handle.id(), "future-element");
        assert!(!handle.exists());
    }

    #[test]
    fn test_parent_traversal() {
        let registry = Arc::new(ElementRegistry::new());
        let parent_id = LayoutNodeId::default();
        let child_id = LayoutNodeId::default();

        registry.register("parent", parent_id);
        registry.register("child", child_id);
        registry.register_parent(child_id, parent_id);

        let child_handle: ElementHandle<()> = ElementHandle::new("child", registry);
        let parent = child_handle.parent();

        assert!(parent.is_some());
        // Note: In real usage with distinct IDs this would work properly
    }

    // =========================================================================
    // On-Ready Callback Tests
    // =========================================================================

    #[test]
    fn test_handle_on_ready_registers_callback() {
        let registry = Arc::new(ElementRegistry::new());

        // Create handle for element that doesn't exist yet
        let handle: ElementHandle<()> = ElementHandle::new("my-element", registry.clone());

        // Register on_ready callback
        handle.on_ready(|_bounds| {
            // Callback logic here
        });

        // Should have pending callback
        assert!(registry.has_pending_on_ready());

        let pending = registry.take_pending_on_ready();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "my-element");
    }

    #[test]
    fn test_handle_on_ready_uses_string_id() {
        let registry = Arc::new(ElementRegistry::new());
        let node_id = LayoutNodeId::default();

        // Register element
        registry.register("progress-bar", node_id);

        // Create handle and register callback
        let handle: ElementHandle<()> = ElementHandle::new("progress-bar", registry.clone());
        handle.on_ready(|_| {});

        // Callback should be registered with string ID
        let pending = registry.take_pending_on_ready();
        assert_eq!(pending[0].0, "progress-bar");
    }

    #[test]
    fn test_handle_on_ready_skips_if_already_triggered() {
        let registry = Arc::new(ElementRegistry::new());

        // Mark as already triggered
        registry.mark_on_ready_triggered("my-element");

        // Create handle and try to register callback
        let handle: ElementHandle<()> = ElementHandle::new("my-element", registry.clone());
        handle.on_ready(|_| {});

        // Should NOT have pending callback
        assert!(!registry.has_pending_on_ready());
    }

    #[test]
    fn test_handle_on_ready_works_before_element_exists() {
        let registry = Arc::new(ElementRegistry::new());

        // Create handle for nonexistent element
        let handle: ElementHandle<()> = ElementHandle::new("future-element", registry.clone());
        assert!(!handle.exists());

        // Register callback anyway
        handle.on_ready(|_| {});

        // Callback should be pending
        assert!(registry.has_pending_on_ready());

        let pending = registry.take_pending_on_ready();
        assert_eq!(pending[0].0, "future-element");
    }

    #[test]
    fn test_handle_on_ready_arc() {
        let registry = Arc::new(ElementRegistry::new());
        let handle: ElementHandle<()> = ElementHandle::new("my-element", registry.clone());

        // Use Arc version for shared callback
        let callback: OnReadyCallback = Arc::new(|_| {});
        handle.on_ready_arc(callback);

        assert!(registry.has_pending_on_ready());
    }
}

// =============================================================================
// MotionHandle - Handle for querying motion animation state
// =============================================================================

/// Handle to a motion animation for querying its state
///
/// Returned by `query_motion("motion-key")` for animation state queries.
/// Use this to check if a parent motion animation has settled before
/// rendering child content with hover effects, etc.
///
/// # Example
///
/// ```ignore
/// use blinc_layout::selector::query_motion;
///
/// // Inside a Stateful on_state callback:
/// let motion = query_motion("dialog-content");
/// if motion.is_settled() {
///     // Safe to render hover effects
///     container.merge(button_with_hover());
/// } else {
///     // Render without hover effects during animation
///     container.merge(button_static());
/// }
/// ```
#[derive(Clone, Debug)]
pub struct MotionHandle {
    /// The stable key used to query this motion
    key: String,
    /// Current animation state
    state: MotionAnimationState,
}

impl MotionHandle {
    /// Create a new motion handle from a stable key
    ///
    /// Queries the current animation state via `BlincContextState`.
    pub fn new(key: impl Into<String>) -> Self {
        let key = key.into();
        let state = BlincContextState::try_get()
            .map(|ctx| ctx.query_motion(&key))
            .unwrap_or(MotionAnimationState::NotFound);
        Self { key, state }
    }

    /// Get the stable key for this motion
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Get the current animation state
    pub fn state(&self) -> MotionAnimationState {
        self.state
    }

    /// Check if the animation is still playing (not settled)
    ///
    /// Returns true if the motion is in `Suspended`, `Waiting`, `Entering`, or `Exiting` state.
    pub fn is_animating(&self) -> bool {
        self.state.is_animating()
    }

    /// Check if the animation has settled (fully visible)
    ///
    /// Returns true if the motion is in `Visible` state.
    /// This is when it's safe to render child content with hover effects.
    pub fn is_settled(&self) -> bool {
        self.state.is_settled()
    }

    /// Check if the motion is suspended (waiting for explicit start)
    ///
    /// A suspended motion is mounted with opacity 0 and waits for `start()` to be called.
    pub fn is_suspended(&self) -> bool {
        self.state.is_suspended()
    }

    /// Check if the element is entering
    pub fn is_entering(&self) -> bool {
        self.state.is_entering()
    }

    /// Check if the element is exiting
    pub fn is_exiting(&self) -> bool {
        self.state.is_exiting()
    }

    /// Get the animation progress (0.0 to 1.0)
    ///
    /// Returns 0.0 for Suspended/Waiting, 1.0 for Visible/Removed, and the actual
    /// progress for Entering/Exiting states.
    pub fn progress(&self) -> f32 {
        self.state.progress()
    }

    /// Check if a motion with this key exists
    pub fn exists(&self) -> bool {
        !matches!(self.state, MotionAnimationState::NotFound)
    }

    /// Start the enter animation for a suspended motion
    ///
    /// Use this to explicitly trigger the enter animation for a motion that was
    /// created with `.suspended()`. The motion transitions from `Suspended` →
    /// `Waiting` or `Entering` state.
    ///
    /// This is useful for tab transitions and other cases where you want to:
    /// 1. Mount the content invisibly (opacity 0)
    /// 2. Perform any setup/measurement
    /// 3. Then trigger the animation manually
    ///
    /// No-op if the motion is not in `Suspended` state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In tabs.rs on_state callback:
    /// let motion_key = format!("tabs_motion:{}", active_tab);
    ///
    /// // Create suspended motion first
    /// let m = motion_derived(&motion_key)
    ///     .suspended()
    ///     .enter_animation(enter)
    ///     .child(content);
    ///
    /// // Then trigger the animation after mounting
    /// query_motion(&motion_key).start();
    /// ```
    pub fn start(&self) {
        // Queue the start to be processed during the next render frame
        crate::queue_global_motion_start(self.key.clone());
    }

    /// Cancel the exit animation and return the motion to Visible state
    ///
    /// Used when an overlay's close is cancelled (e.g., mouse re-enters hover card).
    /// This interrupts the exit animation and immediately sets the motion to fully visible.
    ///
    /// No-op if the motion is not in Exiting state.
    pub fn cancel_exit(&self) {
        // Queue the cancellation to be processed during the next render frame
        crate::queue_global_motion_exit_cancel(self.key.clone());
    }

    /// Trigger the exit animation for this motion
    ///
    /// Used to explicitly trigger the exit animation (e.g., when a hover card
    /// close countdown completes). This transitions the motion from Visible → Exiting.
    ///
    /// No-op if the motion is not in Visible state.
    pub fn exit(&self) {
        // Queue the exit to be processed during the next render frame
        crate::queue_global_motion_exit_start(self.key.clone());
    }
}
