//! Element handle for programmatic element manipulation

use std::sync::Arc;

use crate::element::ElementBounds;
use crate::tree::LayoutNodeId;

use super::registry::ElementRegistry;
use super::ScrollOptions;

/// Handle to a queried element for programmatic manipulation
///
/// Returned by `ctx.query::<T>("element-id")` for typed access,
/// or `ctx.query_any("element-id")` for untyped access.
#[derive(Clone)]
pub struct ElementHandle<T = ()> {
    node_id: LayoutNodeId,
    registry: Arc<ElementRegistry>,
    /// Typed element data (if available)
    _marker: std::marker::PhantomData<T>,
}

impl<T> ElementHandle<T> {
    /// Create a new element handle
    pub(crate) fn new(node_id: LayoutNodeId, registry: Arc<ElementRegistry>) -> Self {
        Self {
            node_id,
            registry,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the underlying layout node ID
    pub fn node_id(&self) -> LayoutNodeId {
        self.node_id
    }

    /// Get the string ID of this element (if registered)
    pub fn id(&self) -> Option<String> {
        self.registry.get_id(self.node_id)
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
        let parent_id = self.registry.get_parent(self.node_id)?;
        Some(ElementHandle::new(parent_id, self.registry.clone()))
    }

    /// Get all ancestors (immediate parent to root)
    pub fn ancestors(&self) -> impl Iterator<Item = ElementHandle<()>> {
        let ancestors = self.registry.ancestors(self.node_id);
        let registry = self.registry.clone();
        ancestors
            .into_iter()
            .map(move |id| ElementHandle::new(id, registry.clone()))
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

        let handle: ElementHandle<()> = ElementHandle::new(node_id, registry);
        assert_eq!(handle.node_id(), node_id);
        assert_eq!(handle.id(), Some("test".to_string()));
    }

    #[test]
    fn test_parent_traversal() {
        let registry = Arc::new(ElementRegistry::new());
        let parent_id = LayoutNodeId::default();
        let child_id = LayoutNodeId::default();

        registry.register("parent", parent_id);
        registry.register("child", child_id);
        registry.register_parent(child_id, parent_id);

        let child_handle: ElementHandle<()> = ElementHandle::new(child_id, registry);
        let parent = child_handle.parent();

        assert!(parent.is_some());
        // Note: In real usage with distinct IDs this would work properly
    }
}
