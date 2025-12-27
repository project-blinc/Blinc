//! Layout tree management

use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;
use taffy::prelude::*;

use crate::element::ElementBounds;

new_key_type! {
    pub struct LayoutNodeId;
}

/// Maps between Blinc node IDs and Taffy node IDs
pub struct LayoutTree {
    taffy: TaffyTree,
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
            let _ = self.taffy.compute_layout(taffy_node, available_space);
        }
    }

    /// Get the computed layout for a node
    pub fn get_layout(&self, id: LayoutNodeId) -> Option<&Layout> {
        self.node_map
            .get(id)
            .and_then(|&taffy_node| self.taffy.layout(taffy_node).ok())
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
    pub fn replace_children(&mut self, parent: LayoutNodeId, new_children: Vec<LayoutNodeId>) -> Vec<LayoutNodeId> {
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
