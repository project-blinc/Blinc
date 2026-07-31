//! Read scopes — the accumulator behind observed dependency tracking.
//!
//! A scope is one render of one reactive region. While it is open, every
//! signal read performed by the JIT'd body records its id here; when it
//! closes, the accumulated set IS the region's dependency set. See
//! `docs/effects-reactivity.md` for why this replaces scraping the AST
//! for `.get()` calls.
//!
//! Two rules, both of which fall out of the stack rather than needing to
//! be enforced:
//!
//! - **The innermost scope wins.** A read inside a nested region belongs
//!   to that region alone, because re-rendering the inner region is
//!   enough when the value changes. Only the top of the stack records.
//! - **A read with no scope open records nothing.** That covers event
//!   handlers, `init` blocks and host calls, and it is what makes a
//!   closure built during a render but invoked later unable to pollute
//!   the render that created it — by then its scope is long gone.
//!
//! Per-thread, matching the call-id stack the widget FFI already keeps:
//! the JIT is driven from one thread today.

use std::cell::RefCell;

/// One region's render, and the signals it has read so far.
struct Scope {
    /// The region this scope belongs to. Carried so `exit` can verify
    /// it is closing the scope it thinks it is.
    region_id: i64,
    /// Raw `SignalId`s, in first-read order, deduplicated.
    reads: Vec<u64>,
}

thread_local! {
    static SCOPES: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

/// How many scopes have ever been opened. Exists so a test can prove
/// the call site actually runs `enter`: while reads do not yet perform,
/// an unopened scope and an open-but-empty one both mount from the
/// fallback, so nothing else distinguishes them.
static OPENED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Total scopes opened since process start.
#[cfg(test)]
pub(crate) fn opened_count() -> usize {
    OPENED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Open a scope for `region_id`. Returns the id so the call can sit in
/// argument position at a `with` site, where evaluation order puts it
/// before the region's view runs.
pub(crate) fn enter(region_id: i64) -> i64 {
    OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    SCOPES.with(|scopes| {
        scopes.borrow_mut().push(Scope {
            region_id,
            reads: Vec::new(),
        })
    });
    region_id
}

/// Record a read against the innermost open scope. A no-op when none is
/// open, which is the common case: most reads happen outside any region.
pub(crate) fn record(signal_raw: u64) {
    SCOPES.with(|scopes| {
        if let Some(scope) = scopes.borrow_mut().last_mut()
            && !scope.reads.contains(&signal_raw)
        {
            scope.reads.push(signal_raw);
        }
    });
}

/// Close the scope for `region_id` and return what it read.
///
/// Returns `None` when the top of the stack belongs to some other
/// region, which means a render did not close its own scope — a bug
/// worth surfacing rather than papering over, since the mismatched
/// scope would otherwise attribute one region's reads to another.
pub(crate) fn exit(region_id: i64) -> Option<Vec<u64>> {
    SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        match scopes.last() {
            Some(top) if top.region_id == region_id => {
                Some(scopes.pop().expect("just checked the top exists").reads)
            }
            Some(top) => {
                tracing::warn!(
                    closing = region_id,
                    open = top.region_id,
                    "read scope closed out of order — the region's deps will \
                     fall back to what was registered at compile time"
                );
                None
            }
            None => {
                tracing::warn!(
                    closing = region_id,
                    "read scope closed with none open — same fallback"
                );
                None
            }
        }
    })
}

/// Whether any scope is open. For tests and for the read handler's fast
/// path, which can skip the record entirely.
#[cfg(test)]
pub(crate) fn depth() -> usize {
    SCOPES.with(|scopes| scopes.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test drives the stack by hand; the thread-local means they
    /// do not share state across threads, but a leaked scope would leak
    /// into the next test on the same thread, so every test closes what
    /// it opens.
    #[test]
    fn a_scope_collects_the_reads_made_inside_it() {
        enter(1);
        record(10);
        record(20);
        assert_eq!(exit(1), Some(vec![10, 20]));
        assert_eq!(depth(), 0, "the scope must be popped");
    }

    /// A body reading the same signal three times depends on it once.
    #[test]
    fn repeated_reads_are_recorded_once() {
        enter(1);
        record(10);
        record(10);
        record(10);
        assert_eq!(exit(1), Some(vec![10]));
    }

    /// Order is first-read order, which keeps the dep set stable across
    /// renders that read the same signals — a set that reordered would
    /// churn the registration for no reason.
    #[test]
    fn reads_keep_first_read_order() {
        enter(1);
        record(30);
        record(10);
        record(30);
        record(20);
        assert_eq!(exit(1), Some(vec![30, 10, 20]));
    }

    /// The rule that makes regions worth having: a read inside a nested
    /// region belongs to that region, NOT to the one around it.
    #[test]
    fn the_innermost_scope_takes_the_read() {
        enter(1);
        record(10);
        enter(2);
        record(20);
        assert_eq!(exit(2), Some(vec![20]), "the inner region read only 20");
        record(30);
        assert_eq!(
            exit(1),
            Some(vec![10, 30]),
            "and the outer never saw the inner's read"
        );
    }

    /// Everything outside a region: event handlers, init blocks, host
    /// calls. Recording must be a no-op rather than a panic or a stray
    /// attribution.
    #[test]
    fn a_read_with_no_scope_open_is_dropped() {
        assert_eq!(depth(), 0);
        record(99);
        assert_eq!(depth(), 0, "recording must not open a scope");
    }

    /// The closure-built-during-render case, which is the one AST
    /// inference gets wrong. By the time the handler runs, the scope is
    /// closed, so its reads cannot land in the render that created it.
    #[test]
    fn a_read_after_the_scope_closed_does_not_reopen_it() {
        enter(1);
        record(10);
        let deps = exit(1);
        record(20); // the click handler, firing later
        assert_eq!(deps, Some(vec![10]), "only what the render itself read");
        assert_eq!(depth(), 0);
    }

    /// Closing the wrong scope is a bug, not something to absorb
    /// silently: `None` tells the caller to fall back rather than mount
    /// with another region's deps.
    #[test]
    fn closing_out_of_order_reports_rather_than_guesses() {
        enter(1);
        record(10);
        assert_eq!(exit(2), None, "region 2 never opened a scope");
        assert_eq!(exit(1), Some(vec![10]), "and region 1's is still intact");
    }

    #[test]
    fn closing_with_nothing_open_reports() {
        assert_eq!(exit(7), None);
    }
}
