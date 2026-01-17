//! Fuchsia asset loading
//!
//! Loads assets from Fuchsia packages using the component's namespace.

use blinc_platform::assets::{AssetLoader, AssetPath};
use blinc_platform::{PlatformError, Result};

/// Fuchsia asset loader
///
/// Loads assets from the component's package data directory.
pub struct FuchsiaAssetLoader {
    /// Base path for assets (typically /pkg/data)
    base_path: String,
}

impl FuchsiaAssetLoader {
    /// Create a new Fuchsia asset loader
    pub fn new() -> Self {
        Self {
            base_path: "/pkg/data".to_string(),
        }
    }

    /// Create with a custom base path
    pub fn with_base_path(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn resolve_path(&self, path: &AssetPath) -> String {
        match path {
            AssetPath::Relative(rel) => format!("{}/{}", self.base_path, rel),
            AssetPath::Absolute(abs) => abs.clone(),
            AssetPath::Embedded(name) => format!("{}/{}", self.base_path, name),
        }
    }
}

impl Default for FuchsiaAssetLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetLoader for FuchsiaAssetLoader {
    fn load(&self, path: &AssetPath) -> Result<Vec<u8>> {
        let full_path = self.resolve_path(path);

        // On Fuchsia, use std::fs to read from package namespace
        #[cfg(target_os = "fuchsia")]
        {
            return std::fs::read(&full_path).map_err(|e| {
                PlatformError::AssetLoad(format!("Failed to load {}: {}", full_path, e))
            });
        }

        // On other platforms, assets aren't available
        #[cfg(not(target_os = "fuchsia"))]
        {
            Err(PlatformError::AssetLoad(format!(
                "Fuchsia assets only available on Fuchsia OS: {}",
                full_path
            )))
        }
    }

    fn exists(&self, path: &AssetPath) -> bool {
        let full_path = self.resolve_path(path);

        // On Fuchsia, check filesystem
        #[cfg(target_os = "fuchsia")]
        {
            return std::path::Path::new(&full_path).exists();
        }

        // On other platforms, always false
        #[cfg(not(target_os = "fuchsia"))]
        {
            let _ = full_path;
            false
        }
    }

    fn platform_name(&self) -> &'static str {
        "fuchsia"
    }
}
