//! Element handle for programmatic element manipulation

use std::sync::Arc;

use crate::element::ElementBounds;
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
    /// Returns None if layout hasn't been computed yet.
    pub fn bounds(&self) -> Option<ElementBounds> {
        // TODO: This needs access to the RenderTree's computed layout
        // For now, return None - will be wired up when integrated with renderer
        None
    }

    /// Check if this element is visible in the viewport
    ///
    /// An element is visible if its bounds intersect with any ancestor
    /// scroll container's viewport.
    pub fn is_visible(&self) -> bool {
        // TODO: Implement visibility check against scroll container viewports
        true
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
        // TODO: Wire up to RenderTree.scroll_node_into_view()
        // This needs access to scroll containers and their offsets
    }

    /// Focus this element
    ///
    /// For focusable elements like TextInput, this sets keyboard focus.
    /// For other elements, this updates the EventRouter's focus state.
    pub fn focus(&self) {
        // TODO: Wire up to EventRouter.set_focus() and TextInput focus
    }

    /// Remove focus from this element
    pub fn blur(&self) {
        // TODO: Wire up to EventRouter focus management
    }

    // =========================================================================
    // Signal Operations
    // =========================================================================

    /// Emit a signal to trigger reactive updates
    ///
    /// Elements depending on this signal will be rebuilt.
    pub fn emit_signal(&self, _signal_id: blinc_core::SignalId) {
        // TODO: Wire up to signal system
        // This needs access to the signal dispatcher
    }

    /// Mark this element as dirty, forcing a rebuild
    pub fn mark_dirty(&self) {
        // TODO: Wire up to dirty tracking system
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
