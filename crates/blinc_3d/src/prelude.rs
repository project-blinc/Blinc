//! Prelude module for common imports
//!
//! ```rust,ignore
//! use blinc_3d::prelude::*;
//! ```

// ECS
pub use crate::ecs::{
    Component, Entity, EntityBuilder, EntityManager, Query, Resource, ResourceMap, System,
    SystemContext, SystemStage, World,
};

// Scene
pub use crate::scene::{Mesh, Object3D, OrthographicCamera, PerspectiveCamera};

// Geometry
pub use crate::geometry::{
    BoxGeometry, CylinderGeometry, Geometry, GeometryHandle, PlaneGeometry, SphereGeometry,
    TorusGeometry, Vertex,
};

// Materials
pub use crate::materials::{
    BasicMaterial, Material, MaterialHandle, PhongMaterial, Side, StandardMaterial, TextureHandle,
};

// Lights
pub use crate::lights::{AmbientLight, DirectionalLight, HemisphereLight, PointLight, SpotLight};

// Integration
pub use crate::integration::{
    create_game_fsm, game_events, game_states, render_scene, AnimatedColor, AnimatedTransform,
    AnimatedVec3, GameStateMachine,
};

// Systems
pub use crate::systems::{
    AnimationSyncSystem, ColorAnimation, DeltaTime, Easing, FloatAnimation, FrameCount,
    Interpolate, QuatAnimation, QuatKeyframe, SphericalInterpolate, TotalTime, TransformSystem,
    TypedKeyframe, TypedKeyframeAnimation, Vec3Animation, VisibilitySystem,
};

// Math
pub use crate::math::{BoundingBox, BoundingSphere, Quat};

// SDF
pub use crate::sdf::{SdfMaterial, SdfNode, SdfOp, SdfPrimitive, SdfScene};

// Render
pub use crate::render::{CameraUniform, GamePipelines, ModelUniform, ShaderRegistry, ShaderId};

// Re-export common types from blinc_core
pub use blinc_core::{Color, Mat4, Vec2, Vec3};

// Re-export animation types
pub use blinc_animation::{Spring, SpringConfig};

// Re-export FSM types
pub use blinc_core::{FsmId, FsmRuntime, StateMachine, Transition};

// Utils - Camera controls
#[cfg(feature = "utils-camera")]
pub use crate::utils::camera::{
    CameraController, CameraInput, CameraKeys, CameraTransform, CameraUpdateContext,
    DroneController, FlyController, FollowController, OrbitController, CameraShake,
};

// Utils - Lighting presets
#[cfg(feature = "utils-lighting")]
pub use crate::utils::lighting::{
    BuiltinPreset, CustomPreset, LightConfig, LightParams, LightType, LightingPreset,
    LightingPresetBuilder, apply_lights,
};

// Utils - Skybox system
#[cfg(feature = "utils-skybox")]
pub use crate::utils::skybox::{
    CubemapSkybox, DayNightCycle, GradientSkybox, ProceduralSkybox, Skybox, SkyboxAsset,
    TimeOfDay, TimeOfDaySystem,
};

// Utils - Mesh loaders
#[cfg(any(feature = "utils-gltf", feature = "utils-obj"))]
pub use crate::utils::loader::{
    LoadError, LoadedMaterial, LoadedMesh, LoadedScene, LoadedVertex, MeshCache, MeshLoader,
    MeshLoaderRegistry, load_mesh,
};
