//! Keep each tree's queued writes and rebuilds in its own namespace.
use super::*;

fn take_for<T>(queue: &Mutex<Vec<T>>, scope: u64, owner: impl Fn(&T) -> u64) -> Vec<T> {
    let mut result = Vec::new();
    let mut discarded = Vec::new();
    {
        let mut queue = queue.lock().unwrap();
        let mut foreign = Vec::new();
        for update in std::mem::take(&mut *queue) {
            let target = owner(&update);
            if target == 0 || target == scope {
                result.push(update);
            } else if crate::tree::scope::is_live(target) {
                foreign.push(update);
            } else {
                discarded.push(update);
            }
        }
        *queue = foreign;
    }
    drop(discarded);
    result
}

pub(crate) fn take_properties_for(scope: u64) -> Vec<PartialPropertyUpdate> {
    take_for(&PENDING_PARTIAL_PROP_UPDATES, scope, |update| {
        update.tree_scope
    })
}

pub(crate) fn take_rebuilds_for(scope: u64) -> Vec<PendingSubtreeRebuild> {
    take_for(&PENDING_SUBTREE_REBUILDS, scope, |update| update.tree_scope)
}

pub(crate) fn has_rebuilds_for(scope: u64) -> bool {
    PENDING_SUBTREE_REBUILDS
        .lock()
        .unwrap()
        .iter()
        .any(|update| update.tree_scope == 0 || update.tree_scope == scope)
}

pub(crate) fn discard_tree_scope_updates(scope: u64) {
    fn discard<T>(queue: &Mutex<Vec<T>>, remove: impl Fn(&T) -> bool) {
        let discarded;
        {
            let mut queue = queue.lock().unwrap();
            let (gone, live) = std::mem::take(&mut *queue).into_iter().partition(remove);
            *queue = live;
            discarded = gone;
        }
        drop(discarded);
    }
    discard(&PENDING_PARTIAL_PROP_UPDATES, |update| {
        update.tree_scope == scope
    });
    discard(&PENDING_SUBTREE_REBUILDS, |update| {
        update.tree_scope == scope
    });
    let removed = {
        let mut updaters = STATEFUL_BASE_UPDATERS.lock().unwrap();
        let keys: Vec<_> = updaters
            .keys()
            .filter(|(owner, _)| *owner == scope)
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|key| updaters.remove(&key))
            .collect::<Vec<_>>()
    };
    drop(removed);
}
