//! Element tree snapshot types for recording UI state.

use super::primitives::{Rect, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A complete snapshot of the element tree at a point in time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeSnapshot {
    /// When this snapshot was taken.
    pub timestamp: Timestamp,
    /// All elements in the tree, keyed by element ID.
    pub elements: HashMap<String, ElementSnapshot>,
    /// The root element ID.
    pub root_id: Option<String>,
    /// Currently focused element ID.
    pub focused_element: Option<String>,
    /// Currently hovered element ID.
    pub hovered_element: Option<String>,
    /// Window dimensions at time of snapshot.
    pub window_size: (u32, u32),
    /// Scale factor at time of snapshot.
    pub scale_factor: f64,
}

impl TreeSnapshot {
    /// Create a new empty snapshot.
    pub fn new(timestamp: Timestamp, window_size: (u32, u32), scale_factor: f64) -> Self {
        Self {
            timestamp,
            elements: HashMap::new(),
            root_id: None,
            focused_element: None,
            hovered_element: None,
            window_size,
            scale_factor,
        }
    }

    /// Get an element by ID.
    pub fn get(&self, id: &str) -> Option<&ElementSnapshot> {
        self.elements.get(id)
    }

    /// Get the total number of elements.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Iterate over all elements.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ElementSnapshot)> {
        self.elements.iter()
    }

    /// Get all visible elements.
    pub fn visible_elements(&self) -> impl Iterator<Item = (&String, &ElementSnapshot)> {
        self.elements.iter().filter(|(_, e)| e.is_visible)
    }
}

/// A snapshot of a single element's state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ElementSnapshot {
    /// The element's string ID.
    pub id: String,
    /// Element type identifier (e.g., "Div", "Text", "Button").
    pub element_type: String,
    /// The element's computed bounds.
    pub bounds: Rect,
    /// Whether the element is visible.
    pub is_visible: bool,
    /// Whether the element is focused.
    pub is_focused: bool,
    /// Whether the element is hovered.
    pub is_hovered: bool,
    /// Whether the element is interactive (can receive events).
    pub is_interactive: bool,
    /// Child element IDs.
    pub children: Vec<String>,
    /// Parent element ID (if any).
    pub parent: Option<String>,
    /// Optional visual properties (for detailed inspection).
    pub visual_props: Option<VisualProps>,
    /// Optional text content (for text elements).
    pub text_content: Option<String>,
}

impl ElementSnapshot {
    /// Create a new element snapshot with minimal required data.
    pub fn new(id: String, element_type: String, bounds: Rect) -> Self {
        Self {
            id,
            element_type,
            bounds,
            is_visible: true,
            is_focused: false,
            is_hovered: false,
            is_interactive: false,
            children: Vec::new(),
            parent: None,
            visual_props: None,
            text_content: None,
        }
    }

    /// Check if this element has children.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Check if this element is a leaf node.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// Optional visual properties for detailed element inspection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisualProps {
    /// Background color (RGBA).
    pub background_color: Option<[f32; 4]>,
    /// Border color (RGBA).
    pub border_color: Option<[f32; 4]>,
    /// Border width.
    pub border_width: Option<f32>,
    /// Border radius.
    pub border_radius: Option<f32>,
    /// Opacity.
    pub opacity: Option<f32>,
    /// Transform matrix (if any).
    pub transform: Option<[f32; 6]>,
    /// Additional CSS-like properties.
    pub styles: HashMap<String, String>,
}

impl Default for VisualProps {
    fn default() -> Self {
        Self {
            background_color: None,
            border_color: None,
            border_width: None,
            border_radius: None,
            opacity: None,
            transform: None,
            styles: HashMap::new(),
        }
    }
}

/// Difference between two tree snapshots.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeDiff {
    /// Elements that were added.
    pub added: Vec<String>,
    /// Elements that were removed.
    pub removed: Vec<String>,
    /// Elements that were modified.
    pub modified: HashMap<String, ElementDiff>,
    /// Whether the focused element changed.
    pub focus_changed: bool,
    /// Whether the hovered element changed.
    pub hover_changed: bool,
}

impl TreeDiff {
    /// Check if there are any changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.modified.is_empty()
            && !self.focus_changed
            && !self.hover_changed
    }

    /// Get the total number of changes.
    pub fn change_count(&self) -> usize {
        self.added.len() + self.removed.len() + self.modified.len()
    }
}

/// Difference for a single element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ElementDiff {
    /// Category of change.
    pub category: ChangeCategory,
    /// What specifically changed.
    pub changes: Vec<PropertyChange>,
}

/// Categories of element changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeCategory {
    /// Visual-only change (color, opacity, etc.).
    Visual,
    /// Layout change (position, size).
    Layout,
    /// Structural change (children added/removed).
    Structural,
    /// State change (focus, hover, etc.).
    State,
}

/// A specific property change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyChange {
    /// Name of the property that changed.
    pub property: String,
    /// Previous value (stringified).
    pub old_value: Option<String>,
    /// New value (stringified).
    pub new_value: Option<String>,
}

/// Compute the difference between two tree snapshots.
pub fn diff_trees(old: &TreeSnapshot, new: &TreeSnapshot) -> TreeDiff {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = HashMap::new();

    // Find added elements
    for id in new.elements.keys() {
        if !old.elements.contains_key(id) {
            added.push(id.clone());
        }
    }

    // Find removed elements
    for id in old.elements.keys() {
        if !new.elements.contains_key(id) {
            removed.push(id.clone());
        }
    }

    // Find modified elements
    for (id, new_elem) in &new.elements {
        if let Some(old_elem) = old.elements.get(id) {
            if let Some(diff) = diff_elements(old_elem, new_elem) {
                modified.insert(id.clone(), diff);
            }
        }
    }

    TreeDiff {
        added,
        removed,
        modified,
        focus_changed: old.focused_element != new.focused_element,
        hover_changed: old.hovered_element != new.hovered_element,
    }
}

/// Compute the difference between two element snapshots.
fn diff_elements(old: &ElementSnapshot, new: &ElementSnapshot) -> Option<ElementDiff> {
    let mut changes = Vec::new();
    let mut category = ChangeCategory::Visual;

    // Check bounds changes (layout)
    if old.bounds != new.bounds {
        category = ChangeCategory::Layout;
        if old.bounds.x != new.bounds.x || old.bounds.y != new.bounds.y {
            changes.push(PropertyChange {
                property: "position".to_string(),
                old_value: Some(format!("({}, {})", old.bounds.x, old.bounds.y)),
                new_value: Some(format!("({}, {})", new.bounds.x, new.bounds.y)),
            });
        }
        if old.bounds.width != new.bounds.width || old.bounds.height != new.bounds.height {
            changes.push(PropertyChange {
                property: "size".to_string(),
                old_value: Some(format!("{}x{}", old.bounds.width, old.bounds.height)),
                new_value: Some(format!("{}x{}", new.bounds.width, new.bounds.height)),
            });
        }
    }

    // Check visibility
    if old.is_visible != new.is_visible {
        category = ChangeCategory::Visual;
        changes.push(PropertyChange {
            property: "visible".to_string(),
            old_value: Some(old.is_visible.to_string()),
            new_value: Some(new.is_visible.to_string()),
        });
    }

    // Check focus/hover state
    if old.is_focused != new.is_focused || old.is_hovered != new.is_hovered {
        category = ChangeCategory::State;
        if old.is_focused != new.is_focused {
            changes.push(PropertyChange {
                property: "focused".to_string(),
                old_value: Some(old.is_focused.to_string()),
                new_value: Some(new.is_focused.to_string()),
            });
        }
        if old.is_hovered != new.is_hovered {
            changes.push(PropertyChange {
                property: "hovered".to_string(),
                old_value: Some(old.is_hovered.to_string()),
                new_value: Some(new.is_hovered.to_string()),
            });
        }
    }

    // Check children (structural)
    if old.children != new.children {
        category = ChangeCategory::Structural;
        changes.push(PropertyChange {
            property: "children".to_string(),
            old_value: Some(format!("{} children", old.children.len())),
            new_value: Some(format!("{} children", new.children.len())),
        });
    }

    if changes.is_empty() {
        None
    } else {
        Some(ElementDiff { category, changes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_diff_detects_added() {
        let old = TreeSnapshot::new(Timestamp::zero(), (800, 600), 1.0);
        let mut new = TreeSnapshot::new(Timestamp::from_micros(1000), (800, 600), 1.0);
        new.elements.insert(
            "new-elem".to_string(),
            ElementSnapshot::new(
                "new-elem".to_string(),
                "Div".to_string(),
                Rect::new(0.0, 0.0, 100.0, 100.0),
            ),
        );

        let diff = diff_trees(&old, &new);
        assert_eq!(diff.added, vec!["new-elem".to_string()]);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn test_tree_diff_detects_removed() {
        let mut old = TreeSnapshot::new(Timestamp::zero(), (800, 600), 1.0);
        old.elements.insert(
            "old-elem".to_string(),
            ElementSnapshot::new(
                "old-elem".to_string(),
                "Div".to_string(),
                Rect::new(0.0, 0.0, 100.0, 100.0),
            ),
        );
        let new = TreeSnapshot::new(Timestamp::from_micros(1000), (800, 600), 1.0);

        let diff = diff_trees(&old, &new);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed, vec!["old-elem".to_string()]);
    }
}
