//! A value that counts the times it is borrowed mutably.
//!
//! What is derived from the value (an index over it, a plan made from it) is valid for as long as
//! the revision is the one it was derived at. A mutable borrow that changes nothing still counts,
//! so a revision only ever errs towards deriving again.

use indexmap::IndexMap;
use std::ops::{Deref, DerefMut};

pub(crate) struct Tracked<T> {
    value: T,
    revision: u64,
}

impl<T> Tracked<T> {
    pub(crate) fn new(value: T) -> Self {
        Self { value, revision: 0 }
    }

    /// How many times the value has been borrowed mutably.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

impl<T: Default> Default for Tracked<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Deref for Tracked<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision = self.revision.wrapping_add(1);
        &mut self.value
    }
}

impl<'a, K, V, S> IntoIterator for &'a Tracked<IndexMap<K, V, S>> {
    type Item = (&'a K, &'a V);
    type IntoIter = indexmap::map::Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
    }
}

impl<'a, K, V, S> IntoIterator for &'a Tracked<std::collections::HashMap<K, V, S>> {
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
