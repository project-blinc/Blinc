//! Motion-subtree texture bake records — Phase 4.2 of the unified
//! property channel ([[project-reactive-architecture-v2]]).
//!
//! # Where this fits
//!
//! Phase 4.1 ships a *detection* pass
//! ([`crate::renderer::RenderTree::compute_subtree_texture_candidates`]) that marks
//! subtree roots whose only active animation source is
//! transform/opacity AND whose descendants carry no independent
//! dynamism. Phase 4.3 will wire a GPU-side bake call so the walker
//! emits primitives for a candidate subtree *once*, into an offscreen
//! `LayerTexture`, and emits a single texture-blit primitive on
//! subsequent frames while the motion is in flight.
//!
//! Phase 4.2 — this module — sits between detection and baking. It
//! provides the **GPU-agnostic bookkeeping layer**: a per-tree
//! registry that tracks which detected candidates have actually been
//! baked, plus a lifecycle state machine
//! ([`MotionSubtreeBakeRecord`]).
//!
//! ## Why this exists as its own substep
//!
//! Without bookkeeping the walker can't tell on any given frame
//! whether a candidate has a live texture (→ emit a blit primitive)
//! or whether the texture has never been baked / was invalidated (→
//! recurse and emit normally). The GPU-side
//! `motion_subtree_textures: NodeMap<LayerTexture>` map
//! (P4.3 lands it on `WindowedContext`, parallel to the existing
//! `css_composited_textures`) is GPU-only; this metadata side keeps
//! the policy decisions testable without spinning up a wgpu device.
//!
//! P4.4 (invalidation triggers) will call
//! [`crate::renderer::RenderTree::invalidate_motion_subtree_bake`] from places like
//! `remove_subtree_nodes`, the binding-deltas pass, and the
//! structural-rebuild path. P4.5 (LRU eviction) will read
//! [`crate::renderer::RenderTree::motion_subtree_bake_count`] for global memory
//! accounting.

use std::collections::{HashMap, HashSet};

use crate::element::ElementBounds;
use crate::tree::LayoutNodeId;
use crate::tree::{NodeMap, NodeSet};

/// Lifecycle state of a motion-subtree texture bake.
///
/// The walker, [`crate::renderer::RenderTree::compute_subtree_texture_candidates`],
/// and the future P4.3 bake hook coordinate via state transitions
/// rather than presence/absence in the map alone — the policy
/// distinction between "candidate but never baked yet" and
/// "candidate, baked, ready to blit" is what decides whether the
/// walker recurses into the subtree this frame or emits a single
/// texture-blit primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionBakeState {
    /// Detected as a candidate this frame, but no GPU bake has
    /// happened yet. The walker emits primitives normally so the
    /// subtree paints on screen; the end-of-paint P4.3 hook
    /// rasterizes the just-emitted primitives into an offscreen
    /// texture and flips this to [`MotionBakeState::Baked`].
    Pending,

    /// GPU texture is live in the per-window cache (keyed by the
    /// same `LayoutNodeId`). The walker emits a single texture-blit
    /// primitive that consumes the cached pixels with the current
    /// frame's parent transform/opacity applied at composite time.
    Baked,

    /// A P4.4 invalidation trigger fired (descendant structural
    /// rebuild, non-transform binding fire, descendant CSS animation
    /// toggle, etc.). The walker reverts to normal emission on the
    /// next frame; if the node is still a candidate, the bake hook
    /// re-rasterizes and flips back to [`MotionBakeState::Baked`].
    Invalidated,
}

/// Per-subtree bookkeeping for a motion-bound texture bake.
#[derive(Debug, Clone, Copy)]
pub struct MotionSubtreeBakeRecord {
    /// Tree-space bounds the texture was sized against at bake time.
    /// Used by the blit primitive to position the cached pixels and
    /// by P4.4 to detect bounds-change → invalidate.
    pub bounds: ElementBounds,
    /// `RenderTree.build_generation` watermark captured when the
    /// record was inserted. A structural rebuild moves the
    /// generation forward and the demote pass drops stale records
    /// even when their node still happens to be a candidate.
    pub build_generation: u64,
    /// Lifecycle state — see [`MotionBakeState`].
    pub state: MotionBakeState,
}

impl MotionSubtreeBakeRecord {
    /// Convenience: create a fresh `Pending` record for the supplied
    /// bounds and generation. Inserted by
    /// [`crate::renderer::RenderTree::prepare_motion_subtree_bake`].
    pub fn pending(bounds: ElementBounds, build_generation: u64) -> Self {
        Self {
            bounds,
            build_generation,
            state: MotionBakeState::Pending,
        }
    }

    /// True when the record reflects a live cached texture that the
    /// walker should blit instead of re-emitting.
    pub fn is_baked(&self) -> bool {
        matches!(self.state, MotionBakeState::Baked)
    }
}

/// Tree-side registry of motion-subtree bake records.
///
/// Wrapped in a [`std::cell::RefCell`] inside [`crate::renderer::RenderTree`]
/// so the walker can mutate state during paint without taking
/// `&mut self`. Cleared per-frame by
/// [`crate::renderer::RenderTree::demote_lapsed_motion_bake_records`]
/// after the detection pass populates the live candidate set.
#[derive(Debug, Default)]
pub struct MotionSubtreeBakeRegistry {
    records: NodeMap<MotionSubtreeBakeRecord>,
}

impl MotionSubtreeBakeRegistry {
    /// Empty registry. Same shape as `Default::default()`; exists for
    /// symmetry with [`crate::state_style_table::StateStyleTable::empty`].
    pub fn empty() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a `Pending` record. Idempotent — calling
    /// twice on the same node with the same bounds is a no-op modulo
    /// re-stamping the generation. Returns `true` if a record was
    /// newly inserted (or its state transitioned away from Pending),
    /// `false` if the existing record was already Pending and was
    /// just refreshed.
    pub fn prepare(
        &mut self,
        node: LayoutNodeId,
        bounds: ElementBounds,
        build_generation: u64,
    ) -> bool {
        match self.records.get_mut(&node) {
            Some(record) if matches!(record.state, MotionBakeState::Pending) => {
                record.bounds = bounds;
                record.build_generation = build_generation;
                false
            }
            _ => {
                self.records.insert(
                    node,
                    MotionSubtreeBakeRecord::pending(bounds, build_generation),
                );
                true
            }
        }
    }

    /// Flip a record to [`MotionBakeState::Baked`]. P4.3 calls this
    /// after the GPU rasterization succeeds; before that point the
    /// walker continues to emit primitives normally so the subtree
    /// stays visible while the texture is being baked. Returns
    /// `true` when a transition actually happened.
    pub fn mark_baked(&mut self, node: LayoutNodeId) -> bool {
        if let Some(record) = self.records.get_mut(&node)
            && !matches!(record.state, MotionBakeState::Baked)
        {
            record.state = MotionBakeState::Baked;
            return true;
        }
        false
    }

    /// Flip a record to [`MotionBakeState::Invalidated`]. P4.4
    /// invalidation triggers call this; the walker reverts to normal
    /// emission on the next paint. Returns `true` when a transition
    /// actually happened.
    pub fn invalidate(&mut self, node: LayoutNodeId) -> bool {
        if let Some(record) = self.records.get_mut(&node)
            && !matches!(record.state, MotionBakeState::Invalidated)
        {
            record.state = MotionBakeState::Invalidated;
            return true;
        }
        false
    }

    /// Drop a node's record entirely. Used by demotion (node left
    /// the candidate set) and by P4.4 structural-rebuild
    /// invalidation. Returns the dropped record if there was one.
    pub fn remove(&mut self, node: LayoutNodeId) -> Option<MotionSubtreeBakeRecord> {
        self.records.remove(&node)
    }

    /// Lookup. Returns `None` for nodes the bake hook hasn't seen.
    pub fn get(&self, node: LayoutNodeId) -> Option<MotionSubtreeBakeRecord> {
        self.records.get(&node).copied()
    }

    /// Number of tracked records — diagnostics / tests / P4.5 LRU
    /// pressure heuristics.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True when the registry tracks no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterator over (node, record) pairs — diagnostics / tests.
    pub fn iter(&self) -> impl Iterator<Item = (&LayoutNodeId, &MotionSubtreeBakeRecord)> {
        self.records.iter()
    }

    /// Drop every record whose node is NOT in `active_candidates`.
    /// Called once per frame from
    /// [`crate::renderer::RenderTree::compute_subtree_texture_candidates`]
    /// after the live candidate set is updated; the demoted records
    /// signal P4.3's GPU side to release the corresponding pooled
    /// textures back to the [`LayerTextureCache`](https://docs.rs/wgpu)
    /// pool. Returns the demoted node ids so callers (eventually the
    /// `WindowedContext` GPU-release pass) can act on them.
    pub fn demote_lapsed(&mut self, active_candidates: &NodeSet) -> Vec<LayoutNodeId> {
        let mut demoted = Vec::new();
        self.records.retain(|node, _| {
            if active_candidates.contains(node) {
                true
            } else {
                demoted.push(*node);
                false
            }
        });
        demoted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::StableNodeId;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> ElementBounds {
        ElementBounds {
            x,
            y,
            width: w,
            height: h,
        }
    }

    /// Builds a distinct fake LayoutNodeId by deriving from ROOT and
    /// hashing through StableNodeId, then mapping back via the
    /// LayoutNodeId namespace. Tests don't actually need a real
    /// layout-tree node — the registry keys on `LayoutNodeId` but
    /// doesn't traverse anything itself, so any distinct value works.
    /// We pull from a tiny in-test SlotMap so the keys are valid.
    fn fake_node(slot: &mut slotmap::SlotMap<LayoutNodeId, ()>) -> LayoutNodeId {
        slot.insert(())
    }

    #[test]
    fn prepare_inserts_pending_record() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let node = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        assert!(reg.is_empty());
        let inserted = reg.prepare(node, bounds(0.0, 0.0, 100.0, 50.0), 1);
        assert!(inserted, "first prepare must insert");
        assert_eq!(reg.len(), 1);

        let record = reg.get(node).expect("record present");
        assert_eq!(record.state, MotionBakeState::Pending);
        assert_eq!(record.build_generation, 1);
        assert_eq!(record.bounds.width, 100.0);
    }

    #[test]
    fn prepare_idempotent_when_already_pending() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let node = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        reg.prepare(node, bounds(0.0, 0.0, 100.0, 50.0), 1);
        let inserted = reg.prepare(node, bounds(0.0, 0.0, 200.0, 80.0), 2);
        assert!(!inserted, "repeat prepare on Pending refreshes in place");
        assert_eq!(reg.len(), 1);
        let record = reg.get(node).unwrap();
        assert_eq!(record.bounds.width, 200.0, "bounds must update on refresh");
        assert_eq!(record.build_generation, 2);
    }

    #[test]
    fn mark_baked_flips_state_once() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let node = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        reg.prepare(node, bounds(0.0, 0.0, 100.0, 50.0), 1);
        assert!(reg.mark_baked(node));
        assert_eq!(reg.get(node).unwrap().state, MotionBakeState::Baked);
        assert!(reg.get(node).unwrap().is_baked());
        // Second call is a no-op.
        assert!(!reg.mark_baked(node));
    }

    #[test]
    fn mark_baked_on_missing_node_is_noop() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let node = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        assert!(!reg.mark_baked(node));
        assert!(reg.is_empty());
    }

    #[test]
    fn invalidate_flips_baked_to_invalidated() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let node = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        reg.prepare(node, bounds(0.0, 0.0, 100.0, 50.0), 1);
        reg.mark_baked(node);

        assert!(reg.invalidate(node));
        assert_eq!(reg.get(node).unwrap().state, MotionBakeState::Invalidated);
        assert!(!reg.get(node).unwrap().is_baked());
        // Idempotent on repeat.
        assert!(!reg.invalidate(node));
    }

    #[test]
    fn remove_drops_record() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let node = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        reg.prepare(node, bounds(0.0, 0.0, 100.0, 50.0), 1);
        assert!(reg.remove(node).is_some());
        assert!(reg.is_empty());
        assert!(reg.remove(node).is_none(), "second remove returns None");
    }

    #[test]
    fn demote_lapsed_drops_only_non_candidates() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let kept = fake_node(&mut slot);
        let dropped = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        reg.prepare(kept, bounds(0.0, 0.0, 100.0, 50.0), 1);
        reg.prepare(dropped, bounds(0.0, 50.0, 100.0, 50.0), 1);
        reg.mark_baked(kept);
        reg.mark_baked(dropped);

        let mut active = crate::tree::NodeSet::default();
        active.insert(kept);

        let demoted = reg.demote_lapsed(&active);
        assert_eq!(demoted.len(), 1);
        assert_eq!(demoted[0], dropped);
        assert_eq!(reg.len(), 1);
        assert!(reg.get(kept).is_some());
        assert!(reg.get(dropped).is_none());
    }

    #[test]
    fn demote_lapsed_empty_candidate_set_clears_registry() {
        let mut slot: slotmap::SlotMap<LayoutNodeId, ()> = slotmap::SlotMap::with_key();
        let n1 = fake_node(&mut slot);
        let n2 = fake_node(&mut slot);

        let mut reg = MotionSubtreeBakeRegistry::empty();
        reg.prepare(n1, bounds(0.0, 0.0, 100.0, 50.0), 1);
        reg.prepare(n2, bounds(0.0, 50.0, 100.0, 50.0), 1);

        let demoted = reg.demote_lapsed(&HashSet::default());
        assert_eq!(demoted.len(), 2);
        assert!(reg.is_empty());
    }

    #[test]
    fn record_pending_helper_sets_state() {
        // StableNodeId / build_generation are u64 so any value works.
        let _ = StableNodeId::ROOT; // smoke import
        let record = MotionSubtreeBakeRecord::pending(bounds(1.0, 2.0, 3.0, 4.0), 42);
        assert_eq!(record.state, MotionBakeState::Pending);
        assert_eq!(record.build_generation, 42);
        assert_eq!(record.bounds.x, 1.0);
    }
}
