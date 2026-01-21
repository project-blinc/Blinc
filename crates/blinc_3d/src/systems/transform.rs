//! Transform hierarchy system

use crate::ecs::{Entity, System, SystemContext, World};
use crate::math::mat4_mul;
use crate::scene::{Hierarchy, Object3D};
use blinc_core::Mat4;
use rustc_hash::FxHashMap;

/// System for propagating transforms through the hierarchy
///
/// This system calculates world matrices for all Object3D components,
/// taking into account parent-child relationships.
pub struct TransformSystem {
    /// Cached world matrices
    world_matrices: FxHashMap<Entity, Mat4>,
    /// Dirty flags for entities that need recalculation
    dirty: FxHashMap<Entity, bool>,
}

impl TransformSystem {
    /// Create a new transform system
    pub fn new() -> Self {
        Self {
            world_matrices: FxHashMap::default(),
            dirty: FxHashMap::default(),
        }
    }

    /// Mark an entity as dirty (needs recalculation)
    pub fn mark_dirty(&mut self, entity: Entity) {
        self.dirty.insert(entity, true);
    }

    /// Get the cached world matrix for an entity
    pub fn world_matrix(&self, entity: Entity) -> Option<Mat4> {
        self.world_matrices.get(&entity).copied()
    }

    /// Calculate world matrix for an entity
    fn calculate_world_matrix(&self, world: &World, entity: Entity) -> Mat4 {
        let local_matrix = world
            .get_component::<Object3D>(entity)
            .map(|obj| obj.local_matrix())
            .unwrap_or(Mat4::IDENTITY);

        // Get parent's world matrix if this entity has a parent
        if let Some(hierarchy) = world.get_component::<Hierarchy>(entity) {
            if let Some(parent) = hierarchy.parent {
                if let Some(parent_world) = self.world_matrices.get(&parent) {
                    return mat4_mul(parent_world, &local_matrix);
                }
            }
        }

        local_matrix
    }

    /// Update all transforms
    pub fn update(&mut self, world: &World) {
        // Get all entities with Object3D
        let entities: Vec<Entity> = world
            .query::<&Object3D>()
            .iter()
            .map(|(entity, _)| entity)
            .collect();

        // Process root entities first, then children
        // This ensures parents are calculated before children
        let mut processed = FxHashMap::default();

        for entity in &entities {
            self.process_entity_recursive(world, *entity, &mut processed);
        }
    }

    /// Process an entity and its children recursively
    fn process_entity_recursive(
        &mut self,
        world: &World,
        entity: Entity,
        processed: &mut FxHashMap<Entity, bool>,
    ) {
        // Skip if already processed
        if processed.get(&entity).copied().unwrap_or(false) {
            return;
        }

        // Process parent first if needed
        if let Some(hierarchy) = world.get_component::<Hierarchy>(entity) {
            if let Some(parent) = hierarchy.parent {
                if !processed.get(&parent).copied().unwrap_or(false) {
                    self.process_entity_recursive(world, parent, processed);
                }
            }
        }

        // Calculate and cache world matrix
        let world_matrix = self.calculate_world_matrix(world, entity);
        self.world_matrices.insert(entity, world_matrix);
        processed.insert(entity, true);

        // Clear dirty flag
        self.dirty.remove(&entity);
    }

    /// Clear all cached data
    pub fn clear(&mut self) {
        self.world_matrices.clear();
        self.dirty.clear();
    }
}

impl Default for TransformSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for TransformSystem {
    fn name(&self) -> &'static str {
        "TransformSystem"
    }

    fn priority(&self) -> i32 {
        -100 // Run early
    }

    fn run(&mut self, ctx: &mut SystemContext) {
        self.update(ctx.world);
    }
}

/// Component for caching world transform
#[derive(Clone, Debug)]
pub struct WorldTransform {
    /// Cached world matrix
    pub matrix: Mat4,
    /// World position
    pub position: blinc_core::Vec3,
    /// World scale (approximate)
    pub scale: blinc_core::Vec3,
}

impl WorldTransform {
    /// Create from a world matrix
    /// Mat4 uses cols[col][row] format
    pub fn from_matrix(matrix: Mat4) -> Self {
        // Extract position from column 3 (translation column)
        let position = blinc_core::Vec3::new(matrix.cols[3][0], matrix.cols[3][1], matrix.cols[3][2]);

        // Approximate scale from matrix column lengths
        let scale = blinc_core::Vec3::new(
            blinc_core::Vec3::new(matrix.cols[0][0], matrix.cols[0][1], matrix.cols[0][2]).length(),
            blinc_core::Vec3::new(matrix.cols[1][0], matrix.cols[1][1], matrix.cols[1][2]).length(),
            blinc_core::Vec3::new(matrix.cols[2][0], matrix.cols[2][1], matrix.cols[2][2]).length(),
        );

        Self {
            matrix,
            position,
            scale,
        }
    }
}

impl Default for WorldTransform {
    fn default() -> Self {
        Self::from_matrix(Mat4::IDENTITY)
    }
}

impl crate::ecs::Component for WorldTransform {}
