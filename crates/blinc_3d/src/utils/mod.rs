//! Game utilities for blinc_3d
//!
//! This module provides common game development utilities:
//!
//! - **Camera Controls**: Orbit, fly, follow, drone, and shake controllers
//! - **Lighting Presets**: Pre-configured lighting setups for various scenarios
//! - **Skybox System**: Procedural, cubemap, and gradient skyboxes with time-of-day
//! - **Mesh Loader**: glTF, OBJ file loading with caching
//! - **Particles**: GPU-instanced particle systems (future)
//! - **Weather**: Fog, rain, snow, clouds (future)
//! - **Terrain**: Procedural terrain generation (future)
//! - **Physics**: Rigid body physics integration (future)
//!
//! # Feature Flags
//!
//! Most utilities are feature-gated to keep the core crate lightweight:
//!
//! - `utils-camera` - Camera controllers
//! - `utils-lighting` - Lighting presets
//! - `utils-skybox` - Skybox system
//! - `utils-gltf` - glTF mesh loading
//! - `utils-obj` - OBJ mesh loading
//! - `utils-loaders` - All mesh loaders
//! - `utils-foundation` - Camera + Lighting + Skybox bundle

#[cfg(feature = "utils-camera")]
pub mod camera;

#[cfg(feature = "utils-lighting")]
pub mod lighting;

#[cfg(feature = "utils-skybox")]
pub mod skybox;

#[cfg(any(feature = "utils-gltf", feature = "utils-obj"))]
pub mod loader;

// Re-exports for convenience
#[cfg(feature = "utils-camera")]
pub use camera::*;

#[cfg(feature = "utils-lighting")]
pub use lighting::*;

#[cfg(feature = "utils-skybox")]
pub use skybox::*;

#[cfg(any(feature = "utils-gltf", feature = "utils-obj"))]
pub use loader::*;
