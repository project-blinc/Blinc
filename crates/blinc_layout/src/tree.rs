//! Layout tree management

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::{Key, SlotMap, new_key_type};
use taffy::prelude::*;
use taffy::{Baselines, LayoutInput, LayoutOutput, compute_leaf_layout};

use crate::element::ElementBounds;
use crate::text_measure::{TextLayoutOptions, measure_text_with_options};

mod relayout;
#[cfg(test)]
mod relayout_tests;
pub(crate) mod scope;
mod store;

new_key_type! {
    pub struct LayoutNodeId;
}

/// A map keyed by node id. Node ids are small integers, so it hashes them with FxHash
/// instead of the standard library's SipHash.
pub type NodeMap<V> = FxHashMap<LayoutNodeId, V>;

/// A set of node ids, hashed as [`NodeMap`] hashes them.
pub type NodeSet = FxHashSet<LayoutNodeId>;

/// Identity derived from a node's build path and explicit key.
/// Preserved across rebuilds and keyed reordering within a tree.
/// Separate trees can produce the same ID; resolve it through its owning `RenderTree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StableNodeId(u64);

/// A map keyed by stable node id, hashed with FxHash as [`NodeMap`] is.
pub type StableMap<V> = FxHashMap<StableNodeId, V>;

/// A set of stable node ids, hashed as [`StableMap`] hashes them.
pub type StableSet = FxHashSet<StableNodeId>;

impl StableNodeId {
    /// Root of a build pass — used as the seed when minting children
    /// before any node has been created. The hash mixes this with the
    /// first child's sibling index, so the actual root node gets a
    /// non-zero id.
    pub const ROOT: Self = Self(0);

    /// Raw u64 representation. Stable across rebuilds for the same
    /// build path; safe to store in FFI / external systems that need a
    /// plain integer handle.
    pub fn to_raw(self) -> u64 {
        self.0
    }

    /// Reconstruct from a raw `u64`. Caller is responsible for the
    /// value originating from `to_raw()` on a valid `StableNodeId`.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Derive a child stable id from this node's id and either its explicit
    /// key or, for unkeyed children, its 0-indexed sibling position.
    ///
    /// Explicit keys intentionally dominate position so keyed children retain
    /// identity when reordered. Callers must suppress duplicate sibling keys;
    /// the renderer falls back to positional identity for those entries.
    pub fn derive_child(self, sibling_index: usize, widget_key: Option<&str>) -> Self {
        use std::hash::{BuildHasher, Hasher};
        // `rustc_hash` is already a workspace dep (used by routes,
        // ElementRegistry) and is the fastest non-cryptographic hash
        // in the tree; collisions are vanishingly rare for the
        // (u64, usize, &str) tuples we feed it.
        //
        // Salt is a fixed non-zero constant. Without it,
        // `ROOT.derive_child(0, None)` hashes to 0 (the FxHasher
        // multiplicative-XOR pipeline produces 0 when fed only
        // zeros from a zero seed), which would collide with
        // `StableNodeId::ROOT` itself — every node off the root
        // would think it WAS the root, cross-contaminating
        // handler_registry / css_anim_store entries. The salt
        // breaks that and `if h == 0 { 1 }` covers the tiny
        // residual chance that some other path hashes to 0.
        const STABLE_ID_SALT: u64 = 0xa017_3b53_0c1e_9d6a;
        let mut hasher = rustc_hash::FxBuildHasher.build_hasher();
        hasher.write_u64(STABLE_ID_SALT);
        hasher.write_u64(self.0);
        if let Some(k) = widget_key {
            hasher.write_u8(1);
            hasher.write(k.as_bytes());
        } else {
            hasher.write_u8(0);
            hasher.write_usize(sibling_index);
        }
        let h = hasher.finish();
        Self(if h == 0 { 1 } else { h })
    }
}

/// Context stored with text nodes for dynamic measurement during layout
///
/// This allows Taffy to call back and measure text with the actual
/// available width, enabling proper multi-line height calculation.
#[derive(Clone, Debug)]
pub struct TextMeasureContext {
    /// The text content to measure
    pub content: String,
    /// Font size in pixels
    pub font_size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Extra spacing between letters in pixels. Measured text must carry
    /// it or a letter-spaced line lays out narrower than it draws.
    pub letter_spacing: f32,
    /// Extra pixels after each word separator, measured for the same reason
    pub word_spacing: f32,
    /// Whether text should wrap
    pub wrap: bool,
    /// Font family name (if any)
    pub font_name: Option<String>,
    /// Generic font category
    pub generic_font: crate::div::GenericFont,
    /// Font weight (100-900)
    pub font_weight: u16,
    /// Whether text is italic
    pub italic: bool,
}

/// Half the leading of one line: the space the line box adds above the
/// ascender and below the descender, split evenly.
///
/// CSS puts half of it above, which is what moves the baseline down from
/// the top of the box. A face whose natural line height already exceeds
/// the line box gives a negative value, so the text overflows evenly
/// rather than being pushed down.
pub(crate) fn half_leading(metrics: &crate::text_measure::TextMetrics) -> f32 {
    let lines = metrics.line_count.max(1) as f32;
    let line_box = metrics.height / lines;
    let face = metrics.ascender - metrics.descender;
    (line_box - face) / 2.0
}

impl LayoutNodeId {
    /// Convert to a raw u64 representation
    ///
    /// This is useful for storing node IDs in type-erased contexts.
    pub fn to_raw(self) -> u64 {
        self.data().as_ffi()
    }

    /// Create from a raw u64 representation
    ///
    /// # Safety
    /// The raw value must have been created by `to_raw()` from a valid LayoutNodeId.
    pub fn from_raw(raw: u64) -> Self {
        Self::from(slotmap::KeyData::from_ffi(raw))
    }
}

/// Measure function for text nodes during Taffy layout
///
/// This is called by Taffy when computing layout for nodes that have
/// a TextMeasureContext. It measures the text with the actual available
/// width to get proper multi-line height.
fn text_measure_function(
    inputs: LayoutInput,
    node_context: Option<&mut TextMeasureContext>,
    style: &Style,
    content_baseline: Option<f32>,
) -> LayoutOutput {
    let mut output = compute_leaf_layout(
        inputs,
        style,
        |_, _| 0.0,
        |known_dimensions, available_space| {
            measure_text_node(known_dimensions, available_space, node_context)
        },
    );

    if let Some(baseline) = content_baseline {
        // taffy measures a baseline from the border-box top.
        let lead_in = resolve_length(style.border.top, inputs.parent_size.width)
            + resolve_length(style.padding.top, inputs.parent_size.width);
        output.baselines = Baselines::from_first(Some(lead_in + baseline));
    }
    output
}

/// A border or padding length in pixels. A percentage resolves against
/// the parent's width, as CSS specifies.
fn resolve_length(length: LengthPercentage, basis: Option<f32>) -> f32 {
    match length.expand() {
        taffy::style::ExpandedLengthPercentage::Length(v) => v,
        taffy::style::ExpandedLengthPercentage::Percent(p) => basis.map_or(0.0, |b| p * b),
        _ => 0.0,
    }
}

/// Distance from a text node's content-box top to its first baseline,
/// worked out from its measure context: half-leading plus the ascender,
/// which is where the paint path puts the first line.
fn context_baseline(ctx: &TextMeasureContext) -> f32 {
    let mut options = TextLayoutOptions::new();
    options.font_name = ctx.font_name.clone();
    options.generic_font = ctx.generic_font;
    options.font_weight = ctx.font_weight;
    options.italic = ctx.italic;
    options.line_height = ctx.line_height;
    options.letter_spacing = ctx.letter_spacing;
    options.word_spacing = ctx.word_spacing;

    let metrics = measure_text_with_options(&ctx.content, ctx.font_size, &options);
    half_leading(&metrics) + metrics.ascender
}

/// The content size of a text node given what taffy already knows of it.
fn measure_text_node(
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    node_context: Option<&mut TextMeasureContext>,
) -> Size<f32> {
    // If dimensions are already known, use them
    let width = known_dimensions.width;
    let height = known_dimensions.height;

    if let (Some(w), Some(h)) = (width, height) {
        return Size {
            width: w,
            height: h,
        };
    }

    // If no context (not a text node), return zero
    let Some(ctx) = node_context else {
        return Size::ZERO;
    };

    // Don't measure if wrapping is disabled
    if !ctx.wrap {
        // For non-wrapping text, use single-line measurement
        let mut options = TextLayoutOptions::new();
        options.font_name = ctx.font_name.clone();
        options.generic_font = ctx.generic_font;
        options.font_weight = ctx.font_weight;
        options.italic = ctx.italic;
        options.line_height = ctx.line_height;
        options.letter_spacing = ctx.letter_spacing;
        options.word_spacing = ctx.word_spacing;
        // No max_width for non-wrapping

        let metrics = measure_text_with_options(&ctx.content, ctx.font_size, &options);
        return Size {
            width: width.unwrap_or(metrics.width),
            height: height.unwrap_or(metrics.height),
        };
    }

    // Determine available width for wrapping
    let max_width = match available_space.width {
        AvailableSpace::Definite(w) => Some(w),
        AvailableSpace::MaxContent => None,
        AvailableSpace::MinContent => Some(0.0), // Force wrapping at every word
    };

    // If we already know the width, use it as max_width
    let max_width = width.or(max_width);

    // Measure text with wrapping
    let mut options = TextLayoutOptions::new();
    options.font_name = ctx.font_name.clone();
    options.generic_font = ctx.generic_font;
    options.font_weight = ctx.font_weight;
    options.italic = ctx.italic;
    options.line_height = ctx.line_height;
    options.letter_spacing = ctx.letter_spacing;
    options.word_spacing = ctx.word_spacing;
    options.max_width = max_width;

    let metrics = measure_text_with_options(&ctx.content, ctx.font_size, &options);

    Size {
        width: width.unwrap_or(metrics.width),
        height: height.unwrap_or(metrics.height),
    }
}

/// The layout nodes, their styles and their computed layouts.
pub struct LayoutTree {
    scope_id: u64,
    nodes: store::Nodes,
    /// Nodes whose `align_self` is incidental, not authored.
    ///
    /// `w_fit`/`h_fit` set `align_self: Start` for exactly one reason:
    /// without it a content-sized item stretches across the cross axis.
    /// Taffy then gives that incidental value precedence over the
    /// parent's `align_items`, which is CSS-correct but wrong in intent
    /// — a `cn::button` is `w_fit` inside, so a row of differently-sized
    /// buttons hung from their top edges however the row was styled.
    ///
    /// Marking it keeps CSS's guarantee intact: an authored `align_self`
    /// still beats the parent, because only the incidental one is
    /// listed here. See [`Self::resolve_incidental_align_self`].
    incidental_align_self: FxHashSet<LayoutNodeId>,
    /// Moves on every change to the tree or its styles, and on every layout that computes.
    /// Geometry derived from the tree is valid for as long as it is the one it was derived at.
    epoch: u64,
    /// The root and space of the last layout, which a layout with neither changed and no node
    /// changed since would only give again.
    laid_out: Option<(LayoutNodeId, Size<AvailableSpace>)>,
    /// Nodes whose layout an edit may have changed since the last layout, each listed once.
    changed: Vec<LayoutNodeId>,
    /// Counts layouts, to mark the nodes one lays out again.
    relayout_stamp: u32,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            scope_id: scope::allocate(),
            nodes: SlotMap::with_key(),
            incidental_align_self: FxHashSet::default(),
            epoch: 0,
            laid_out: None,
            changed: Vec::new(),
            relayout_stamp: 0,
        }
    }

    /// A number that changes whenever the tree, its styles or its layout do.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Namespace for this layout tree's queued work and subscriptions.
    pub(crate) fn scope_id(&self) -> u64 {
        self.scope_id
    }

    pub(crate) fn enter_scope(&self) -> scope::Guard {
        scope::enter(self.scope_id)
    }

    /// Create a new layout node with the given style
    pub fn create_node(&mut self, style: Style) -> LayoutNodeId {
        self.epoch = self.epoch.wrapping_add(1);
        self.nodes.insert(store::Node::new(style, None))
    }

    /// Create a new text layout node with measure context
    ///
    /// This allows Taffy to dynamically measure text with the actual available
    /// width during layout, enabling proper multi-line height calculation.
    pub fn create_text_node(&mut self, style: Style, context: TextMeasureContext) -> LayoutNodeId {
        self.epoch = self.epoch.wrapping_add(1);
        self.nodes.insert(store::Node::new(style, Some(context)))
    }

    /// Change a text node's measure context in place, so the next `compute_layout` measures it
    /// again.
    ///
    /// False when `id` is gone or is not a text node: a node made by
    /// `create_node` has no context to change.
    pub fn update_text(
        &mut self,
        id: LayoutNodeId,
        f: impl FnOnce(&mut TextMeasureContext),
    ) -> bool {
        self.epoch = self.epoch.wrapping_add(1);
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        let Some(context) = node.context.as_mut() else {
            return false;
        };
        f(context);
        node.derived_baseline = None;
        self.mark_changed(id);
        true
    }

    /// A text node's measure context; `None` for any other node.
    pub fn text_context(&self, id: LayoutNodeId) -> Option<&TextMeasureContext> {
        self.nodes.get(id).and_then(|node| node.context.as_ref())
    }

    /// Set the style for a node
    pub fn set_style(&mut self, id: LayoutNodeId, style: Style) {
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        // An unchanged style is left alone, so a pass that writes every node's style back does
        // not cost a layout.
        if node.style == style {
            return;
        }
        node.style = style;
        let parent = node.parent;
        self.epoch = self.epoch.wrapping_add(1);
        self.mark_changed(id);
        // The parent lays its children out by their styles.
        if let Some(parent) = parent {
            self.mark_changed(parent);
        }
    }

    /// Get the style for a node
    pub fn get_style(&self, id: LayoutNodeId) -> Option<Style> {
        self.style(id).cloned()
    }

    /// Borrow the style for a node.
    pub fn style(&self, id: LayoutNodeId) -> Option<&Style> {
        self.nodes.get(id).map(|node| &node.style)
    }

    /// Add a child to a parent node
    pub fn add_child(&mut self, parent: LayoutNodeId, child: LayoutNodeId) {
        self.epoch = self.epoch.wrapping_add(1);
        if !self.nodes.contains_key(parent) {
            return;
        }
        let Some(node) = self.nodes.get_mut(child) else {
            return;
        };
        node.parent = Some(parent);
        self.nodes[parent].children.push(child);
        self.mark_changed(parent);
    }

    /// Compute layout for a tree rooted at the given node
    ///
    /// With the same root and space as the last layout and no node changed since, the layout
    /// would come out the same, so nothing runs and the epoch stays. With the same root and
    /// space, only what an edit can reach is laid out again; see `relayout`.
    pub fn compute_layout(&mut self, root: LayoutNodeId, available_space: Size<AvailableSpace>) {
        self.resolve_incidental_align_self(root);
        if !self.nodes.contains_key(root) {
            self.epoch = self.epoch.wrapping_add(1);
            self.laid_out = None;
            return;
        }
        let same = self.laid_out == Some((root, available_space));
        if same && self.changed.is_empty() && !self.nodes[root].cache.is_empty() {
            return;
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.laid_out = Some((root, available_space));
        self.relayout(root, available_space, same);
    }

    /// Record where a text node's first baseline sits below its
    /// content-box top, for `align-items: baseline`.
    pub fn set_text_baseline(&mut self, id: LayoutNodeId, offset: f32) {
        self.epoch = self.epoch.wrapping_add(1);
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        if node.reported_baseline != Some(offset) {
            node.reported_baseline = Some(offset);
            self.mark_changed(id);
        }
    }

    /// Replace the baseline a text node's builder reported, if it reported
    /// one: the metrics it came from have changed.
    pub fn refresh_text_baseline(&mut self, id: LayoutNodeId, offset: f32) {
        self.epoch = self.epoch.wrapping_add(1);
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        if node.reported_baseline.is_some_and(|b| b != offset) {
            node.reported_baseline = Some(offset);
            self.mark_changed(id);
        }
    }

    /// Record that this node's `align_self` was set as a side effect of
    /// sizing (`w_fit`/`h_fit`), not because an author asked for it.
    pub fn mark_incidental_align_self(&mut self, id: LayoutNodeId) {
        self.incidental_align_self.insert(id);
    }

    /// Forget that a node's `align_self` was incidental.
    ///
    /// Called when something authored one on top: a CSS `align-self`
    /// rule lands on the same node `w_fit` marked, and the author's
    /// intent replaces the incidental default.
    pub fn clear_incidental_align_self(&mut self, id: LayoutNodeId) {
        self.incidental_align_self.remove(&id);
    }

    /// Let a parent's `align_items` outrank an INCIDENTAL `align_self`.
    ///
    /// An authored `align_self` keeps CSS's precedence: it beats the
    /// parent, which is the control an author is entitled to. Only the
    /// value `w_fit`/`h_fit` set to prevent stretching yields, and only
    /// to a parent that actually names an alignment.
    fn resolve_incidental_align_self(&mut self, _root: LayoutNodeId) {
        if self.incidental_align_self.is_empty() {
            return;
        }
        // The nodes marked, each against its parent: what has to change is among them, so the
        // tree is not walked.
        let marked: Vec<LayoutNodeId> = self.incidental_align_self.iter().copied().collect();
        for child in marked {
            if let Some(mut style) = self.get_style(child)
                && self.yield_incidental_align_self(child, &mut style)
            {
                self.set_style(child, style);
            }
        }
    }

    /// Drop `style`'s `align_self` when it is the incidental one of `id` and the parent names an
    /// alignment, as layout does. Whether it dropped one. Restyling a node passes its style
    /// through this so a style that layout would change back is not set.
    pub fn yield_incidental_align_self(&self, id: LayoutNodeId, style: &mut Style) -> bool {
        if style.align_self.is_none() || !self.incidental_align_self.contains(&id) {
            return false;
        }
        let parent_names_alignment = self
            .nodes
            .get(id)
            .and_then(|node| node.parent)
            .and_then(|parent| self.nodes.get(parent))
            .is_some_and(|parent| parent.style.align_items.is_some());
        if parent_names_alignment {
            style.align_self = None;
        }
        parent_names_alignment
    }

    /// Get the computed layout for a node
    pub fn get_layout(&self, id: LayoutNodeId) -> Option<&Layout> {
        self.nodes.get(id).map(|node| &node.layout)
    }

    /// Check if a node exists in this tree
    pub fn node_exists(&self, id: LayoutNodeId) -> bool {
        self.nodes.contains_key(id)
    }

    /// Remove a node. Its children stay in the tree, detached.
    pub fn remove_node(&mut self, id: LayoutNodeId) {
        self.epoch = self.epoch.wrapping_add(1);
        let Some(node) = self.nodes.remove(id) else {
            return;
        };
        if let Some(parent) = node.parent
            && let Some(p) = self.nodes.get_mut(parent)
        {
            p.children.retain(|&c| c != id);
            self.mark_changed(parent);
        }
        for child in node.children {
            if let Some(c) = self.nodes.get_mut(child)
                && c.parent == Some(id)
            {
                c.parent = None;
            }
        }
    }

    /// Get children of a layout node
    pub fn children(&self, parent: LayoutNodeId) -> Vec<LayoutNodeId> {
        self.children_iter(parent).collect()
    }

    /// Borrow the child list for walks that do not modify the tree.
    pub(crate) fn children_iter(
        &self,
        parent: LayoutNodeId,
    ) -> impl Iterator<Item = LayoutNodeId> + '_ {
        self.nodes
            .get(parent)
            .into_iter()
            .flat_map(|node| node.children.iter().copied())
            .filter(move |&child| self.nodes.contains_key(child))
    }

    /// How many children `parent` has, without listing them.
    pub fn child_count(&self, parent: LayoutNodeId) -> usize {
        self.nodes.get(parent).map_or(0, |node| node.children.len())
    }

    /// Number of children layout actually lays out under `parent`.
    ///
    /// [`Self::children`] drops any child that is no longer in the tree, so
    /// a stale entry is invisible to every tree walk. Comparing this against
    /// `children().len()` is how that is detected.
    pub fn taffy_child_count(&self, parent: LayoutNodeId) -> usize {
        self.child_count(parent)
    }

    /// Whether a node is laid out as `display: none`: it takes no space, and
    /// neither it nor anything under it is drawn.
    pub fn is_display_none(&self, id: LayoutNodeId) -> bool {
        self.nodes
            .get(id)
            .is_some_and(|node| node.style.display == Display::None)
    }

    /// Get computed layout as ElementBounds with parent offset
    pub fn get_bounds(&self, id: LayoutNodeId, parent_offset: (f32, f32)) -> Option<ElementBounds> {
        self.get_layout(id)
            .map(|layout| ElementBounds::from_layout(layout, parent_offset))
    }

    /// Get absolute bounds by walking up the parent chain to accumulate offsets.
    pub fn get_absolute_bounds(&self, id: LayoutNodeId) -> Option<ElementBounds> {
        let node = self.nodes.get(id)?;
        let layout = &node.layout;
        let mut offset_x = 0.0f32;
        let mut offset_y = 0.0f32;
        let mut at = node.parent;
        while let Some(parent) = at.and_then(|p| self.nodes.get(p)) {
            offset_x += parent.layout.location.x;
            offset_y += parent.layout.location.y;
            at = parent.parent;
        }
        Some(ElementBounds {
            x: offset_x + layout.location.x,
            y: offset_y + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        })
    }

    // Baseline alignment is taffy's: a text leaf reports its first baseline
    // from the measure function, and taffy's flexbox and block layout carry
    // it up through containers and align `align-items: baseline` rows, each
    // wrapped line on its own baseline, ignoring out-of-flow children.

    /// Iterate over ancestors of a node (parent, grandparent, ...) as LayoutNodeIds.
    pub fn ancestors(&self, id: LayoutNodeId) -> Vec<LayoutNodeId> {
        let mut result = Vec::new();
        let mut at = self.nodes.get(id).and_then(|node| node.parent);
        while let Some(parent) = at {
            let Some(node) = self.nodes.get(parent) else {
                break;
            };
            result.push(parent);
            at = node.parent;
        }
        result
    }

    /// Get the content size for a scrollable node
    ///
    /// Returns (content_width, content_height) representing the total size of all content
    /// inside this node. This may be larger than the node's size when content overflows.
    /// Useful for computing scroll bounds.
    pub fn get_content_size(&self, id: LayoutNodeId) -> Option<(f32, f32)> {
        self.get_layout(id).map(|layout| {
            (
                layout.scrollable_overflow_rect.right,
                layout.scrollable_overflow_rect.bottom,
            )
        })
    }

    /// Get the number of nodes in the tree
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Remove all children from a node (but keep the node itself)
    pub fn clear_children(&mut self, parent: LayoutNodeId) {
        self.epoch = self.epoch.wrapping_add(1);
        for child in self.children(parent) {
            self.remove_subtree(child);
        }
    }

    /// Remove a node and all its descendants
    pub fn remove_subtree(&mut self, id: LayoutNodeId) {
        self.epoch = self.epoch.wrapping_add(1);
        for child in self.children(id) {
            self.remove_subtree(child);
        }
        self.remove_node(id);
    }

    /// Replace children of a node with new children
    /// Returns the IDs of the old children that were removed
    pub fn replace_children(
        &mut self,
        parent: LayoutNodeId,
        new_children: Vec<LayoutNodeId>,
    ) -> Vec<LayoutNodeId> {
        self.epoch = self.epoch.wrapping_add(1);
        if !self.nodes.contains_key(parent) {
            return Vec::new();
        }
        let old_children = self.children(parent);
        for &child in &old_children {
            self.nodes[child].parent = None;
        }
        let mut kept = Vec::with_capacity(new_children.len());
        for child in new_children {
            let Some(node) = self.nodes.get_mut(child) else {
                continue;
            };
            // A child taken from another parent leaves that parent's list.
            if let Some(previous) = node.parent.replace(parent)
                && previous != parent
                && let Some(p) = self.nodes.get_mut(previous)
            {
                p.children.retain(|&c| c != child);
                self.mark_changed(previous);
            }
            kept.push(child);
        }
        self.nodes[parent].children = kept;
        self.mark_changed(parent);
        old_children
    }
}

impl Drop for LayoutTree {
    fn drop(&mut self) {
        scope::retire(self.scope_id);
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}
