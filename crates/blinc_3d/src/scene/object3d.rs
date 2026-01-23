//! Base 3D object component

use crate::ecs::Component;
use crate::math::{Mat4Ext, Quat};
use blinc_core::{Mat4, Vec3};

/// Layers for rendering and culling
#[derive(Clone, Copy, Debug, Default)]
pub struct Layers(pub u32);

impl Layers {
    /// Default layer (0)
    pub const DEFAULT: Layers = Layers(1);
    /// All layers
    pub const ALL: Layers = Layers(u32::MAX);

    /// Check if this layer set intersects with another
    pub fn intersects(&self, other: Layers) -> bool {
        (self.0 & other.0) != 0
    }

    /// Enable a layer
    pub fn enable(&mut self, layer: u8) {
        self.0 |= 1 << layer;
    }

    /// Disable a layer
    pub fn disable(&mut self, layer: u8) {
        self.0 &= !(1 << layer);
    }

    /// Check if a layer is enabled
    pub fn is_enabled(&self, layer: u8) -> bool {
        (self.0 & (1 << layer)) != 0
    }
}

/// Base component for all 3D objects in the scene graph
///
/// This component stores the local transform (position, rotation, scale)
/// and visibility/rendering properties.
#[derive(Clone, Debug)]
pub struct Object3D {
    /// Local position relative to parent
    pub position: Vec3,
    /// Local rotation as quaternion
    pub rotation: Quat,
    /// Local scale
    pub scale: Vec3,
    /// Visibility flag
    pub visible: bool,
    /// Whether this object casts shadows
    pub cast_shadows: bool,
    /// Whether this object receives shadows
    pub receive_shadows: bool,
    /// User-defined layers for culling/rendering
    pub layers: Layers,
    /// Frustum culling enabled
    pub frustum_culled: bool,
    /// Render order (higher = rendered later)
    pub render_order: i32,
}

impl Default for Object3D {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::new(1.0, 1.0, 1.0),
            visible: true,
            cast_shadows: false,
            receive_shadows: false,
            layers: Layers::DEFAULT,
            frustum_culled: true,
            render_order: 0,
        }
    }
}

impl Component for Object3D {}

impl Object3D {
    /// Create a new Object3D at the origin
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with position
    pub fn at(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: Vec3::new(x, y, z),
            ..Default::default()
        }
    }

    /// Set position
    pub fn with_position(mut self, x: f32, y: f32, z: f32) -> Self {
        self.position = Vec3::new(x, y, z);
        self
    }

    /// Set rotation from Euler angles (radians)
    pub fn with_rotation(mut self, x: f32, y: f32, z: f32) -> Self {
        self.rotation = Quat::from_euler(x, y, z);
        self
    }

    /// Set scale
    pub fn with_scale(mut self, x: f32, y: f32, z: f32) -> Self {
        self.scale = Vec3::new(x, y, z);
        self
    }

    /// Set uniform scale
    pub fn with_uniform_scale(mut self, s: f32) -> Self {
        self.scale = Vec3::new(s, s, s);
        self
    }

    /// Enable shadow casting
    pub fn with_shadows(mut self, cast: bool, receive: bool) -> Self {
        self.cast_shadows = cast;
        self.receive_shadows = receive;
        self
    }

    /// Compute local transformation matrix
    pub fn local_matrix(&self) -> Mat4 {
        use crate::math::mat4_mul;
        let translation = <Mat4 as Mat4Ext>::from_translation(self.position);
        let rotation = self.rotation.to_mat4();
        let scale = <Mat4 as Mat4Ext>::from_scale(self.scale);
        mat4_mul(&mat4_mul(&translation, &rotation), &scale)
    }

    /// Look at a target position
    pub fn look_at(&mut self, target: Vec3) {
        // Calculate direction from position to target
        let dir = Vec3::new(
            target.x - self.position.x,
            target.y - self.position.y,
            target.z - self.position.z,
        );
        // Negate because forward() returns -Z in local space,
        // but Quat::look_at treats input as +Z direction
        let forward = Vec3::new(-dir.x, -dir.y, -dir.z);
        self.rotation = Quat::look_at(forward, Vec3::new(0.0, 1.0, 0.0));
    }

    /// Rotate around local X axis
    pub fn rotate_x(&mut self, angle: f32) {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), angle);
        self.rotation = self.rotation * q;
    }

    /// Rotate around local Y axis
    pub fn rotate_y(&mut self, angle: f32) {
        let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), angle);
        self.rotation = self.rotation * q;
    }

    /// Rotate around local Z axis
    pub fn rotate_z(&mut self, angle: f32) {
        let q = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), angle);
        self.rotation = self.rotation * q;
    }

    /// Rotate around an arbitrary axis
    pub fn rotate_on_axis(&mut self, axis: Vec3, angle: f32) {
        let q = Quat::from_axis_angle(axis, angle);
        self.rotation = self.rotation * q;
    }

    /// Translate along local X axis
    pub fn translate_x(&mut self, distance: f32) {
        let dir = self.rotation.rotate_vec3(Vec3::new(1.0, 0.0, 0.0));
        self.position.x += dir.x * distance;
        self.position.y += dir.y * distance;
        self.position.z += dir.z * distance;
    }

    /// Translate along local Y axis
    pub fn translate_y(&mut self, distance: f32) {
        let dir = self.rotation.rotate_vec3(Vec3::new(0.0, 1.0, 0.0));
        self.position.x += dir.x * distance;
        self.position.y += dir.y * distance;
        self.position.z += dir.z * distance;
    }

    /// Translate along local Z axis
    pub fn translate_z(&mut self, distance: f32) {
        let dir = self.rotation.rotate_vec3(Vec3::new(0.0, 0.0, 1.0));
        self.position.x += dir.x * distance;
        self.position.y += dir.y * distance;
        self.position.z += dir.z * distance;
    }

    /// Get the forward direction (negative Z in local space)
    pub fn forward(&self) -> Vec3 {
        self.rotation.rotate_vec3(Vec3::new(0.0, 0.0, -1.0))
    }

    /// Get the up direction (positive Y in local space)
    pub fn up(&self) -> Vec3 {
        self.rotation.rotate_vec3(Vec3::new(0.0, 1.0, 0.0))
    }

    /// Get the right direction (positive X in local space)
    pub fn right(&self) -> Vec3 {
        self.rotation.rotate_vec3(Vec3::new(1.0, 0.0, 0.0))
    }
}
