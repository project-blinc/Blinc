//! Mesh component

use crate::ecs::Component;
use crate::geometry::GeometryHandle;
use crate::materials::MaterialHandle;

/// Mesh component combining geometry and material
///
/// Attach this to an entity with an Object3D to render 3D geometry.
#[derive(Clone, Debug)]
pub struct Mesh {
    /// Handle to the geometry data
    pub geometry: GeometryHandle,
    /// Handle to the material
    pub material: MaterialHandle,
}

impl Component for Mesh {}

impl Mesh {
    /// Create a new mesh
    pub fn new(geometry: GeometryHandle, material: MaterialHandle) -> Self {
        Self { geometry, material }
    }
}

/// Skinned mesh for animated characters
#[derive(Clone, Debug)]
pub struct SkinnedMesh {
    /// Base mesh
    pub mesh: Mesh,
    /// Skeleton for bone animation
    pub skeleton: Option<SkeletonHandle>,
}

impl Component for SkinnedMesh {}

/// Handle to a skeleton resource
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SkeletonHandle(pub u64);

/// Instance of a mesh for instanced rendering
#[derive(Clone, Debug)]
pub struct MeshInstance {
    /// Geometry handle
    pub geometry: GeometryHandle,
    /// Material override (optional)
    pub material: Option<MaterialHandle>,
    /// Instance-specific transform offset
    pub transform_offset: blinc_core::Mat4,
    /// Instance-specific color tint
    pub color_tint: blinc_core::Color,
}

impl Default for MeshInstance {
    fn default() -> Self {
        Self {
            geometry: GeometryHandle(0),
            material: None,
            transform_offset: blinc_core::Mat4::IDENTITY,
            color_tint: blinc_core::Color::WHITE,
        }
    }
}

/// Component for instanced rendering of many similar objects
#[derive(Clone, Debug)]
pub struct InstancedMesh {
    /// Base geometry
    pub geometry: GeometryHandle,
    /// Base material
    pub material: MaterialHandle,
    /// Per-instance transforms
    pub instance_transforms: Vec<blinc_core::Mat4>,
    /// Per-instance colors (optional)
    pub instance_colors: Option<Vec<blinc_core::Color>>,
}

impl Component for InstancedMesh {}

impl InstancedMesh {
    /// Create a new instanced mesh
    pub fn new(geometry: GeometryHandle, material: MaterialHandle) -> Self {
        Self {
            geometry,
            material,
            instance_transforms: Vec::new(),
            instance_colors: None,
        }
    }

    /// Add an instance
    pub fn add_instance(&mut self, transform: blinc_core::Mat4) {
        self.instance_transforms.push(transform);
    }

    /// Add an instance with color
    pub fn add_instance_with_color(&mut self, transform: blinc_core::Mat4, color: blinc_core::Color) {
        self.instance_transforms.push(transform);
        if self.instance_colors.is_none() {
            self.instance_colors = Some(vec![blinc_core::Color::WHITE; self.instance_transforms.len() - 1]);
        }
        self.instance_colors.as_mut().unwrap().push(color);
    }

    /// Get instance count
    pub fn instance_count(&self) -> usize {
        self.instance_transforms.len()
    }

    /// Clear all instances
    pub fn clear(&mut self) {
        self.instance_transforms.clear();
        if let Some(colors) = &mut self.instance_colors {
            colors.clear();
        }
    }
}
