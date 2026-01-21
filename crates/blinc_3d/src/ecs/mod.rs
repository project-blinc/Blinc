//! Entity Component System (ECS)
//!
//! A lightweight ECS implementation optimized for 3D game development.
//!
//! ## Overview
//!
//! - **Entity**: A unique identifier for game objects
//! - **Component**: Data attached to entities (position, mesh, etc.)
//! - **System**: Logic that processes entities with specific components
//! - **World**: Container for all entities, components, and resources

mod component;
mod entity;
mod query;
mod schedule;
mod system;
mod world;

pub use component::{Component, ComponentRegistry, ComponentStorage, DenseStorage, SparseStorage, StorageType};
pub use entity::{Entity, EntityBuilder, EntityManager, EntityMeta};
pub use query::{Query, QueryIter, WorldQuery};
pub use schedule::{Schedule, ScheduleBuilder};
pub use system::{BoxedSystem, System, SystemContext, SystemStage};
pub use world::{Resource, ResourceMap, World};
