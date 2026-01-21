//! Visibility and frustum culling system

use crate::ecs::{Entity, System, SystemContext, World};
use crate::math::{BoundingBox, BoundingSphere, Vec3Ext, mat4_mul};
use crate::scene::{Object3D, PerspectiveCamera};
use blinc_core::{Mat4, Vec3};
use rustc_hash::FxHashSet;

/// System for determining visibility and performing frustum culling
pub struct VisibilitySystem {
    /// Set of visible entities after culling
    visible_entities: FxHashSet<Entity>,
    /// Frustum planes for culling
    frustum_planes: [Vec4; 6],
}

/// 4D vector for plane equations
#[derive(Clone, Copy, Debug)]
struct Vec4 {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

impl Vec4 {
    fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    fn normalize_plane(&self) -> Self {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if len > 0.0 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
                w: self.w / len,
            }
        } else {
            *self
        }
    }

    fn dot_point(&self, point: Vec3) -> f32 {
        self.x * point.x + self.y * point.y + self.z * point.z + self.w
    }
}

impl VisibilitySystem {
    /// Create a new visibility system
    pub fn new() -> Self {
        Self {
            visible_entities: FxHashSet::default(),
            frustum_planes: [Vec4::new(0.0, 0.0, 0.0, 0.0); 6],
        }
    }

    /// Check if an entity is visible
    pub fn is_visible(&self, entity: Entity) -> bool {
        self.visible_entities.contains(&entity)
    }

    /// Get all visible entities
    pub fn visible_entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.visible_entities.iter().copied()
    }

    /// Get visible entity count
    pub fn visible_count(&self) -> usize {
        self.visible_entities.len()
    }

    /// Extract frustum planes from view-projection matrix
    /// Mat4 uses cols[col][row] format
    fn extract_frustum_planes(&mut self, vp: &Mat4) {
        // Left plane: row3 + row0
        self.frustum_planes[0] = Vec4::new(
            vp.cols[0][3] + vp.cols[0][0],
            vp.cols[1][3] + vp.cols[1][0],
            vp.cols[2][3] + vp.cols[2][0],
            vp.cols[3][3] + vp.cols[3][0],
        )
        .normalize_plane();

        // Right plane: row3 - row0
        self.frustum_planes[1] = Vec4::new(
            vp.cols[0][3] - vp.cols[0][0],
            vp.cols[1][3] - vp.cols[1][0],
            vp.cols[2][3] - vp.cols[2][0],
            vp.cols[3][3] - vp.cols[3][0],
        )
        .normalize_plane();

        // Bottom plane: row3 + row1
        self.frustum_planes[2] = Vec4::new(
            vp.cols[0][3] + vp.cols[0][1],
            vp.cols[1][3] + vp.cols[1][1],
            vp.cols[2][3] + vp.cols[2][1],
            vp.cols[3][3] + vp.cols[3][1],
        )
        .normalize_plane();

        // Top plane: row3 - row1
        self.frustum_planes[3] = Vec4::new(
            vp.cols[0][3] - vp.cols[0][1],
            vp.cols[1][3] - vp.cols[1][1],
            vp.cols[2][3] - vp.cols[2][1],
            vp.cols[3][3] - vp.cols[3][1],
        )
        .normalize_plane();

        // Near plane: row3 + row2
        self.frustum_planes[4] = Vec4::new(
            vp.cols[0][3] + vp.cols[0][2],
            vp.cols[1][3] + vp.cols[1][2],
            vp.cols[2][3] + vp.cols[2][2],
            vp.cols[3][3] + vp.cols[3][2],
        )
        .normalize_plane();

        // Far plane: row3 - row2
        self.frustum_planes[5] = Vec4::new(
            vp.cols[0][3] - vp.cols[0][2],
            vp.cols[1][3] - vp.cols[1][2],
            vp.cols[2][3] - vp.cols[2][2],
            vp.cols[3][3] - vp.cols[3][2],
        )
        .normalize_plane();
    }

    /// Test if a bounding sphere is inside or intersects the frustum
    fn sphere_in_frustum(&self, center: Vec3, radius: f32) -> bool {
        for plane in &self.frustum_planes {
            if plane.dot_point(center) < -radius {
                return false;
            }
        }
        true
    }

    /// Test if a bounding box is inside or intersects the frustum
    fn box_in_frustum(&self, bounds: &BoundingBox) -> bool {
        let corners = [
            Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
            Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
            Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
            Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
            Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
            Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
        ];

        for plane in &self.frustum_planes {
            let mut all_outside = true;
            for corner in &corners {
                if plane.dot_point(*corner) >= 0.0 {
                    all_outside = false;
                    break;
                }
            }
            if all_outside {
                return false;
            }
        }
        true
    }

    /// Update visibility for all entities
    pub fn update(&mut self, world: &World, camera_entity: Entity) {
        self.visible_entities.clear();

        // Get camera matrices
        let camera_transform = world
            .get_component::<Object3D>(camera_entity)
            .cloned()
            .unwrap_or_default();

        let view_projection = if let Some(camera) = world.get_component::<PerspectiveCamera>(camera_entity) {
            let proj = camera.projection_matrix();
            let view = camera.view_matrix(&camera_transform);
            mat4_mul(&proj, &view)
        } else {
            Mat4::IDENTITY
        };

        self.extract_frustum_planes(&view_projection);

        // Check each entity with Object3D
        for (entity, object) in world.query::<&Object3D>() {
            // Skip invisible objects
            if !object.visible {
                continue;
            }

            // Skip objects that don't want frustum culling
            if !object.frustum_culled {
                self.visible_entities.insert(entity);
                continue;
            }

            // Check against bounding sphere if available
            if let Some(bounds) = world.get_component::<BoundingSphere>(entity) {
                // Vec3 + Vec3 manually
                let world_center = Vec3::new(
                    object.position.x + bounds.center.x,
                    object.position.y + bounds.center.y,
                    object.position.z + bounds.center.z,
                );
                let world_radius = bounds.radius * object.scale.max_element();

                if self.sphere_in_frustum(world_center, world_radius) {
                    self.visible_entities.insert(entity);
                }
            }
            // Check against bounding box if available
            else if let Some(bounds) = world.get_component::<BoundingBox>(entity) {
                // Transform AABB corners and create new bounds
                let local_matrix = object.local_matrix();
                let corners = bounds.corners();
                let mut world_bounds = BoundingBox::empty();
                for corner in &corners {
                    use crate::math::Mat4Ext;
                    let world_corner = local_matrix.transform_point(*corner);
                    world_bounds.expand_to_include(world_corner);
                }
                if self.box_in_frustum(&world_bounds) {
                    self.visible_entities.insert(entity);
                }
            }
            // No bounds, assume visible
            else {
                self.visible_entities.insert(entity);
            }
        }
    }

    /// Clear visibility data
    pub fn clear(&mut self) {
        self.visible_entities.clear();
    }
}

impl Default for VisibilitySystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for VisibilitySystem {
    fn name(&self) -> &'static str {
        "VisibilitySystem"
    }

    fn priority(&self) -> i32 {
        -50 // Run after transform but before rendering
    }

    fn run(&mut self, ctx: &mut SystemContext) {
        // Note: Camera entity would need to be passed via resources or context
        // For now, find the first entity with a camera component
        if let Some((camera_entity, _)) = ctx.world.query::<&PerspectiveCamera>().iter().next() {
            self.update(ctx.world, camera_entity);
        }
    }
}

/// Occlusion query result
#[derive(Clone, Copy, Debug)]
pub struct OcclusionResult {
    /// Entity that was queried
    pub entity: Entity,
    /// Whether entity is occluded
    pub occluded: bool,
    /// Number of visible samples (if available)
    pub visible_samples: u32,
}

/// Layer mask for selective visibility
#[derive(Clone, Copy, Debug, Default)]
pub struct LayerMask(pub u32);

impl LayerMask {
    /// Create a layer mask with all layers enabled
    pub fn all() -> Self {
        Self(u32::MAX)
    }

    /// Create a layer mask with no layers
    pub fn none() -> Self {
        Self(0)
    }

    /// Create a mask for a single layer
    pub fn layer(layer: u32) -> Self {
        Self(1 << layer.min(31))
    }

    /// Check if a layer is enabled
    pub fn has_layer(&self, layer: u32) -> bool {
        (self.0 & (1 << layer.min(31))) != 0
    }

    /// Enable a layer
    pub fn enable_layer(&mut self, layer: u32) {
        self.0 |= 1 << layer.min(31);
    }

    /// Disable a layer
    pub fn disable_layer(&mut self, layer: u32) {
        self.0 &= !(1 << layer.min(31));
    }

    /// Check if two masks have overlapping layers
    pub fn overlaps(&self, other: &LayerMask) -> bool {
        (self.0 & other.0) != 0
    }
}

impl crate::ecs::Component for LayerMask {}
