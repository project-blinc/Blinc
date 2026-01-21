//! Mesh loader system
//!
//! Provides unified API for loading 3D models from various formats:
//!
//! - glTF 2.0 (`.gltf`, `.glb`) - Feature: `utils-gltf`
//! - Wavefront OBJ (`.obj`) - Feature: `utils-obj`
//!
//! # Example
//!
//! ```ignore
//! use blinc_3d::utils::loader::*;
//!
//! let mut loader = MeshLoaderRegistry::new();
//!
//! // Load a glTF file
//! let scene = loader.load("models/character.glb")?;
//!
//! // Spawn all meshes from the loaded scene
//! for mesh in &scene.meshes {
//!     let entity = world.spawn()
//!         .insert(Object3D::from_transform(&mesh.transform))
//!         .insert(Mesh::from_loaded(mesh, &mut resources))
//!         .id();
//! }
//! ```

mod scene;
mod cache;

#[cfg(feature = "utils-gltf")]
mod gltf;

#[cfg(feature = "utils-obj")]
mod obj;

pub use scene::*;
pub use cache::*;

#[cfg(feature = "utils-gltf")]
pub use self::gltf::GltfLoader;

#[cfg(feature = "utils-obj")]
pub use self::obj::ObjLoader;

use std::path::Path;

/// Error type for mesh loading operations
#[derive(Debug)]
pub enum LoadError {
    /// File not found
    NotFound(String),
    /// IO error
    Io(std::io::Error),
    /// Parse error
    Parse(String),
    /// Unsupported format
    UnsupportedFormat(String),
    /// Invalid data
    InvalidData(String),
    /// Missing dependency
    MissingDependency(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound(path) => write!(f, "File not found: {}", path),
            LoadError::Io(err) => write!(f, "IO error: {}", err),
            LoadError::Parse(msg) => write!(f, "Parse error: {}", msg),
            LoadError::UnsupportedFormat(ext) => write!(f, "Unsupported format: {}", ext),
            LoadError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
            LoadError::MissingDependency(msg) => write!(f, "Missing dependency: {}", msg),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            LoadError::NotFound(err.to_string())
        } else {
            LoadError::Io(err)
        }
    }
}

/// Trait for mesh loaders
pub trait MeshLoader: Send + Sync {
    /// Load a scene from a file path
    fn load(&self, path: &Path) -> Result<LoadedScene, LoadError>;

    /// Get supported file extensions
    fn supported_extensions(&self) -> &[&str];

    /// Check if this loader can handle the given extension
    fn can_load(&self, extension: &str) -> bool {
        let ext_lower = extension.to_lowercase();
        self.supported_extensions()
            .iter()
            .any(|e| e.to_lowercase() == ext_lower)
    }

    /// Get the loader name for debugging
    fn name(&self) -> &'static str;
}

/// Registry of available mesh loaders
pub struct MeshLoaderRegistry {
    loaders: Vec<Box<dyn MeshLoader>>,
    cache: MeshCache,
}

impl MeshLoaderRegistry {
    /// Create a new loader registry with all available loaders
    pub fn new() -> Self {
        let mut loaders: Vec<Box<dyn MeshLoader>> = Vec::new();

        #[cfg(feature = "utils-gltf")]
        loaders.push(Box::new(GltfLoader::new()));

        #[cfg(feature = "utils-obj")]
        loaders.push(Box::new(ObjLoader::new()));

        Self {
            loaders,
            cache: MeshCache::new(100), // Default 100 entries
        }
    }

    /// Create with a specific cache size
    pub fn with_cache_size(mut self, max_entries: usize) -> Self {
        self.cache = MeshCache::new(max_entries);
        self
    }

    /// Register a custom loader
    pub fn register(&mut self, loader: Box<dyn MeshLoader>) {
        self.loaders.push(loader);
    }

    /// Load a mesh file
    pub fn load(&mut self, path: impl AsRef<Path>) -> Result<LoadedScene, LoadError> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        // Check cache first
        if let Some(cached) = self.cache.get(&path_str) {
            return Ok(cached.clone());
        }

        // Find appropriate loader
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let loader = self
            .loaders
            .iter()
            .find(|l| l.can_load(extension))
            .ok_or_else(|| LoadError::UnsupportedFormat(extension.to_string()))?;

        // Load the scene
        let scene = loader.load(path)?;

        // Cache the result
        self.cache.insert(path_str, scene.clone());

        Ok(scene)
    }

    /// Load without caching
    pub fn load_uncached(&self, path: impl AsRef<Path>) -> Result<LoadedScene, LoadError> {
        let path = path.as_ref();

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let loader = self
            .loaders
            .iter()
            .find(|l| l.can_load(extension))
            .ok_or_else(|| LoadError::UnsupportedFormat(extension.to_string()))?;

        loader.load(path)
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Check if a format is supported
    pub fn supports_format(&self, extension: &str) -> bool {
        self.loaders.iter().any(|l| l.can_load(extension))
    }

    /// Get all supported extensions
    pub fn supported_extensions(&self) -> Vec<&str> {
        self.loaders
            .iter()
            .flat_map(|l| l.supported_extensions().iter().copied())
            .collect()
    }
}

impl Default for MeshLoaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to load a mesh file
pub fn load_mesh(path: impl AsRef<Path>) -> Result<LoadedScene, LoadError> {
    MeshLoaderRegistry::new().load_uncached(path)
}
