//! Entry points for tree creation + incremental update.
//!
//! Three flavours of entry, all `pub` (forming the user-facing
//! `RenderTree` build surface):
//!
//! - `from_element` / `from_element_with_registry` — fresh-tree
//!   constructors. Stamp the tree hash, dispatch to `build_element`,
//!   return the populated tree. The `_with_registry` variant binds
//!   to a shared `ElementRegistry` so id-based query operations on
//!   the windowed context can hit the same store.
//! - `update_if_changed` — coarse-grained refresh: hash-compare, full
//!   rebuild on mismatch. Preserves node-state / scroll / motion /
//!   active scroll-ref data across the rebuild.
//! - `incremental_update` — fine-grained refresh: walks per-node
//!   hashes via `analyze_changes`, then dispatches to either
//!   `rebuild_changed_subtrees` (when child counts changed),
//!   `update_render_props_in_place` (when only visual / layout / handler
//!   props changed), or no-op. Returns an `UpdateResult` so the
//!   caller knows whether to recompute layout.
//!
//! `UpdateResult` itself stays in `renderer/mod.rs` next to the
//! `RenderTree` struct definition.

use std::sync::Arc;

use crate::diff::DivHash;
use crate::div::ElementBuilder;
use crate::selector::ElementRegistry;
use crate::tree::LayoutTree;

use super::super::{RenderTree, UpdateResult};

impl RenderTree {
    /// Build a render tree from an element builder.
    ///
    /// `build_element` runs the three-phase walk
    /// (layout-build → mint stable ids → collect render props) so
    /// callers don't need to call `mint_stable_ids_walk` /
    /// `auto_fill_animation_stable_keys` separately.
    pub fn from_element<E: ElementBuilder>(element: &E) -> Self {
        let mut tree = Self::new();
        tree.tree_hash = Some(DivHash::compute_element_tree(element));
        tree.tree_topology_hash = Some(DivHash::compute_topology_tree(element));
        tree.build_element(element);
        tree
    }

    /// Build a render tree from an element builder with a shared element registry
    ///
    /// This ensures element IDs are registered to the shared registry during build,
    /// rather than to an internal registry that gets replaced later.
    pub fn from_element_with_registry<E: ElementBuilder>(
        element: &E,
        registry: Arc<ElementRegistry>,
    ) -> Self {
        let mut tree = Self::new();
        // Clear the shared registry before building to avoid duplicate ID warnings
        registry.clear();
        // Set shared registry BEFORE building so IDs are registered correctly
        tree.element_registry = registry;
        tree.tree_hash = Some(DivHash::compute_element_tree(element));
        tree.tree_topology_hash = Some(DivHash::compute_topology_tree(element));
        tree.build_element(element);
        tree
    }

    // `tree_hash`, `matches_element` moved to `renderer/queries.rs`.

    /// Update the render tree from a new element if it has changed
    ///
    /// Returns `true` if the tree was updated, `false` if no changes were detected.
    /// This is an optimization to skip full rebuilds when the UI hasn't changed.
    pub fn update_if_changed<E: ElementBuilder>(&mut self, element: &E) -> bool {
        self.invalidate_mouse_move_pipeline_cache();
        let new_hash = DivHash::compute_element_tree(element);
        let new_topology_hash = DivHash::compute_topology_tree(element);

        // If hash matches, no changes - skip rebuild
        if self.tree_hash == Some(new_hash) && self.tree_topology_hash == Some(new_topology_hash) {
            return false;
        }

        // Hash differs - need to rebuild
        // For now, do a full rebuild. Future optimization: use diff for incremental updates
        self.tree_hash = Some(new_hash);
        self.tree_topology_hash = Some(new_topology_hash);

        // Clear existing data that will be repopulated during rebuild.
        //
        // NOT cleared anymore:
        // - `handler_registry` is now keyed by `StableNodeId`. The
        //   new build overwrites entries at the same stable id with
        //   fresh closures (so the registry is never empty at a
        //   key that survived the rebuild), and
        //   `sweep_stale_handlers` at the end of `build_element`
        //   evicts entries whose stable id didn't survive. This
        //   replaces the destructive wipe that used to churn
        //   closures on every rebuild.
        self.render_nodes.clear();
        self.element_registry.clear();
        // Clear scroll_refs HashMap (node_id keyed) - it will be repopulated during rebuild
        // but active_scroll_refs persists for process_pending_scroll_refs
        self.scroll_refs.clear();

        // Preserve node_states, scroll_offsets, scroll_physics, motion_bindings, active_scroll_refs
        // as these should survive rebuilds

        // Rebuild the layout tree. `build_element` runs the three-
        // phase walk (layout build → mint stable ids → collect
        // render props), so handlers registered during collect see
        // their stable ids already and the survival maps stay
        // coherent.
        self.layout_tree = LayoutTree::new();
        self.build_element(element);

        true
    }

    /// Incrementally update the render tree from a new element
    ///
    /// This method attempts to apply minimal updates based on what changed:
    /// - If nothing changed: returns NoChanges, no work done
    /// - If only visual props changed: updates render props, returns VisualOnly
    /// - If layout changed: updates props + needs relayout, returns LayoutChanged
    /// - If children changed: rebuilds affected subtrees, returns ChildrenChanged
    ///
    /// The caller should:
    /// - NoChanges: skip layout and just render
    /// - VisualOnly: skip layout, just render with updated props
    /// - LayoutChanged: call compute_layout(), then render
    /// - ChildrenChanged: call compute_layout(), then render
    pub fn incremental_update<E: ElementBuilder>(&mut self, element: &E) -> UpdateResult {
        let new_tree_hash = DivHash::compute_element_tree(element);
        let new_topology_hash = DivHash::compute_topology_tree(element);

        // Quick path: if tree hash matches, nothing changed
        if self.tree_hash == Some(new_tree_hash)
            && self.tree_topology_hash == Some(new_topology_hash)
        {
            return UpdateResult::NoChanges;
        }

        // Tree hash diverged — handlers / cursor styles / stylesheet
        // rules may all change as part of this update. Invalidate the
        // bare-mouse-move pipeline cache so the windowed app's prelude
        // re-derives the early-return predicate on the next move.
        self.invalidate_mouse_move_pipeline_cache();

        // Tree hash differs - analyze what kind of changes occurred
        // Walk the tree comparing per-node hashes to detect change categories
        let Some(root_id) = self.root else {
            // No existing tree - build it (this is initial build, not an update)
            self.tree_hash = Some(new_tree_hash);
            self.tree_topology_hash = Some(new_topology_hash);
            self.root = Some(self.build_element(element));
            return UpdateResult::ChildrenChanged;
        };

        // Analyze changes by comparing stored hashes with new element
        let changes = self.analyze_changes(element, root_id);

        tracing::trace!(
            "incremental_update: layout={}, visual={}, children={}, handlers={}",
            changes.layout,
            changes.visual,
            changes.children,
            changes.handlers
        );

        // Update tree hash
        self.tree_hash = Some(new_tree_hash);
        self.tree_topology_hash = Some(new_topology_hash);

        // Determine update strategy based on change category
        if changes.children {
            // Children changed - rebuild affected subtrees in place
            // Walk tree and rebuild nodes with changed children.
            // `rebuild_children_in_place` runs its own mint pass
            // between layout-build and collect, so we don't need to
            // mint again here.
            self.rebuild_changed_subtrees(element, root_id);
            // Also update props for nodes that didn't get rebuilt
            self.update_render_props_in_place(element, root_id);
            UpdateResult::ChildrenChanged
        } else if changes.layout {
            // Layout changed - update props and need relayout
            self.update_render_props_in_place(element, root_id);
            UpdateResult::LayoutChanged
        } else if changes.visual || changes.handlers {
            // Only visual/handler changes - update props in place, no layout needed
            self.update_render_props_in_place(element, root_id);
            UpdateResult::VisualOnly
        } else {
            // No changes detected (shouldn't happen if tree hash differed)
            UpdateResult::NoChanges
        }
    }
}
