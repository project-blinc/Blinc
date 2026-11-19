//! A value that counts the times it is borrowed mutably.
//!
//! What is derived from the value (an index over it, a plan made from it) is valid for as long as
//! the revision is the one it was derived at. A mutable borrow that changes nothing still counts,
//! so a revision only ever errs towards deriving again.
//!
//! A map of values that name their own key also keeps a log of what changed since it was last
//! taken: the keys inserted, and those whose structure changed, with whether the font inputs did.
//! A pass that would otherwise look at every value can look at those. A borrow that is not one of
//! the logged kinds (iterating mutably, clearing) makes the log say "everything".

use super::structure::Structural;
use indexmap::{Equivalent, IndexMap};
use std::hash::{BuildHasher, Hash};
use std::ops::{Deref, DerefMut};

pub(crate) struct Tracked<T, K = ()> {
    value: T,
    revision: u64,
    /// Keys inserted or changed since the log was taken, with whether the font inputs changed.
    changed: Vec<(K, bool)>,
    /// Something changed that the log does not name.
    changed_all: bool,
}

/// Entries kept in the log before it gives up and says "everything".
const LOG_LIMIT: usize = 1 << 16;

impl<T, K> Tracked<T, K> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value,
            revision: 0,
            changed: Vec::new(),
            // Nothing is derived yet: the first pass looks at everything.
            changed_all: false,
        }
    }

    /// What changed since the log was last taken, or `None` when it cannot be said: look at
    /// everything. Taking empties the log.
    pub(crate) fn take_changes(&mut self) -> Option<Vec<(K, bool)>> {
        if std::mem::take(&mut self.changed_all) {
            self.changed.clear();
            return None;
        }
        Some(std::mem::take(&mut self.changed))
    }

    fn log(&mut self, key: K, font: bool) {
        if self.changed.len() >= LOG_LIMIT {
            self.changed_all = true;
            self.changed.clear();
        } else {
            self.changed.push((key, font));
        }
    }

    /// How many times the value has been borrowed mutably.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

/// A mutable borrow of one value in a tracked map. The revision moves when it is dropped, and
/// only if the part of the value that derived data reads has changed.
pub(crate) struct Touched<'a, K: Copy, V: Structural> {
    key: K,
    value: &'a mut V,
    revision: &'a mut u64,
    changed: &'a mut Vec<(K, bool)>,
    changed_all: &'a mut bool,
    before: Option<V::Structure>,
}

impl<K: Copy, V: Structural> Deref for Touched<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value
    }
}

impl<K: Copy, V: Structural> DerefMut for Touched<'_, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value
    }
}

impl<K: Copy, V: Structural> Drop for Touched<'_, K, V> {
    fn drop(&mut self) {
        if let Some(before) = self.before.take() {
            let after = self.value.structure();
            if before != after {
                *self.revision = self.revision.wrapping_add(1);
                if self.changed.len() >= LOG_LIMIT {
                    *self.changed_all = true;
                    self.changed.clear();
                } else {
                    self.changed
                        .push((self.key, V::font_changed(&before, &after)));
                }
            }
        }
    }
}

impl<K: Hash + Eq + Copy, V: Structural, S: BuildHasher> Tracked<IndexMap<K, V, S>, K> {
    /// A mutable borrow of the value at `key`; see [`Touched`].
    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<Touched<'_, K, V>>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        let Tracked {
            value,
            revision,
            changed,
            changed_all,
        } = self;
        let (_, key, value) = value.get_full_mut(key)?;
        let key = *key;
        let before = Some(value.structure());
        Some(Touched {
            key,
            value,
            revision,
            changed,
            changed_all,
            before,
        })
    }

    /// Insert a value, logged as inserted.
    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.revision = self.revision.wrapping_add(1);
        self.log(key, false);
        self.value.insert(key, value)
    }

    /// Remove a value. Nothing derived from the others changes by it, so the log stays as it is.
    pub(crate) fn swap_remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        Q: ?Sized + Hash + Equivalent<K>,
    {
        self.revision = self.revision.wrapping_add(1);
        self.value.swap_remove(key)
    }
}

impl<T: Default, K> Default for Tracked<T, K> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T, K> Deref for Tracked<T, K> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T, K> DerefMut for Tracked<T, K> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision = self.revision.wrapping_add(1);
        // A borrow the log has no name for: whoever reads it looks at everything.
        self.changed_all = true;
        &mut self.value
    }
}

impl<'a, K, V, S, L> IntoIterator for &'a Tracked<IndexMap<K, V, S>, L> {
    type Item = (&'a K, &'a V);
    type IntoIter = indexmap::map::Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
    }
}

impl<'a, K, V, S, L> IntoIterator for &'a Tracked<std::collections::HashMap<K, V, S>, L> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::collections::hash_map::Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn reads_leave_the_revision_and_writes_move_it() {
        let mut map: Tracked<HashMap<u32, u32>> = Tracked::default();
        let start = map.revision();
        map.insert(1, 2);
        assert_ne!(map.revision(), start);
        let written = map.revision();
        let _ = map.get(&1);
        let _ = map.len();
        let _ = map.contains_key(&1);
        for _ in &map {}
        assert_eq!(map.revision(), written);
        let _ = map.get_mut(&1);
        assert_ne!(map.revision(), written);
    }
}
