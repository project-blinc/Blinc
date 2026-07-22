//! Subtree rebuild + maintenance flow.
//!
//! Five concerns:
//!
//! - **Hash-driven rebuild**: `rebuild_changed_subtrees` /
//!   `rebuild_changed_subtrees_boxed` walk the tree comparing the
//!   stored `DivHash` per node against the rebuilt element tree.
//!   When a node's children count changes, they hand off to
//!   `rebuild_children_in_place`; otherwise they recurse to refine
//!   the diff.
//! - **Top-level subtree replace**: `rebuild_children` is the
//!   user-facing entry point that wipes a node's children, builds
//!   the new one from an `ElementBuilder`, registers parent/index
//!   metadata, and re-applies the stylesheet base styles for the
//!   subtree.
//! - **Removal**: `remove_subtree_nodes` walks a subtree DFS and
//!   deletes the per-node `RenderTree` state (render_nodes, hashes,
//!   bounds caches, transitions, scroll/handler/storage records).
//!   Used by every rebuild/replace path that needs to clean up
//!   before re-adding.
//! - **Stateful-driven rebuilds**: `process_pending_subtree_rebuilds`
//!   drains the queue stamped by `crate::stateful` whenever a
//!   `Stateful` widget's deps change, dispatches each entry to either
//!   the props-only fast path or a structural rebuild, and re-applies
//!   stylesheet base styles for the rebuilt subtrees.
//! - **Props-only update**: `update_subtree_props_recursive` /
//!   `update_subtree_props_from_builder` carry through a state
//!   change when the layout shape didn't change — re-derive
//!   `RenderProps` from the new builder, write back, recurse.

use crate::diff::DivHash;
use crate::div::ElementBuilder;
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

impl RenderTree {
    /// True if any slot in `child_builders`/`child_node_ids` has a topology
    /// hash that no longer matches the stored one for that node.
    ///
    /// Equal child counts do not imply equal child identity — a same-shaped
    /// reorder still needs a rebuild so ids, handlers, and other node-keyed
    /// state cannot remain attached positionally. Shared by the generic and
    /// boxed `rebuild_changed_subtrees` variants below.
    fn any_child_topology_changed(
        &self,
        child_builders: &[Box<dyn ElementBuilder>],
        child_node_ids: &[LayoutNodeId],
    ) -> bool {
        child_builders
            .iter()
            .zip(child_node_ids.iter())
            .any(|(child, child_id)| {
                let new_topology = DivHash::compute_topology_tree(child.as_ref());
                self.node_hashes
                    .get(child_id)
                    .is_none_or(|&(_, _, stored_topology)| stored_topology != new_topology)
            })
    }

    /// Rebuild subtrees for nodes with changed children
    ///
    /// This walks the tree comparing stored hashes with the new element tree.
    /// When it finds a node whose children have changed (different count),
    /// it rebuilds that subtree in place.
    pub(crate) fn rebuild_changed_subtrees<E: ElementBuilder>(
        &mut self,
        element: &E,
        node_id: LayoutNodeId,
    ) {
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        // Check if children count changed - rebuild children of this node
        if child_node_ids.len() != child_builders.len() {
            self.rebuild_children_in_place(node_id, child_builders);
            return;
        }

        if self.any_child_topology_changed(child_builders, &child_node_ids) {
            self.rebuild_children_in_place(node_id, child_builders);
            return;
        }

        // Same child count - check each child for deeper changes
        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            // Get stored hash for this child
            if let Some(&(_, stored_tree_hash, _)) = self.node_hashes.get(&child_node_id) {
                let new_tree_hash = DivHash::compute_element_tree(child_builder.as_ref());
                if stored_tree_hash != new_tree_hash {
                    // Child's subtree changed - check if it's the child count or deeper changes
                    let child_children_count = self.layout_tree.children(child_node_id).len();
                    let new_children_count = child_builder.children_builders().len();

                    if child_children_count != new_children_count {
                        // This child's children changed - rebuild its children
                        self.rebuild_children_in_place(
                            child_node_id,
                            child_builder.children_builders(),
                        );
                    } else {
                        // Recurse to find deeper changes
                        self.rebuild_changed_subtrees_boxed(child_builder.as_ref(), child_node_id);
                    }
                }
            }
        }
    }

    /// Rebuild subtrees for boxed element builder
    pub(crate) fn rebuild_changed_subtrees_boxed(
        &mut self,
        element: &dyn ElementBuilder,
        node_id: LayoutNodeId,
    ) {
        let child_node_ids = self.layout_tree.children(node_id);
        let child_builders = element.children_builders();

        if child_node_ids.len() != child_builders.len() {
            self.rebuild_children_in_place(node_id, child_builders);
            return;
        }

        if self.any_child_topology_changed(child_builders, &child_node_ids) {
            self.rebuild_children_in_place(node_id, child_builders);
            return;
        }

        for (child_builder, &child_node_id) in child_builders.iter().zip(child_node_ids.iter()) {
            if let Some(&(_, stored_tree_hash, _)) = self.node_hashes.get(&child_node_id) {
                let new_tree_hash = DivHash::compute_element_tree(child_builder.as_ref());
                if stored_tree_hash != new_tree_hash {
                    let child_children_count = self.layout_tree.children(child_node_id).len();
                    let new_children_count = child_builder.children_builders().len();

                    if child_children_count != new_children_count {
                        self.rebuild_children_in_place(
                            child_node_id,
                            child_builder.children_builders(),
                        );
                    } else {
                        self.rebuild_changed_subtrees_boxed(child_builder.as_ref(), child_node_id);
                    }
                }
            }
        }
    }

    /// Rebuild only the children of a specific node
    ///
    /// This is used for incremental updates when a stateful element's
    /// dependencies change. Instead of rebuilding the entire tree,
    /// we only rebuild the affected subtree.
    ///
    /// # Arguments
    /// * `parent_id` - The node whose children should be rebuilt
    /// * `new_child` - The new child element builder
    ///
    /// # Returns
    /// The ID of the new child node
    pub fn rebuild_children<E: ElementBuilder>(
        &mut self,
        parent_id: LayoutNodeId,
        new_child: &E,
    ) -> LayoutNodeId {
        // The rebuild registers fresh handlers + applies stylesheet
        // base styles for the new subtree. Invalidate the
        // bare-mouse-move pipeline cache so the next mouse-move
        // re-derives the early-return predicate.
        self.invalidate_mouse_move_pipeline_cache();

        // 1. Remove old children from layout tree and render nodes
        let old_children = self.layout_tree.children(parent_id);
        for child_id in &old_children {
            self.remove_subtree_nodes(*child_id);
        }
        self.layout_tree.clear_children(parent_id);

        // 2. Build the new child element into the layout tree
        let new_child_id = new_child.build(&mut self.layout_tree);

        // 3. Add the new child to the parent
        self.layout_tree.add_child(parent_id, new_child_id);

        // 4. Pre-register element ids for the new subtree BEFORE
        // mint so `widget_key` is consistent across mint passes —
        // see `build_element` for the full rationale.
        self.register_element_ids_walk(new_child, new_child_id);

        // 5. Mint stable ids over the updated tree BEFORE collect.
        // Handler registration in collect_render_props (Phase 3)
        // looks up `self.stable_id_or_warn(node_id)` — without this
        // mint, the new subtree's nodes have no stable id yet and
        // the warn fires per node.
        self.build_generation = self.build_generation.wrapping_add(1);
        self.mint_stable_ids_walk();

        // 5. Collect render props for the new subtree, then run the
        // standard post-build housekeeping.
        self.collect_render_props(new_child, new_child_id);
        self.auto_fill_animation_stable_keys();
        self.sweep_stale_handlers();
        self.sweep_stale_css_animations();

        new_child_id
    }

    /// Remove render nodes for a subtree (but don't touch layout tree)
    pub(crate) fn remove_subtree_nodes(&mut self, node_id: LayoutNodeId) {
        // Remove children first
        let children = self.layout_tree.children(node_id);
        for child_id in children {
            self.remove_subtree_nodes(child_id);
        }

        // Remove this node's render data. Handler registry removal
        // uses the stable id (mapping looked up before we drop the
        // mapping below) so the registry stays in sync.
        let stable_for_remove = self.stable_id(node_id);
        self.render_nodes.swap_remove(&node_id);
        if let Some(stable) = stable_for_remove {
            self.handler_registry.remove(stable);
        }
        self.node_states.remove(&node_id);
        self.scroll_offsets.remove(&node_id);
        self.scroll_physics.remove(&node_id);
        self.scroll_refs.remove(&node_id);
        // Unregister from element registry (removes by node_id)
        self.element_registry.unregister(node_id);
        // Remove layout animation config (but keep stable-key animations running)
        self.layout_animation_configs.remove(&node_id);
        self.layout_animations.remove(&node_id);
        self.previous_bounds.remove(&node_id);

        // Remove CSS state tracking data (prevents accumulation across rebuilds)
        self.base_styles.remove(&node_id);
        self.base_taffy_styles.remove(&node_id);
        self.node_hashes.remove(&node_id);
        self.layout_bounds_storages.remove(&node_id);
        self.animated_render_bounds.remove(&node_id);
        self.motion_bindings.remove(&node_id);
        self.hover_css_animations.remove(&node_id);
        self.complex_state_affected.remove(&node_id);

        // Evict any signal-bound property bindings registered against
        // this node. Without this, a removed node's bindings would
        // still fire on signal changes and queue updates against a
        // dead LayoutNodeId. See [[project-reactive-architecture-v2]]
        // Phase 2.
        crate::binding::unregister_node(node_id);

        // CSS animations/transitions are stable-keyed and intentionally
        // NOT drained here. The eager drain conflated "node removed"
        // with "node about to be re-registered under the same stable
        // id" — pushing a sibling overlay onto OverlayStack triggers a
        // Structural rebuild of the overlay layer, which torn down
        // every existing entry's wrapper. For a survivor (cn-context-
        // menu while a submenu is being added) the wrapper re-mints
        // with the SAME stable id, but the drain had already wiped its
        // ActiveCssAnimation, so start_all_css_animations's
        // already_has gate fell through and restarted the enter
        // animation — visible as an opacity flicker.
        //
        // Survivor-aware cleanup runs after mint via
        // sweep_stale_css_animations: any stable_id with no live
        // stable_to_layout mapping after re-mint is genuinely gone and
        // gets dropped. This mirrors handler_registry's
        // sweep_stale_handlers contract.

        // Drop the stable-id mapping for this layout node. The
        // post-rebuild `mint_stable_ids_walk` will repopulate
        // mappings for surviving / new nodes; this prevents stale
        // entries from lingering between the remove call and the
        // re-mint (mostly defensive — the mint walk would overwrite
        // anyway, but removed-and-not-re-added nodes would otherwise
        // hold a forwarding entry to a freed slotmap key).
        if let Some(stable) = self.layout_to_stable.remove(&node_id) {
            self.stable_to_layout.remove(&stable);
        }
    }

    /// Process all pending subtree rebuilds
    ///
    /// This is called by the windowed app after processing events.
    /// It applies queued child rebuilds without rebuilding the entire tree.
    /// Process pending subtree rebuilds
    ///
    /// Returns true if any rebuild requires layout recomputation.
    /// Visual-only rebuilds (hover/press) return false.
    ///
    /// Processes only rebuilds for nodes that still exist in this tree.
    /// Rebuilds for nodes removed by an earlier pending rebuild are stale and
    /// are dropped; otherwise the global queue stays non-empty forever and the
    /// app keeps requesting redraws while idle.
    pub fn process_pending_subtree_rebuilds(&mut self) -> bool {
        self.process_pending_subtree_rebuilds_routed(None)
    }

    /// Router-aware variant of [`Self::process_pending_subtree_rebuilds`].
    /// When `router` is `Some`, the per-rebuild
    /// `apply_stylesheet_base_styles_for_subtree` call runs an extra
    /// state-style pass restricted to the rebuilt subtree so
    /// `:focus` / `:hover` / `:active` rules survive the base-class
    /// write that ends every rebuild. Required on desktop because
    /// animation-tick frames (Stateful refresh while a spring or
    /// keyframe is still running) re-rebuild the subtree without
    /// changing the EventRouter fingerprint, so Phase 4's gated
    /// `apply_stylesheet_state_styles` would skip and leave the
    /// just-clobbered base standing. Callers without a live router
    /// (cold mobile/web paths, tests) pass `None`.
    pub fn process_pending_subtree_rebuilds_routed(
        &mut self,
        router: Option<&crate::event_router::EventRouter>,
    ) -> bool {
        let pending = crate::stateful::take_pending_subtree_rebuilds();
        if pending.is_empty() {
            return false;
        }

        // Subtree rebuilds register/unregister handlers and may apply
        // CSS overrides that change cursor styles. Invalidate the
        // bare-mouse-move pipeline cache so the next mouse-move
        // re-derives the early-return predicate.
        self.invalidate_mouse_move_pipeline_cache();

        tracing::debug!("Processing {} pending subtree rebuilds", pending.len());

        let mut needs_layout = false;
        let mut stale_rebuilds = 0usize;
        let mut superseded_rebuilds = 0usize;
        // Only true structural rebuilds (children added/removed/reordered)
        // can supersede other pending entries. Layout-prop rebuilds patch
        // taffy styles on existing children without tearing them down, so
        // a descendant's prop update on the same frame should still apply.
        let structural_rebuilds_by_node: std::collections::HashMap<LayoutNodeId, usize> = pending
            .iter()
            .enumerate()
            .filter_map(|(idx, rebuild)| {
                matches!(rebuild.kind, crate::stateful::RebuildKind::Structural)
                    .then_some((rebuild.parent_id, idx))
            })
            .collect();

        for (idx, rebuild) in pending.into_iter().enumerate() {
            // Skip stale rebuilds. This can happen when multiple statefuls queue
            // work in one input cycle and a parent subtree rebuild removes a child
            // that also queued its own hover/press refresh.
            if !self.layout_tree.node_exists(rebuild.parent_id) {
                tracing::debug!(
                    "Subtree rebuild: node {:?} no longer exists, dropping stale rebuild",
                    rebuild.parent_id
                );
                stale_rebuilds += 1;
                continue;
            }

            // Drop work that will be overwritten by a pending structural rebuild.
            // Navigation clicks often queue button visual state updates and an
            // outlet replacement in the same event turn. Processing descendant
            // updates first is wasted work, and on slower Linux machines that can
            // make a simple route change feel sticky.
            if let Some(&structural_idx) = structural_rebuilds_by_node.get(&rebuild.parent_id) {
                if idx < structural_idx {
                    tracing::debug!(
                        "Subtree rebuild: node {:?} superseded by later structural rebuild",
                        rebuild.parent_id
                    );
                    superseded_rebuilds += 1;
                    continue;
                }
            }
            if self
                .layout_tree
                .ancestors(rebuild.parent_id)
                .iter()
                .any(|ancestor| structural_rebuilds_by_node.contains_key(ancestor))
            {
                tracing::debug!(
                    "Subtree rebuild: node {:?} superseded by pending ancestor rebuild",
                    rebuild.parent_id
                );
                superseded_rebuilds += 1;
                continue;
            }

            tracing::debug!(
                "Subtree rebuild: processing node {:?}, kind={:?}",
                rebuild.parent_id,
                rebuild.kind
            );
            if matches!(rebuild.kind, crate::stateful::RebuildKind::Structural) {
                // Full structural rebuild - remove old children and build new ones
                needs_layout = true;

                // Update the parent node's own render props AND layout style
                // This is critical for overlay layer where size changes from 0x0 to full viewport
                if let Some(render_node) = self.render_nodes.get_mut(&rebuild.parent_id) {
                    let mut new_props = rebuild.new_child.render_props();
                    new_props.node_id = Some(rebuild.parent_id);
                    new_props.motion = render_node.props.motion.clone();
                    render_node.props = new_props;
                }
                // Update parent node's CSS class registrations so that
                // apply_stylesheet_base_styles_for_subtree matches the current
                // classes (e.g., cn-checkbox--checked added/removed on toggle).
                let parent_classes = rebuild.new_child.element_classes();
                if !parent_classes.is_empty() {
                    self.element_registry
                        .register_classes(rebuild.parent_id, parent_classes.to_vec());
                } else {
                    self.element_registry.clear_classes(rebuild.parent_id);
                }
                self.base_styles.remove(&rebuild.parent_id);
                // Also update the taffy layout style (width, height, padding, etc.)
                if let Some(style) = rebuild.new_child.layout_style() {
                    self.layout_tree.set_style(rebuild.parent_id, style.clone());
                }

                // Re-register scroll_physics and event_handlers for the parent node.
                // Without this, Stateful containers with overflow_y_scroll() lose their
                // scroll state during rebuilds because only children get collect_render_props_boxed.
                if let Some(physics) = rebuild.new_child.scroll_physics() {
                    if let Some(scheduler) = self.animations.upgrade() {
                        physics.lock().unwrap().set_scheduler(&scheduler);
                    }
                    self.scroll_physics.insert(rebuild.parent_id, physics);
                    if rebuild.new_child.viewport_cull() {
                        self.viewport_cull_scrolls.insert(rebuild.parent_id);
                    }
                }
                {
                    let handlers = rebuild.new_child.event_handlers();
                    if !handlers.is_empty() {
                        let stable_id = self.stable_id_or_warn(rebuild.parent_id);
                        self.handler_registry.register(stable_id, handlers.clone());
                    }
                }

                // Always remove old children first (even if new children is empty)
                // This fixes the bug where SVG checkmarks would persist after unchecking
                let old_children = self.layout_tree.children(rebuild.parent_id);
                for child_id in &old_children {
                    self.remove_subtree_nodes(*child_id);
                }
                self.layout_tree.clear_children(rebuild.parent_id);

                // Two-phase: build all new layout nodes first, then
                // mint stable ids once, then collect. Collect must
                // run with stable ids available so handlers /
                // physics / motion bindings register against stable
                // keys — otherwise the registry entries don't
                // survive the next rebuild.
                let children = rebuild.new_child.children_builders();
                let mut built: Vec<LayoutNodeId> = Vec::with_capacity(children.len());
                for child in children {
                    let child_id = child.build(&mut self.layout_tree);
                    self.layout_tree.add_child(rebuild.parent_id, child_id);
                    built.push(child_id);
                }

                // Pre-register element ids for the new subtree
                // BEFORE mint so `widget_key` is read consistently
                // on every mint pass. Without this, mint derives
                // each `.id()`'d descendant's stable id with
                // `widget_key=None` the first time (registry not
                // yet populated) but `widget_key=Some(...)` on
                // every subsequent mint — descendants' stable ids
                // shift and previously-registered handlers go
                // orphaned. See `build_element` for the same fix
                // at initial build.
                for (child, child_id) in children.iter().zip(built.iter()) {
                    self.register_element_ids_walk(child.as_ref(), *child_id);
                }

                // Mint stable ids over the now-complete tree before
                // collect runs (collect inserts handlers etc.).
                self.build_generation = self.build_generation.wrapping_add(1);
                self.mint_stable_ids_walk();

                for (child, child_id) in children.iter().zip(built.iter()) {
                    self.collect_render_props_boxed(child.as_ref(), *child_id);
                }

                self.auto_fill_animation_stable_keys();
                self.sweep_stale_handlers();
                self.sweep_stale_css_animations();

                // Apply CSS base styles (class/complex selectors) to new subtree nodes.
                // collect_render_props_boxed only applies #id styles; class-based
                // styles are applied by apply_stylesheet_base_styles() which only
                // runs at full tree creation. Without this, new children from
                // stateful rebuilds lose CSS class styles (border-radius, etc.).
                self.apply_stylesheet_base_styles_for_subtree(rebuild.parent_id, router);
                // Class-driven LAYOUT props (padding / size / flex) too —
                // base styles above only cover visual props, so without
                // this a rebuilt node loses its class padding (accordion
                // trigger collapse, menubar submenu-row shift).
                self.apply_stylesheet_layout_overrides_for_subtree(rebuild.parent_id);
            } else if matches!(rebuild.kind, crate::stateful::RebuildKind::LayoutProps) {
                // Layout-prop update — patch taffy `Style` + render
                // props on every existing layout node, then mark the
                // frame dirty so taffy recomputes layout. No children
                // teardown, no stable-id reminting, no handler re-
                // registration. This is the path spring-animated
                // `.w()` / `.h()` / `.left()` take.
                needs_layout = true;
                self.update_subtree_layout_recursive(rebuild.parent_id, &rebuild.new_child, router);
            } else {
                // Visual-only update - just update render props of existing children
                // Don't remove/rebuild, just walk the tree and update props
                self.update_subtree_props_recursive(rebuild.parent_id, &rebuild.new_child, router);
            }
        }

        if stale_rebuilds > 0 {
            tracing::debug!("Dropped {} stale subtree rebuild(s)", stale_rebuilds);
        }
        if superseded_rebuilds > 0 {
            tracing::debug!(
                "Dropped {} superseded subtree rebuild(s)",
                superseded_rebuilds
            );
        }

        // Mint already ran per subtree-rebuild above, between layout
        // build and collect. No additional walk needed here.

        needs_layout
    }

    /// Recursively update render props for existing children without rebuilding
    ///
    /// This walks the existing layout tree children alongside the new element definition
    /// and updates render props for matching nodes (by position in child order).
    fn update_subtree_props_recursive(
        &mut self,
        parent_id: LayoutNodeId,
        new_element: &crate::div::Div,
        router: Option<&crate::event_router::EventRouter>,
    ) {
        self.update_subtree_props_from_builder(parent_id, new_element, router);
    }

    /// Recursively update taffy `Style` AND render props for existing
    /// children without rebuilding. Used by the LayoutProps rebuild
    /// path: tree topology is unchanged, only dimensions / inset /
    /// padding / margin shifted (typically from a spring driving a
    /// `.w()` / `.h()` / `.left()`).
    ///
    /// Walks paired (existing layout node, new builder) and:
    /// - replaces `RenderProps` (same as the Visual path)
    /// - calls `layout_tree.set_style(node, new_builder.layout_style())`,
    ///   which marks the taffy node dirty so the next `compute_layout`
    ///   reflows just this subtree.
    fn update_subtree_layout_recursive(
        &mut self,
        parent_id: LayoutNodeId,
        new_element: &crate::div::Div,
        router: Option<&crate::event_router::EventRouter>,
    ) {
        // Update the parent node's own layout style + render props
        // FIRST (the existing `update_subtree_props_from_builder` only
        // walks children, not the parent). Without this, animations
        // bound to the Stateful's own dimensions wouldn't take effect.
        //
        // Use `effective_layout_style` rather than `layout_style` so
        // widget-internal style adjustments (e.g. Notch reserving
        // scoop padding) are preserved across the patch. The raw
        // `layout_style()` value doesn't include those adjustments and
        // patching with it would silently strip the padding taffy was
        // given at original build time.
        if let Some(style) = new_element.effective_layout_style() {
            self.layout_tree.set_style(parent_id, style);
        }
        if let Some(render_node) = self.render_nodes.get_mut(&parent_id) {
            let mut new_props = new_element.render_props();
            new_props.node_id = Some(parent_id);
            new_props.motion = render_node.props.motion.clone();
            render_node.props = new_props;
        }
        self.update_subtree_layout_from_builder(parent_id, new_element, router);
    }

    fn update_subtree_layout_from_builder(
        &mut self,
        parent_id: LayoutNodeId,
        new_element: &dyn crate::div::ElementBuilder,
        router: Option<&crate::event_router::EventRouter>,
    ) {
        let existing_children = self.layout_tree.children(parent_id);
        let new_children = new_element.children_builders();

        for (i, child_id) in existing_children.iter().enumerate() {
            if let Some(new_child) = new_children.get(i) {
                // Patch taffy style first so the next compute_layout
                // sees the new dimensions. `effective_layout_style`
                // preserves widget-internal style adjustments (Notch
                // scoop padding, etc.) — see the parent-node patch
                // above for the rationale.
                if let Some(style) = new_child.effective_layout_style() {
                    self.layout_tree.set_style(*child_id, style);
                }

                // Full replace of render props — preserve node_id and motion.
                let mut new_props = new_child.render_props();
                if let Some(render_node) = self.render_nodes.get_mut(child_id) {
                    new_props.node_id = render_node.props.node_id;
                    new_props.motion = render_node.props.motion.clone();
                    render_node.props = new_props;
                    render_node.element_type =
                        Self::determine_element_type_boxed(new_child.as_ref());
                }

                // Re-register event handlers (closures may have captured
                // new state on this refresh).
                if let Some(handlers) = new_child.event_handlers() {
                    let stable_id = self.stable_id_or_warn(*child_id);
                    self.handler_registry.register(stable_id, handlers.clone());
                }

                if !new_child.children_builders().is_empty() {
                    self.update_subtree_layout_from_builder(*child_id, new_child.as_ref(), router);
                }
            }
        }

        // Re-apply CSS base styles since the full-replace cleared them
        // (mirrors the visual path's final step).
        self.apply_stylesheet_base_styles_for_subtree(parent_id, router);
        // Layout overrides too — the full replace cleared class-driven
        // padding / size / flex along with the visual props.
        self.apply_stylesheet_layout_overrides_for_subtree(parent_id);
    }

    /// Update subtree props from a generic ElementBuilder (for recursion)
    ///
    /// Uses full replacement (not merge) so that properties cleared back to defaults
    /// (e.g. transform removed on drag end) are properly reflected. Preserves node_id
    /// and motion which are not set by builders.
    fn update_subtree_props_from_builder(
        &mut self,
        parent_id: LayoutNodeId,
        new_element: &dyn crate::div::ElementBuilder,
        router: Option<&crate::event_router::EventRouter>,
    ) {
        let existing_children = self.layout_tree.children(parent_id);
        let new_children = new_element.children_builders();

        for (i, child_id) in existing_children.iter().enumerate() {
            if let Some(new_child) = new_children.get(i) {
                // Full replace of visual props, preserving node_id and motion
                let mut new_props = new_child.render_props();
                if let Some(render_node) = self.render_nodes.get_mut(child_id) {
                    new_props.node_id = render_node.props.node_id;
                    new_props.motion = render_node.props.motion.clone();
                    render_node.props = new_props;
                    // Also update element_type (SVG tint, text content, image data, etc.)
                    // Without this, visual-only rebuilds leave stale element data.
                    render_node.element_type =
                        Self::determine_element_type_boxed(new_child.as_ref());
                }

                // Re-register event handlers from the new element builder.
                // During visual-only rebuilds the tree structure doesn't change,
                // but callbacks may capture new closure state that needs updating.
                if let Some(handlers) = new_child.event_handlers() {
                    let stable_id = self.stable_id_or_warn(*child_id);
                    self.handler_registry.register(stable_id, handlers.clone());
                }

                // Update CSS class registrations so apply_stylesheet_base_styles_for_subtree
                // uses the current classes (not stale ones from the previous build).
                // Without this, adding/removing classes (e.g. cn-sidebar-item--active)
                // wouldn't take effect during visual-only rebuilds.
                let new_classes = new_child.element_classes();
                let old_classes = self.element_registry.get_classes(*child_id);
                let classes_changed = new_classes != old_classes.as_deref().unwrap_or(&[]);
                if !new_classes.is_empty() {
                    self.element_registry
                        .register_classes(*child_id, new_classes.to_vec());
                } else {
                    self.element_registry.clear_classes(*child_id);
                }
                // Invalidate base_styles cache when classes change so that
                // apply_complex_selector_styles resets to the correct base
                // (e.g., node gaining --active needs its new base to include that)
                if classes_changed {
                    self.base_styles.remove(child_id);
                }

                // Recursively update grandchildren
                if !new_child.children_builders().is_empty() {
                    self.update_subtree_props_from_builder(*child_id, new_child.as_ref(), router);
                }
            }
        }

        // Re-apply CSS base styles since the full replace cleared them
        self.apply_stylesheet_base_styles_for_subtree(parent_id, router);
        // Layout overrides too — see the sibling full-replace path above.
        self.apply_stylesheet_layout_overrides_for_subtree(parent_id);
    }
}
