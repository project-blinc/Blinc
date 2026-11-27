//! Minting `StableNodeId`s for the live layout tree.
//!
//! A node's stable id derives from its parent's, its place among the siblings that are not
//! rows, and its element id when that is unique among the siblings. A row of a list or branch
//! is named by its row key instead, so it keeps its id when rows come and go around it.
//! Changing one node's children therefore changes the ids of that node's descendants only.

use std::collections::HashMap;

use crate::selector::is_ref_assigned_id;
use crate::tree::{LayoutNodeId, StableNodeId};

use super::RenderTree;

impl RenderTree {
    /// Walk the live layout tree and (re)mint a `StableNodeId` for
    /// every node, populating `stable_to_layout` and `layout_to_stable`.
    ///
    /// Called after every full rebuild from `from_element*`. Stable
    /// ids derive from `(parent_stable, sibling_index, element_id_if_set)`,
    /// so call order + DOM structure + explicit element ids together
    /// determine the id: deterministic per frame, stable across
    /// rebuilds for the same structure.
    ///
    /// Caller responsibility: bump `build_generation` before calling so
    /// the eventual sweep can compare against stamps.
    pub(crate) fn mint_stable_ids_walk(&mut self) {
        let _tree_scope = self.layout_tree.enter_scope();
        // Wipe the maps; they're entirely regenerated each rebuild
        // (consumers that need cross-frame stability key by
        // `StableNodeId` directly; the mapping cache is rebuild-local).
        self.stable_to_layout.clear();
        self.layout_to_stable.clear();

        let Some(root_id) = self.root else { return };
        let root_widget_key = self.widget_key(root_id);
        let root_stable = StableNodeId::ROOT.derive_child(0, root_widget_key.as_deref());
        self.register_stable(root_stable, root_id);
        self.mint_descendants(root_id, root_stable);
    }

    /// Re-mint the stable ids under `parent` after its children changed, keeping every other
    /// node's. The ids of removed nodes must already be gone from the maps, as
    /// `remove_subtree_nodes` leaves them. Without an id for `parent`, re-mints the whole tree.
    pub(crate) fn mint_stable_ids_under(&mut self, parent: LayoutNodeId) {
        let Some(&stable) = self.layout_to_stable.get(&parent) else {
            self.mint_stable_ids_walk();
            return;
        };
        let _tree_scope = self.layout_tree.enter_scope();
        self.mint_descendants(parent, stable);
    }

    /// The element id that names `node`: the one it was built with.
    fn widget_key(&self, node: LayoutNodeId) -> Option<String> {
        self.element_registry
            .get_id(node)
            .filter(|id| !is_ref_assigned_id(id))
    }

    fn mint_descendants(&mut self, node: LayoutNodeId, stable: StableNodeId) {
        // The registration needs `&mut self`, so the descent keeps its own stack rather than
        // recursing through borrows of the layout tree.
        let mut stack: Vec<(LayoutNodeId, StableNodeId)> = vec![(node, stable)];
        while let Some((node, stable)) = stack.pop() {
            let children = self.layout_tree.children(node);
            // A row of a list or branch is named by its row, so it keeps its
            // id when rows come and go around it; the children that are not
            // rows count their place among themselves.
            let keys: Vec<(Option<String>, bool)> = children
                .iter()
                .map(|&child| match crate::region::row_key(child) {
                    Some(key) => (Some(key), true),
                    None => (self.widget_key(child), false),
                })
                .collect();
            let mut key_counts: HashMap<&str, usize> = HashMap::new();
            for (key, _) in &keys {
                if let Some(key) = key {
                    *key_counts.entry(key.as_str()).or_default() += 1;
                }
            }
            let mut slot = 0usize;
            for (&child, (widget_key, is_row)) in children.iter().zip(&keys) {
                let unique_key = widget_key
                    .as_deref()
                    .filter(|key| key_counts.get(*key) == Some(&1));
                let index = if *is_row { 0 } else { slot };
                if !*is_row {
                    slot += 1;
                }
                let child_stable = stable.derive_child(index, unique_key);
                self.register_stable(child_stable, child);
                stack.push((child, child_stable));
            }
        }
    }
}
