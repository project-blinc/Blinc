//! Game utilities for blinc_3d
//!
//! This module provides common game development utilities:
//!
//! - **Camera Controls**: Orbit, fly, follow, drone, and shake controllers
//! - **Lighting Presets**: Pre-configured lighting setups for various scenarios
//! - **Skybox System**: Procedural, cubemap, and gradient skyboxes with time-of-day
//! - **Mesh Loader**: glTF, OBJ file loading with caching
//! - **Particles**: GPU-instanced particle systems
//! - **Physics**: Rigid body physics integration
//! - **Terrain**: Procedural terrain generation
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
//! - `utils-particles` - Particle systems
//! - `utils-rapier` - Rapier physics backend
//! - `utils-parry` - Parry collision backend
//! - `utils-terrain` - Procedural terrain
//! - `utils-foundation` - Camera + Lighting + Skybox bundle

#[cfg(feature = "utils-camera")]
pub mod camera;

#[cfg(feature = "utils-lighting")]
pub mod lighting;

#[cfg(feature = "utils-skybox")]
pub mod skybox;

#[cfg(any(feature = "utils-gltf", feature = "utils-obj"))]
pub mod loader;

#[cfg(feature = "utils-particles")]
pub mod particles;

#[cfg(any(feature = "utils-rapier", feature = "utils-parry"))]
pub mod physics;

#[cfg(feature = "utils-terrain")]
pub mod terrain;

// Re-exports for convenience
#[cfg(feature = "utils-camera")]
pub use camera::*;

#[cfg(feature = "utils-lighting")]
pub use lighting::*;

#[cfg(feature = "utils-skybox")]
pub use skybox::*;

#[cfg(any(feature = "utils-gltf", feature = "utils-obj"))]
pub use loader::*;

#[cfg(feature = "utils-particles")]
pub use particles::*;

#[cfg(any(feature = "utils-rapier", feature = "utils-parry"))]
pub use physics::*;

#[cfg(feature = "utils-terrain")]
pub use terrain::*;
