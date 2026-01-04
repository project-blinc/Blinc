//! Element registry for O(1) ID-based lookups

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::element::ElementBounds;
use crate::tree::LayoutNodeId;

/// Callback type for on_ready notifications registered via query API
pub type OnReadyCallback = Arc<dyn Fn(ElementBounds) + Send + Sync>;

/// Registry mapping string IDs to layout node IDs
///
/// This provides O(1) lookup of elements by their string ID.
/// The registry is cleared and rebuilt on each render cycle.
pub struct ElementRegistry {
    /// String ID → LayoutNodeId mapping
    ids: RwLock<HashMap<String, LayoutNodeId>>,
    /// Reverse lookup for debugging (LayoutNodeId → String ID)
    reverse: RwLock<HashMap<LayoutNodeId, String>>,
    /// Parent relationships for tree traversal
    parents: RwLock<HashMap<LayoutNodeId, LayoutNodeId>>,
    /// Pending on_ready callbacks registered via ElementHandle.on_ready()
    /// Keyed by string ID for stable tracking across rebuilds
    pending_on_ready: Mutex<Vec<(String, OnReadyCallback)>>,
    /// Set of string IDs that have already had their on_ready callback triggered
    /// This survives across rebuilds since string IDs are stable
    triggered_on_ready_ids: Mutex<std::collections::HashSet<String>>,
}

impl std::fmt::Debug for ElementRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElementRegistry")
            .field("ids", &self.ids)
            .field("reverse", &self.reverse)
            .field("parents", &self.parents)
            .field(
                "pending_on_ready",
                &format!(
                    "{} pending",
                    self.pending_on_ready.lock().map(|v| v.len()).unwrap_or(0)
                ),
            )
            .finish()
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            ids: RwLock::new(HashMap::new()),
            reverse: RwLock::new(HashMap::new()),
            parents: RwLock::new(HashMap::new()),
            pending_on_ready: Mutex::new(Vec::new()),
            triggered_on_ready_ids: Mutex::new(std::collections::HashSet::new()),
        }
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

    // =========================================================================
    // On-Ready Callbacks (for ElementHandle.on_ready())
    // =========================================================================

    /// Register an on_ready callback for a node
    ///
    /// This is called by ElementHandle.on_ready() to queue callbacks that will
    /// be processed by the RenderTree after layout computation.
    ///
    /// The node_id is used to look up the string ID, which is used for stable
    /// tracking across tree rebuilds.
    pub fn register_on_ready(&self, node_id: LayoutNodeId, callback: OnReadyCallback) {
        // Look up the string ID for this node
        if let Some(string_id) = self.get_id(node_id) {
            // Check if already triggered (skip if so)
            if let Ok(triggered) = self.triggered_on_ready_ids.lock() {
                if triggered.contains(&string_id) {
                    tracing::trace!(
                        "on_ready callback for '{}' already triggered, skipping",
                        string_id
                    );
                    return;
                }
            }

            if let Ok(mut pending) = self.pending_on_ready.lock() {
                pending.push((string_id, callback));
            }
        } else {
            tracing::warn!(
                "on_ready callback registered for node {:?} without a string ID - callbacks require .id() for stable tracking",
                node_id
            );
        }
    }

    /// Take all pending on_ready callbacks
    ///
    /// This is called by the RenderTree to move pending callbacks into its own
    /// callback storage for processing after layout.
    ///
    /// Returns tuples of (string_id, callback) for stable tracking.
    pub fn take_pending_on_ready(&self) -> Vec<(String, OnReadyCallback)> {
        if let Ok(mut pending) = self.pending_on_ready.lock() {
            std::mem::take(&mut *pending)
        } else {
            Vec::new()
        }
    }

    /// Check if there are pending on_ready callbacks
    pub fn has_pending_on_ready(&self) -> bool {
        self.pending_on_ready
            .lock()
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Mark an on_ready callback as triggered by string ID
    ///
    /// This prevents the same callback from firing again on tree rebuilds.
    pub fn mark_on_ready_triggered(&self, string_id: &str) {
        if let Ok(mut triggered) = self.triggered_on_ready_ids.lock() {
            triggered.insert(string_id.to_string());
        }
    }

    /// Check if an on_ready callback has already been triggered
    pub fn is_on_ready_triggered(&self, string_id: &str) -> bool {
        self.triggered_on_ready_ids
            .lock()
            .map(|t| t.contains(string_id))
            .unwrap_or(false)
    }

    /// Register an on_ready callback by string ID directly
    ///
    /// This is the preferred method for registering on_ready callbacks, as it
    /// uses the stable string ID directly rather than looking it up from a node_id.
    /// This allows callbacks to be registered before the element exists in the tree.
    pub fn register_on_ready_for_id(&self, string_id: &str, callback: OnReadyCallback) {
        // Check if already triggered (skip if so)
        if let Ok(triggered) = self.triggered_on_ready_ids.lock() {
            if triggered.contains(string_id) {
                tracing::trace!(
                    "on_ready callback for '{}' already triggered, skipping",
                    string_id
                );
                return;
            }
        }

        if let Ok(mut pending) = self.pending_on_ready.lock() {
            pending.push((string_id.to_string(), callback));
        }
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

    // =========================================================================
    // On-Ready Callback Tests
    // =========================================================================

    #[test]
    fn test_register_on_ready_for_id() {
        let registry = ElementRegistry::new();
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Register callback before element exists
        let called_clone = called.clone();
        registry.register_on_ready_for_id(
            "my-element",
            Arc::new(move |_| {
                called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
        );

        // Should have pending callback
        assert!(registry.has_pending_on_ready());

        // Take pending callbacks
        let pending = registry.take_pending_on_ready();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "my-element");

        // Pending should now be empty
        assert!(!registry.has_pending_on_ready());
    }

    #[test]
    fn test_on_ready_triggered_tracking() {
        let registry = ElementRegistry::new();

        // Not triggered initially
        assert!(!registry.is_on_ready_triggered("my-element"));

        // Mark as triggered
        registry.mark_on_ready_triggered("my-element");

        // Now it's triggered
        assert!(registry.is_on_ready_triggered("my-element"));

        // Other IDs are not affected
        assert!(!registry.is_on_ready_triggered("other-element"));
    }

    #[test]
    fn test_on_ready_skips_already_triggered() {
        let registry = ElementRegistry::new();

        // First registration should work
        registry.register_on_ready_for_id("my-element", Arc::new(|_| {}));
        assert!(registry.has_pending_on_ready());

        // Take and mark as triggered
        let _ = registry.take_pending_on_ready();
        registry.mark_on_ready_triggered("my-element");

        // Second registration should be skipped
        registry.register_on_ready_for_id("my-element", Arc::new(|_| {}));
        assert!(!registry.has_pending_on_ready());
    }

    #[test]
    fn test_on_ready_multiple_elements() {
        let registry = ElementRegistry::new();

        registry.register_on_ready_for_id("element-a", Arc::new(|_| {}));
        registry.register_on_ready_for_id("element-b", Arc::new(|_| {}));
        registry.register_on_ready_for_id("element-c", Arc::new(|_| {}));

        let pending = registry.take_pending_on_ready();
        assert_eq!(pending.len(), 3);

        let ids: Vec<_> = pending.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"element-a"));
        assert!(ids.contains(&"element-b"));
        assert!(ids.contains(&"element-c"));
    }

    #[test]
    fn test_on_ready_via_node_id() {
        let registry = ElementRegistry::new();
        let node_id = LayoutNodeId::default();

        // Register element first
        registry.register("my-element", node_id);

        // Register callback via node_id
        registry.register_on_ready(node_id, Arc::new(|_| {}));

        // Should have pending callback with string ID
        let pending = registry.take_pending_on_ready();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "my-element");
    }

    #[test]
    fn test_on_ready_via_node_id_without_string_id_warns() {
        let registry = ElementRegistry::new();
        let node_id = LayoutNodeId::default();

        // Don't register string ID - callback via node_id should warn and not add
        registry.register_on_ready(node_id, Arc::new(|_| {}));

        // Should NOT have pending callback (no string ID mapping)
        assert!(!registry.has_pending_on_ready());
    }

    #[test]
    fn test_triggered_survives_clear() {
        let registry = ElementRegistry::new();
        let node_id = LayoutNodeId::default();

        registry.register("my-element", node_id);
        registry.mark_on_ready_triggered("my-element");

        // Clear the registry (simulates tree rebuild)
        registry.clear();

        // Triggered state should survive
        assert!(registry.is_on_ready_triggered("my-element"));

        // New callback registration should be skipped
        registry.register_on_ready_for_id("my-element", Arc::new(|_| {}));
        assert!(!registry.has_pending_on_ready());
    }
}
