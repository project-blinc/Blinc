//! A live tree namespace complements its rebuild-stable node identities.
use std::cell::Cell;
use std::collections::HashSet;
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
static LIVE: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static RETIRED: Mutex<Vec<u64>> = Mutex::new(Vec::new());
thread_local! { static CURRENT: Cell<u64> = const { Cell::new(0) }; }

pub(crate) struct Guard(u64);
impl Drop for Guard {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

pub(crate) fn current() -> u64 {
    CURRENT.get()
}
pub(crate) fn enter(scope: u64) -> Guard {
    Guard(CURRENT.replace(scope))
}
pub(crate) fn allocate() -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    LIVE.lock().unwrap().insert(id);
    id
}
pub(crate) fn is_live(scope: u64) -> bool {
    scope == 0 || LIVE.lock().unwrap().contains(&scope)
}
pub(crate) fn take_retired() -> Vec<u64> {
    std::mem::take(&mut *RETIRED.lock().unwrap())
}
pub(crate) fn retire(scope: u64) {
    LIVE.lock().unwrap().remove(&scope);
    if crate::binding::registry_is_initialized() {
        RETIRED.lock().unwrap().push(scope);
        crate::binding::sweep_retired_scopes();
    }
    crate::stateful::discard_tree_scope_updates(scope);
    crate::region::forget_tree_scope(scope);
}
