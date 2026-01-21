//! Scene hierarchy component

use crate::ecs::{Component, Entity};
use smallvec::SmallVec;

/// Component storing parent-child relationships
///
/// This is automatically managed by the EntityManager but can be
/// queried directly for traversal.
#[derive(Clone, Debug, Default)]
pub struct Hierarchy {
    /// Parent entity (None for root objects)
    pub parent: Option<Entity>,
    /// Child entities
    pub children: SmallVec<[Entity; 4]>,
}

impl Component for Hierarchy {}

impl Hierarchy {
    /// Create a new hierarchy node (root)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with parent
    pub fn with_parent(parent: Entity) -> Self {
        Self {
            parent: Some(parent),
            children: SmallVec::new(),
        }
    }

    /// Check if this is a root node
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// Check if this has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Get child count
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// Cached world transform for efficient hierarchy traversal
#[derive(Clone, Debug)]
pub struct GlobalTransform {
    /// Cached world matrix
    pub matrix: blinc_core::Mat4,
    /// Whether the cache is valid
    pub dirty: bool,
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self {
            matrix: blinc_core::Mat4::IDENTITY,
            dirty: true,
        }
    }
}

impl Component for GlobalTransform {}

impl GlobalTransform {
    /// Mark as needing update
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Update the cached matrix
    pub fn update(&mut self, matrix: blinc_core::Mat4) {
        self.matrix = matrix;
        self.dirty = false;
    }

    /// Get world position
    pub fn position(&self) -> blinc_core::Vec3 {
        blinc_core::Vec3::new(
            self.matrix.cols[3][0],
            self.matrix.cols[3][1],
            self.matrix.cols[3][2],
        )
    }
}
