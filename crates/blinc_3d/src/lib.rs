//! # Blinc 3D
//!
//! A 3D game/application library for the Blinc UI framework.
//!
//! This crate provides:
//! - **ECS** (Entity Component System) for game development
//! - **Three.js-inspired API** for 3D graphics
//! - **Integration** with Blinc's animation, FSM, and color systems
//! - **Default shader pipelines** for common rendering scenarios
//! - **SDF rendering** for procedural geometry
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use blinc_3d::prelude::*;
//! use blinc_layout::prelude::*;
//!
//! fn game_view(world: &World, camera: Entity) -> impl ElementBuilder {
//!     canvas(move |ctx, bounds| {
//!         render_scene(ctx, world, camera, bounds);
//!     })
//!     .w_full()
//!     .h_full()
//! }
//! ```

// ECS - Entity Component System
pub mod ecs;

// Scene graph
pub mod scene;

// Geometry primitives
pub mod geometry;

// Material system
pub mod materials;

// Lighting
pub mod lights;

// Shader pipelines
pub mod render;

// SDF system (always available, feature flag controls extended functionality)
pub mod sdf;

// Node graph system
pub mod nodegraph;

// Blinc integration
pub mod integration;

// Built-in systems
pub mod systems;

// Math utilities
pub mod math;

// Utility modules (feature-gated)
#[cfg(any(
    feature = "utils-camera",
    feature = "utils-lighting",
    feature = "utils-skybox",
    feature = "utils-gltf",
    feature = "utils-obj"
))]
pub mod utils;

// Prelude for common imports
pub mod prelude;

// Re-export core types at crate root
pub use ecs::{Component, Entity, Query, System, SystemContext, SystemStage, World};
pub use geometry::{BoxGeometry, CylinderGeometry, Geometry, PlaneGeometry, SphereGeometry, TorusGeometry, Vertex};
pub use integration::{render_scene, AnimatedColor, AnimatedTransform, AnimatedVec3};
pub use lights::{AmbientLight, DirectionalLight, HemisphereLight, PointLight, SpotLight};
pub use materials::{BasicMaterial, Material, PhongMaterial, Side, StandardMaterial};
pub use math::{BoundingBox, BoundingSphere, Quat};
pub use scene::{Mesh, Object3D, OrthographicCamera, PerspectiveCamera};
pub use sdf::{SdfMaterial, SdfNode, SdfScene};
pub use nodegraph::{Connection, Node, NodeGraphSystem, NodePorts, NodeValue, OnTrigger, PortDef, PortDirection, PortType, PortTypeId, TriggerContext, Triggered};
