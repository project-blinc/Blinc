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
//! `Scroll` carries a [`ScrollRef`] and `Div` a [`DivRef`]. Both are
//! resolved to their node by the renderer when the element that bound
//! them is built, so this module only has to hand the right handle to
//! the right binder.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use blinc_layout::selector::{DivRef, ScrollRef};

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
    Element(DivRef),
}

fn registry() -> &'static Mutex<HashMap<u64, Handle>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, Handle>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
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
            RefKind::Element => Handle::Element(DivRef::new()),
        });
}

/// The scroll handle for `id`, for a widget that has been handed one.
pub fn scroll_ref_by_id(id: i64) -> Option<ScrollRef> {
    match registry().lock().expect("ref registry").get(&(id as u64)) {
        Some(Handle::Scroll(r)) => Some(r.clone()),
        _ => None,
    }
}

/// The element handle for `id`.
pub fn div_ref_by_id(id: i64) -> Option<DivRef> {
    match registry().lock().expect("ref registry").get(&(id as u64)) {
        Some(Handle::Element(r)) => Some(r.clone()),
        _ => None,
    }
}

/// Bind whatever handle `id` names to a `Div` being built.
///
/// Which kind it is decides what binding means, and only the handle
/// knows — the call site passes an opaque id.
pub fn bind_div(id: i64, widget: &mut blinc_layout::div::Div) {
    let handle = registry()
        .lock()
        .expect("ref registry")
        .get(&(id as u64))
        .cloned();
    match handle {
        Some(Handle::Element(r)) => *widget = std::mem::take(widget).bind(&r),
        Some(Handle::Scroll(r)) => *widget = std::mem::take(widget).bind_scroll(&r),
        None => tracing::warn!(id, "no handle for this ref"),
    }
}

fn with_scroll(id: i64, action: &str, f: impl FnOnce(&ScrollRef)) {
    match scroll_ref_by_id(id) {
        Some(r) => f(&r),
        None => tracing::warn!(id, action, "not a Scroll ref"),
    }
}

fn with_element(id: i64, action: &str, f: impl FnOnce(&DivRef)) {
    match div_ref_by_id(id) {
        Some(r) => f(&r),
        None => tracing::warn!(id, action, "not a Div ref"),
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

    /// An element ref carries a real handle, so a builder has something
    /// to bind and a method something to act on.
    #[test]
    fn an_element_ref_carries_a_div_handle() {
        mint(4001, RefKind::Element);
        let handle = div_ref_by_id(4001).expect("minted");
        assert!(!handle.exists(), "nothing bound it yet");
    }

    /// A kind is fixed at declaration: a Scroll method on an element ref
    /// finds nothing rather than acting on some other handle.
    #[test]
    fn kinds_do_not_answer_for_each_other() {
        mint(3001, RefKind::Element);
        assert!(scroll_ref_by_id(3001).is_none());
        assert!(div_ref_by_id(3001).is_some());

        mint(3002, RefKind::Scroll);
        assert!(div_ref_by_id(3002).is_none());
    }
}
