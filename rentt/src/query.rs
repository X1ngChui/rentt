#![allow(dead_code)]

use std::marker::PhantomData;

use crate::{component::Component, entity::Entity};

/// A trait for querying entities and their components in a world.
///
/// This trait provides methods to iterate over entities with specific components and
/// their associated data. Implementors must define an associated entity type `Entt`.
pub trait Queryable {
    /// The entity type associated with this queryable instance.
    type Entt: Entity;

    /// Returns the number of entities with the specified component `T`.
    ///
    /// # Type Parameters
    /// - `T`: The component type to query.
    ///
    /// # Returns
    /// The number of entities that have the component `T`.
    fn len<T: Component>(&mut self) -> usize;

    /// Returns an iterator over entities and their immutable component references.
    ///
    /// # Type Parameters
    /// - `T`: The component type to query.
    ///
    /// # Returns
    /// An iterator yielding tuples of `(Entt, &T)` for each entity with component `T`.
    fn iter<T: Component>(&mut self) -> impl Iterator<Item = (Self::Entt, &T)>;

    /// Returns an iterator over entities and their mutable component references.
    ///
    /// # Type Parameters
    /// - `T`: The component type to query.
    ///
    /// # Returns
    /// An iterator yielding tuples of `(Entt, &mut T)` for each entity with component `T`.
    fn iter_mut<T: Component>(&mut self) -> impl Iterator<Item = (Self::Entt, &mut T)>;
}

/// A trait for fetching component data from a queryable world.
///
/// This trait defines how to iterate over component data of a specific type
/// from a world that implements `Queryable`.
///
/// # Type Parameters
/// - `'w`: The lifetime of the world and fetched items.
/// - `W`: The queryable world type.
pub trait Fetch<'w, W: Queryable> {
    /// The type of the fetched item, tied to the lifetime `'w`.
    type Item: 'w;

    /// Returns an iterator over entities and their fetched items.
    ///
    /// # Parameters
    /// - `world`: A mutable reference to the queryable world.
    ///
    /// # Returns
    /// An iterator yielding tuples of `(W::Entt, Self::Item)` for each matching entity.
    fn iter(world: &'w mut W) -> impl Iterator<Item = (W::Entt, Self::Item)>;
}

/// Implementation of `Fetch` for immutable component references.
///
/// Allows querying immutable references to components of type `T`.
impl<'w, W: Queryable, T: Component> Fetch<'w, W> for &'w T {
    type Item = &'w T;

    /// Iterates over entities with immutable component references.
    ///
    /// Delegates to the world's `iter` method for the component type `T`.
    fn iter(world: &'w mut W) -> impl Iterator<Item = (W::Entt, Self::Item)> {
        world.iter::<T>()
    }
}

/// Implementation of `Fetch` for mutable component references.
///
/// Allows querying mutable references to components of type `T`.
impl<'w, W: Queryable, T: Component> Fetch<'w, W> for &'w mut T {
    type Item = &'w mut T;

    /// Iterates over entities with mutable component references.
    ///
    /// Delegates to the world's `iter_mut` method for the component type `T`.
    fn iter(world: &'w mut W) -> impl Iterator<Item = (W::Entt, Self::Item)> {
        world.iter_mut::<T>()
    }
}

/// A query for fetching component data from a world.
///
/// This struct encapsulates a query for a specific component type `T`
/// from a world `W`. It uses `PhantomData` to track the component type
/// without storing any actual data of that type.
///
/// # Type Parameters
/// - `'w`: The lifetime of the world reference.
/// - `W`: The queryable world type.
/// - `T`: The type of data to fetch (e.g., `&T` or `&mut T`).
pub struct Query<'w, W: Queryable, T> {
    world: &'w mut W,
    _phantom: PhantomData<T>,
}

impl<'w, W: Queryable, T: Fetch<'w, W>> Query<'w, W, T> {
    /// Creates a new query for the given world.
    ///
    /// # Parameters
    /// - `world`: A mutable reference to the queryable world.
    ///
    /// # Returns
    /// A new `Query` instance for fetching data of type `T`.
    pub fn new(world: &'w mut W) -> Self {
        Self {
            world,
            _phantom: PhantomData,
        }
    }

    /// Returns an iterator over the query results.
    ///
    /// # Returns
    /// An iterator yielding tuples of `(W::Entt, T::Item)` for each matching entity.
    pub fn iter(&'w mut self) -> impl Iterator<Item = (W::Entt, T::Item)> {
        T::iter(self.world)
    }
}
