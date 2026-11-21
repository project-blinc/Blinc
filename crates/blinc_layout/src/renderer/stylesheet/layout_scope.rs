//! The CSS layout pass limited to the nodes whose matches a frame's updates can change.
//!
//! Property and region updates record where they changed the tree, and the pass then
//! matches the layout rules on those nodes alone instead of on every node. The pass only
//! applies the rules a node matches, so a node whose matches did not change would take the
//! same rules again; leaving it out changes nothing.
//!
//! What an update can change the matches of:
//! - a region's rows coming and going: the container's subtree (sibling combinators,
//!   `:last-child`) and, with `:has` rules, its ancestors;
//! - a class or attribute toggled on a node: its parent's subtree (descendant and sibling
//!   combinators reach the node's descendants and later siblings) and, with `:has` rules,
//!   the parent's ancestors;
//! - a layout property written on a node: the node itself, whose layout style was
//!   rewritten under the rules it matches.
//!
//! A `:has` outside a selector's subject reaches siblings of ancestors, which this does not
//! follow, so such a stylesheet takes the whole-tree pass.

use std::collections::HashSet;

use crate::css_parser::{ComplexSelector, SelectorPart};
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

/// Where an update changed the tree, for [`RenderTree::apply_stylesheet_layout_overrides_scoped`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Restyle {
    /// The node alone.
    Node(LayoutNodeId),
    /// The node and everything under it.
    Subtree(LayoutNodeId),
}

impl RenderTree {
    /// Record that an update changed what layout rules may match from `restyle`.
    pub(crate) fn note_layout_restyle(&mut self, restyle: Restyle) {
        self.layout_restyle.push(restyle);
    }

    /// The CSS layout pass on the nodes the recorded updates can have changed the matches
    /// of, or on every node when `whole` or when the stylesheet's `:has` reaches further.
    /// Clears the record.
    pub fn apply_stylesheet_layout_overrides_scoped(&mut self, whole: bool) {
        let recorded = std::mem::take(&mut self.layout_restyle);
        let Some(stylesheet) = self.stylesheet.clone() else {
            return;
        };
        let rules = stylesheet.complex_rules();
        if whole || recorded.is_empty() || rules.iter().any(|(s, _)| has_beyond_subject(s)) {
            self.apply_stylesheet_layout_overrides();
            return;
        }
        let ancestors = rules.iter().any(|(s, _)| has_in_subject(s));
        // Many updates name one root: the rows of a grid toggling classes all name the grid.
        let roots: HashSet<Restyle> = recorded.into_iter().collect();
        let mut nodes = crate::tree::NodeSet::default();
        let mut subtree = Vec::new();
        for restyle in roots {
            let root = match restyle {
                Restyle::Node(node) => {
                    nodes.insert(node);
                    node
                }
                Restyle::Subtree(node) => {
                    subtree.clear();
                    self.collect_subtree_ids(node, &mut subtree);
                    nodes.extend(subtree.iter().copied());
                    node
                }
            };
            if ancestors {
                nodes.extend(self.layout_tree.ancestors(root));
            }
        }
        nodes.retain(|node| self.render_nodes.contains_key(node));
        if !nodes.is_empty() {
            self.apply_stylesheet_layout_overrides_filtered(Some(&nodes));
        }
    }
}

fn has_in(parts: &[SelectorPart]) -> bool {
    parts.iter().any(|p| matches!(p, SelectorPart::Has(_)))
}

/// A `:has` in the subject compound: it reaches the subject's descendants.
fn has_in_subject(selector: &ComplexSelector) -> bool {
    selector
        .segments
        .last()
        .is_some_and(|(compound, _)| has_in(&compound.parts))
}

/// A `:has` in a compound before the subject: it reaches beyond the subject's ancestors.
fn has_beyond_subject(selector: &ComplexSelector) -> bool {
    let n = selector.segments.len();
    selector.segments[..n.saturating_sub(1)]
        .iter()
        .any(|(compound, _)| has_in(&compound.parts))
}
