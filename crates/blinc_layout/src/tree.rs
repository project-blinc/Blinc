//! Layout tree management

use slotmap::{new_key_type, Key, SlotMap};
use std::collections::HashMap;
use taffy::prelude::*;

use crate::element::ElementBounds;
use crate::text_measure::{measure_text_with_options, TextLayoutOptions};

new_key_type! {
    pub struct LayoutNodeId;
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
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: SlotMap::with_key(),
            reverse_map: HashMap::new(),
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
        if let Some(&taffy_node) = self.node_map.get(root) {
            let _ = self.taffy.compute_layout_with_measure(
                taffy_node,
                available_space,
                text_measure_function,
            );
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

    /// Get computed layout as ElementBounds with parent offset
    pub fn get_bounds(&self, id: LayoutNodeId, parent_offset: (f32, f32)) -> Option<ElementBounds> {
        self.get_layout(id)
            .map(|layout| ElementBounds::from_layout(layout, parent_offset))
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
        let children_to_remove: Vec<_> = children.iter().copied().collect();

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
