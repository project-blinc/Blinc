//! Click-outside detection system
//!
//! Components (like select dropdowns) can register to be notified when a click
//! occurs outside their subtree. The event router calls `fire_click_outside()`
//! on every mouse down, passing the hit target's ancestor element IDs.

#![allow(clippy::incompatible_msrv)]

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

type DismissCallback = Arc<dyn Fn() + Send + Sync>;

struct ClickOutsideEntry {
    /// Element IDs that define "inside". If ANY of these IDs appears in the
    /// hit target's ancestor element IDs, the click is considered inside.
    /// Multi-id support lets a parent overlay treat its own subtree *and*
    /// any open child overlays (e.g. submenus) as inside, so a click in the
    /// submenu doesn't dismiss the parent.
    element_ids: Vec<String>,
    /// Called when a click occurs outside the element's subtree.
    on_dismiss: DismissCallback,
}

static REGISTRY: LazyLock<Mutex<HashMap<String, ClickOutsideEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a click-outside handler.
///
/// - `key`: Unique key for this registration (used to unregister).
/// - `element_id`: The element ID in the DOM tree. Clicks whose ancestor chain
///   does NOT include this ID trigger the dismiss callback.
/// - `on_dismiss`: Callback to invoke on click-outside.
pub fn register_click_outside(
    key: &str,
    element_id: &str,
    on_dismiss: impl Fn() + Send + Sync + 'static,
) {
    register_click_outside_multi(key, &[element_id.to_string()], on_dismiss);
}

/// Like `register_click_outside` but accepts multiple element IDs. A click
/// is "inside" if any of the IDs appears in the hit target's ancestor chain.
/// Used by overlays that own descendant overlays (e.g. context_menu with
/// open submenus).
pub fn register_click_outside_multi(
    key: &str,
    element_ids: &[String],
    on_dismiss: impl Fn() + Send + Sync + 'static,
) {
    if let Ok(mut reg) = REGISTRY.lock() {
        reg.insert(
            key.to_string(),
            ClickOutsideEntry {
                element_ids: element_ids.to_vec(),
                on_dismiss: Arc::new(on_dismiss),
            },
        );
    }
}

/// Update the "inside" element IDs for an already-registered handler without
/// touching its callback. Lets a parent overlay re-arm its inside-set when a
/// child overlay opens / closes.
pub fn update_click_outside_ids(key: &str, element_ids: &[String]) {
    if let Ok(mut reg) = REGISTRY.lock()
        && let Some(entry) = reg.get_mut(key)
    {
        entry.element_ids = element_ids.to_vec();
    }
}

/// Unregister a click-outside handler.
pub fn unregister_click_outside(key: &str) {
    if let Ok(mut reg) = REGISTRY.lock() {
        reg.remove(key);
    }
}

/// Fire click-outside callbacks for a click event.
///
/// Called by the event router on every mouse down.
/// `ancestor_element_ids` contains the element IDs of nodes in the hit target's ancestor chain.
/// If empty (click on empty space), all handlers fire.
pub fn fire_click_outside(ancestor_element_ids: &[String]) {
    let callbacks: Vec<(String, DismissCallback)> = {
        let Ok(reg) = REGISTRY.lock() else {
            return;
        };
        if reg.is_empty() {
            return;
        }
        reg.iter()
            .filter(|(_, entry)| {
                !entry
                    .element_ids
                    .iter()
                    .any(|id| ancestor_element_ids.contains(id))
            })
            .map(|(key, entry)| (key.clone(), Arc::clone(&entry.on_dismiss)))
            .collect()
    };

    for (_, cb) in callbacks {
        cb();
    }
}

/// Clear all registrations.
pub fn clear_click_outside_handlers() {
    if let Ok(mut reg) = REGISTRY.lock() {
        reg.clear();
    }
}
