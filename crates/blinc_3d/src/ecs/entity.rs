//! Entity management

use rustc_hash::FxHashMap;
use slotmap::{new_key_type, SlotMap};
use smallvec::SmallVec;

new_key_type! {
    /// Unique identifier for an entity in the world
    pub struct Entity;
}

/// Entity metadata for debugging and scene hierarchy
#[derive(Clone, Debug, Default)]
pub struct EntityMeta {
    /// Optional name for debugging
    pub name: Option<String>,
    /// Tags for filtering
    pub tags: SmallVec<[String; 4]>,
    /// Whether the entity is enabled
    pub enabled: bool,
}

impl EntityMeta {
    /// Create new entity metadata
    pub fn new() -> Self {
        Self {
            name: None,
            tags: SmallVec::new(),
            enabled: true,
        }
    }

    /// Set the entity name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// Manages entity lifecycle and hierarchy
pub struct EntityManager {
    /// Entity storage with metadata
    entities: SlotMap<Entity, EntityMeta>,
    /// Parent-child relationships
    children: FxHashMap<Entity, SmallVec<[Entity; 8]>>,
    /// Child-parent relationships
    parents: FxHashMap<Entity, Entity>,
}

impl Default for EntityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityManager {
    /// Create a new entity manager
    pub fn new() -> Self {
        Self {
            entities: SlotMap::with_key(),
            children: FxHashMap::default(),
            parents: FxHashMap::default(),
        }
    }

    /// Spawn a new entity
    pub fn spawn(&mut self) -> Entity {
        self.entities.insert(EntityMeta::new())
    }

    /// Spawn a new entity with metadata
    pub fn spawn_with_meta(&mut self, meta: EntityMeta) -> Entity {
        self.entities.insert(meta)
    }

    /// Check if an entity exists
    pub fn exists(&self, entity: Entity) -> bool {
        self.entities.contains_key(entity)
    }

    /// Get entity metadata
    pub fn get_meta(&self, entity: Entity) -> Option<&EntityMeta> {
        self.entities.get(entity)
    }

    /// Get mutable entity metadata
    pub fn get_meta_mut(&mut self, entity: Entity) -> Option<&mut EntityMeta> {
        self.entities.get_mut(entity)
    }

    /// Despawn an entity and all its children
    pub fn despawn(&mut self, entity: Entity) {
        // Recursively despawn children
        if let Some(children) = self.children.remove(&entity) {
            for child in children {
                self.despawn(child);
            }
        }

        // Remove from parent's children list
        if let Some(parent) = self.parents.remove(&entity) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|e| *e != entity);
            }
        }

        // Remove the entity
        self.entities.remove(entity);
    }

    /// Set parent-child relationship
    pub fn set_parent(&mut self, child: Entity, parent: Entity) {
        // Remove from old parent
        if let Some(old_parent) = self.parents.remove(&child) {
            if let Some(siblings) = self.children.get_mut(&old_parent) {
                siblings.retain(|e| *e != child);
            }
        }

        // Add to new parent
        self.parents.insert(child, parent);
        self.children.entry(parent).or_default().push(child);
    }

    /// Remove parent relationship
    pub fn remove_parent(&mut self, child: Entity) {
        if let Some(parent) = self.parents.remove(&child) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|e| *e != child);
            }
        }
    }

    /// Get parent of an entity
    pub fn parent(&self, entity: Entity) -> Option<Entity> {
        self.parents.get(&entity).copied()
    }

    /// Get children of an entity
    pub fn children(&self, entity: Entity) -> Option<&[Entity]> {
        self.children.get(&entity).map(|v| v.as_slice())
    }

    /// Get all entities
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.keys()
    }

    /// Get entity count
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

/// Builder for spawning entities with components
pub struct EntityBuilder<'w> {
    world: &'w mut super::World,
    entity: Entity,
}

impl<'w> EntityBuilder<'w> {
    /// Create a new entity builder
    pub(crate) fn new(world: &'w mut super::World, entity: Entity) -> Self {
        Self { world, entity }
    }

    /// Insert a component
    pub fn insert<C: super::Component>(self, component: C) -> Self {
        self.world.insert(self.entity, component);
        self
    }

    /// Set entity name
    pub fn name(self, name: impl Into<String>) -> Self {
        if let Some(meta) = self.world.entities.get_meta_mut(self.entity) {
            meta.name = Some(name.into());
        }
        self
    }

    /// Add a tag
    pub fn tag(self, tag: impl Into<String>) -> Self {
        if let Some(meta) = self.world.entities.get_meta_mut(self.entity) {
            meta.tags.push(tag.into());
        }
        self
    }

    /// Set parent entity
    pub fn parent(self, parent: Entity) -> Self {
        self.world.entities.set_parent(self.entity, parent);
        self
    }

    /// Get the entity ID
    pub fn id(self) -> Entity {
        self.entity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_spawn_despawn() {
        let mut manager = EntityManager::new();

        let e1 = manager.spawn();
        let e2 = manager.spawn();

        assert!(manager.exists(e1));
        assert!(manager.exists(e2));
        assert_eq!(manager.len(), 2);

        manager.despawn(e1);
        assert!(!manager.exists(e1));
        assert!(manager.exists(e2));
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_entity_hierarchy() {
        let mut manager = EntityManager::new();

        let parent = manager.spawn();
        let child1 = manager.spawn();
        let child2 = manager.spawn();

        manager.set_parent(child1, parent);
        manager.set_parent(child2, parent);

        assert_eq!(manager.parent(child1), Some(parent));
        assert_eq!(manager.parent(child2), Some(parent));

        let children = manager.children(parent).unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.contains(&child1));
        assert!(children.contains(&child2));
    }

    #[test]
    fn test_recursive_despawn() {
        let mut manager = EntityManager::new();

        let parent = manager.spawn();
        let child = manager.spawn();
        let grandchild = manager.spawn();

        manager.set_parent(child, parent);
        manager.set_parent(grandchild, child);

        assert_eq!(manager.len(), 3);

        manager.despawn(parent);

        assert_eq!(manager.len(), 0);
        assert!(!manager.exists(parent));
        assert!(!manager.exists(child));
        assert!(!manager.exists(grandchild));
    }
}
