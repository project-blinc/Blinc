//! Layout tree management

use slotmap::{Key, SlotMap, new_key_type};
use std::collections::HashMap;
use taffy::prelude::*;

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
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    _node_id: NodeId,
    node_context: Option<&mut TextMeasureContext>,
    _style: &Style,
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
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: SlotMap::with_key(),
            reverse_map: HashMap::new(),
            incidental_align_self: std::collections::HashSet::new(),
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
        if let Some(&taffy_node) = self.node_map.get(root) {
            let _ = self.taffy.compute_layout_with_measure(
                taffy_node,
                available_space,
                text_measure_function,
            );
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
        self.get_layout(id)
            .map(|layout| ElementBounds::from_layout(layout, parent_offset))
    }

    /// Get absolute bounds by walking up the taffy parent chain to accumulate offsets.
    pub fn get_absolute_bounds(&self, id: LayoutNodeId) -> Option<ElementBounds> {
        let &taffy_node = self.node_map.get(id)?;
        let layout = self.taffy.layout(taffy_node).ok()?;

        // Walk up parent chain to accumulate absolute offset
        let mut offset_x = 0.0f32;
        let mut offset_y = 0.0f32;
        let mut current = taffy_node;
        while let Some(parent) = self.taffy.parent(current) {
            if let Ok(parent_layout) = self.taffy.layout(parent) {
                offset_x += parent_layout.location.x;
                offset_y += parent_layout.location.y;
            }
            current = parent;
        }

        Some(ElementBounds {
            x: offset_x + layout.location.x,
            y: offset_y + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        })
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
        self.get_layout(id)
            .map(|layout| (layout.content_size.width, layout.content_size.height))
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
