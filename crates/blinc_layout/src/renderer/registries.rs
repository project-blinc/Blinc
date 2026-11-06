//! Layout-bounds-storage registration on `RenderTree`.
//!
//! Elements that need to react to their computed bounds (e.g.
//! `TextInput` rescaling its scroll offset when the field width
//! changes) register a `LayoutBoundsStorage` here. After every
//! `compute_layout` call, `update_layout_bounds_storages` populates
//! each registered storage and fires the optional `on_change`
//! callback when the new bounds differ from the cached ones.
//!
//! Other registries (handler, element, dirty-tracker, animation
//! scheduler, css-anim store) live as one-line accessors on
//! `RenderTree` itself in `mod.rs` — only the bounds-storage
//! subsystem is large enough to warrant its own module.

use crate::div::ElementBuilder;
use crate::tree::LayoutNodeId;

use super::{LayoutBoundsCallback, LayoutBoundsEntry, LayoutBoundsStorage, RenderTree};

impl RenderTree {
    /// Register a layout bounds storage for a node
    ///
    /// After layout is computed, the storage will be updated with the node's
    /// computed bounds. This allows elements to react to layout changes.
    pub fn register_layout_bounds_storage(
        &mut self,
        node_id: LayoutNodeId,
        storage: LayoutBoundsStorage,
    ) {
        self.layout_bounds_storages.insert(
            node_id,
            LayoutBoundsEntry {
                storage,
                on_change: None,
            },
        );
    }

    /// Register a layout bounds storage with a change callback
    ///
    /// The callback is invoked when the computed bounds change (width or height differ).
    /// This is useful for elements that need to react to layout changes, like TextInput
    /// which needs to recalculate scroll offset when its width changes.
    pub fn register_layout_bounds_storage_with_callback(
        &mut self,
        node_id: LayoutNodeId,
        storage: LayoutBoundsStorage,
        on_change: LayoutBoundsCallback,
    ) {
        self.layout_bounds_storages.insert(
            node_id,
            LayoutBoundsEntry {
                storage,
                on_change: Some(on_change),
            },
        );
    }

    /// Unregister a layout bounds storage
    pub fn unregister_layout_bounds_storage(&mut self, node_id: LayoutNodeId) {
        self.layout_bounds_storages.remove(&node_id);
    }

    /// Register layout bounds storage from an element builder
    ///
    /// This helper checks both layout_bounds_storage() and layout_bounds_callback()
    /// from the ElementBuilder trait and registers them together.
    pub(super) fn register_element_bounds_storage(
        &mut self,
        node_id: LayoutNodeId,
        element: &dyn ElementBuilder,
    ) {
        if let Some(storage) = element.layout_bounds_storage() {
            let callback = element.layout_bounds_callback();
            self.layout_bounds_storages.insert(
                node_id,
                LayoutBoundsEntry {
                    storage,
                    on_change: callback,
                },
            );
        }
    }

    /// Update all registered layout bounds storages after layout computation
    ///
    /// When bounds change (width or height differ), the on_change callback is invoked.
    pub(super) fn update_layout_bounds_storages(&self) {
        for (&node_id, entry) in &self.layout_bounds_storages {
            if let Some(bounds) = self.layout_tree.get_bounds(node_id, (0.0, 0.0)) {
                let should_notify = if let Ok(mut guard) = entry.storage.lock() {
                    // Check if bounds changed (compare width and height)
                    let changed = match guard.as_ref() {
                        Some(old_bounds) => {
                            (old_bounds.width - bounds.width).abs() > 0.01
                                || (old_bounds.height - bounds.height).abs() > 0.01
                        }
                        None => true, // First time getting bounds
                    };
                    *guard = Some(bounds);
                    changed
                } else {
                    false
                };

                // Invoke callback if bounds changed and callback exists
                if should_notify && let Some(ref callback) = entry.on_change {
                    callback(bounds);
                }
            }
        }
    }

    /// Clear all layout bounds storages
    ///
    /// This should be called on window resize to ensure that cached bounds
    /// don't influence the new layout computation. Each element will get
    /// fresh bounds on the next `compute_layout` call.
    pub fn clear_layout_bounds_storages(&self) {
        for entry in self.layout_bounds_storages.values() {
            if let Ok(mut guard) = entry.storage.lock() {
                *guard = None;
            }
        }
    }
}
