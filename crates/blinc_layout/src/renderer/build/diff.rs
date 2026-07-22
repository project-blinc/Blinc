//! Incremental change analysis + in-place render-prop updates.
//!
//! This is the diff side of the build flow:
//!
//! - `analyze_changes` / `analyze_changes_boxed` walk the new
//!   element tree against the stored one and classify each node's
//!   diff as `ChangeCategory::None`, `Visual`, or `Structural`. The
//!   classification drives whether `update_render_props_in_place`
//!   can patch the existing node or whether the subtree has to be
//!   torn down and rebuilt.
//! - `props_visually_equal` is the per-node fast comparator backing
//!   the `Visual`/`None` distinction — bails on any animation /
//!   motion / canvas-render-fn mismatch where a deep struct compare
//!   isn't safe.
//! - `update_render_props_in_place` /
//!   `update_render_props_in_place_boxed` carry the patch through:
//!   they re-derive `RenderProps` from the new builder, apply the
//!   stored stylesheet's base/state styles, and write the result
//!   back without touching the layout tree. When child counts diff,
//!   they fall back to `rebuild_children_in_place` (also here)
//!   which does a clean wipe + recollect under the same parent.

use crate::diff::{ChangeCategory, DivHash, render_props_eq};
use crate::div::ElementBuilder;
use crate::element::RenderProps;
use crate::tree::LayoutNodeId;
use std::collections::{HashMap, HashSet};

use super::super::{RenderNode, RenderTree};

impl RenderTree {
    /// Rebuild children of a node in place
    ///
    /// This removes old children and builds new ones from the provided element builders.
    ///
    /// Mint stable ids over the whole tree AFTER the new layout
    /// nodes exist and BEFORE collect runs — collect_render_props
    /// registers handlers / scroll physics / motion bindings at the
    /// freshly-minted stable ids. Without this ordering the
    /// handlers would key on a stable id that doesn't exist yet.
    pub(crate) fn rebuild_children_in_place(
        &mut self,
        parent_id: LayoutNodeId,
        new_children: &[Box<dyn ElementBuilder>],
    ) {
        if self.try_reconcile_keyed_children(parent_id, new_children) {
            return;
        }

        // Remove old children
        let old_children = self.layout_tree.children(parent_id);
        for child_id in &old_children {
            self.remove_subtree_nodes(*child_id);
        }
        self.layout_tree.clear_children(parent_id);

        // Layout-only pass: build the new subtree first so the mint
        // walk sees the complete new shape.
        let mut built: Vec<LayoutNodeId> = Vec::with_capacity(new_children.len());
        for child in new_children {
            let child_id = child.build(&mut self.layout_tree);
            self.layout_tree.add_child(parent_id, child_id);
            built.push(child_id);
        }

        // Stable ids incorporate author-supplied element ids, so the
        // fresh subtree's ids must be registered before reminting.
        for (child, child_id) in new_children.iter().zip(built.iter()) {
            self.register_element_ids_walk(child.as_ref(), *child_id);
        }

        // Mint stable ids over the updated tree before collect, so
        // handler registration during collect uses stable keys.
        self.build_generation = self.build_generation.wrapping_add(1);
        self.mint_stable_ids_walk();

        // Now collect render props with stable ids available.
        for (child, child_id) in new_children.iter().zip(built.iter()) {
            self.collect_render_props_boxed(child.as_ref(), *child_id);
        }

        self.auto_fill_animation_stable_keys();
        self.sweep_stale_handlers();
        self.sweep_stale_css_animations();

        // Apply class-based stylesheet styles to the freshly-built
        // subtree. `collect_render_props_boxed` above only applies
        // inline + `#id` styles (it does register the new nodes'
        // classes, so the class index below finds them), NOT class
        // rules. Without this, an in-place rebuilt subtree loses every
        // `.class`-driven property: the accordion trigger that just
        // toggled dropped its `.cn-accordion-trigger` padding while
        // untouched siblings kept theirs. Mirrors the other rebuild
        // paths (`process_pending_subtree_rebuilds_routed`,
        // `update_subtree_*_from_builder`). `None` router: any `:hover`
        // /`:active` state styles re-resolve on the next state pass.
        self.apply_stylesheet_base_styles_for_subtree(parent_id, None);
        self.apply_stylesheet_layout_overrides_for_subtree(parent_id);
    }

    /// Reconcile uniquely keyed direct children while retaining the layout ids
    /// of unchanged survivors. Unkeyed children remain positional.
    fn try_reconcile_keyed_children(
        &mut self,
        parent_id: LayoutNodeId,
        new_children: &[Box<dyn ElementBuilder>],
    ) -> bool {
        let old_children = self.layout_tree.children(parent_id);
        let mut old_key_counts = HashMap::<String, usize>::new();
        let mut old_by_key = HashMap::<String, LayoutNodeId>::new();
        for &child_id in &old_children {
            if let Some(key) = self.element_registry.get_id(child_id) {
                *old_key_counts.entry(key.clone()).or_default() += 1;
                old_by_key.insert(key, child_id);
            }
        }

        let mut new_key_counts = HashMap::<String, usize>::new();
        for child in new_children {
            if let Some(key) = child.element_id() {
                *new_key_counts.entry(key.to_string()).or_default() += 1;
            }
        }

        let new_topologies: Vec<DivHash> = new_children
            .iter()
            .map(|child| DivHash::compute_topology_tree(child.as_ref()))
            .collect();

        let can_preserve_keyed_survivor =
            new_children
                .iter()
                .zip(new_topologies.iter())
                .any(|(child, &new_topology)| {
                    let Some(key) = child.element_id() else {
                        return false;
                    };
                    if new_key_counts.get(key) != Some(&1) || old_key_counts.get(key) != Some(&1) {
                        return false;
                    }
                    let Some(&old_id) = old_by_key.get(key) else {
                        return false;
                    };
                    self.node_hashes
                        .get(&old_id)
                        .is_some_and(|&(_, _, old_topology)| old_topology == new_topology)
                });
        if !can_preserve_keyed_survivor {
            return false;
        }

        let mut reused = HashSet::new();
        let mut fresh = HashSet::new();
        let mut reconciled = Vec::with_capacity(new_children.len());

        for (index, (child, &new_topology)) in
            new_children.iter().zip(new_topologies.iter()).enumerate()
        {
            let keyed_match = child.element_id().and_then(|key| {
                if new_key_counts.get(key) != Some(&1) || old_key_counts.get(key) != Some(&1) {
                    return None;
                }
                let old_id = *old_by_key.get(key)?;
                self.node_hashes
                    .get(&old_id)
                    .is_some_and(|&(_, _, old_topology)| old_topology == new_topology)
                    .then_some(old_id)
            });

            let positional_match = if child.element_id().is_none() {
                old_children.get(index).copied().filter(|old_id| {
                    !reused.contains(old_id)
                        && self.element_registry.get_id(*old_id).is_none()
                        && self
                            .node_hashes
                            .get(old_id)
                            .is_some_and(|&(_, _, old_topology)| old_topology == new_topology)
                })
            } else {
                None
            };

            let child_id = if let Some(old_id) = keyed_match.or(positional_match) {
                reused.insert(old_id);
                old_id
            } else {
                let new_id = child.build(&mut self.layout_tree);
                fresh.insert(new_id);
                new_id
            };
            reconciled.push(child_id);
        }

        self.layout_tree
            .replace_children(parent_id, reconciled.clone());

        for old_id in old_children {
            if !reused.contains(&old_id) {
                self.remove_subtree_nodes(old_id);
                self.layout_tree.remove_subtree(old_id);
            }
        }

        for (child, &child_id) in new_children.iter().zip(reconciled.iter()) {
            if fresh.contains(&child_id) {
                self.register_element_ids_walk(child.as_ref(), child_id);
            }
        }
        self.element_registry
            .replace_children(parent_id, &reconciled);

        self.build_generation = self.build_generation.wrapping_add(1);
        self.mint_stable_ids_walk();

        for (child, &child_id) in new_children.iter().zip(reconciled.iter()) {
            if fresh.contains(&child_id) {
                self.collect_render_props_boxed(child.as_ref(), child_id);
            } else {
                self.update_render_props_in_place_boxed(child.as_ref(), child_id);
            }
        }

        self.auto_fill_animation_stable_keys();
        self.sweep_stale_handlers();
        self.sweep_stale_css_animations();
        self.apply_stylesheet_base_styles_for_subtree(parent_id, None);
        self.apply_stylesheet_layout_overrides_for_subtree(parent_id);
        true
    }

    /// True if `child_node_id`'s stored topology hash no longer matches
    /// `child_builder`'s. Shared by the generic and boxed `analyze_changes`
    /// variants, which both need to detect a same-shaped reorder (equal
    /// tree hash length, different child identity) and bail out to a full
    /// children rebuild rather than diffing positionally.
    fn child_topology_changed(
        &self,
        child_builder: &dyn ElementBuilder,
        child_node_id: LayoutNodeId,
    ) -> bool {
        let child_topology = DivHash::compute_topology_tree(child_builder);
        self.node_hashes
            .get(&child_node_id)
            .is_none_or(|&(_, _, stored_topology)| stored_topology != child_topology)
    }

    /// Analyze what categories of changes occurred between stored tree and new element
    pub(crate) fn analyze_changes<E: ElementBuilder>(
        &self,
        element: &E,
        node_id: LayoutNodeId,
    ) -> ChangeCategory {
        let mut changes = ChangeCategory::none();

        // Get stored hash for this node
        let Some(&(stored_own_hash, stored_tree_hash, stored_topology_hash)) =
            self.node_hashes.get(&node_id)
        else {
            // No stored hash - treat as everything changed
            changes.layout = true;
            changes.visual = true;
            changes.children = true;
            return changes;
        };

        // Compute new hashes
        let new_own_hash = DivHash::compute_element(element);
        let new_tree_hash = DivHash::compute_element_tree(element);
        let new_topology_hash = DivHash::compute_topology_tree(element);

        // If tree hashes match, nothing changed in this subtree
        if stored_tree_hash == new_tree_hash && stored_topology_hash == new_topology_hash {
            return changes;
        }

        // Tree hash differs - analyze further
        if stored_own_hash != new_own_hash {
            // This node's own properties changed
            // Check render props to distinguish visual vs layout
            if let Some(old_render_node) = self.render_nodes.get(&node_id) {
                let new_props = element.render_props();
                let old_props = &old_render_node.props;

                // Visual change detection: compare render-only properties
                if !Self::props_visually_equal(old_props, &new_props) {
                    changes.visual = true;
                }

                // Layout change: if hash differs but not just visual, assume layout changed
                // (We can't access Style directly from ElementBuilder, so we infer)
                if !changes.visual {
                    changes.layout = true;
                }
            } else {
                // No old render node - everything changed
                changes.layout = true;
                changes.visual = true;
            }
        }

        // Check children
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Different number of children = structural change
        if child_node_ids.len() != child_builders.len() {
            changes.children = true;
            return changes;
        }

        // Recursively check children
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            if self.child_topology_changed(child_builder.as_ref(), child_node_id) {
                changes.children = true;
                return changes;
            }
            let child_changes = self.analyze_changes_boxed(child_builder.as_ref(), child_node_id);
            changes.layout = changes.layout || child_changes.layout;
            changes.visual = changes.visual || child_changes.visual;
            changes.children = changes.children || child_changes.children;
            changes.handlers = changes.handlers || child_changes.handlers;

            // Short circuit if children changed (need full rebuild anyway)
            if changes.children {
                return changes;
            }
        }

        changes
    }

    /// Analyze changes for a boxed element builder
    pub(crate) fn analyze_changes_boxed(
        &self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) -> ChangeCategory {
        let mut changes = ChangeCategory::none();

        let Some(&(stored_own_hash, stored_tree_hash, stored_topology_hash)) =
            self.node_hashes.get(&node_id)
        else {
            changes.layout = true;
            changes.visual = true;
            changes.children = true;
            return changes;
        };

        let new_own_hash = DivHash::compute_element(element);
        let new_tree_hash = DivHash::compute_element_tree(element);
        let new_topology_hash = DivHash::compute_topology_tree(element);

        if stored_tree_hash == new_tree_hash && stored_topology_hash == new_topology_hash {
            return changes;
        }

        if stored_own_hash != new_own_hash {
            if let Some(old_render_node) = self.render_nodes.get(&node_id) {
                let new_props = element.render_props();
                let old_props = &old_render_node.props;

                if !Self::props_visually_equal(old_props, &new_props) {
                    changes.visual = true;
                }
                if !changes.visual {
                    changes.layout = true;
                }
            } else {
                changes.layout = true;
                changes.visual = true;
            }
        }

        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        if child_node_ids.len() != child_builders.len() {
            changes.children = true;
            return changes;
        }

        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            if self.child_topology_changed(child_builder.as_ref(), child_node_id) {
                changes.children = true;
                return changes;
            }
            let child_changes = self.analyze_changes_boxed(child_builder.as_ref(), child_node_id);
            changes.layout = changes.layout || child_changes.layout;
            changes.visual = changes.visual || child_changes.visual;
            changes.children = changes.children || child_changes.children;
            changes.handlers = changes.handlers || child_changes.handlers;

            if changes.children {
                return changes;
            }
        }

        changes
    }

    /// Compare render props for visual equality
    pub(crate) fn props_visually_equal(old: &RenderProps, new: &RenderProps) -> bool {
        render_props_eq(old, new)
    }

    /// Update render props in place without rebuilding the tree
    pub(crate) fn update_render_props_in_place<E: ElementBuilder>(
        &mut self,
        element: &E,
        node_id: LayoutNodeId,
    ) {
        // Update this node's props
        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            // Preserve motion from old props (set by parent)
            new_props.motion = render_node.props.motion.clone();
            render_node.props = new_props;
        } else {
            // Render node doesn't exist - create it
            tracing::debug!(
                "update_render_props_in_place: creating missing render_node for {:?}",
                node_id
            );
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            let element_type = Self::determine_element_type(element);
            self.render_nodes.insert(
                node_id,
                RenderNode {
                    props: new_props,
                    element_type,
                },
            );
        }

        // Update taffy node's layout style if element provides one
        // This is critical for layout changes (width, height, padding, etc.)
        if let Some(style) = element.layout_style() {
            self.layout_tree.set_style(node_id, style.clone());
        }

        // Update stored hash
        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        let topology_hash = DivHash::compute_topology_tree(element);
        self.node_hashes
            .insert(node_id, (own_hash, tree_hash, topology_hash));

        // Update event handlers
        if let Some(handlers) = element.event_handlers() {
            let stable_id = self.stable_id_or_warn(node_id);
            self.handler_registry.register(stable_id, handlers.clone());
        }

        // Update scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            tracing::trace!("Registering scroll physics for node {:?}", node_id);
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
            if element.viewport_cull() {
                self.viewport_cull_scrolls.insert(node_id);
            }
        }

        // Update motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        // Recursively update children
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Handle mismatch between layout children and builder children
        if child_node_ids.len() != child_builders.len() {
            // Rebuild children in place to fix the mismatch
            self.rebuild_children_in_place(node_id, child_builders);
        } else {
            for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter())
            {
                self.update_render_props_in_place_boxed(child_builder.as_ref(), child_node_id);
            }
        }
    }

    /// Update render props for a boxed element builder
    pub(crate) fn update_render_props_in_place_boxed(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) {
        if let Some(render_node) = self.render_nodes.get_mut(&node_id) {
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            new_props.motion = render_node.props.motion.clone();
            render_node.props = new_props;
            render_node.element_type = Self::determine_element_type_boxed(element);
        } else {
            // Render node doesn't exist - this can happen if the tree structure changed
            // but rebuild_children_in_place wasn't called for this subtree.
            // Create a new render node entry.
            tracing::debug!(
                "update_render_props_in_place_boxed: creating missing render_node for {:?}",
                node_id
            );
            let mut new_props = element.render_props();
            new_props.node_id = Some(node_id);
            let element_type = Self::determine_element_type_boxed(element);
            self.render_nodes.insert(
                node_id,
                RenderNode {
                    props: new_props,
                    element_type,
                },
            );
        }

        // Update taffy node's layout style if element provides one
        // This is critical for layout changes (width, height, padding, etc.)
        if let Some(style) = element.layout_style() {
            self.layout_tree.set_style(node_id, style.clone());
        }

        let new_classes = element.element_classes();
        let old_classes = self.element_registry.get_classes(node_id);
        let classes_changed = new_classes != old_classes.as_deref().unwrap_or(&[]);
        if !new_classes.is_empty() {
            self.element_registry
                .register_classes(node_id, new_classes.to_vec());
        } else {
            self.element_registry.clear_classes(node_id);
        }
        if classes_changed {
            self.base_styles.remove(&node_id);
        }

        let own_hash = DivHash::compute_element(element);
        let tree_hash = DivHash::compute_element_tree(element);
        let topology_hash = DivHash::compute_topology_tree(element);
        self.node_hashes
            .insert(node_id, (own_hash, tree_hash, topology_hash));

        if let Some(handlers) = element.event_handlers() {
            let stable_id = self.stable_id_or_warn(node_id);
            self.handler_registry.register(stable_id, handlers.clone());
        }

        // Update scroll physics if this is a scroll element
        if let Some(physics) = element.scroll_physics() {
            // Set the animation scheduler for bounce springs
            if let Some(scheduler) = self.animations.upgrade() {
                physics.lock().unwrap().set_scheduler(&scheduler);
            }
            self.scroll_physics.insert(node_id, physics);
            if element.viewport_cull() {
                self.viewport_cull_scrolls.insert(node_id);
            }
        }

        // Update motion bindings if this element has continuous animations
        if let Some(bindings) = element.motion_bindings() {
            self.motion_bindings.insert(node_id, bindings);
        }

        // Register layout bounds storage if element wants bounds updates
        self.register_element_bounds_storage(node_id, element);

        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Handle mismatch between layout children and builder children
        if child_node_ids.len() != child_builders.len() {
            // Rebuild children in place to fix the mismatch
            self.rebuild_children_in_place(node_id, child_builders);
        } else {
            for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter())
            {
                self.update_render_props_in_place_boxed(child_builder.as_ref(), child_node_id);
            }
        }
    }
}
