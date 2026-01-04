//! Stable unique key generation for component instances.
//!
//! Uses source location + call counter to generate deterministic keys that are:
//! - **Unique within a frame**: Multiple calls at same source location get different indices
//! - **Stable across rebuilds**: Same call order produces same keys
//!
//! This is similar to how React uses array indices for list items, or how browsers
//! identify DOM elements by their tree position.
//!
//! # Example
//!
//! ```ignore
//! // In a loop - each iteration gets a unique key based on call order
//! for item in items {
//!     let dropdown = cn::dropdown_menu(&item.name);
//!     // First iteration: "dropdown:file.rs:10:5:0"
//!     // Second iteration: "dropdown:file.rs:10:5:1"
//!     // etc.
//! }
//!
//! // On rebuild, same call order = same keys
//! ```

use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Global call counters per source location, reset at the start of each frame.
static CALL_COUNTERS: LazyLock<Mutex<HashMap<(&'static str, u32, u32), usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Reset all call counters. Call this at the start of each UI build pass.
pub fn reset_call_counters() {
    CALL_COUNTERS.lock().unwrap().clear();
}

/// Get and increment the call counter for a source location.
fn next_call_index(file: &'static str, line: u32, column: u32) -> usize {
    let mut counters = CALL_COUNTERS.lock().unwrap();
    let key = (file, line, column);
    let index = counters.get(&key).copied().unwrap_or(0);
    counters.insert(key, index + 1);
    index
}

/// Generates a stable unique key for component instances.
///
/// Key format: `{prefix}:{file}:{line}:{col}:{index}`
/// - prefix: Component type (e.g., "dropdown", "button", "motion")
/// - file:line:col: Source location for debugging
/// - index: Call order index at this source location (0, 1, 2, ...)
///
/// The key is generated on creation and cached for the builder's lifetime.
/// Keys are deterministic based on call order within each frame.
pub struct InstanceKey {
    key: OnceCell<String>,
    prefix: &'static str,
    file: &'static str,
    line: u32,
    column: u32,
    /// Call index at this source location (assigned at creation time)
    index: usize,
}

impl InstanceKey {
    /// Create from track_caller location with auto-assigned call index.
    ///
    /// Each call at the same source location gets a unique index (0, 1, 2, ...).
    /// Indices reset at the start of each frame via `reset_call_counters()`.
    #[track_caller]
    pub fn new(prefix: &'static str) -> Self {
        let loc = std::panic::Location::caller();
        let index = next_call_index(loc.file(), loc.line(), loc.column());
        Self {
            key: OnceCell::new(),
            prefix,
            file: loc.file(),
            line: loc.line(),
            column: loc.column(),
            index,
        }
    }

    /// Create with explicit user-provided key (for deterministic keys).
    ///
    /// Use this when you need a stable, predictable key that doesn't change
    /// between rebuilds (e.g., for testing or programmatic element access).
    pub fn explicit(key: impl Into<String>) -> Self {
        let instance = Self {
            key: OnceCell::new(),
            prefix: "",
            file: "",
            line: 0,
            column: 0,
            index: 0,
        };
        // Pre-populate the key
        let _ = instance.key.set(key.into());
        instance
    }

    /// Get or generate the unique key.
    ///
    /// Returns a key with format: `{prefix}:{file}:{line}:{col}:{index}`
    pub fn get(&self) -> &str {
        self.key.get_or_init(|| {
            format!(
                "{}:{}:{}:{}:{}",
                self.prefix, self.file, self.line, self.column, self.index
            )
        })
    }

    /// Create a derived key for sub-components.
    ///
    /// Useful for creating hierarchical keys for internal state:
    /// ```ignore
    /// let key = InstanceKey::new("dropdown");
    /// let open_key = key.derive("open");      // "dropdown:...:0_open"
    /// let handle_key = key.derive("handle");  // "dropdown:...:0_handle"
    /// ```
    pub fn derive(&self, suffix: &str) -> String {
        format!("{}_{}", self.get(), suffix)
    }

    /// Get the source location info for debugging.
    pub fn location(&self) -> (&'static str, u32, u32) {
        (self.file, self.line, self.column)
    }

    /// Get the call index at this source location.
    pub fn index(&self) -> usize {
        self.index
    }
}

impl std::fmt::Debug for InstanceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InstanceKey({})", self.get())
    }
}

impl Clone for InstanceKey {
    fn clone(&self) -> Self {
        // When cloning, we want to preserve the same key
        Self::explicit(self.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_keys_in_loop() {
        reset_call_counters();
        let mut keys = Vec::new();
        for _ in 0..5 {
            let key = InstanceKey::new("test");
            keys.push(key.get().to_string());
        }
        // All keys should be unique
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), 5);

        // Keys should have sequential indices
        assert!(keys[0].ends_with(":0"));
        assert!(keys[1].ends_with(":1"));
        assert!(keys[2].ends_with(":2"));
    }

    #[test]
    fn test_keys_stable_across_rebuilds() {
        // Helper function simulating a component that creates keys
        fn create_keys() -> (String, String) {
            let key1 = InstanceKey::new("test").get().to_string();
            let key2 = InstanceKey::new("test").get().to_string();
            (key1, key2)
        }

        // First "frame"
        reset_call_counters();
        let (key1_frame1, key2_frame1) = create_keys();

        // Second "frame" - same function = same source locations = same keys
        reset_call_counters();
        let (key1_frame2, key2_frame2) = create_keys();

        assert_eq!(key1_frame1, key1_frame2);
        assert_eq!(key2_frame1, key2_frame2);
        // Also verify they're different from each other
        assert_ne!(key1_frame1, key2_frame1);
    }

    #[test]
    fn test_explicit_key() {
        let key = InstanceKey::explicit("my-custom-key");
        assert_eq!(key.get(), "my-custom-key");
    }

    #[test]
    fn test_derive() {
        let key = InstanceKey::explicit("base");
        assert_eq!(key.derive("child"), "base_child");
        assert_eq!(key.derive("other"), "base_other");
    }

    #[test]
    fn test_key_stability() {
        reset_call_counters();
        let key = InstanceKey::new("test");
        let first = key.get().to_string();
        let second = key.get().to_string();
        assert_eq!(first, second);
    }

    #[test]
    fn test_clone_preserves_key() {
        reset_call_counters();
        let key = InstanceKey::new("test");
        let original = key.get().to_string();
        let cloned = key.clone();
        assert_eq!(cloned.get(), original);
    }

    #[test]
    fn test_different_source_locations_independent() {
        reset_call_counters();

        // Helper functions at different source locations
        fn create_dropdown() -> InstanceKey {
            InstanceKey::new("dropdown")
        }
        fn create_button() -> InstanceKey {
            InstanceKey::new("button")
        }

        let dropdown1 = create_dropdown();
        let button1 = create_button();
        let dropdown2 = create_dropdown();

        // Each source location has its own counter sequence
        // dropdown1 and dropdown2 come from same location (create_dropdown)
        assert!(dropdown1.get().contains("dropdown") && dropdown1.get().ends_with(":0"));
        assert!(button1.get().contains("button") && button1.get().ends_with(":0"));
        assert!(dropdown2.get().contains("dropdown") && dropdown2.get().ends_with(":1"));
    }
}
