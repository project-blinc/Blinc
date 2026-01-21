//! SDF (Signed Distance Field) system
//!
//! Provides tools for creating and rendering procedural 3D geometry
//! using signed distance fields and raymarching.

mod codegen;
mod operations;
mod primitives;

pub use codegen::SdfCodegen;
pub use operations::SdfOp;
pub use primitives::SdfPrimitive;

use crate::math::Mat4Ext;
use crate::scene::Object3D;
use blinc_core::{Color, Mat4, Vec3};

/// SDF material for raymarched surfaces
#[derive(Clone, Debug)]
pub struct SdfMaterial {
    /// Base color
    pub color: Color,
    /// Metalness (0-1)
    pub metalness: f32,
    /// Roughness (0-1)
    pub roughness: f32,
    /// Emissive color
    pub emissive: Color,
    /// Emissive intensity
    pub emissive_intensity: f32,
}

impl Default for SdfMaterial {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            metalness: 0.0,
            roughness: 0.5,
            emissive: Color::BLACK,
            emissive_intensity: 0.0,
        }
    }
}

impl SdfMaterial {
    /// Create a new SDF material
    pub fn new(color: Color) -> Self {
        Self {
            color,
            ..Default::default()
        }
    }

    /// Set metalness
    pub fn metalness(mut self, metalness: f32) -> Self {
        self.metalness = metalness.clamp(0.0, 1.0);
        self
    }

    /// Set roughness
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    /// Set emissive
    pub fn emissive(mut self, color: Color, intensity: f32) -> Self {
        self.emissive = color;
        self.emissive_intensity = intensity;
        self
    }
}

/// A node in the SDF scene graph
#[derive(Clone, Debug)]
pub struct SdfNode {
    /// Unique ID for this node
    pub id: u32,
    /// The SDF primitive or operation
    pub content: SdfNodeContent,
    /// Transform for this node
    pub transform: SdfTransform,
    /// Material (only used for primitives)
    pub material: SdfMaterial,
}

/// Content of an SDF node
#[derive(Clone, Debug)]
pub enum SdfNodeContent {
    /// A primitive shape
    Primitive(SdfPrimitive),
    /// A boolean operation combining two nodes
    Operation {
        op: SdfOp,
        left: Box<SdfNode>,
        right: Box<SdfNode>,
    },
}

/// Transform for SDF nodes
#[derive(Clone, Debug)]
pub struct SdfTransform {
    /// Position offset
    pub position: Vec3,
    /// Rotation in radians (Euler angles XYZ)
    pub rotation: Vec3,
    /// Scale factor
    pub scale: Vec3,
}

impl Default for SdfTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }
}

impl SdfTransform {
    /// Create a new transform
    pub fn new() -> Self {
        Self::default()
    }

    /// Set position
    pub fn position(mut self, pos: Vec3) -> Self {
        self.position = pos;
        self
    }

    /// Set rotation in radians
    pub fn rotation(mut self, rot: Vec3) -> Self {
        self.rotation = rot;
        self
    }

    /// Set uniform scale
    pub fn scale_uniform(mut self, s: f32) -> Self {
        self.scale = Vec3::new(s, s, s);
        self
    }

    /// Set non-uniform scale
    pub fn scale(mut self, s: Vec3) -> Self {
        self.scale = s;
        self
    }

    /// Convert to transformation matrix
    pub fn to_matrix(&self) -> Mat4 {
        use crate::math::mat4_mul;
        let translation = <Mat4 as Mat4Ext>::from_translation(self.position);
        let rotation_x = <Mat4 as Mat4Ext>::from_rotation_x(self.rotation.x);
        let rotation_y = <Mat4 as Mat4Ext>::from_rotation_y(self.rotation.y);
        let rotation_z = <Mat4 as Mat4Ext>::from_rotation_z(self.rotation.z);
        let scale = <Mat4 as Mat4Ext>::from_scale(self.scale);

        // Chain multiplications: translation * rotation_z * rotation_y * rotation_x * scale
        let m1 = mat4_mul(&translation, &rotation_z);
        let m2 = mat4_mul(&m1, &rotation_y);
        let m3 = mat4_mul(&m2, &rotation_x);
        mat4_mul(&m3, &scale)
    }
}

/// SDF scene containing multiple SDF nodes
#[derive(Clone, Debug)]
pub struct SdfScene {
    /// Root node of the scene
    root: Option<SdfNode>,
    /// Next node ID
    next_id: u32,
}

impl SdfScene {
    /// Create a new empty SDF scene
    pub fn new() -> Self {
        Self {
            root: None,
            next_id: 0,
        }
    }

    /// Create a sphere SDF
    pub fn sphere(radius: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Sphere { radius }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Create a box SDF
    pub fn cube(size: f32) -> SdfNode {
        Self::box_node(Vec3::new(size * 0.5, size * 0.5, size * 0.5))
    }

    /// Create a box SDF with half extents
    pub fn box_node(half_extents: Vec3) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Box { half_extents }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Create a torus SDF
    pub fn torus(major_radius: f32, minor_radius: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Torus {
                major_radius,
                minor_radius,
            }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Create a cylinder SDF
    pub fn cylinder(height: f32, radius: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Cylinder { height, radius }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Create a plane SDF
    pub fn plane(normal: Vec3, offset: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Plane {
                normal: normal.normalize(),
                offset,
            }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Create a capsule SDF
    pub fn capsule(start: Vec3, end: Vec3, radius: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Capsule { start, end, radius }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Create a cone SDF
    pub fn cone(height: f32, radius: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Primitive(SdfPrimitive::Cone { height, radius }),
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Union of two SDFs
    pub fn union(a: SdfNode, b: SdfNode) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Operation {
                op: SdfOp::Union,
                left: Box::new(a),
                right: Box::new(b),
            },
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Smooth union of two SDFs
    pub fn smooth_union(a: SdfNode, b: SdfNode, k: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Operation {
                op: SdfOp::SmoothUnion { k },
                left: Box::new(a),
                right: Box::new(b),
            },
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Subtraction of b from a
    pub fn subtract(a: SdfNode, b: SdfNode) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Operation {
                op: SdfOp::Subtract,
                left: Box::new(a),
                right: Box::new(b),
            },
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Smooth subtraction
    pub fn smooth_subtract(a: SdfNode, b: SdfNode, k: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Operation {
                op: SdfOp::SmoothSubtract { k },
                left: Box::new(a),
                right: Box::new(b),
            },
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Intersection of two SDFs
    pub fn intersect(a: SdfNode, b: SdfNode) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Operation {
                op: SdfOp::Intersect,
                left: Box::new(a),
                right: Box::new(b),
            },
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Smooth intersection
    pub fn smooth_intersect(a: SdfNode, b: SdfNode, k: f32) -> SdfNode {
        SdfNode {
            id: 0,
            content: SdfNodeContent::Operation {
                op: SdfOp::SmoothIntersect { k },
                left: Box::new(a),
                right: Box::new(b),
            },
            transform: SdfTransform::default(),
            material: SdfMaterial::default(),
        }
    }

    /// Set the root node
    pub fn set_root(&mut self, node: SdfNode) {
        self.root = Some(self.assign_ids(node));
    }

    /// Assign unique IDs to all nodes
    fn assign_ids(&mut self, mut node: SdfNode) -> SdfNode {
        node.id = self.next_id;
        self.next_id += 1;

        if let SdfNodeContent::Operation { left, right, .. } = &mut node.content {
            *left = Box::new(self.assign_ids(*left.clone()));
            *right = Box::new(self.assign_ids(*right.clone()));
        }

        node
    }

    /// Get the root node
    pub fn root(&self) -> Option<&SdfNode> {
        self.root.as_ref()
    }

    /// Generate WGSL code for this scene
    pub fn to_wgsl(&self) -> String {
        SdfCodegen::generate(self)
    }
}

impl Default for SdfScene {
    fn default() -> Self {
        Self::new()
    }
}

impl SdfNode {
    /// Set the transform position
    pub fn at(mut self, pos: Vec3) -> Self {
        self.transform.position = pos;
        self
    }

    /// Set the transform rotation
    pub fn rotated(mut self, rot: Vec3) -> Self {
        self.transform.rotation = rot;
        self
    }

    /// Set uniform scale
    pub fn scaled(mut self, s: f32) -> Self {
        self.transform.scale = Vec3::new(s, s, s);
        self
    }

    /// Set non-uniform scale
    pub fn scaled_xyz(mut self, s: Vec3) -> Self {
        self.transform.scale = s;
        self
    }

    /// Set material
    pub fn with_material(mut self, material: SdfMaterial) -> Self {
        self.material = material;
        self
    }

    /// Set color
    pub fn with_color(mut self, color: Color) -> Self {
        self.material.color = color;
        self
    }
}

/// SDF raymarching configuration
#[derive(Clone, Debug)]
pub struct SdfRaymarchConfig {
    /// Maximum raymarching steps
    pub max_steps: u32,
    /// Maximum raymarching distance
    pub max_distance: f32,
    /// Surface epsilon (hit threshold)
    pub epsilon: f32,
    /// Normal estimation epsilon
    pub normal_epsilon: f32,
}

impl Default for SdfRaymarchConfig {
    fn default() -> Self {
        Self {
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            normal_epsilon: 0.0001,
        }
    }
}

/// SDF uniform data for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SdfUniform {
    /// Camera position
    pub camera_pos: [f32; 4],
    /// Camera direction
    pub camera_dir: [f32; 4],
    /// Camera up vector
    pub camera_up: [f32; 4],
    /// Camera right vector
    pub camera_right: [f32; 4],
    /// Resolution (width, height)
    pub resolution: [f32; 2],
    /// Time for animation
    pub time: f32,
    /// Field of view in radians
    pub fov: f32,
    /// Raymarch config
    pub max_steps: u32,
    pub max_distance: f32,
    pub epsilon: f32,
    pub _padding: f32,
}

impl Default for SdfUniform {
    fn default() -> Self {
        Self {
            camera_pos: [0.0, 0.0, 5.0, 1.0],
            camera_dir: [0.0, 0.0, -1.0, 0.0],
            camera_up: [0.0, 1.0, 0.0, 0.0],
            camera_right: [1.0, 0.0, 0.0, 0.0],
            resolution: [800.0, 600.0],
            time: 0.0,
            fov: 0.8,
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            _padding: 0.0,
        }
    }
}
