//! Node storage for the layout tree, and the view taffy's algorithms lay it out through.
//!
//! Each node keeps what its parent asked of it during layout, with the answers it gave. A node
//! whose content changed can be asked the same again: when every answer is the same, the
//! parent's layout cannot change, so layout stops at that node. See `relayout`.

use slotmap::SlotMap;
use taffy::{
    BlockContext, Cache, CacheTree, Display, Layout, LayoutBlockContainer, LayoutFlexboxContainer,
    LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, Overflow, RunMode,
    Style, TraversePartialTree, TraverseTree, compute_block_layout, compute_cached_layout,
    compute_flexbox_layout, compute_grid_layout, compute_hidden_layout,
};

use super::{LayoutNodeId, TextMeasureContext, context_baseline, text_measure_function};

/// Distinct queries a node keeps before it stops being one that can be asked again.
const MAX_ASKED: usize = 24;

pub(super) type Nodes = SlotMap<LayoutNodeId, Node>;

pub(super) fn tid(id: LayoutNodeId) -> NodeId {
    NodeId::from(id.to_raw())
}

pub(super) fn lid(id: NodeId) -> LayoutNodeId {
    LayoutNodeId::from_raw(u64::from(id))
}

pub(super) struct Node {
    pub style: Style,
    pub context: Option<TextMeasureContext>,
    pub parent: Option<LayoutNodeId>,
    pub children: Vec<LayoutNodeId>,
    pub cache: Cache,
    /// Where the parent put the node and how big it made it, before rounding.
    pub unrounded: Layout,
    /// The same, rounded to whole pixels against the node's absolute position.
    pub layout: Layout,
    /// First baseline the node's builder reported.
    pub reported_baseline: Option<f32>,
    /// First baseline worked out from the measure context, for a node with none reported.
    pub derived_baseline: Option<f32>,
    /// What the parent asked of this node since the parent's cache was last cleared, with the
    /// answers, latest last.
    pub asked: Vec<(LayoutInput, LayoutOutput)>,
    /// A query could not be logged: it carried the parent's float state, or there were too
    /// many. The node cannot be asked again outside its parent's layout.
    pub unreplayable: bool,
    /// Listed in the tree's changed nodes.
    pub queued: bool,
    /// Its own layout may have changed: it was edited, or a child's answers did.
    pub edited: bool,
    /// On the path from a changed node up to its top, during a relayout.
    pub on_spine: bool,
    /// Children on the spine not yet settled, during a relayout.
    pub pending: u32,
    /// The relayout that last laid this node out again.
    pub relaid: u32,
}

impl Node {
    pub fn new(style: Style, context: Option<TextMeasureContext>) -> Self {
        Self {
            style,
            context,
            parent: None,
            children: Vec::new(),
            cache: Cache::new(),
            unrounded: Layout::new(),
            layout: Layout::new(),
            reported_baseline: None,
            derived_baseline: None,
            asked: Vec::new(),
            unreplayable: false,
            queued: false,
            edited: false,
            on_spine: false,
            pending: 0,
            relaid: 0,
        }
    }

    fn measure(&mut self, inputs: LayoutInput) -> LayoutOutput {
        // A baseline the builder reported wins; a node with only a measure context derives one
        // from it.
        let baseline = self.reported_baseline.or_else(|| {
            let ctx = self.context.as_ref()?;
            Some(
                *self
                    .derived_baseline
                    .get_or_insert_with(|| context_baseline(ctx)),
            )
        });
        text_measure_function(inputs, self.context.as_mut(), &self.style, baseline)
    }

    fn note_asked(&mut self, input: LayoutInput, output: LayoutOutput, replayable: bool) {
        if self.unreplayable {
            return;
        }
        if let Some(i) = self.asked.iter().position(|(q, _)| *q == input) {
            self.asked.remove(i);
        } else if !replayable || self.asked.len() == MAX_ASKED {
            self.unreplayable = true;
            self.asked.clear();
            return;
        }
        self.asked.push((input, output));
    }
}

/// Clear a node's cache. Its children's logs go with it: the parent will ask them afresh.
pub(super) fn clear_cache(nodes: &mut Nodes, id: LayoutNodeId) {
    let node = &mut nodes[id];
    node.cache.clear();
    for i in 0..nodes[id].children.len() {
        let child = nodes[id].children[i];
        if let Some(child) = nodes.get_mut(child) {
            child.asked.clear();
            child.unreplayable = false;
        }
    }
}

/// Whether an answer given again matches the one the parent laid out with. A measure only ever
/// reaches the parent as a size (taffy's cache keeps no more), so only that and the margin
/// state, which taffy does not cache, are compared. A layout's scrollable overflow reaches the
/// parent only on an axis where the node's own overflow propagates, as taffy's containers add
/// it up; on the other axes it is the node's own, written back by the caller.
pub(super) fn same_answer(
    style: &Style,
    input: &LayoutInput,
    old: &LayoutOutput,
    new: &LayoutOutput,
) -> bool {
    let margins = old.top_margin == new.top_margin
        && old.bottom_margin == new.bottom_margin
        && old.margins_can_collapse_through == new.margins_can_collapse_through;
    if input.run_mode == RunMode::ComputeSize {
        return old.size == new.size && margins;
    }
    let scroll = style.overflow.x.is_scroll_container() || style.overflow.y.is_scroll_container();
    let open = !scroll && !style.contain.contains_scrollable_overflow();
    let (a, b) = (old.scrollable_overflow_rect, new.scrollable_overflow_rect);
    let x = !(open && style.overflow.x == Overflow::Visible)
        || (a.left == b.left && a.right == b.right);
    let y = !(open && style.overflow.y == Overflow::Visible)
        || (a.top == b.top && a.bottom == b.bottom);
    old.size == new.size && old.baselines == new.baselines && margins && x && y
}

/// The tree as taffy's algorithms see it.
pub(super) struct Pass<'a> {
    pub nodes: &'a mut Nodes,
}

impl Pass<'_> {
    /// Lay out or measure a node, logging the query on it.
    pub fn child_layout(
        &mut self,
        id: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        // A query carrying the parent's float state cannot be asked again without it.
        let replayable = block_ctx.is_none();
        let output = if inputs.run_mode == RunMode::PerformHiddenLayout {
            compute_hidden_layout(self, id)
        } else {
            compute_cached_layout(self, id, inputs, |tree, id, inputs| {
                let node = &tree.nodes[lid(id)];
                match (node.style.display, !node.children.is_empty()) {
                    (Display::None, _) => compute_hidden_layout(tree, id),
                    (Display::Block, true) => compute_block_layout(tree, id, inputs, block_ctx),
                    (Display::FlowRoot, true) => compute_block_layout(tree, id, inputs, None),
                    (Display::Flex, true) => compute_flexbox_layout(tree, id, inputs),
                    (Display::Grid, true) => compute_grid_layout(tree, id, inputs),
                    (_, false) => tree.nodes[lid(id)].measure(inputs),
                }
            })
        };
        self.nodes[lid(id)].note_asked(inputs, output, replayable);
        output
    }
}

pub(super) struct ChildIds<'a>(core::slice::Iter<'a, LayoutNodeId>);

impl Iterator for ChildIds<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<NodeId> {
        self.0.next().map(|&id| tid(id))
    }
}

impl TraversePartialTree for Pass<'_> {
    type ChildIter<'b>
        = ChildIds<'b>
    where
        Self: 'b;

    #[inline]
    fn child_ids(&self, parent: NodeId) -> ChildIds<'_> {
        ChildIds(self.nodes[lid(parent)].children.iter())
    }

    #[inline]
    fn child_count(&self, parent: NodeId) -> usize {
        self.nodes[lid(parent)].children.len()
    }

    #[inline]
    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        tid(self.nodes[lid(parent)].children[index])
    }
}

impl TraverseTree for Pass<'_> {}

impl LayoutPartialTree for Pass<'_> {
    type CoreContainerStyle<'b>
        = &'b Style
    where
        Self: 'b;

    type CustomIdent = String;

    #[inline]
    fn get_core_container_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }

    #[inline]
    fn set_unrounded_layout(&mut self, id: NodeId, layout: &Layout) {
        self.nodes[lid(id)].unrounded = *layout;
    }

    #[inline]
    fn compute_child_layout(&mut self, id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.child_layout(id, inputs, None)
    }
}

impl CacheTree for Pass<'_> {
    #[inline]
    fn cache_get(&mut self, id: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.nodes[lid(id)].cache.get(input)
    }

    #[inline]
    fn cache_store(&mut self, id: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.nodes[lid(id)].cache.store(input, output)
    }

    fn cache_clear(&mut self, id: NodeId) {
        clear_cache(self.nodes, lid(id));
    }
}

impl LayoutBlockContainer for Pass<'_> {
    type BlockContainerStyle<'b>
        = &'b Style
    where
        Self: 'b;
    type BlockItemStyle<'b>
        = &'b Style
    where
        Self: 'b;

    #[inline]
    fn get_block_container_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }

    #[inline]
    fn get_block_child_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }

    #[inline]
    fn compute_block_child_layout(
        &mut self,
        id: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        self.child_layout(id, inputs, block_ctx)
    }
}

impl LayoutFlexboxContainer for Pass<'_> {
    type FlexboxContainerStyle<'b>
        = &'b Style
    where
        Self: 'b;
    type FlexboxItemStyle<'b>
        = &'b Style
    where
        Self: 'b;

    #[inline]
    fn get_flexbox_container_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }

    #[inline]
    fn get_flexbox_child_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }
}

impl LayoutGridContainer for Pass<'_> {
    type GridContainerStyle<'b>
        = &'b Style
    where
        Self: 'b;
    type GridItemStyle<'b>
        = &'b Style
    where
        Self: 'b;

    #[inline]
    fn get_grid_container_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }

    #[inline]
    fn get_grid_child_style(&self, id: NodeId) -> &Style {
        &self.nodes[lid(id)].style
    }
}

#[inline]
fn round(value: f32) -> f32 {
    (value + 0.5).floor()
}

/// Round a subtree's layouts to whole pixels, as taffy's `round_layout` does. Edges are rounded
/// at their absolute position, so `cumulative` is the unrounded position of the node's parent.
pub(super) fn round_subtree(nodes: &mut Nodes, id: LayoutNodeId, cumulative: (f32, f32)) {
    let u = nodes[id].unrounded;
    let x = cumulative.0 + u.location.x;
    let y = cumulative.1 + u.location.y;
    let mut l = u;
    l.location.x = round(u.location.x);
    l.location.y = round(u.location.y);
    l.size.width = round(x + u.size.width) - round(x);
    l.size.height = round(y + u.size.height) - round(y);
    l.scrollbar_size.width = round(u.scrollbar_size.width);
    l.scrollbar_size.height = round(u.scrollbar_size.height);
    l.border.left = round(x + u.border.left) - round(x);
    l.border.right = round(x + u.size.width) - round(x + u.size.width - u.border.right);
    l.border.top = round(y + u.border.top) - round(y);
    l.border.bottom = round(y + u.size.height) - round(y + u.size.height - u.border.bottom);
    l.padding.left = round(x + u.padding.left) - round(x);
    l.padding.right = round(x + u.size.width) - round(x + u.size.width - u.padding.right);
    l.padding.top = round(y + u.padding.top) - round(y);
    l.padding.bottom = round(y + u.size.height) - round(y + u.size.height - u.padding.bottom);
    let r = u.scrollable_overflow_rect;
    l.scrollable_overflow_rect.left = round(x + r.left) - round(x);
    l.scrollable_overflow_rect.right = round(x + r.right) - round(x);
    l.scrollable_overflow_rect.top = round(y + r.top) - round(y);
    l.scrollable_overflow_rect.bottom = round(y + r.bottom) - round(y);
    nodes[id].layout = l;
    for i in 0..nodes[id].children.len() {
        let child = nodes[id].children[i];
        round_subtree(nodes, child, (x, y));
    }
}
