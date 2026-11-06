//! Window state persistence
//!
//! Save and restore window position, size, and maximized state across launches.
//! State is stored as JSON in the platform's app data directory.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::window_state::WindowStateStore;
//!
//! let store = WindowStateStore::new("my_app");
//!
//! // Load saved state and apply to config
//! let mut config = WindowConfig::default();
//! if let Some(saved) = store.load("main") {
//!     config = saved.apply_to(config);
//! }
//!
//! // After window is created, save state on close
//! store.save("main", &SavedWindowState {
//!     x: 100, y: 200, width: 800, height: 600, maximized: false,
//! });
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use blinc_platform::WindowConfig;

/// Persisted window state
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SavedWindowState {
    /// Window X position (logical pixels)
    pub x: i32,
    /// Window Y position (logical pixels)
    pub y: i32,
    /// Window width (logical pixels)
    pub width: u32,
    /// Window height (logical pixels)
    pub height: u32,
    /// Whether the window was maximized
    pub maximized: bool,
}

impl SavedWindowState {
    /// Apply saved state to a WindowConfig, preserving other config fields
    pub fn apply_to(&self, mut config: WindowConfig) -> WindowConfig {
        config.width = self.width;
        config.height = self.height;
        config.position = Some((self.x, self.y));
        config
    }
}

/// Store for persisting window state across app launches
pub struct WindowStateStore {
    path: PathBuf,
}

impl WindowStateStore {
    /// Create a new store for the given app name.
    ///
    /// State is stored in `~/.config/<app_name>/window_state.json` on Linux,
    /// `~/Library/Application Support/<app_name>/window_state.json` on macOS,
    /// `%APPDATA%/<app_name>/window_state.json` on Windows.
    pub fn new(app_name: &str) -> Self {
        let dir = dirs_path(app_name);
        Self {
            path: dir.join("window_state.json"),
        }
    }

    /// Create a store with a custom file path
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Load saved state for a named window. Returns None if no state exists.
    pub fn load(&self, window_name: &str) -> Option<SavedWindowState> {
        let content = std::fs::read_to_string(&self.path).ok()?;
        let map: HashMap<String, SavedWindowState> = serde_json::from_str(&content).ok()?;
        map.get(window_name).cloned()
    }

    /// Save state for a named window. Creates the directory if needed.
    pub fn save(&self, window_name: &str, state: &SavedWindowState) -> bool {
        // Load existing state map or create new
        let mut map: HashMap<String, SavedWindowState> = self
            .path
            .exists()
            .then(|| {
                std::fs::read_to_string(&self.path)
                    .ok()
                    .and_then(|c| serde_json::from_str(&c).ok())
            })
            .flatten()
            .unwrap_or_default();

        map.insert(window_name.to_string(), state.clone());

        // Ensure directory exists
        if let Some(parent) = self.path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!("Failed to create window state directory: {}", e);
            return false;
        }

        match serde_json::to_string_pretty(&map) {
            Ok(json) => std::fs::write(&self.path, json).is_ok(),
            Err(_) => false,
        }
    }

    /// Remove saved state for a window
    pub fn remove(&self, window_name: &str) -> bool {
        if let Ok(content) = std::fs::read_to_string(&self.path)
            && let Ok(mut map) = serde_json::from_str::<HashMap<String, SavedWindowState>>(&content)
        {
            map.remove(window_name);
            if let Ok(json) = serde_json::to_string_pretty(&map) {
                return std::fs::write(&self.path, json).is_ok();
            }
        }
        false
    }
}

/// Get the platform-specific app data directory
fn dirs_path(app_name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(app_name)
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| "C:\\".to_string());
        PathBuf::from(appdata).join(app_name)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".config").join(app_name)
    }
}
