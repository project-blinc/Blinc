//! SDF Mesh component for ECS-integrated SDF rendering
//!
//! This component allows SDF (Signed Distance Field) objects to be
//! entities in the ECS World, rendered alongside regular meshes.

use crate::ecs::Component;
use crate::sdf::{SdfMaterial, SdfNode, SdfScene};

/// SDF Mesh component for rendering procedural geometry via raymarching
///
/// Attach this to an entity with an Object3D to render SDF geometry.
/// The entity's Object3D transform is applied to the entire SDF scene.
///
/// # Example
///
/// ```ignore
/// use blinc_3d::prelude::*;
///
/// // Create entity with SDF geometry
/// world.spawn()
///     .insert(Object3D {
///         position: Vec3::new(0.0, 1.0, 0.0),
///         ..Default::default()
///     })
///     .insert(SdfMesh::sphere(1.0)
///         .with_color(Color::rgb(0.3, 0.7, 1.0)));
///
/// // Combine multiple SDF primitives
/// let union = SdfScene::union(
///     SdfScene::sphere(0.5),
///     SdfScene::cube(0.8).at(Vec3::new(0.3, 0.0, 0.0)),
/// );
/// world.spawn()
///     .insert(Object3D::default())
///     .insert(SdfMesh::from_scene(union));
/// ```
#[derive(Clone, Debug)]
pub struct SdfMesh {
    /// The SDF scene to render
    pub scene: SdfScene,
    /// Whether this SDF mesh casts shadows (not yet implemented)
    pub cast_shadows: bool,
    /// Whether this SDF mesh receives shadows (not yet implemented)
    pub receive_shadows: bool,
}

impl Component for SdfMesh {}

impl SdfMesh {
    /// Create an SDF mesh from an existing scene
    pub fn from_scene(scene: SdfScene) -> Self {
        Self {
            scene,
            cast_shadows: false,
            receive_shadows: false,
        }
    }

    /// Create an SDF mesh from a single node
    pub fn from_node(node: SdfNode) -> Self {
        let mut scene = SdfScene::new();
        scene.set_root(node);
        Self::from_scene(scene)
    }

    /// Create a sphere SDF mesh
    pub fn sphere(radius: f32) -> Self {
        let mut scene = SdfScene::new();
        scene.set_root(SdfScene::sphere(radius));
        Self::from_scene(scene)
    }

    /// Create a cube SDF mesh
    pub fn cube(size: f32) -> Self {
        let mut scene = SdfScene::new();
        scene.set_root(SdfScene::cube(size));
        Self::from_scene(scene)
    }

    /// Create a box SDF mesh with different dimensions
    pub fn box_shape(half_extents: blinc_core::Vec3) -> Self {
        let mut scene = SdfScene::new();
        scene.set_root(SdfScene::box_node(half_extents));
        Self::from_scene(scene)
    }

    /// Create a torus SDF mesh
    pub fn torus(major_radius: f32, minor_radius: f32) -> Self {
        let mut scene = SdfScene::new();
        scene.set_root(SdfScene::torus(major_radius, minor_radius));
        Self::from_scene(scene)
    }

    /// Create a cylinder SDF mesh
    pub fn cylinder(height: f32, radius: f32) -> Self {
        let mut scene = SdfScene::new();
        scene.set_root(SdfScene::cylinder(height, radius));
        Self::from_scene(scene)
    }

    /// Create a capsule SDF mesh
    ///
    /// The capsule is oriented vertically along the Y axis.
    pub fn capsule(height: f32, radius: f32) -> Self {
        let mut scene = SdfScene::new();
        let half_height = height / 2.0;
        let start = blinc_core::Vec3::new(0.0, -half_height, 0.0);
        let end = blinc_core::Vec3::new(0.0, half_height, 0.0);
        scene.set_root(SdfScene::capsule(start, end, radius));
        Self::from_scene(scene)
    }

    /// Set the color of the SDF mesh (modifies the root node's material)
    pub fn with_color(mut self, color: blinc_core::Color) -> Self {
        if let Some(root) = self.scene.root_mut() {
            root.material.color = color;
        }
        self
    }

    /// Set the material properties
    pub fn with_material(mut self, material: SdfMaterial) -> Self {
        if let Some(root) = self.scene.root_mut() {
            root.material = material;
        }
        self
    }

    /// Enable/disable shadow casting
    pub fn cast_shadows(mut self, enabled: bool) -> Self {
        self.cast_shadows = enabled;
        self
    }

    /// Enable/disable shadow receiving
    pub fn receive_shadows(mut self, enabled: bool) -> Self {
        self.receive_shadows = enabled;
        self
    }
}

impl Default for SdfMesh {
    fn default() -> Self {
        Self::sphere(1.0)
    }
}
