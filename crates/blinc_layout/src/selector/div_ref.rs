//! `DivRef` — a handle onto an ordinary element, bound rather than named.
//!
//! The counterpart to [`crate::selector::ScrollRef`], and the same
//! contract: create one, `.bind()` it to an element, and the renderer
//! resolves it to that element's node while building. Methods then act
//! on whatever it is bound to.
//!
//! [`crate::selector::ElementHandle`] answers the same questions, but
//! keyed by a string id the author has to invent and keep unique. A ref
//! is bound by the builder, so the identity is the binding.

use std::sync::{Arc, Mutex, Weak};

use crate::tree::LayoutNodeId;

use super::ScrollOptions;
use super::registry::ElementRegistry;

/// Shared inner state, so a ref survives the rebuilds of what it points at.
#[derive(Default)]
pub struct DivRefInner {
    /// Set by the renderer once the element is built.
    node_id: Option<LayoutNodeId>,
    /// Weak: the registry owns the tree, not the other way round.
    registry: Option<Weak<ElementRegistry>>,
}

/// A handle onto an element.
///
/// ```rust,ignore
/// let card = DivRef::new();
///
/// div().bind(&card).child(text("hello"))
///
/// // Later:
/// card.focus();
/// card.scroll_into_view();
/// ```
#[derive(Clone, Default)]
pub struct DivRef {
    inner: Arc<Mutex<DivRefInner>>,
}

impl DivRef {
    /// A ref bound to nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared state, for holding a ref across rebuilds.
    pub fn inner(&self) -> Arc<Mutex<DivRefInner>> {
        Arc::clone(&self.inner)
    }

    /// Point this ref at `node_id`. Called by the renderer while
    /// building, the same moment a `ScrollRef` is resolved.
    pub fn bind_to_node(&self, node_id: LayoutNodeId, registry: Weak<ElementRegistry>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.node_id = Some(node_id);
        inner.registry = Some(registry);
    }

    /// The node this ref is bound to, if it has been built.
    pub fn node_id(&self) -> Option<LayoutNodeId> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).node_id
    }

    /// Whether the element exists in the current tree.
    pub fn exists(&self) -> bool {
        self.node_id().is_some()
    }

    /// This element's laid-out bounds, once layout has run.
    pub fn bounds(&self) -> Option<blinc_core::context_state::Bounds> {
        let (node_id, registry) = self.resolved()?;
        registry.get_bounds(&registry.get_id(node_id)?)
    }

    /// Give this element keyboard focus.
    pub fn focus(&self) {
        self.with_string_id("focus", |id| {
            if let Some(ctx) = blinc_core::context_state::BlincContextState::try_get() {
                ctx.set_focus(Some(id));
            }
        });
    }

    /// Drop focus, if this element holds it.
    pub fn blur(&self) {
        self.with_string_id("blur", |id| {
            if let Some(ctx) = blinc_core::context_state::BlincContextState::try_get()
                && ctx.is_focused(id)
            {
                ctx.set_focus(None);
            }
        });
    }

    /// Bring this element into view inside its scroll container.
    pub fn scroll_into_view(&self) {
        self.scroll_into_view_with(ScrollOptions::default());
    }

    /// Bring this element into view, with options.
    pub fn scroll_into_view_with(&self, _options: ScrollOptions) {
        self.with_string_id("scroll_into_view", |id| {
            if let Some(ctx) = blinc_core::context_state::BlincContextState::try_get() {
                ctx.scroll_element_into_view(id);
            }
        });
    }

    fn resolved(&self) -> Option<(LayoutNodeId, Arc<ElementRegistry>)> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let node_id = inner.node_id?;
        let registry = inner.registry.as_ref()?.upgrade()?;
        Some((node_id, registry))
    }

    /// The element's string id, which is what the focus and scroll
    /// callbacks are keyed by.
    ///
    /// An element bound to a ref is given one at bind time if it has
    /// none, so this only fails when the ref points at nothing yet.
    fn with_string_id(&self, action: &str, f: impl FnOnce(&str)) {
        let Some((node_id, registry)) = self.resolved() else {
            tracing::warn!(action, "DivRef is not bound to a built element yet");
            return;
        };
        match registry.get_id(node_id) {
            Some(id) => f(&id),
            None => tracing::warn!(action, ?node_id, "the bound element carries no id"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbound_ref_reports_nothing_and_does_nothing() {
        let r = DivRef::new();
        assert!(!r.exists());
        assert_eq!(r.node_id(), None);
        assert_eq!(r.bounds(), None);
        // Every command is a no-op rather than a panic.
        r.focus();
        r.blur();
        r.scroll_into_view();
    }

    /// Clones share one binding, so a ref handed to a builder and kept
    /// by the caller are the same handle.
    #[test]
    fn clones_share_the_binding() {
        let a = DivRef::new();
        let b = a.clone();
        a.bind_to_node(LayoutNodeId::default(), Weak::new());
        assert_eq!(b.node_id(), Some(LayoutNodeId::default()));
    }
}
