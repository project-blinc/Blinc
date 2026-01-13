//! iOS bundle asset loading
//!
//! Loads assets from the iOS app bundle.

use blinc_platform::assets::{AssetLoader, AssetPath};
use blinc_platform::{PlatformError, Result};

#[cfg(target_os = "ios")]
use objc2_foundation::NSBundle;

#[cfg(target_os = "ios")]
use std::path::PathBuf;

#[cfg(target_os = "ios")]
use tracing::{debug, warn};

/// iOS bundle asset loader
///
/// Loads assets from the main bundle's resource directory.
#[cfg(target_os = "ios")]
pub struct IOSAssetLoader {
    /// Bundle resource path
    resource_path: Option<PathBuf>,
}

#[cfg(target_os = "ios")]
impl IOSAssetLoader {
    /// Create a new iOS asset loader
    pub fn new() -> Self {
        // Get the main bundle's resource path
        let resource_path = unsafe {
            let bundle = NSBundle::mainBundle();
            bundle.resourcePath().map(|ns_path| {
                let path_str = ns_path.to_string();
                PathBuf::from(path_str)
            })
        };

        if let Some(ref path) = resource_path {
            debug!("iOS asset loader initialized with path: {:?}", path);
        } else {
            warn!("Failed to get main bundle resource path");
        }

        Self { resource_path }
    }

    /// Get the full path for an asset
    fn resolve_path(&self, path: &AssetPath) -> Option<PathBuf> {
        let path_str = match path {
            AssetPath::Relative(s) => s.as_str(),
            AssetPath::Absolute(s) => return Some(PathBuf::from(s)),
            AssetPath::Embedded(s) => *s,
        };

        self.resource_path.as_ref().map(|base| base.join(path_str))
    }
}

#[cfg(target_os = "ios")]
impl Default for IOSAssetLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "ios")]
impl AssetLoader for IOSAssetLoader {
    fn load(&self, path: &AssetPath) -> Result<Vec<u8>> {
        let full_path = self
            .resolve_path(path)
            .ok_or_else(|| PlatformError::AssetLoad("No resource path available".to_string()))?;

        debug!("Loading iOS asset: {:?}", full_path);

        std::fs::read(&full_path).map_err(|e| {
            PlatformError::AssetLoad(format!("Failed to load '{}': {}", full_path.display(), e))
        })
    }

    fn exists(&self, path: &AssetPath) -> bool {
        self.resolve_path(path).map(|p| p.exists()).unwrap_or(false)
    }

    fn platform_name(&self) -> &'static str {
        "ios"
    }
}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub struct IOSAssetLoader;

#[cfg(not(target_os = "ios"))]
impl IOSAssetLoader {
    /// Create a placeholder asset loader
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(target_os = "ios"))]
impl Default for IOSAssetLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_os = "ios"))]
impl AssetLoader for IOSAssetLoader {
    fn load(&self, _path: &AssetPath) -> Result<Vec<u8>> {
        Err(PlatformError::Unsupported(
            "iOS asset loader not available on this platform".to_string(),
        ))
    }

    fn exists(&self, _path: &AssetPath) -> bool {
        false
    }

    fn platform_name(&self) -> &'static str {
        "ios-stub"
    }
}
