//! Stable unique key generation for component instances.
//!
//! Solves the problem of key collisions when components are created in loops or closures,
//! where `#[track_caller]` alone would give the same source location key to all instances.
//!
//! # Example
//!
//! ```ignore
//! // In a loop - each iteration gets a unique key
//! for item in items {
//!     let dropdown = cn::dropdown_menu(&item.name);
//!     // dropdown.key is unique per iteration due to UUID
//! }
//!
//! // Explicit key for deterministic behavior
//! let btn = ButtonBuilder::with_key("settings-save", "Save");
//! ```

use std::cell::OnceCell;
use uuid::Uuid;

/// Generates a stable unique key for component instances.
///
/// Key format: `{prefix}:{file}:{line}:{col}:{uuid}`
/// - prefix: Component type (e.g., "dropdown", "button", "motion")
/// - file:line:col: Source location for debugging
/// - uuid: Unique identifier per instance
///
/// The key is lazily generated on first access and cached for the builder's lifetime.
pub struct InstanceKey {
    key: OnceCell<String>,
    prefix: &'static str,
    file: &'static str,
    line: u32,
    column: u32,
}

impl InstanceKey {
    /// Create from track_caller location with auto-generated UUID.
    ///
    /// Each call creates a new instance that will generate a unique key
    /// when `get()` is first called.
    #[track_caller]
    pub fn new(prefix: &'static str) -> Self {
        let loc = std::panic::Location::caller();
        Self {
            key: OnceCell::new(),
            prefix,
            file: loc.file(),
            line: loc.line(),
            column: loc.column(),
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
        };
        // Pre-populate the key
        let _ = instance.key.set(key.into());
        instance
    }

    /// Get or generate the unique key.
    ///
    /// On first call, generates a key with format:
    /// `{prefix}:{file}:{line}:{col}:{uuid}`
    ///
    /// Subsequent calls return the cached key.
    pub fn get(&self) -> &str {
        self.key.get_or_init(|| {
            format!(
                "{}:{}:{}:{}:{}",
                self.prefix,
                self.file,
                self.line,
                self.column,
                Uuid::new_v4().as_simple()
            )
        })
    }

    /// Create a derived key for sub-components.
    ///
    /// Useful for creating hierarchical keys for internal state:
    /// ```ignore
    /// let key = InstanceKey::new("dropdown");
    /// let open_key = key.derive("open");      // "dropdown:...:uuid_open"
    /// let handle_key = key.derive("handle");  // "dropdown:...:uuid_handle"
    /// ```
    pub fn derive(&self, suffix: &str) -> String {
        format!("{}_{}", self.get(), suffix)
    }

    /// Get the source location info for debugging.
    pub fn location(&self) -> (&'static str, u32, u32) {
        (self.file, self.line, self.column)
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
        let mut keys = Vec::new();
        for _ in 0..5 {
            let key = InstanceKey::new("test");
            keys.push(key.get().to_string());
        }
        // All keys should be unique
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(unique.len(), 5);
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
        let key = InstanceKey::new("test");
        let first = key.get().to_string();
        let second = key.get().to_string();
        assert_eq!(first, second);
    }

    #[test]
    fn test_clone_preserves_key() {
        let key = InstanceKey::new("test");
        let original = key.get().to_string();
        let cloned = key.clone();
        assert_eq!(cloned.get(), original);
    }
}
