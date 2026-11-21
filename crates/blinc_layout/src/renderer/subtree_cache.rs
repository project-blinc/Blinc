//! Answers about subtrees that the paint, the hit test and the scroll rows ask every frame and
//! that change only with the layout, the render nodes or the motion bindings.
//!
//! Each answer is kept against the revisions of all three, the same key the child index is made
//! at, and all are dropped together when any of them moves. While a layout animation runs, content
//! moves by the clock, which no revision records, so nothing is kept and callers ask afresh.

use super::RenderTree;
use super::child_index::Key;
use crate::tree::LayoutNodeId;
use rustc_hash::FxHashMap;
use std::hash::Hash;

/// What a subtree's culling reads: the union of the absolute layout rects of the node and its
/// descendants down to those that clip, as `[x0, y0, x1, y1]`. `None` when something among them
/// moves its content or has no bounds, so the subtree is never culled.
#[derive(Clone, Copy)]
pub(super) struct Extent {
    pub(super) rect: Option<[f32; 4]>,
}

#[derive(Default)]
pub(super) struct SubtreeCache {
    key: Option<Key>,
    pub(super) extents: FxHashMap<LayoutNodeId, Extent>,
    /// Whether every node under a node passes the scroll-row checks that do not change by the
    /// frame.
    pub(super) qualifies: FxHashMap<LayoutNodeId, bool>,
    /// `later_content_overlaps_with` by node and whether every kind of content counts.
    pub(super) overlaps: FxHashMap<(LayoutNodeId, bool), bool>,
}

impl SubtreeCache {
    fn at(&mut self, key: Key) -> &mut Self {
        if self.key != Some(key) {
            self.key = Some(key);
            self.extents.clear();
            self.qualifies.clear();
            self.overlaps.clear();
        }
        self
    }
}

impl RenderTree {
    /// The answer kept in `map` for `at`, or `compute`'s, kept when the tree's revisions allow.
    /// `compute` may ask the cache again, for other entries.
    pub(super) fn memo<K: Hash + Eq + Copy, T: Copy>(
        &self,
        map: fn(&mut SubtreeCache) -> &mut FxHashMap<K, T>,
        at: K,
        compute: impl FnOnce() -> T,
    ) -> T {
        let Some(key) = self.index_key() else {
            return compute();
        };
        if let Some(value) = map(self.subtree_cache.borrow_mut().at(key)).get(&at) {
            return *value;
        }
        let value = compute();
        map(self.subtree_cache.borrow_mut().at(key)).insert(at, value);
        value
    }
}
