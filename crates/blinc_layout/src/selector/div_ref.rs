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

/// Serial for auto-assigned element ids, so each ref's id is its own.
fn next_serial() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

use super::ScrollOptions;
use super::registry::ElementRegistry;

/// Shared inner state, so a ref survives the rebuilds of what it points at.
#[derive(Default)]
pub struct DivRefInner {
    /// Set by the renderer once the element is built.
    node_id: Option<LayoutNodeId>,
    /// Weak: the registry owns the tree, not the other way round.
    registry: Option<Weak<ElementRegistry>>,
    /// The element's id as of the last bind.
    ///
    /// A `LayoutNodeId` alone cannot say whether the binding still
    /// holds: ids are reissued on rebuild, so an element that stopped
    /// binding this ref leaves the old id pointing at whatever now
    /// occupies that slot. Checking the id back is what separates "the
    /// element I bound" from "some element".
    element_id: Option<String>,
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
    ///
    /// Gives the element an id if it has none. Focus and scroll-into-view
    /// are keyed by string id in the context callbacks, so a bound
    /// element without one would accept every command and perform none.
    /// The id is derived from the node, so it cannot collide with one
    /// the author chose.
    pub fn bind_to_node(&self, node_id: LayoutNodeId, registry: Weak<ElementRegistry>) {
        let mut element_id = None;
        if let Some(registry) = registry.upgrade() {
            let id = registry.get_id(node_id).unwrap_or_else(|| {
                // Derived from this ref, not from the node: a node id is
                // reissued, so a node-derived id would match again after
                // recycling and defeat the staleness check below.
                let fresh = format!("__blinc_ref_{}", next_serial());
                registry.register(fresh.clone(), node_id);
                fresh
            });
            element_id = Some(id);
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.node_id = Some(node_id);
        inner.element_id = element_id;
        inner.registry = Some(registry);
    }

    /// The node this ref is bound to, while the binding holds.
    pub fn node_id(&self) -> Option<LayoutNodeId> {
        self.resolved().map(|(node_id, _, _)| node_id)
    }

    /// Whether the element this ref was bound to is still in the tree.
    ///
    /// False once it stops being built, rather than true against
    /// whatever inherited its node id.
    pub fn exists(&self) -> bool {
        self.resolved().is_some()
    }

    /// This element's laid-out bounds, once layout has run.
    pub fn bounds(&self) -> Option<blinc_core::context_state::Bounds> {
        let (_, registry, element_id) = self.resolved()?;
        registry.get_bounds(&element_id)
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

    /// The binding, if it still holds.
    ///
    /// The id check is the whole point: a rebuild that stops binding
    /// this ref leaves the node id in place, and that id is reissued to
    /// some other element. Without this, a command would land on it.
    fn resolved(&self) -> Option<(LayoutNodeId, Arc<ElementRegistry>, String)> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let node_id = inner.node_id?;
        let registry = inner.registry.as_ref()?.upgrade()?;
        let element_id = inner.element_id.clone()?;
        (registry.get_id(node_id).as_deref() == Some(element_id.as_str()))
            .then_some((node_id, registry, element_id))
    }

    /// The element's string id, which is what the focus and scroll
    /// callbacks are keyed by. Assigned at bind time if the element had
    /// none, so this only fails when the ref points at nothing yet.
    fn with_string_id(&self, action: &str, f: impl FnOnce(&str)) {
        match self.resolved() {
            Some((_, _, element_id)) => f(&element_id),
            None => tracing::warn!(action, "DivRef is not bound to a live element"),
        }
    }
}

/// A ref is handed to builders and captured by handlers, and a
/// `Stateful` callback is `Send + Sync`, so this has to hold for a ref
/// to be usable from one.
const _: fn() = || {
    fn assert<T: Send + Sync>() {}
    assert::<DivRef>();
    assert::<super::ScrollRef>();
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A ref bound while its tree is gone is not bound: the weak
    /// registry cannot upgrade, so there is nothing to check against.
    #[test]
    fn a_binding_against_a_dropped_tree_does_not_hold() {
        let r = DivRef::new();
        r.bind_to_node(LayoutNodeId::default(), Weak::new());
        assert!(!r.exists());
    }

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

    /// The shape a handler takes: captured by value into a `Send +
    /// Sync` closure, called later, acting on what it is bound to.
    #[test]
    fn a_ref_survives_capture_into_a_send_sync_closure() {
        let card = DivRef::new();
        let captured = card.clone();
        let handler: Box<dyn Fn() + Send + Sync> = Box::new(move || {
            captured.focus();
            captured.scroll_into_view();
        });

        // Unbound: the calls are no-ops rather than panics, which is
        // what a handler firing before its element exists needs.
        handler();

        // A real registry: binding against a dead one is not a binding,
        // which is what keeps a ref from outliving its tree.
        let registry = Arc::new(ElementRegistry::new());
        card.bind_to_node(LayoutNodeId::default(), Arc::downgrade(&registry));
        handler();
        assert!(card.exists(), "the binding outlived the capture");
    }

    /// Clones share one binding, so a ref handed to a builder and kept
    /// by the caller are the same handle.
    #[test]
    fn clones_share_the_binding() {
        let a = DivRef::new();
        let b = a.clone();
        let registry = Arc::new(ElementRegistry::new());
        a.bind_to_node(LayoutNodeId::default(), Arc::downgrade(&registry));
        assert_eq!(b.node_id(), Some(LayoutNodeId::default()));
    }
}
