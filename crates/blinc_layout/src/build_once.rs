//! Reentrancy-tolerant lazy build for widget builders.

use std::cell::OnceCell;

/// Build `cell`'s value once, tolerating a reentrant call.
///
/// The shape every cn builder uses: hold a `OnceCell` of the built
/// element so `build` and `render_props` describe the same instance and
/// the identity methods can hand out references.
///
/// [`OnceCell::get_or_init`] panics if the initialiser reaches the same
/// cell, and widget building does reach back: an element shared into a
/// `Stateful`'s content outlives that stateful's rebuilds, and a rebuild
/// walks the tree asking every element for its type, which lands back in
/// the same builder while the first ask is still inside the cell. That
/// is a panic in a running app, not just in tests.
///
/// Building outside the cell trades that panic for a wasted build on
/// the reentrant path: the inner call reaches the cell first and wins
/// it, the outer `set` loses, and both callers get the stored instance.
/// A builder's construction is pure, so the loser costs work, never
/// state, and callers still share one instance.
pub fn build_once<T>(cell: &OnceCell<T>, build: impl FnOnce() -> T) -> &T {
    if let Some(built) = cell.get() {
        return built;
    }
    let built = build();
    let _ = cell.set(built);
    cell.get().expect("just set, and nothing clears a OnceCell")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_builds_once_and_reuses() {
        let cell: OnceCell<i32> = OnceCell::new();
        assert_eq!(*build_once(&cell, || 7), 7);
        assert_eq!(*build_once(&cell, || panic!("must not rebuild")), 7);
    }

    /// What `get_or_init` panics on. Every caller sees one instance,
    /// which is what the identity methods handing out references need.
    #[test]
    fn a_reentrant_build_resolves_to_one_value() {
        let cell: OnceCell<i32> = OnceCell::new();
        let value = *build_once(&cell, || {
            // The inner build reaches the cell first and wins it; the
            // outer `set` then loses, and its own result is discarded
            // in favour of the stored one.
            *build_once(&cell, || 1) + 10
        });
        assert_eq!(cell.get(), Some(&1), "the inner build populated the cell");
        assert_eq!(value, 1, "and the outer caller sees that same instance");
        assert_eq!(*build_once(&cell, || 99), 1, "as does every later caller");
    }
}
