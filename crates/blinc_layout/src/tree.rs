//! Layout tree management

use slotmap::{Key, SlotMap, new_key_type};
use std::collections::HashMap;
use taffy::prelude::*;
use taffy::{LayoutInput, LayoutOutput, compute_leaf_layout};

use crate::element::ElementBounds;
use crate::text_measure::{TextLayoutOptions, measure_text_with_options};

new_key_type! {
    pub struct LayoutNodeId;
}

/// Stable identity for a layout node across tree rebuilds.
///
/// `LayoutNodeId` is a slotmap key — every full rebuild reconstructs the
/// slotmap and regenerates keys, so any subsystem that holds a
/// `LayoutNodeId` across builds (motion bindings, FLIP previous bounds,
/// event-handler captures inside `Stateful`, …) gets a dangling key.
/// `StableNodeId` survives those rebuilds: it's derived from the build
/// path (parent stable id ⊕ sibling index, plus the
/// `InstanceKey` of any source-located widget) and recomputed
/// deterministically each frame.
///
/// The build mints the id and registers a two-way mapping on
/// `RenderTree` (`stable_to_layout` / `layout_to_stable`). Subsystems
/// that need rebuild-stable state key on `StableNodeId`; the renderer's
/// per-frame caches (render_nodes, hashes, layout bounds) stay on
/// `LayoutNodeId` because they're already wiped and rebuilt every pass.
///
/// Layout still flows through `LayoutNodeId` — `StableNodeId` is the
/// identity bookkeeping handle, not a paint-side replacement. See
/// `project_stable_node_id_design` (memory) for the phased migration
/// plan and which maps move when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StableNodeId(u64);

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
    _node_id: NodeId,
    node_context: Option<&mut TextMeasureContext>,
    style: &Style,
) -> LayoutOutput {
    compute_leaf_layout(
        inputs,
        style,
        |_, _| 0.0,
        |known_dimensions, available_space| {
            measure_text_node(known_dimensions, available_space, node_context)
        },
    )
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
    options.max_width = max_width;

    let metrics = measure_text_with_options(&ctx.content, ctx.font_size, &options);

    Size {
        width: width.unwrap_or(metrics.width),
        height: height.unwrap_or(metrics.height),
    }
}

/// Maps between Blinc node IDs and Taffy node IDs
pub struct LayoutTree {
    taffy: TaffyTree<TextMeasureContext>,
    node_map: SlotMap<LayoutNodeId, NodeId>,
    /// Reverse mapping from Taffy NodeId to our LayoutNodeId
    reverse_map: HashMap<NodeId, LayoutNodeId>,
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
    incidental_align_self: std::collections::HashSet<LayoutNodeId>,
    /// Downward shift applied to a node after taffy, so baseline-aligned
    /// items in a flex line share a baseline. See `align_baselines`.
    baseline_shift: std::collections::HashMap<LayoutNodeId, f32>,
    /// Distance from a text node's content-box top to its first
    /// baseline, as the element that built it measured.
    ///
    /// Non-wrapping text gets fixed dimensions rather than a measure
    /// context, so there is nothing for `baseline_offset` to measure
    /// from. The builder already has the metrics, so it reports them.
    text_baseline: std::collections::HashMap<LayoutNodeId, f32>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: SlotMap::with_key(),
            reverse_map: HashMap::new(),
            incidental_align_self: std::collections::HashSet::new(),
            baseline_shift: std::collections::HashMap::new(),
            text_baseline: std::collections::HashMap::new(),
        }
    }

    /// Create a new layout node with the given style
    pub fn create_node(&mut self, style: Style) -> LayoutNodeId {
        let taffy_node = self.taffy.new_leaf(style).unwrap();
        let id = self.node_map.insert(taffy_node);
        self.reverse_map.insert(taffy_node, id);
        id
    }

    /// Create a new text layout node with measure context
    ///
    /// This allows Taffy to dynamically measure text with the actual available
    /// width during layout, enabling proper multi-line height calculation.
    pub fn create_text_node(&mut self, style: Style, context: TextMeasureContext) -> LayoutNodeId {
        let taffy_node = self.taffy.new_leaf_with_context(style, context).unwrap();
        let id = self.node_map.insert(taffy_node);
        self.reverse_map.insert(taffy_node, id);
        id
    }

    /// Change a text node's measure context in place and mark the node
    /// dirty, so the next `compute_layout` measures it again.
    ///
    /// False when `id` is gone or is not a text node: a node made by
    /// `create_node` has no context to change.
    pub fn update_text(
        &mut self,
        id: LayoutNodeId,
        f: impl FnOnce(&mut TextMeasureContext),
    ) -> bool {
        let Some(&taffy_node) = self.node_map.get(id) else {
            return false;
        };
        let Some(context) = self.taffy.get_node_context_mut(taffy_node) else {
            return false;
        };
        f(context);
        // `get_node_context_mut` leaves taffy's cached layout in place.
        let _ = self.taffy.mark_dirty(taffy_node);
        true
    }

    /// A text node's measure context; `None` for any other node.
    pub fn text_context(&self, id: LayoutNodeId) -> Option<&TextMeasureContext> {
        self.node_map
            .get(id)
            .and_then(|&taffy_node| self.taffy.get_node_context(taffy_node))
    }

    /// Set the style for a node
    pub fn set_style(&mut self, id: LayoutNodeId, style: Style) {
        if let Some(&taffy_node) = self.node_map.get(id) {
            let _ = self.taffy.set_style(taffy_node, style);
        }
    }

    /// Get the style for a node
    pub fn get_style(&self, id: LayoutNodeId) -> Option<Style> {
        self.node_map
            .get(id)
            .and_then(|&taffy_node| self.taffy.style(taffy_node).ok())
            .cloned()
    }

    /// Add a child to a parent node
    pub fn add_child(&mut self, parent: LayoutNodeId, child: LayoutNodeId) {
        if let (Some(&parent_node), Some(&child_node)) =
            (self.node_map.get(parent), self.node_map.get(child))
        {
            let _ = self.taffy.add_child(parent_node, child_node);
        }
    }

    /// Compute layout for a tree rooted at the given node
    pub fn compute_layout(&mut self, root: LayoutNodeId, available_space: Size<AvailableSpace>) {
        self.resolve_incidental_align_self(root);
        self.baseline_shift.clear();
        if let Some(&taffy_node) = self.node_map.get(root) {
            let _ = self.taffy.compute_layout_with_measure(
                taffy_node,
                available_space,
                text_measure_function,
            );
        }
        self.align_baselines(root);
    }

    /// Record where a text node's first baseline sits below its
    /// content-box top, for `align-items: baseline`.
    pub fn set_text_baseline(&mut self, id: LayoutNodeId, offset: f32) {
        self.text_baseline.insert(id, offset);
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
    fn resolve_incidental_align_self(&mut self, root: LayoutNodeId) {
        if self.incidental_align_self.is_empty() {
            return;
        }
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let children = self.children(id);
            if self.get_style(id).is_some_and(|s| s.align_items.is_some()) {
                for &child in &children {
                    if self.incidental_align_self.contains(&child)
                        && let Some(mut style) = self.get_style(child)
                        && style.align_self.is_some()
                    {
                        style.align_self = None;
                        self.set_style(child, style);
                    }
                }
            }
            stack.extend(children);
        }
    }

    /// Get the computed layout for a node
    pub fn get_layout(&self, id: LayoutNodeId) -> Option<&Layout> {
        self.node_map
            .get(id)
            .and_then(|&taffy_node| self.taffy.layout(taffy_node).ok())
    }

    /// Check if a node exists in this tree
    pub fn node_exists(&self, id: LayoutNodeId) -> bool {
        self.node_map.contains_key(id)
    }

    /// Remove a node
    pub fn remove_node(&mut self, id: LayoutNodeId) {
        if let Some(taffy_node) = self.node_map.remove(id) {
            self.reverse_map.remove(&taffy_node);
            let _ = self.taffy.remove(taffy_node);
        }
    }

    /// Get children of a layout node
    pub fn children(&self, parent: LayoutNodeId) -> Vec<LayoutNodeId> {
        let Some(&taffy_node) = self.node_map.get(parent) else {
            return Vec::new();
        };

        let Ok(children) = self.taffy.children(taffy_node) else {
            return Vec::new();
        };

        children
            .iter()
            .filter_map(|&child_taffy| self.reverse_map.get(&child_taffy).copied())
            .collect()
    }

    /// Number of children taffy actually lays out under `parent`.
    ///
    /// [`Self::children`] drops any child missing from `reverse_map`, so a
    /// node detached from the id maps but still attached in taffy is
    /// invisible to every tree walk while still contributing to its
    /// parent's size. Comparing this against `children().len()` is how
    /// that is detected.
    pub fn taffy_child_count(&self, parent: LayoutNodeId) -> usize {
        self.node_map
            .get(parent)
            .and_then(|&taffy_node| self.taffy.children(taffy_node).ok())
            .map(|c| c.len())
            .unwrap_or(0)
    }

    /// Get computed layout as ElementBounds with parent offset
    pub fn get_bounds(&self, id: LayoutNodeId, parent_offset: (f32, f32)) -> Option<ElementBounds> {
        self.get_layout(id).map(|layout| {
            let mut bounds = ElementBounds::from_layout(layout, parent_offset);
            // Only this node's own shift: the paint walk accumulates
            // parent offsets itself, and a parent's bounds already
            // carried its shift when the walk passed through it.
            bounds.y += self.shift_of(id);
            bounds
        })
    }

    /// Get absolute bounds by walking up the taffy parent chain to accumulate offsets.
    pub fn get_absolute_bounds(&self, id: LayoutNodeId) -> Option<ElementBounds> {
        let &taffy_node = self.node_map.get(id)?;
        let layout = self.taffy.layout(taffy_node).ok()?;

        // Walk up parent chain to accumulate absolute offset
        let mut offset_x = 0.0f32;
        let mut offset_y = 0.0f32;
        // A baseline shift moves a node and everything under it, so an
        // ancestor's shift accumulates the same way its position does.
        let mut shift = self.shift_of(id);
        let mut current = taffy_node;
        while let Some(parent) = self.taffy.parent(current) {
            if let Ok(parent_layout) = self.taffy.layout(parent) {
                offset_x += parent_layout.location.x;
                offset_y += parent_layout.location.y;
            }
            if let Some(&parent_id) = self.reverse_map.get(&parent) {
                shift += self.shift_of(parent_id);
            }
            current = parent;
        }

        Some(ElementBounds {
            x: offset_x + layout.location.x,
            y: offset_y + layout.location.y + shift,
            width: layout.size.width,
            height: layout.size.height,
        })
    }

    // ── Baseline alignment ──────────────────────────────────────────
    //
    // taffy's `compute_leaf_layout` gives a text leaf no first baseline,
    // and its flexbox then falls back to `unwrap_or(size.height)`, so
    // `align-items: baseline` aligns box bottoms. Since a text box is
    // `font_size * line_height` tall, that puts each item's baseline at
    // `row_bottom - 0.4 * font_size` or so: the bigger the text, the
    // higher its baseline sits.
    //
    // The shifts are computed here and added in `get_absolute_bounds`,
    // which is the only path anything positions from. taffy's own stored
    // layout is left alone, because there is no public way to write it.

    /// Distance from a node's border-box top down to the baseline its
    /// text is drawn on, or `None` for a node that has no baseline of
    /// its own.
    ///
    /// A text leaf: top border + top padding + half-leading + ascender,
    /// which is where the paint path puts the first line. A container:
    /// its first baseline-bearing child's, in this node's coordinates,
    /// which is what CSS means by a box's first baseline.
    fn baseline_offset(&self, id: LayoutNodeId) -> Option<f32> {
        let layout = self.get_layout(id)?;
        let lead_in = layout.border.top + layout.padding.top;

        if let Some(&reported) = self.text_baseline.get(&id) {
            return Some(lead_in + reported);
        }

        if let Some(ctx) = self.text_context(id) {
            let mut options = crate::text_measure::TextLayoutOptions::new();
            options.font_name = ctx.font_name.clone();
            options.generic_font = ctx.generic_font;
            options.font_weight = ctx.font_weight;
            options.italic = ctx.italic;
            options.line_height = ctx.line_height;
            options.letter_spacing = ctx.letter_spacing;

            let metrics = crate::text_measure::measure_text_with_options(
                &ctx.content,
                ctx.font_size,
                &options,
            );
            return Some(lead_in + half_leading(&metrics) + metrics.ascender);
        }

        // A container takes its first child's baseline. Walking in order
        // rather than taking child 0 skips children that have none, such
        // as an icon beside a label. Out-of-flow children are skipped
        // for the same reason they are not aligned: they are not flex
        // items, so they do not define the container's first baseline.
        for child in self.children(id) {
            if !self.is_in_flow(child) {
                continue;
            }
            if let (Some(child_layout), Some(offset)) =
                (self.get_layout(child), self.baseline_offset(child))
            {
                return Some(child_layout.location.y + offset + self.shift_of(child));
            }
        }

        None
    }

    /// The shift already assigned to a node, zero if none.
    fn shift_of(&self, id: LayoutNodeId) -> f32 {
        self.baseline_shift.get(&id).copied().unwrap_or(0.0)
    }

    /// Whether a child takes part in its parent's baseline alignment.
    ///
    /// `align_self` outranks the parent's `align_items`, except where
    /// `w_fit`/`h_fit` set it as a side effect: that is not an author
    /// asking to opt out, so such a node follows the parent.
    /// Whether a child is in flow, and so a flex item at all.
    ///
    /// An absolutely positioned child of a flex container is not a flex
    /// item: it is positioned against the container's padding box and
    /// takes no part in alignment. Shifting one would move a box whose
    /// position its author computed.
    fn is_in_flow(&self, child: LayoutNodeId) -> bool {
        self.get_style(child)
            .is_none_or(|s| s.position != Position::Absolute)
    }

    fn aligns_to_baseline(&self, parent_align: Option<AlignItems>, child: LayoutNodeId) -> bool {
        if !self.is_in_flow(child) {
            return false;
        }

        let authored = self
            .get_style(child)
            .and_then(|s| s.align_self)
            .filter(|_| !self.incidental_align_self.contains(&child));

        match authored {
            Some(a) => a == AlignSelf::BASELINE,
            None => parent_align == Some(AlignItems::BASELINE),
        }
    }

    /// Line up the baselines of every baseline-aligned flex line in the
    /// subtree, deepest first so a container's own baseline already
    /// reflects its aligned children.
    fn align_baselines(&mut self, id: LayoutNodeId) {
        for child in self.children(id) {
            self.align_baselines(child);
        }

        let Some(style) = self.get_style(id) else {
            return;
        };
        if style.display != Display::Flex {
            return;
        }
        // Baselines run along the cross axis of a row. In a column the
        // cross axis is horizontal, where CSS aligns by a vertical
        // baseline Blinc has no notion of, so leave those alone.
        if matches!(
            style.flex_direction,
            FlexDirection::Column | FlexDirection::ColumnReverse
        ) {
            return;
        }

        let participants: Vec<LayoutNodeId> = self
            .children(id)
            .into_iter()
            .filter(|&c| self.aligns_to_baseline(style.align_items, c))
            .collect();
        if participants.len() < 2 {
            return;
        }

        // Each wrapped line aligns on its own baseline, so group by the
        // row a child was placed on before shifting anything.
        let mut lines: Vec<Vec<(LayoutNodeId, f32)>> = Vec::new();
        for child in participants {
            let Some(offset) = self.baseline_offset(child) else {
                continue;
            };
            let Some(layout) = self.get_layout(child) else {
                continue;
            };
            let top = layout.location.y;
            let height = layout.size.height;
            // Where the baseline sits in the PARENT, not in the child:
            // taffy has already placed the boxes at different tops, so
            // aligning the within-box offsets alone would leave them
            // apart by exactly that difference.
            let baseline = top + offset;

            match lines.iter_mut().find(|line| {
                line.iter().any(|&(other, _)| {
                    self.get_layout(other).is_some_and(|l| {
                        // Same line when the boxes overlap vertically.
                        top < l.location.y + l.size.height && l.location.y < top + height
                    })
                })
            }) {
                Some(line) => line.push((child, baseline)),
                None => lines.push(vec![(child, baseline)]),
            }
        }

        for line in lines {
            let Some(deepest) = line
                .iter()
                .map(|&(_, baseline)| baseline)
                .fold(None::<f32>, |acc, b| Some(acc.map_or(b, |a: f32| a.max(b))))
            else {
                continue;
            };
            for (child, baseline) in line {
                let shift = deepest - baseline;
                if shift.abs() > f32::EPSILON {
                    *self.baseline_shift.entry(child).or_insert(0.0) += shift;
                }
            }
        }
    }

    /// Iterate over ancestors of a node (parent, grandparent, ...) as LayoutNodeIds.
    pub fn ancestors(&self, id: LayoutNodeId) -> Vec<LayoutNodeId> {
        let mut result = Vec::new();
        let Some(&taffy_node) = self.node_map.get(id) else {
            return result;
        };
        let mut current = taffy_node;
        while let Some(parent) = self.taffy.parent(current) {
            if let Some(&layout_id) = self.reverse_map.get(&parent) {
                result.push(layout_id);
            }
            current = parent;
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
        self.node_map.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.node_map.is_empty()
    }

    /// Remove all children from a node (but keep the node itself)
    pub fn clear_children(&mut self, parent: LayoutNodeId) {
        let Some(&parent_taffy) = self.node_map.get(parent) else {
            return;
        };

        // Get current children
        let Ok(children) = self.taffy.children(parent_taffy) else {
            return;
        };

        // Collect children to remove
        let children_to_remove: Vec<_> = children.to_vec();

        // Remove each child from taffy and our maps
        for child_taffy in children_to_remove {
            if let Some(&child_id) = self.reverse_map.get(&child_taffy) {
                // Recursively remove this child's subtree
                self.remove_subtree(child_id);
            }
        }
    }

    /// Remove a node and all its descendants
    pub fn remove_subtree(&mut self, id: LayoutNodeId) {
        // First get and remove all children recursively
        let children = self.children(id);
        for child in children {
            self.remove_subtree(child);
        }

        // Then remove this node
        self.remove_node(id);
    }

    /// Replace children of a node with new children
    /// Returns the IDs of the old children that were removed
    pub fn replace_children(
        &mut self,
        parent: LayoutNodeId,
        new_children: Vec<LayoutNodeId>,
    ) -> Vec<LayoutNodeId> {
        let Some(&parent_taffy) = self.node_map.get(parent) else {
            return Vec::new();
        };

        // Get current children
        let old_children = self.children(parent);

        // Set new children in taffy
        let new_taffy_children: Vec<_> = new_children
            .iter()
            .filter_map(|&id| self.node_map.get(id).copied())
            .collect();

        let _ = self.taffy.set_children(parent_taffy, &new_taffy_children);

        old_children
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}
