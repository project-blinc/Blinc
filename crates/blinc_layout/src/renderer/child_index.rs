//! An index over the children of a container with very many of them: which draw near a rect
//! without looking at each, and whether a scroll container's content can be moved by a row.
//!
//! The index holds geometry and the checks that read the tree, and is made again when the layout,
//! the render nodes or the motion bindings change. Making it costs less than the scans of the
//! children that it stands in for in the frame it is made.

use super::{AnimationStatus, ElementType, RenderTree};
use crate::tree::LayoutNodeId;
use std::collections::HashMap;

/// Containers with fewer children than this are scanned.
const MIN_CHILDREN: usize = 256;

/// What the index was made from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    layout: u64,
    nodes: u64,
    bindings: u64,
}

pub(super) struct ChildIndex {
    key: Key,
    children: Vec<LayoutNodeId>,
    /// `[x0, y0, x1, y1]` per child: the child and what under it draws, in layout space.
    extent: Vec<[f32; 4]>,
    /// The axis the children are ordered along: 0 for x, 1 for y.
    axis: usize,
    /// Child positions by where their extent starts on the axis.
    order: Vec<u32>,
    /// The furthest end on the axis reached by `order[..=i]`.
    reach: Vec<f32>,
    /// Children that are never culled: something under them moves.
    always: Vec<u32>,
    /// Children on the foreground layer, which a hit test does not cull.
    foreground: Vec<u32>,
    /// Whether every node under the container passes the checks that do not change by the frame.
    qualifies: bool,
}

/// What walking one child's subtree found.
struct Walked {
    rect: [f32; 4],
    mover: bool,
    qualifies: bool,
}

impl RenderTree {
    /// The children of `node` that do not draw wholly outside `view`, in order: `draws_outside`
    /// for each, without looking at each when the container has very many.
    pub fn visible_children(
        &self,
        node: LayoutNodeId,
        shift: (f32, f32),
        view: super::cull::CullRect,
    ) -> Vec<LayoutNodeId> {
        let overscan = self.cull_overscan();
        let found = self.with_child_index(node, |ix| {
            // The view in layout space, widened the way `draws_outside` widens it.
            let lo = [view[0] - shift.0 - overscan, view[1] - shift.1 - overscan];
            let hi = [
                view[0] + view[2] - shift.0 + overscan,
                view[1] + view[3] - shift.1 + overscan,
            ];
            ix.within(lo, hi, false)
        });
        if let Some(found) = found {
            return found;
        }
        self.layout_tree
            .children(node)
            .into_iter()
            .filter(|child| !self.draws_outside(*child, shift, view))
            .collect()
    }

    /// The children of `node` a hit at a point in `view` can reach: those that `visible_children`
    /// gives, and the children on the foreground layer, which draw above what clips them and are
    /// not culled.
    pub fn hit_candidates(
        &self,
        node: LayoutNodeId,
        shift: (f32, f32),
        view: super::cull::CullRect,
    ) -> Vec<LayoutNodeId> {
        let overscan = self.cull_overscan();
        let found = self.with_child_index(node, |ix| {
            let lo = [view[0] - shift.0 - overscan, view[1] - shift.1 - overscan];
            let hi = [
                view[0] + view[2] - shift.0 + overscan,
                view[1] + view[3] - shift.1 + overscan,
            ];
            ix.within(lo, hi, true)
        });
        if let Some(found) = found {
            return found;
        }
        let foreground = |child: &LayoutNodeId| {
            self.render_nodes
                .get(child)
                .is_some_and(|n| n.props.layer == crate::element::RenderLayer::Foreground)
        };
        self.layout_tree
            .children(node)
            .into_iter()
            .filter(|child| foreground(child) || !self.draws_outside(*child, shift, view))
            .collect()
    }

    /// Whether the checks on the subtrees under `node` that do not change by the frame pass,
    /// when `node` has an index.
    pub(super) fn indexed_qualification(&self, node: LayoutNodeId) -> Option<bool> {
        self.with_child_index(node, |ix| ix.qualifies)
    }

    fn index_key(&self) -> Option<Key> {
        // A layout animation moves content by the clock, which no revision records.
        if !self.layout_animations.is_empty() || !self.layout_animations_by_key.is_empty() {
            return None;
        }
        Some(Key {
            layout: self.layout_tree.epoch(),
            nodes: self.render_nodes.revision(),
            bindings: self.motion_bindings.revision(),
        })
    }

    fn with_child_index<R>(
        &self,
        node: LayoutNodeId,
        read: impl FnOnce(&ChildIndex) -> R,
    ) -> Option<R> {
        if self.layout_tree.child_count(node) < MIN_CHILDREN {
            return None;
        }
        let key = self.index_key()?;
        let mut indexes = self.child_indexes.borrow_mut();
        if let Some(ix) = indexes.get(&node)
            && ix.key == key
        {
            return Some(read(ix));
        }
        // What was made for another state is not coming back.
        indexes.retain(|_, ix| ix.key == key);
        let ix = self.build_child_index(node, key);
        let out = read(&ix);
        indexes.insert(node, ix);
        Some(out)
    }

    fn build_child_index(&self, node: LayoutNodeId, key: Key) -> ChildIndex {
        let origin = self
            .layout_tree
            .get_absolute_bounds(node)
            .map(|b| (b.x, b.y))
            .unwrap_or((0.0, 0.0));
        let children = self.layout_tree.children(node);
        let mut extent = Vec::with_capacity(children.len());
        let mut always = Vec::new();
        let mut foreground = Vec::new();
        let mut qualifies = true;
        for (position, child) in children.iter().enumerate() {
            let mut walked = Walked {
                rect: [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                mover: false,
                qualifies: true,
            };
            self.walk_for_index(*child, origin, true, &mut walked);
            if walked.mover {
                always.push(position as u32);
            }
            if self
                .render_nodes
                .get(child)
                .is_some_and(|n| n.props.layer == crate::element::RenderLayer::Foreground)
            {
                foreground.push(position as u32);
            }
            qualifies &= walked.qualifies;
            extent.push(walked.rect);
        }

        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for e in &extent {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(e[axis]);
                hi[axis] = hi[axis].max(e[axis + 2]);
            }
        }
        let axis = if hi[1] - lo[1] >= hi[0] - lo[0] { 1 } else { 0 };
        let mut order: Vec<u32> = (0..children.len() as u32).collect();
        order.sort_by(|a, b| extent[*a as usize][axis].total_cmp(&extent[*b as usize][axis]));
        let mut reach = Vec::with_capacity(order.len());
        let mut furthest = f32::NEG_INFINITY;
        for position in &order {
            furthest = furthest.max(extent[*position as usize][axis + 2]);
            reach.push(furthest);
        }
        ChildIndex {
            key,
            children,
            extent,
            axis,
            order,
            reach,
            always,
            foreground,
            qualifies,
        }
    }

    /// Add `node` and what under it `draws_outside` would look at to `walked`: the rect and the
    /// movers stop at a node that clips its content, the checks for a scroll do not.
    fn walk_for_index(
        &self,
        node: LayoutNodeId,
        parent_origin: (f32, f32),
        in_region: bool,
        walked: &mut Walked,
    ) {
        let Some(layout) = self.layout_tree.get_layout(node) else {
            // `draws_outside` never culls a node it has no bounds for.
            if in_region {
                walked.mover = true;
            }
            return;
        };
        let (x, y) = (
            parent_origin.0 + layout.location.x,
            parent_origin.1 + layout.location.y,
        );
        if in_region {
            let rect = &mut walked.rect;
            rect[0] = rect[0].min(x);
            rect[1] = rect[1].min(y);
            rect[2] = rect[2].max(x + layout.size.width);
            rect[3] = rect[3].max(y + layout.size.height);
            walked.mover |= self.moves_its_content(node);
        }
        walked.qualifies &= self.scroll_node_qualifies(node);
        let clips = self
            .render_nodes
            .get(&node)
            .is_some_and(|n| n.props.clips_content);
        for child in self.layout_tree.children(node) {
            self.walk_for_index(child, (x, y), in_region && !clips, walked);
        }
    }

    /// The checks a node must pass for its scroll container's content to be moved by a row, bar
    /// the one that changes by the frame.
    pub(super) fn scroll_node_qualifies(&self, node: LayoutNodeId) -> bool {
        let Some(render) = self.render_nodes.get(&node) else {
            return true;
        };
        let props = &render.props;
        !self.moves_its_content(node)
            && !props.is_overlay_root
            && !props.is_glass()
            && props.opacity >= 0.999
            && props.layer_effects.is_empty()
            && props.clip_path.is_none()
            && !matches!(
                render.element_type,
                ElementType::Svg(_) | ElementType::Canvas(_)
            )
    }

    /// What keeps the content of the scroll container `container` off rows: for each reason, how
    /// many nodes under it have it (the first that applies to a node). Empty when it qualifies.
    pub fn scroll_row_blockers(
        &self,
        container: LayoutNodeId,
    ) -> std::collections::BTreeMap<&'static str, usize> {
        let mut out = std::collections::BTreeMap::new();
        let mut stack = self.layout_tree.children(container);
        let status = self.current_animation_status.borrow();
        while let Some(node) = stack.pop() {
            stack.extend(self.layout_tree.children(node));
            let reason = match self.render_nodes.get(&node) {
                None => None,
                Some(render) => {
                    let p = &render.props;
                    if matches!(render.element_type, ElementType::Svg(_)) {
                        Some("svg")
                    } else if matches!(render.element_type, ElementType::Canvas(_)) {
                        Some("canvas")
                    } else if self.moves_its_content(node) {
                        Some("moves its content")
                    } else if p.is_overlay_root {
                        Some("overlay root")
                    } else if p.is_glass() {
                        Some("glass")
                    } else if p.opacity < 0.999 {
                        Some("opacity")
                    } else if !p.layer_effects.is_empty() {
                        Some("layer effect")
                    } else if p.clip_path.is_some() {
                        Some("clip path")
                    } else if matches!(status.get(&node), Some(AnimationStatus::Animating(_))) {
                        Some("animating")
                    } else {
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                *out.entry(reason).or_insert(0) += 1;
            }
        }
        out
    }

    /// Whether anything under `container` is animating now.
    pub(super) fn animating_under(&self, container: LayoutNodeId) -> bool {
        let status = self.current_animation_status.borrow();
        status.iter().any(|(node, status)| {
            matches!(status, AnimationStatus::Animating(_))
                && (*node == container || self.layout_tree.ancestors(*node).contains(&container))
        })
    }
}

impl ChildIndex {
    /// The children whose extent comes within the rect `[lo, hi]`, in order, and those that move.
    fn within(&self, lo: [f32; 2], hi: [f32; 2], with_foreground: bool) -> Vec<LayoutNodeId> {
        let axis = self.axis;
        let cross = 1 - axis;
        let before = self
            .order
            .partition_point(|c| self.extent[*c as usize][axis] < hi[axis]);
        let from = self.reach.partition_point(|end| *end <= lo[axis]);
        let mut hits: Vec<u32> = Vec::new();
        for position in from..before.max(from) {
            let c = self.order[position];
            let e = &self.extent[c as usize];
            if e[axis + 2] > lo[axis] && e[cross + 2] > lo[cross] && e[cross] < hi[cross] {
                hits.push(c);
            }
        }
        hits.extend(self.always.iter().copied());
        if with_foreground {
            hits.extend(self.foreground.iter().copied());
        }
        hits.sort_unstable();
        hits.dedup();
        hits.into_iter()
            .map(|c| self.children[c as usize])
            .collect()
    }
}

pub(super) type ChildIndexes = HashMap<LayoutNodeId, ChildIndex>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::div::div;
    use crate::widgets::scroll::scroll;

    fn long_scroll() -> (RenderTree, LayoutNodeId) {
        let mut list = div().flex_col().w(200.0);
        for _ in 0..MIN_CHILDREN * 4 {
            list = list.child(div().w_full().h(20.0));
        }
        let ui = scroll().id("s").w(200.0).h(100.0).child(list);
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 100.0);
        let node = tree.query_by_id("s").unwrap();
        (tree, node)
    }

    #[test]
    fn a_change_that_breaks_the_qualification_is_seen_through_the_index() {
        let (mut tree, scroll) = long_scroll();
        // Twice: the second answer comes from the index.
        assert!(tree.scroll_content_qualifies(scroll));
        assert!(tree.scroll_content_qualifies(scroll));
        let list = tree.layout_tree.children(scroll)[0];
        let row = tree.layout_tree.children(list)[MIN_CHILDREN * 3];
        tree.update_render_props(row, |props| props.opacity = 0.5);
        assert!(
            !tree.scroll_content_qualifies(scroll),
            "a faded row still qualified after the index was made"
        );
        tree.update_render_props(row, |props| props.opacity = 1.0);
        assert!(tree.scroll_content_qualifies(scroll));
    }

    #[test]
    fn an_animating_row_is_seen_without_a_change_to_the_tree() {
        let (tree, scroll) = long_scroll();
        assert!(tree.scroll_content_qualifies(scroll));
        let list = tree.layout_tree.children(scroll)[0];
        let row = tree.layout_tree.children(list)[7];
        tree.current_animation_status.borrow_mut().insert(
            row,
            AnimationStatus::Animating(crate::renderer::AnimatedKind::Css),
        );
        assert!(!tree.scroll_content_qualifies(scroll));
        tree.current_animation_status.borrow_mut().clear();
        assert!(tree.scroll_content_qualifies(scroll));
    }
}
