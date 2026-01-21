//! World container for all entities, components, and resources

use super::{
    Component, ComponentRegistry, DenseStorage, Entity, EntityBuilder, EntityManager, Query,
    SparseStorage, StorageType, WorldQuery,
};
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

/// Marker trait for resources (global state)
pub trait Resource: 'static + Send + Sync {}

// Implement Resource for common types
impl<T: 'static + Send + Sync> Resource for T {}

/// Storage for global resources
pub struct ResourceMap {
    resources: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Default for ResourceMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceMap {
    /// Create a new resource map
    pub fn new() -> Self {
        Self {
            resources: FxHashMap::default(),
        }
    }

    /// Insert a resource
    pub fn insert<R: Resource>(&mut self, resource: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(resource));
    }

    /// Get a resource reference
    pub fn get<R: Resource>(&self) -> Option<&R> {
        self.resources
            .get(&TypeId::of::<R>())
            .and_then(|r| r.downcast_ref())
    }

    /// Get a mutable resource reference
    pub fn get_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.resources
            .get_mut(&TypeId::of::<R>())
            .and_then(|r| r.downcast_mut())
    }

    /// Remove a resource
    pub fn remove<R: Resource>(&mut self) -> Option<R> {
        self.resources
            .remove(&TypeId::of::<R>())
            .and_then(|r| r.downcast().ok())
            .map(|r| *r)
    }

    /// Check if a resource exists
    pub fn contains<R: Resource>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<R>())
    }
}

/// The World contains all entities, components, and resources
pub struct World {
    /// Entity manager
    pub(crate) entities: EntityManager,
    /// Component storage registry
    components: ComponentRegistry,
    /// Global resources
    resources: ResourceMap,
    /// Geometry handles
    next_geometry_id: u64,
    /// Material handles
    next_material_id: u64,
    /// Texture handles
    next_texture_id: u64,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Create a new empty world
    pub fn new() -> Self {
        Self {
            entities: EntityManager::new(),
            components: ComponentRegistry::new(),
            resources: ResourceMap::new(),
            next_geometry_id: 1,
            next_material_id: 1,
            next_texture_id: 1,
        }
    }

    // === Entity Operations ===

    /// Spawn a new entity
    pub fn spawn(&mut self) -> EntityBuilder<'_> {
        let entity = self.entities.spawn();
        EntityBuilder::new(self, entity)
    }

    /// Despawn an entity and all its components
    pub fn despawn(&mut self, entity: Entity) {
        self.components.remove_all(entity);
        self.entities.despawn(entity);
    }

    /// Check if an entity exists
    pub fn exists(&self, entity: Entity) -> bool {
        self.entities.exists(entity)
    }

    /// Get entity count
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    // === Component Operations ===

    /// Insert a component for an entity
    pub fn insert<C: Component>(&mut self, entity: Entity, component: C) {
        // Ensure storage exists
        self.components.get_or_create::<C>();

        match C::STORAGE {
            StorageType::Dense => {
                if let Some(storage) = self.components.get_dense_mut::<C>() {
                    storage.insert(entity, component);
                }
            }
            StorageType::Sparse => {
                if let Some(storage) = self.components.get_sparse_mut::<C>() {
                    storage.insert(entity, component);
                }
            }
        }
    }

    /// Get a component reference
    pub fn get<C: Component>(&self, entity: Entity) -> Option<&C> {
        match C::STORAGE {
            StorageType::Dense => self.components.get_dense::<C>()?.get(entity),
            StorageType::Sparse => self.components.get_sparse::<C>()?.get(entity),
        }
    }

    /// Get a mutable component reference
    pub fn get_mut<C: Component>(&mut self, entity: Entity) -> Option<&mut C> {
        match C::STORAGE {
            StorageType::Dense => self.components.get_dense_mut::<C>()?.get_mut(entity),
            StorageType::Sparse => self.components.get_sparse_mut::<C>()?.get_mut(entity),
        }
    }

    /// Alias for get - get a component reference
    pub fn get_component<C: Component>(&self, entity: Entity) -> Option<&C> {
        self.get::<C>(entity)
    }

    /// Alias for get_mut - get a mutable component reference
    pub fn get_component_mut<C: Component>(&mut self, entity: Entity) -> Option<&mut C> {
        self.get_mut::<C>(entity)
    }

    /// Check if an entity has a component
    pub fn has<C: Component>(&self, entity: Entity) -> bool {
        self.components
            .get::<C>()
            .map(|s| s.contains(entity))
            .unwrap_or(false)
    }

    /// Remove a component from an entity
    pub fn remove<C: Component>(&mut self, entity: Entity) -> bool {
        self.components
            .get_mut::<C>()
            .map(|s| s.remove(entity))
            .unwrap_or(false)
    }

    // === Query Operations ===

    /// Query entities with specific components
    pub fn query<Q: WorldQuery>(&self) -> Query<'_, Q> {
        Query::new(self)
    }

    // === Resource Operations ===

    /// Get a resource
    pub fn resource<R: Resource>(&self) -> Option<&R> {
        self.resources.get::<R>()
    }

    /// Get a mutable resource
    pub fn resource_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.resources.get_mut::<R>()
    }

    /// Insert or update a resource
    pub fn insert_resource<R: Resource>(&mut self, resource: R) {
        self.resources.insert(resource);
    }

    /// Remove a resource
    pub fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.remove::<R>()
    }

    // === Handle Generation ===

    /// Add a geometry and get a handle
    pub fn add_geometry(&mut self, _geometry: super::super::geometry::Geometry) -> super::super::geometry::GeometryHandle {
        let id = self.next_geometry_id;
        self.next_geometry_id += 1;
        super::super::geometry::GeometryHandle(id)
    }

    /// Add a material and get a handle
    pub fn add_material<M: super::super::materials::Material + 'static>(&mut self, _material: M) -> super::super::materials::MaterialHandle {
        let id = self.next_material_id;
        self.next_material_id += 1;
        super::super::materials::MaterialHandle(id)
    }

    /// Add a texture and get a handle
    pub fn add_texture(&mut self) -> super::super::materials::TextureHandle {
        let id = self.next_texture_id;
        self.next_texture_id += 1;
        super::super::materials::TextureHandle(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    impl Component for Position {}

    #[derive(Clone, Debug, PartialEq)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    impl Component for Velocity {}

    #[test]
    fn test_world_spawn_despawn() {
        let mut world = World::new();

        let entity = world.spawn().id();
        assert!(world.exists(entity));
        assert_eq!(world.entity_count(), 1);

        world.despawn(entity);
        assert!(!world.exists(entity));
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn test_world_components() {
        let mut world = World::new();

        let entity = world.spawn().id();
        world.insert(entity, Position { x: 1.0, y: 2.0 });

        assert!(world.has::<Position>(entity));
        assert!(!world.has::<Velocity>(entity));

        let pos = world.get::<Position>(entity).unwrap();
        assert_eq!(pos.x, 1.0);
        assert_eq!(pos.y, 2.0);
    }

    #[test]
    fn test_world_resources() {
        let mut world = World::new();

        #[derive(Debug, PartialEq)]
        struct GameTime(f32);

        world.insert_resource(GameTime(0.0));

        assert!(world.resource::<GameTime>().is_some());
        assert_eq!(world.resource::<GameTime>().unwrap().0, 0.0);

        world.resource_mut::<GameTime>().unwrap().0 = 1.0;
        assert_eq!(world.resource::<GameTime>().unwrap().0, 1.0);
    }
}
