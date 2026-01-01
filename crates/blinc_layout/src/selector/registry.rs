//! Element registry for O(1) ID-based lookups

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::tree::LayoutNodeId;

/// Registry mapping string IDs to layout node IDs
///
/// This provides O(1) lookup of elements by their string ID.
/// The registry is cleared and rebuilt on each render cycle.
#[derive(Debug, Default)]
pub struct ElementRegistry {
    /// String ID → LayoutNodeId mapping
    ids: RwLock<HashMap<String, LayoutNodeId>>,
    /// Reverse lookup for debugging (LayoutNodeId → String ID)
    reverse: RwLock<HashMap<LayoutNodeId, String>>,
    /// Parent relationships for tree traversal
    parents: RwLock<HashMap<LayoutNodeId, LayoutNodeId>>,
}

impl ElementRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new registry wrapped in Arc for sharing
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Register an element ID
    ///
    /// If the ID already exists, the old mapping is replaced (last-wins).
    /// In debug builds, a warning is logged for duplicate IDs.
    pub fn register(&self, id: impl Into<String>, node_id: LayoutNodeId) {
        let id = id.into();

        #[cfg(debug_assertions)]
        {
            if let Ok(ids) = self.ids.read() {
                if ids.contains_key(&id) {
                    tracing::warn!("Duplicate element ID registered: {}", id);
                }
            }
        }

        if let Ok(mut ids) = self.ids.write() {
            ids.insert(id.clone(), node_id);
        }
        if let Ok(mut reverse) = self.reverse.write() {
            reverse.insert(node_id, id);
        }
    }

    /// Register a parent-child relationship for tree traversal
    pub fn register_parent(&self, child: LayoutNodeId, parent: LayoutNodeId) {
        if let Ok(mut parents) = self.parents.write() {
            parents.insert(child, parent);
        }
    }

    /// Look up a node ID by string ID
    pub fn get(&self, id: &str) -> Option<LayoutNodeId> {
        self.ids.read().ok()?.get(id).copied()
    }

    /// Look up a string ID by node ID (for debugging)
    pub fn get_id(&self, node_id: LayoutNodeId) -> Option<String> {
        self.reverse.read().ok()?.get(&node_id).cloned()
    }

    /// Get the parent of a node
    pub fn get_parent(&self, node_id: LayoutNodeId) -> Option<LayoutNodeId> {
        self.parents.read().ok()?.get(&node_id).copied()
    }

    /// Get all ancestors of a node (from immediate parent to root)
    pub fn ancestors(&self, node_id: LayoutNodeId) -> Vec<LayoutNodeId> {
        let mut result = Vec::new();
        let mut current = node_id;

        while let Some(parent) = self.get_parent(current) {
            result.push(parent);
            current = parent;
        }

        result
    }

    /// Check if an ID is registered
    pub fn contains(&self, id: &str) -> bool {
        self.ids.read().ok().is_some_and(|ids| ids.contains_key(id))
    }

    /// Get the number of registered IDs
    pub fn len(&self) -> usize {
        self.ids.read().ok().map(|ids| ids.len()).unwrap_or(0)
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all registrations (called between render cycles)
    pub fn clear(&self) {
        if let Ok(mut ids) = self.ids.write() {
            ids.clear();
        }
        if let Ok(mut reverse) = self.reverse.write() {
            reverse.clear();
        }
        if let Ok(mut parents) = self.parents.write() {
            parents.clear();
        }
    }

    /// Unregister a specific node (e.g., on unmount)
    pub fn unregister(&self, node_id: LayoutNodeId) {
        // Get the string ID first
        let id = self.get_id(node_id);

        // Remove from reverse map
        if let Ok(mut reverse) = self.reverse.write() {
            reverse.remove(&node_id);
        }

        // Remove from ID map
        if let Some(id) = id {
            if let Ok(mut ids) = self.ids.write() {
                ids.remove(&id);
            }
        }

        // Remove from parents map
        if let Ok(mut parents) = self.parents.write() {
            parents.remove(&node_id);
        }
    }

    /// Get all registered IDs (for debugging)
    pub fn all_ids(&self) -> Vec<String> {
        self.ids
            .read()
            .ok()
            .map(|ids| ids.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let registry = ElementRegistry::new();
        let node_id = LayoutNodeId::default();

        registry.register("test-id", node_id);

        assert_eq!(registry.get("test-id"), Some(node_id));
        assert_eq!(registry.get("nonexistent"), None);
    }

    #[test]
    fn test_reverse_lookup() {
        let registry = ElementRegistry::new();
        let node_id = LayoutNodeId::default();

        registry.register("my-element", node_id);

        assert_eq!(registry.get_id(node_id), Some("my-element".to_string()));
    }

    #[test]
    fn test_clear() {
        let registry = ElementRegistry::new();
        let node_id = LayoutNodeId::default();

        registry.register("test-id", node_id);
        assert!(registry.contains("test-id"));

        registry.clear();
        assert!(!registry.contains("test-id"));
        assert!(registry.is_empty());
    }

    #[test]
    fn test_duplicate_id_last_wins() {
        let registry = ElementRegistry::new();
        let node1 = LayoutNodeId::default();
        // Note: In real usage these would be different IDs from the slotmap

        registry.register("same-id", node1);
        // In a real scenario with different node IDs, the second registration
        // would overwrite the first
        assert_eq!(registry.get("same-id"), Some(node1));
    }
}
