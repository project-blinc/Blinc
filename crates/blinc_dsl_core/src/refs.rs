//! Element handles a `.blinc` source can hold and drive.
//!
//! A `ref pages: Scroll` declaration mints a handle keyed by its own
//! source span; `ref = pages` hands that handle to a widget, which
//! binds it; `pages.scroll_to_top()` acts on it. The DSL never sees the
//! id — it passes it as an opaque `i64`, the same way a signal-bound
//! prop carries a `SignalId`.
//!
//! Keyed by the compiler-minted id rather than an author-chosen name,
//! so two declarations cannot collide however they are spelled.
//!
//! ## Kinds
//!
//! `Scroll` carries a [`ScrollRef`], which the renderer resolves to the
//! node that bound it. `Div` carries nothing: the element takes an id
//! derived from the handle, and the methods reach it through the
//! element registry, which is where `ctx.query(...)` already looks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use blinc_layout::selector::{ElementHandle, ElementRegistry, ScrollRef};

/// What a declared ref points at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefKind {
    /// `ref x: Scroll`
    Scroll,
    /// `ref x: Div`
    Element,
}

#[derive(Clone)]
enum Handle {
    Scroll(ScrollRef),
    /// The element carries an id derived from the handle's own, so
    /// nothing has to be stored here to find it again.
    Element,
}

fn registry() -> &'static Mutex<HashMap<u64, Handle>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, Handle>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The element registry the host is currently rendering into.
///
/// A host function called from a DSL closure has no context in scope —
/// the closure runs long after the frame that built it — so the build
/// closure, which does receive one, leaves the registry here. Same
/// bridge the `with` regions use for the view renderer.
fn element_registry() -> &'static Mutex<Option<Arc<ElementRegistry>>> {
    static BRIDGE: OnceLock<Mutex<Option<Arc<ElementRegistry>>>> = OnceLock::new();
    BRIDGE.get_or_init(|| Mutex::new(None))
}

/// Install the registry to resolve element refs against. Call once per
/// frame from the build closure: `refs::set_element_registry(ctx.element_registry().clone())`.
pub fn set_element_registry(registry: Arc<ElementRegistry>) {
    *element_registry().lock().expect("registry bridge") = Some(registry);
}

/// The element id a `Div` ref stamps on whatever it binds to.
///
/// Derived rather than author-supplied, so it cannot collide with an
/// id the source chose or with another ref's.
pub fn element_id_for(id: i64) -> String {
    format!("__blinc_ref_{id}")
}

fn handle_for(id: i64) -> Option<ElementHandle<()>> {
    let registry = element_registry()
        .lock()
        .expect("registry bridge")
        .clone()?;
    Some(ElementHandle::new(element_id_for(id), registry))
}

/// Ensure a handle exists for `id`, keeping any already minted.
///
/// Recompiling the same source yields the same id, and reusing the
/// handle is what lets a scroll position survive a hot reload rather
/// than jumping to the top on every edit.
pub(crate) fn mint(id: u64, kind: RefKind) {
    registry()
        .lock()
        .expect("ref registry")
        .entry(id)
        .or_insert_with(|| match kind {
            RefKind::Scroll => Handle::Scroll(ScrollRef::new()),
            RefKind::Element => Handle::Element,
        });
}

/// The scroll handle for `id`, for a widget that has been handed one.
pub fn scroll_ref_by_id(id: i64) -> Option<ScrollRef> {
    match registry().lock().expect("ref registry").get(&(id as u64)) {
        Some(Handle::Scroll(r)) => Some(r.clone()),
        _ => None,
    }
}

/// Whether `id` names an element ref, so a builder knows to stamp its
/// id rather than bind a scroller.
pub fn is_element_ref(id: i64) -> bool {
    matches!(
        registry().lock().expect("ref registry").get(&(id as u64)),
        Some(Handle::Element)
    )
}

fn with_scroll(id: i64, action: &str, f: impl FnOnce(&ScrollRef)) {
    match scroll_ref_by_id(id) {
        Some(r) => f(&r),
        None => tracing::warn!(id, action, "not a Scroll ref"),
    }
}

fn with_element(id: i64, action: &str, f: impl FnOnce(&ElementHandle<()>)) {
    match handle_for(id) {
        Some(h) => f(&h),
        None => tracing::warn!(
            id,
            action,
            "no element registry installed — the host has to call \
             `refs::set_element_registry` from its build closure",
        ),
    }
}

/// `pages.scroll_to_top()`.
pub fn scroll_to_top(id: i64) {
    with_scroll(id, "scroll_to_top", |r| r.scroll_to_top());
}

/// `pages.scroll_to_bottom()`.
pub fn scroll_to_bottom(id: i64) {
    with_scroll(id, "scroll_to_bottom", |r| r.scroll_to_bottom());
}

/// `pages.scroll_by(dx, dy)`.
pub fn scroll_by(id: i64, dx: f64, dy: f64) {
    with_scroll(id, "scroll_by", |r| r.scroll_by(dx as f32, dy as f32));
}

/// `card.focus()`.
pub fn focus(id: i64) {
    with_element(id, "focus", |h| h.focus());
}

/// `card.blur()`.
pub fn blur(id: i64) {
    with_element(id, "blur", |h| h.blur());
}

/// `card.scroll_into_view()`.
pub fn scroll_into_view(id: i64) {
    with_element(id, "scroll_into_view", |h| h.scroll_into_view());
}

/// `card.click()`.
pub fn click(id: i64) {
    with_element(id, "click", |h| h.click());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinct declarations get distinct handles, which is the point of
    /// keying on the span rather than on a name.
    #[test]
    fn each_id_gets_its_own_handle() {
        mint(1001, RefKind::Scroll);
        mint(1002, RefKind::Scroll);
        // By inner state, not by signal id: a fresh `ScrollRef` does
        // not mint one, so two of them compare equal on that alone.
        let a = scroll_ref_by_id(1001).expect("minted");
        let b = scroll_ref_by_id(1002).expect("minted");
        assert!(!std::sync::Arc::ptr_eq(&a.inner(), &b.inner()));
    }

    /// Recompiling mints the same id, and must not replace the handle:
    /// the widget bound to it is still holding the old one.
    #[test]
    fn minting_twice_keeps_the_first_handle() {
        mint(2001, RefKind::Scroll);
        let first = scroll_ref_by_id(2001).expect("minted").inner();
        mint(2001, RefKind::Scroll);
        assert!(std::sync::Arc::ptr_eq(
            &scroll_ref_by_id(2001).expect("minted").inner(),
            &first
        ));
    }

    #[test]
    fn an_unminted_id_is_a_no_op() {
        scroll_to_top(999_999);
        focus(999_999);
    }

    /// A kind is fixed at declaration: a Scroll method on an element ref
    /// finds nothing rather than acting on some other handle.
    #[test]
    fn kinds_do_not_answer_for_each_other() {
        mint(3001, RefKind::Element);
        assert!(scroll_ref_by_id(3001).is_none());
        assert!(is_element_ref(3001));

        mint(3002, RefKind::Scroll);
        assert!(!is_element_ref(3002));
    }
}
