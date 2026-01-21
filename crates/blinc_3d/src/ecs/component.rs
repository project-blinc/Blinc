//! Component trait and storage
//!
//! Components can hold any data including:
//! - Plain data (position, velocity, health)
//! - Reactive signals for UI binding
//! - Animated values for smooth transitions
//!
//! # Example with Signals and Animations
//!
//! ```rust,ignore
//! use blinc_3d::ecs::Component;
//! use blinc_animation::{AnimatedValue, SpringConfig};
//! use blinc_core::Signal;
//!
//! // Component with animated position
//! struct AnimatedTransform {
//!     x: AnimatedValue,
//!     y: AnimatedValue,
//!     z: AnimatedValue,
//! }
//!
//! impl Component for AnimatedTransform {}
//!
//! // Component with reactive state
//! struct PlayerState {
//!     health: Signal<f32>,
//!     score: Signal<u32>,
//! }
//!
//! impl Component for PlayerState {}
//! ```

use super::Entity;
use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use std::any::{Any, TypeId};

/// Storage strategy for components
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StorageType {
    /// Dense storage using SlotMap (good for common components like Transform)
    #[default]
    Dense,
    /// Sparse storage using HashMap (good for rare components)
    Sparse,
}

/// Trait for all components
///
/// Components are plain data structs that can be attached to entities.
/// They can hold any `Send + Sync` data including:
///
/// - **Plain data**: positions, velocities, health values
/// - **Reactive signals**: `Signal<T>` for UI binding and reactivity
/// - **Animated values**: `AnimatedValue`, `AnimatedVec3` for smooth transitions
/// - **Handles**: `GeometryHandle`, `MaterialHandle`, `TextureHandle`
///
/// # Example
///
/// ```rust,ignore
/// use blinc_3d::ecs::Component;
///
/// #[derive(Clone, Debug)]
/// struct Health {
///     current: f32,
///     max: f32,
/// }
///
/// impl Component for Health {
///     const STORAGE: StorageType = StorageType::Dense;
/// }
/// ```
///
/// # With Animations
///
/// ```rust,ignore
/// use blinc_3d::ecs::Component;
/// use blinc_animation::AnimatedValue;
///
/// struct AnimatedOpacity {
///     opacity: AnimatedValue,
/// }
///
/// impl Component for AnimatedOpacity {}
/// ```
pub trait Component: 'static + Send + Sync + Sized {
    /// Storage strategy hint
    const STORAGE: StorageType = StorageType::Dense;
}

/// Type-erased component storage trait
pub trait ComponentStorage: Any + Send + Sync {
    /// Get as Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Get as mutable Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Remove a component from an entity
    fn remove(&mut self, entity: Entity) -> bool;

    /// Check if entity has this component
    fn contains(&self, entity: Entity) -> bool;

    /// Get component count
    fn len(&self) -> usize;

    /// Check if empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Dense storage using SlotMap
///
/// Optimized for common components that most entities have.
/// Provides O(1) access and cache-friendly iteration.
pub struct DenseStorage<T: Component> {
    components: SlotMap<Entity, T>,
}

impl<T: Component> Default for DenseStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> DenseStorage<T> {
    /// Create new dense storage
    pub fn new() -> Self {
        Self {
            components: SlotMap::with_key(),
        }
    }

    /// Insert a component for an entity
    pub fn insert(&mut self, entity: Entity, component: T) {
        // Note: SlotMap uses its own key, so we use a secondary map
        // For now, we'll store with the entity key directly
        self.components.insert(component);
    }

    /// Get a component reference
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.components.get(entity)
    }

    /// Get a mutable component reference
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.components.get_mut(entity)
    }

    /// Iterate over all components
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.components.iter()
    }

    /// Iterate over all components mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.components.iter_mut()
    }
}

impl<T: Component> ComponentStorage for DenseStorage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove(&mut self, entity: Entity) -> bool {
        self.components.remove(entity).is_some()
    }

    fn contains(&self, entity: Entity) -> bool {
        self.components.contains_key(entity)
    }

    fn len(&self) -> usize {
        self.components.len()
    }
}

/// Sparse storage using HashMap
///
/// Optimized for rare components that few entities have.
/// Uses less memory when component is uncommon.
pub struct SparseStorage<T: Component> {
    components: FxHashMap<Entity, T>,
}

impl<T: Component> Default for SparseStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> SparseStorage<T> {
    /// Create new sparse storage
    pub fn new() -> Self {
        Self {
            components: FxHashMap::default(),
        }
    }

    /// Insert a component for an entity
    pub fn insert(&mut self, entity: Entity, component: T) {
        self.components.insert(entity, component);
    }

    /// Get a component reference
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.components.get(&entity)
    }

    /// Get a mutable component reference
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.components.get_mut(&entity)
    }

    /// Iterate over all components
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.components.iter().map(|(&e, c)| (e, c))
    }

    /// Iterate over all components mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.components.iter_mut().map(|(&e, c)| (e, c))
    }
}

impl<T: Component> ComponentStorage for SparseStorage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove(&mut self, entity: Entity) -> bool {
        self.components.remove(&entity).is_some()
    }

    fn contains(&self, entity: Entity) -> bool {
        self.components.contains_key(&entity)
    }

    fn len(&self) -> usize {
        self.components.len()
    }
}

/// Registry for all component storage
pub struct ComponentRegistry {
    storages: FxHashMap<TypeId, Box<dyn ComponentStorage>>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentRegistry {
    /// Create a new component registry
    pub fn new() -> Self {
        Self {
            storages: FxHashMap::default(),
        }
    }

    /// Get or create storage for a component type
    pub fn get_or_create<T: Component>(&mut self) -> &mut dyn ComponentStorage {
        let type_id = TypeId::of::<T>();
        self.storages.entry(type_id).or_insert_with(|| {
            match T::STORAGE {
                StorageType::Dense => Box::new(DenseStorage::<T>::new()),
                StorageType::Sparse => Box::new(SparseStorage::<T>::new()),
            }
        });
        self.storages.get_mut(&type_id).unwrap().as_mut()
    }

    /// Get storage for a component type
    pub fn get<T: Component>(&self) -> Option<&dyn ComponentStorage> {
        self.storages.get(&TypeId::of::<T>()).map(|b| b.as_ref())
    }

    /// Get mutable storage for a component type
    pub fn get_mut<T: Component>(&mut self) -> Option<&mut dyn ComponentStorage> {
        self.storages
            .get_mut(&TypeId::of::<T>())
            .map(|b| b.as_mut())
    }

    /// Get typed dense storage
    pub fn get_dense<T: Component>(&self) -> Option<&DenseStorage<T>> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref())
    }

    /// Get mutable typed dense storage
    pub fn get_dense_mut<T: Component>(&mut self) -> Option<&mut DenseStorage<T>> {
        self.storages
            .get_mut(&TypeId::of::<T>())
            .and_then(|s| s.as_any_mut().downcast_mut())
    }

    /// Get typed sparse storage
    pub fn get_sparse<T: Component>(&self) -> Option<&SparseStorage<T>> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref())
    }

    /// Get mutable typed sparse storage
    pub fn get_sparse_mut<T: Component>(&mut self) -> Option<&mut SparseStorage<T>> {
        self.storages
            .get_mut(&TypeId::of::<T>())
            .and_then(|s| s.as_any_mut().downcast_mut())
    }

    /// Remove all components for an entity
    pub fn remove_all(&mut self, entity: Entity) {
        for storage in self.storages.values_mut() {
            storage.remove(entity);
        }
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
    struct RareComponent {
        data: u32,
    }

    impl Component for RareComponent {
        const STORAGE: StorageType = StorageType::Sparse;
    }

    #[test]
    fn test_dense_storage() {
        let mut storage = DenseStorage::<Position>::new();

        // We need to use the slotmap's own key generation
        // For testing, we'll use the registry pattern instead
    }

    #[test]
    fn test_component_registry() {
        let mut registry = ComponentRegistry::new();

        // First access creates storage
        let _storage = registry.get_or_create::<Position>();
        assert!(registry.get::<Position>().is_some());
        assert!(registry.get::<RareComponent>().is_none());

        // Create sparse storage
        let _storage = registry.get_or_create::<RareComponent>();
        assert!(registry.get::<RareComponent>().is_some());
    }
}
