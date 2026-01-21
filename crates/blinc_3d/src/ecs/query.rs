//! Query system for iterating over entities with specific components

use super::{Component, Entity, World};
use std::marker::PhantomData;

/// Trait for query parameter types
pub trait WorldQuery {
    /// The item type returned by the query
    type Item<'a>;

    /// Check if an entity matches this query in the world
    fn matches(world: &World, entity: Entity) -> bool;

    /// Fetch the item for an entity
    fn fetch<'a>(world: &'a World, entity: Entity) -> Option<Self::Item<'a>>;
}

/// Query builder for type-safe component access
pub struct Query<'w, Q: WorldQuery> {
    world: &'w World,
    _marker: PhantomData<Q>,
}

impl<'w, Q: WorldQuery> Query<'w, Q> {
    /// Create a new query
    pub(crate) fn new(world: &'w World) -> Self {
        Self {
            world,
            _marker: PhantomData,
        }
    }

    /// Iterate over all matching entities
    pub fn iter(&self) -> QueryIter<'w, Q> {
        QueryIter {
            world: self.world,
            entities: self.world.entities.iter().collect::<Vec<_>>().into_iter(),
            _marker: PhantomData,
        }
    }

    /// Get a single entity's components
    pub fn get(&self, entity: Entity) -> Option<Q::Item<'w>> {
        if Q::matches(self.world, entity) {
            Q::fetch(self.world, entity)
        } else {
            None
        }
    }
}

impl<'w, Q: WorldQuery> IntoIterator for Query<'w, Q> {
    type Item = (Entity, Q::Item<'w>);
    type IntoIter = QueryIter<'w, Q>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over query results
pub struct QueryIter<'w, Q: WorldQuery> {
    world: &'w World,
    entities: std::vec::IntoIter<Entity>,
    _marker: PhantomData<Q>,
}

impl<'w, Q: WorldQuery> Iterator for QueryIter<'w, Q> {
    type Item = (Entity, Q::Item<'w>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entity = self.entities.next()?;
            if Q::matches(self.world, entity) {
                if let Some(item) = Q::fetch(self.world, entity) {
                    return Some((entity, item));
                }
            }
        }
    }
}

// Implement WorldQuery for single component reference
impl<T: Component> WorldQuery for &T {
    type Item<'a> = &'a T;

    fn matches(world: &World, entity: Entity) -> bool {
        world.has::<T>(entity)
    }

    fn fetch<'a>(world: &'a World, entity: Entity) -> Option<Self::Item<'a>> {
        world.get::<T>(entity)
    }
}

// Implement WorldQuery for tuples (up to 4 components)
impl<A: WorldQuery> WorldQuery for (A,) {
    type Item<'a> = (A::Item<'a>,);

    fn matches(world: &World, entity: Entity) -> bool {
        A::matches(world, entity)
    }

    fn fetch<'a>(world: &'a World, entity: Entity) -> Option<Self::Item<'a>> {
        Some((A::fetch(world, entity)?,))
    }
}

impl<A: WorldQuery, B: WorldQuery> WorldQuery for (A, B) {
    type Item<'a> = (A::Item<'a>, B::Item<'a>);

    fn matches(world: &World, entity: Entity) -> bool {
        A::matches(world, entity) && B::matches(world, entity)
    }

    fn fetch<'a>(world: &'a World, entity: Entity) -> Option<Self::Item<'a>> {
        Some((A::fetch(world, entity)?, B::fetch(world, entity)?))
    }
}

impl<A: WorldQuery, B: WorldQuery, C: WorldQuery> WorldQuery for (A, B, C) {
    type Item<'a> = (A::Item<'a>, B::Item<'a>, C::Item<'a>);

    fn matches(world: &World, entity: Entity) -> bool {
        A::matches(world, entity) && B::matches(world, entity) && C::matches(world, entity)
    }

    fn fetch<'a>(world: &'a World, entity: Entity) -> Option<Self::Item<'a>> {
        Some((
            A::fetch(world, entity)?,
            B::fetch(world, entity)?,
            C::fetch(world, entity)?,
        ))
    }
}

impl<A: WorldQuery, B: WorldQuery, C: WorldQuery, D: WorldQuery> WorldQuery for (A, B, C, D) {
    type Item<'a> = (A::Item<'a>, B::Item<'a>, C::Item<'a>, D::Item<'a>);

    fn matches(world: &World, entity: Entity) -> bool {
        A::matches(world, entity)
            && B::matches(world, entity)
            && C::matches(world, entity)
            && D::matches(world, entity)
    }

    fn fetch<'a>(world: &'a World, entity: Entity) -> Option<Self::Item<'a>> {
        Some((
            A::fetch(world, entity)?,
            B::fetch(world, entity)?,
            C::fetch(world, entity)?,
            D::fetch(world, entity)?,
        ))
    }
}

/// Optional component query wrapper
pub struct With<T>(PhantomData<T>);

impl<T: Component> WorldQuery for With<T> {
    type Item<'a> = ();

    fn matches(world: &World, entity: Entity) -> bool {
        world.has::<T>(entity)
    }

    fn fetch<'a>(_world: &'a World, _entity: Entity) -> Option<Self::Item<'a>> {
        Some(())
    }
}

/// Exclusion filter
pub struct Without<T>(PhantomData<T>);

impl<T: Component> WorldQuery for Without<T> {
    type Item<'a> = ();

    fn matches(world: &World, entity: Entity) -> bool {
        !world.has::<T>(entity)
    }

    fn fetch<'a>(_world: &'a World, _entity: Entity) -> Option<Self::Item<'a>> {
        Some(())
    }
}
