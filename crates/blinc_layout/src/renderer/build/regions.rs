//! Bringing the rows of a list or branch up to date in place.

use std::collections::{HashMap, HashSet};

use crate::div::ElementBuilder;
use crate::region::{RowId, RowNode};
use crate::renderer::RenderTree;
use crate::tree::LayoutNodeId;

impl RenderTree {
    /// Read the source of the region `parent` holds and make the rows match.
    ///
    /// A row that stays keeps its node, so its handlers, bindings, scroll
    /// position and animations stay too. A new row is built and put at its
    /// place, and a row that went is torn down before the scope it was built
    /// under is disposed. Children before and after the rows are not touched.
    pub(crate) fn reconcile_region(&mut self, parent: LayoutNodeId, region_id: u64) {
        let Some(region) = crate::region::region_of(parent, region_id) else {
            return;
        };
        if !self.render_nodes.contains_key(&parent) {
            return;
        }
        let evaluation = region.logic.borrow_mut().evaluate();
        // A row that is leaving is asked again next frame.
        if region.logic.borrow().is_leaving() {
            crate::stateful::queue_region_update(parent, region_id);
        }

        let old_live: Vec<(RowId, Option<LayoutNodeId>)> = region.live.borrow().clone();
        let node_of: HashMap<RowId, Option<LayoutNodeId>> = old_live.iter().copied().collect();
        let old_count = old_live.iter().filter(|(_, node)| node.is_some()).count();

        let children = self.layout_tree.children(parent);
        let start = region.start.min(children.len());
        let end = (start + old_count).min(children.len());
        let head = children[..start].to_vec();
        let tail = children[end..].to_vec();

        // Rows that went: their nodes first, then what they created.
        for id in &evaluation.removed {
            if let Some(Some(node)) = node_of.get(id) {
                self.remove_subtree_nodes(*node);
                self.layout_tree.remove_subtree(*node);
            }
        }
        for id in &evaluation.removed {
            region.logic.borrow_mut().dispose_row(*id);
        }

        // A row that is leaving keeps its place: after the nearest row before
        // it that is still there, or first if there is none.
        let kept: HashSet<RowId> = evaluation
            .rows
            .iter()
            .filter(|plan| matches!(plan.node, RowNode::Keep))
            .map(|plan| plan.id)
            .collect();
        let mut first: Vec<RowId> = Vec::new();
        let mut after: HashMap<RowId, Vec<RowId>> = HashMap::new();
        let mut anchor: Option<RowId> = None;
        for (id, _) in &old_live {
            if kept.contains(id) {
                anchor = Some(*id);
            } else if evaluation.lingering.contains(id) {
                match anchor {
                    Some(anchor) => after.entry(anchor).or_default().push(*id),
                    None => first.push(*id),
                }
            }
        }

        let mut live = Vec::with_capacity(evaluation.rows.len() + evaluation.lingering.len());
        let mut row_nodes: Vec<LayoutNodeId> = Vec::new();
        let mut built: Vec<(Box<dyn ElementBuilder>, LayoutNodeId, RowId)> = Vec::new();
        let linger = |ids: &[RowId], live: &mut Vec<_>, row_nodes: &mut Vec<LayoutNodeId>| {
            for id in ids {
                let node = node_of.get(id).copied().flatten();
                row_nodes.extend(node);
                live.push((*id, node));
            }
        };
        linger(&first, &mut live, &mut row_nodes);
        for plan in evaluation.rows {
            match plan.node {
                RowNode::Keep => {
                    let node = node_of.get(&plan.id).copied().flatten();
                    row_nodes.extend(node);
                    live.push((plan.id, node));
                    if let Some(ids) = after.get(&plan.id) {
                        linger(ids, &mut live, &mut row_nodes);
                    }
                }
                RowNode::Build(builder) => {
                    let node = builder.build(&mut self.layout_tree);
                    crate::region::set_row_key(node, region.id, plan.id);
                    row_nodes.push(node);
                    live.push((plan.id, Some(node)));
                    built.push((builder, node, plan.id));
                }
                RowNode::Empty => live.push((plan.id, None)),
            }
        }
        *region.live.borrow_mut() = live;

        let mut all = head;
        all.extend(row_nodes);
        all.extend(tail);
        self.layout_tree.replace_children(parent, all.clone());
        let total = all.len();
        for (index, &child) in all.iter().enumerate() {
            self.element_registry.register_parent(child, parent);
            self.element_registry
                .register_child_index(child, index, total);
        }

        if built.is_empty() && evaluation.removed.is_empty() && all.len() == children.len() {
            // Same rows, in whatever order: only the layout moved.
            self.build_generation = self.build_generation.wrapping_add(1);
            self.mint_stable_ids_walk();
            return;
        }

        self.invalidate_mouse_move_pipeline_cache();
        for (builder, node, _) in &built {
            self.register_element_ids_walk(builder.as_ref(), *node);
        }
        self.build_generation = self.build_generation.wrapping_add(1);
        self.mint_stable_ids_walk();
        for (builder, node, _) in &built {
            self.collect_render_props_boxed(builder.as_ref(), *node);
        }
        self.auto_fill_animation_stable_keys();
        self.sweep_stale_handlers();
        self.sweep_stale_css_animations();
        for (_, node, _) in &built {
            self.apply_stylesheet_base_styles_for_subtree(*node, None);
            self.apply_stylesheet_layout_overrides_for_subtree(*node);
        }
    }
}
