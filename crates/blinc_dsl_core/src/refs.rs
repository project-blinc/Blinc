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

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use blinc_layout::selector::ScrollRef;

fn registry() -> &'static Mutex<HashMap<u64, ScrollRef>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, ScrollRef>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Ensure a handle exists for `id`, keeping any already minted.
///
/// Recompiling the same source yields the same id, and reusing the
/// handle is what lets a scroll position survive a hot reload rather
/// than jumping to the top on every edit.
pub(crate) fn mint(id: u64) {
    registry()
        .lock()
        .expect("ref registry")
        .entry(id)
        .or_default();
}

/// The handle for `id`, for a widget that has been handed one.
pub fn scroll_ref_by_id(id: i64) -> Option<ScrollRef> {
    registry()
        .lock()
        .expect("ref registry")
        .get(&(id as u64))
        .cloned()
}

fn with_ref(id: i64, action: &str, f: impl FnOnce(&ScrollRef)) {
    match scroll_ref_by_id(id) {
        Some(r) => f(&r),
        None => tracing::warn!(
            id,
            action,
            "no handle for this ref — nothing minted one at compile time",
        ),
    }
}

/// `pages.scroll_to_top()`.
pub fn scroll_to_top(id: i64) {
    with_ref(id, "scroll_to_top", |r| r.scroll_to_top());
}

/// `pages.scroll_to_bottom()`.
pub fn scroll_to_bottom(id: i64) {
    with_ref(id, "scroll_to_bottom", |r| r.scroll_to_bottom());
}

/// `pages.scroll_by(dx, dy)`.
pub fn scroll_by(id: i64, dx: f64, dy: f64) {
    with_ref(id, "scroll_by", |r| r.scroll_by(dx as f32, dy as f32));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinct declarations get distinct handles, which is the point of
    /// keying on the span rather than on a name.
    #[test]
    fn each_id_gets_its_own_handle() {
        mint(1001);
        mint(1002);
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
        mint(2001);
        let first = scroll_ref_by_id(2001).expect("minted").inner();
        mint(2001);
        assert!(std::sync::Arc::ptr_eq(
            &scroll_ref_by_id(2001).expect("minted").inner(),
            &first
        ));
    }

    #[test]
    fn an_unminted_id_is_a_no_op() {
        scroll_to_top(999_999);
    }
}
