//! What could take the pointer from the element last hit while the pointer stays
//! inside that element's bounds: the elements the hit test prefers over it.

use std::collections::HashMap;

use crate::element::RenderLayer;
use crate::renderer::RenderTree;
use crate::tree::LayoutNodeId;

/// A rect `(x, y, width, height)` in window coordinates.
pub(super) type Rect = (f32, f32, f32, f32);

fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// The parts of `leaf` covered by an element the hit test would take first: at
/// each level of `chain` (root to leaf), the later siblings of the chain's child
/// and any sibling on the foreground layer. A sibling that does not clip counts
/// with its children, which can reach past it. One whose place depends on the
/// scroll (fixed or sticky) counts as covering the whole leaf.
pub(super) fn rects_above(
    tree: &RenderTree,
    chain: &[LayoutNodeId],
    chain_bounds: &HashMap<u64, Rect>,
    leaf: Rect,
) -> Vec<Rect> {
    let mut covering = Vec::new();
    // Under a culling scroll container only the children near the leaf are
    // looked at, as in the hit test.
    let mut culled = false;
    for pair in chain.windows(2) {
        let (parent, child) = (pair[0], pair[1]);
        let Some(&(x, y, _, _)) = chain_bounds.get(&parent.to_raw()) else {
            continue;
        };
        let scroll = tree.get_scroll_offset(parent);
        let offset = (x + scroll.0, y + scroll.1);
        culled |= tree.is_viewport_cull_scroll(parent);
        let siblings = match tree.layout_tree.get_absolute_bounds(parent) {
            Some(at) if culled => tree.hit_candidates(
                parent,
                (offset.0 - at.x, offset.1 - at.y),
                [leaf.0, leaf.1, leaf.2, leaf.3],
            ),
            _ => tree.layout_tree.children(parent),
        };
        // Candidates keep the tree's order. Without the chain's child among them
        // nothing says which come after it, so all count.
        let mut later = !siblings.contains(&child);
        for sibling in siblings {
            if sibling == child {
                later = true;
                continue;
            }
            let Some(props) = tree.get_render_node(sibling).map(|n| &n.props) else {
                continue;
            };
            if !later && props.layer != RenderLayer::Foreground {
                continue;
            }
            if props.is_fixed || props.is_sticky {
                covering.push(leaf);
                continue;
            }
            let Some(bounds) = tree.layout_tree.get_bounds(sibling, offset) else {
                continue;
            };
            let children = tree.layout_tree.children(sibling);
            // Decoration the pointer passes through, with nothing inside it.
            if props.pointer_events_none && children.is_empty() {
                continue;
            }
            let rect = (bounds.x, bounds.y, bounds.width, bounds.height);
            if overlaps(rect, leaf) {
                covering.push(rect);
            }
            if !props.clips_content {
                let inner = (bounds.x, bounds.y);
                covering.extend(children.into_iter().filter_map(|grandchild| {
                    let b = tree.layout_tree.get_bounds(grandchild, inner)?;
                    let rect = (b.x, b.y, b.width, b.height);
                    overlaps(rect, leaf).then_some(rect)
                }));
            }
        }
    }
    covering
}
