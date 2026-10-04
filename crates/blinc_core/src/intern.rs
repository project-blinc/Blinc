//! Process-wide string interner for repeated identifiers.
//!
//! Many parts of the framework store string identifiers that are heavily
//! reused — CSS class names (e.g. `"cn-button--primary"`), stable motion
//! keys, stateful context keys. Storing each occurrence as its own
//! `String` allocates the same bytes thousands of times. Interning them
//! through a single `Arc<str>` pool means each unique string is allocated
//! once and every later use bumps a refcount.
//!
//! # Example
//!
//! ```ignore
//! use blinc_core::intern::intern;
//!
//! let a = intern("cn-button--primary");
//! let b = intern("cn-button--primary");
//! // `a` and `b` point at the same allocation.
//! assert!(std::sync::Arc::ptr_eq(&a, &b));
//! ```
//!
//! # Notes
//!
//! - The pool is **append-only**. Interned strings live for the life of
//!   the process. This is fine for class names / type names / component
//!   keys (which are bounded by source-code occurrences), but
//!   inappropriate for unbounded user-supplied strings (free-form text
//!   input, file contents). Don't pour those in.
//! - Lock contention is minimal in practice because interning happens
//!   during element construction, not in the per-frame paint loop.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

static POOL: OnceLock<Mutex<HashSet<Arc<str>>>> = OnceLock::new();

fn pool() -> &'static Mutex<HashSet<Arc<str>>> {
    POOL.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Get the canonical `Arc<str>` for `s`. If the string has been
/// interned before, returns a clone of the existing handle; otherwise
/// allocates once and stashes it for future callers.
pub fn intern(s: &str) -> Arc<str> {
    let mut pool = pool().lock().unwrap();
    if let Some(existing) = pool.get(s) {
        return Arc::clone(existing);
    }
    let arc: Arc<str> = Arc::from(s);
    pool.insert(Arc::clone(&arc));
    arc
}

/// Number of unique strings currently held by the interner pool.
/// Useful for diagnostics / tests.
pub fn pool_size() -> usize {
    pool().lock().unwrap().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_same_arc_for_equal_strings() {
        let a = intern("cn-button");
        let b = intern("cn-button");
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn returns_distinct_arcs_for_different_strings() {
        let a = intern("foo");
        let b = intern("bar");
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(&*a, "foo");
        assert_eq!(&*b, "bar");
    }

    /// Interning the same string repeatedly adds one entry, however
    /// many times it is asked for.
    ///
    /// Asserted through the pool's own bookkeeping rather than its
    /// total size. The pool is process-global, so a sibling test
    /// interning its own keys moves the total between two reads of it;
    /// under `cargo test`, which runs a binary's tests as threads, that
    /// made `pool_size() <= before + 1` fail intermittently. `nextest`
    /// gives each test its own process and hid it.
    #[test]
    fn pool_size_is_deduped() {
        const KEY: &str = "dedup-test-key-aaaa";
        let x = intern(KEY);
        let y = intern(KEY);
        let z = intern(KEY);

        // One entry: all three handles are the same allocation.
        assert!(Arc::ptr_eq(&x, &y));
        assert!(Arc::ptr_eq(&y, &z));

        // And the pool holds that one, not three.
        assert_eq!(
            pool()
                .lock()
                .unwrap()
                .iter()
                .filter(|k| &***k == KEY)
                .count(),
            1
        );
    }
}
