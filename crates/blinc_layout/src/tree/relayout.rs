//! Laying out again only what an edit can reach.
//!
//! An edit lists the node whose layout it may change. At layout time each listed node, after
//! the listed nodes below it, is asked again what its parent asked of it. When every answer is the same the parent
//! is unaffected: the node's subtree has been laid out again by the replay, and that subtree is
//! rounded. When an answer differs, the parent is laid out again in turn, and so on up. Only an
//! edit whose answers change all the way up lays the tree out from its root.

use std::sync::OnceLock;

use taffy::{AvailableSpace, RunMode, Size, compute_root_layout};

use super::LayoutTree;
use super::store::{Pass, clear_cache, round_subtree, same_answer, tid};
use crate::tree::LayoutNodeId;

/// `BLINC_LAYOUT_BOUNDARIES=0` lays out from the root after every edit, as taffy's own tree does.
/// Safe to run with; slower.
fn boundaries_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("BLINC_LAYOUT_BOUNDARIES").map_or(true, |v| v != "0"))
}

/// `BLINC_LAYOUT_CHECK=1` lays the tree out from scratch after every layout and panics when a
/// node's layout differs. For tests; much slower.
fn check_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("BLINC_LAYOUT_CHECK").is_ok_and(|v| v == "1"))
}

impl LayoutTree {
    /// List a node whose layout an edit may have changed.
    pub(super) fn mark_changed(&mut self, id: LayoutNodeId) {
        if let Some(node) = self.nodes.get_mut(id)
            && !node.queued
        {
            node.queued = true;
            self.changed.push(id);
        }
    }

    /// Lay out the changed nodes again, from `root` when an edit reaches it.
    pub(super) fn relayout(&mut self, root: LayoutNodeId, space: Size<AvailableSpace>, same: bool) {
        if same && boundaries_enabled() {
            // A subtree may sit in another tree than `root`'s, so each is rounded even when
            // the root is laid out and rounded whole after.
            for id in self.settle_changed() {
                self.round_from(id, root);
            }
            if self.nodes[root].cache.is_empty() {
                self.layout_from(root, space);
            }
        } else {
            self.invalidate_changed();
            self.layout_from(root, space);
        }
        if check_enabled() {
            let differ = self.check_from_scratch(root, space);
            assert_eq!(differ, 0, "layout differs from a layout from scratch");
        }
    }

    /// Clear the cache of every changed node and everything above it, as taffy's tree does.
    fn invalidate_changed(&mut self) {
        self.relayout_stamp = self.relayout_stamp.wrapping_add(1);
        let stamp = self.relayout_stamp;
        for id in std::mem::take(&mut self.changed) {
            let mut at = Some(id);
            if let Some(node) = self.nodes.get_mut(id) {
                node.queued = false;
            }
            while let Some(id) = at {
                let Some(node) = self.nodes.get_mut(id) else {
                    break;
                };
                if node.relaid == stamp {
                    break;
                }
                node.relaid = stamp;
                at = node.parent;
                clear_cache(&mut self.nodes, id);
            }
        }
    }

    /// Settle the changed nodes bottom-up. Returns the roots of the subtrees laid out again,
    /// below any node laid out again from above. A changed path that reaches a top leaves the
    /// top's cache empty, for whoever lays it out as a root.
    fn settle_changed(&mut self) -> Vec<LayoutNodeId> {
        self.relayout_stamp = self.relayout_stamp.wrapping_add(1);
        let stamp = self.relayout_stamp;
        let changed = std::mem::take(&mut self.changed);

        // Mark the path above each changed node; each node counts its children on a path.
        for &id in &changed {
            let Some(node) = self.nodes.get_mut(id) else {
                continue;
            };
            node.queued = false;
            node.edited = true;
            let mut at = id;
            loop {
                let node = &mut self.nodes[at];
                if node.on_spine {
                    break;
                }
                node.on_spine = true;
                let Some(parent) = node.parent else {
                    break;
                };
                self.nodes[parent].pending += 1;
                at = parent;
            }
        }

        // A node settles once every child on a path below it has.
        let mut ready: Vec<LayoutNodeId> = changed
            .into_iter()
            .filter(|&id| {
                self.nodes
                    .get(id)
                    .is_some_and(|n| n.on_spine && n.pending == 0)
            })
            .collect();
        let mut replayed = Vec::new();
        while let Some(id) = ready.pop() {
            let node = &mut self.nodes[id];
            node.on_spine = false;
            let parent = node.parent;
            let edited = std::mem::take(&mut node.edited);
            let mut up = false;
            if edited {
                node.relaid = stamp;
                clear_cache(&mut self.nodes, id);
                match parent {
                    // A parent laid out again asks afresh, so asking for it is wasted.
                    Some(p) if !self.nodes[p].edited && self.replay(id) => replayed.push(id),
                    Some(_) => up = true,
                    None => {}
                }
            }
            if let Some(p) = parent {
                let parent = &mut self.nodes[p];
                parent.edited |= up;
                parent.pending -= 1;
                if parent.pending == 0 {
                    ready.push(p);
                }
            }
        }

        // A subtree under one laid out again from above is rounded with it.
        replayed.retain(|&id| {
            let mut at = self.nodes[id].parent;
            while let Some(p) = at {
                if self.nodes[p].relaid == stamp {
                    return false;
                }
                at = self.nodes[p].parent;
            }
            true
        });
        replayed
    }

    /// Ask a node again what its parent asked of it, laying its subtree out again. Whether
    /// every answer is the same.
    fn replay(&mut self, id: LayoutNodeId) -> bool {
        let node = &mut self.nodes[id];
        if node.unreplayable || node.asked.is_empty() {
            return false;
        }
        // Asking logs each query on the node again, in the same order.
        let asked = std::mem::take(&mut node.asked);
        let mut pass = Pass {
            nodes: &mut self.nodes,
        };
        let mut overflow = None;
        let same = asked.iter().all(|(input, old)| {
            let new = pass.child_layout(tid(id), *input, None);
            if input.run_mode == RunMode::PerformLayout {
                overflow = Some(new.scrollable_overflow_rect);
            }
            same_answer(&pass.nodes[id].style, input, old, &new)
        });
        // The parent copies the overflow of its last layout of the node into the node's own
        // layout; it may differ on an axis the parent does not read.
        if same && let Some(rect) = overflow {
            self.nodes[id].unrounded.scrollable_overflow_rect = rect;
        }
        same
    }

    pub(super) fn layout_from(&mut self, root: LayoutNodeId, space: Size<AvailableSpace>) {
        let mut pass = Pass {
            nodes: &mut self.nodes,
        };
        compute_root_layout(&mut pass, tid(root), space);
        round_subtree(&mut self.nodes, root, (0.0, 0.0));
    }

    /// Round a subtree laid out again, against its parent's unrounded position in `root`, the
    /// origin a layout from the root rounds against.
    fn round_from(&mut self, id: LayoutNodeId, root: LayoutNodeId) {
        let (mut x, mut y) = (0.0, 0.0);
        let mut at = self.nodes[id].parent;
        while let Some(p) = at {
            let node = &self.nodes[p];
            x += node.unrounded.location.x;
            y += node.unrounded.location.y;
            at = if p == root { None } else { node.parent };
        }
        round_subtree(&mut self.nodes, id, (x, y));
    }

    /// Lay the tree out from scratch and log every node whose layout differs from the one the
    /// last layout left. The number of nodes that differ.
    pub fn check_from_scratch(&mut self, root: LayoutNodeId, space: Size<AvailableSpace>) -> usize {
        let mut before = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            before.push((id, node.layout));
            stack.extend(node.children.iter().copied());
        }
        let ids: Vec<LayoutNodeId> = self.nodes.keys().collect();
        for id in ids {
            clear_cache(&mut self.nodes, id);
        }
        self.layout_from(root, space);
        let mut differ = 0;
        for (id, old) in before {
            let new = self.nodes[id].layout;
            if new != old {
                differ += 1;
                if differ <= 8 {
                    tracing::warn!(
                        target: "blinc_layout::relayout",
                        ?id,
                        old = ?(old.location, old.size),
                        new = ?(new.location, new.size),
                        "layout differs from a layout from scratch"
                    );
                }
            }
        }
        differ
    }
}
