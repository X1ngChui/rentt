//! A specialized map that associates entities with values.
//!
//! `EntityMap` is a `HashMap`-like structure that uses entity IDs as keys.
//! It is typically used in ECS-like systems where entities are represented by compact identifiers,
//! and values are stored or updated independently.
//!
//! This map provides standard operations such as insertion, removal, lookup, and iteration,
//! while automatically handling the entity keys in a type-safe way.
//!
//! # Examples
//!
//! ```ignore
//! let mut map = EntityMap::<Entity32, i32>::new();
//! let e1 = ...;
//! let e2 = ...;
//!
//! assert_eq!(map.insert(e1, 42), None);
//! assert_eq!(map.insert(e2, 100), None);
//!
//! assert_eq!(map.get(e1), Some(&42));
//! assert_eq!(map.get(e2), Some(&100));
//!
//! assert_eq!(map.remove(e1), Some(42));
//! assert_eq!(map.get(e1), None);
//! ```
//!
//! # Features
//!
//! - Type-safe entity keying
//! - Efficient insert, remove, and lookup
//! - Entity and value iteration support (`entities()`, `values()`, `values_mut()`, `iter()`)
//!
//! # Type Parameters
//!
//! * `K`: Entity type, typically `DefaultEntity`.
//! * `V`: Value type to be stored.

use crate::{entity::Entity, entity_fields::EntityId};
use std::{hint::unreachable_unchecked, mem::replace, ptr};

/// Removes element at `index` from `vec` by swapping it with the last element,
/// and returning the removed value.
///
/// # Safety
///
/// Caller must ensure that `index` is within bounds of `vec`.
#[inline]
unsafe fn swap_remove_unchecked<T>(vec: &mut Vec<T>, index: usize) -> T {
    let len = vec.len();
    debug_assert!(index < len);
    unsafe {
        let value = ptr::read(vec.as_ptr().add(index));
        let base_ptr = vec.as_mut_ptr();
        ptr::copy(base_ptr.add(len - 1), base_ptr.add(index), 1);
        vec.set_len(len - 1);
        value
    }
}

/// A fixed-size sparse set pool data structure.
///
/// This structure maps external entity IDs to internal storage slots.
/// Internally uses sparse/dense representation for O(1) insertion/removal/lookup.
#[derive(Debug)]
struct Pool<T, I: EntityId> {
    /// Sparse mapping: entity index => dense array index
    indices: Box<[Option<I>]>,
    /// Dense array of (entity id, value) pairs
    values: Vec<(I, T)>,
}

#[allow(dead_code)]
impl<T, I: EntityId> Pool<T, I> {
    /// Creates a new pool with the given capacity.
    ///
    /// The `capacity` argument specifies the maximum number of entity IDs that can be handled.
    fn new(capacity: usize) -> Self {
        Self {
            indices: vec![None; capacity].into_boxed_slice(),
            values: Vec::new(),
        }
    }

    /// Returns the number of stored elements.
    #[inline]
    fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns the capacity of the pool.
    #[inline]
    fn capacity(&self) -> usize {
        self.indices.len()
    }

    /// Returns `true` if the pool is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns `true` if the pool is full.
    #[inline]
    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Checks whether the given entity ID exists.
    #[inline]
    fn contains(&self, index: I) -> bool {
        if index.into_index() >= self.capacity() {
            false
        } else {
            unsafe { self.contains_unchecked(index) }
        }
    }

    /// Checks for entity existence without bounds check.
    ///
    /// # Safety
    ///
    /// Caller must ensure `index` is within bounds of `indices`.
    #[inline]
    unsafe fn contains_unchecked(&self, index: I) -> bool {
        debug_assert!(index.into_index() < self.capacity());
        match unsafe { self.indices.get_unchecked(index.into_index()) } {
            None => false,
            Some(_) => true,
        }
    }

    /// Gets immutable reference to value by entity id.
    #[inline]
    fn get(&self, index: I) -> Option<&T> {
        if self.contains(index) {
            Some(unsafe { self.get_unchecked(index) })
        } else {
            None
        }
    }

    /// Gets value without safety checks.
    ///
    /// # Safety
    ///
    /// Caller must ensure the entity exists.
    #[inline]
    unsafe fn get_unchecked(&self, index: I) -> &T {
        debug_assert!(unsafe { self.contains_unchecked(index) });

        let dense_index = match unsafe { self.indices.get_unchecked(index.into_index()) } {
            None => unsafe { unreachable_unchecked() },
            Some(i) => *i,
        };

        unsafe { &self.values.get_unchecked(dense_index.into_index()).1 }
    }

    /// Gets mutable reference to value by entity id.
    #[inline]
    fn get_mut(&mut self, index: I) -> Option<&mut T> {
        if self.contains(index) {
            Some(unsafe { self.get_unchecked_mut(index) })
        } else {
            None
        }
    }

    /// Gets value without safety checks.
    ///
    /// # Safety
    ///
    /// Caller must ensure the entity exists.
    #[inline]
    unsafe fn get_unchecked_mut(&mut self, index: I) -> &mut T {
        debug_assert!(unsafe { self.contains_unchecked(index) });

        let dense_index = match unsafe { self.indices.get_unchecked(index.into_index()) } {
            None => unsafe { unreachable_unchecked() },
            Some(i) => *i,
        };

        unsafe { &mut self.values.get_unchecked_mut(dense_index.into_index()).1 }
    }

    /// Inserts new entity and value, returning old value if it exists.
    ///
    /// # Safety
    ///
    /// Caller must ensure that the pool is not full and `index` is within valid range.
    unsafe fn insert(&mut self, index: I, value: T) -> Option<T> {
        debug_assert!(!self.is_full());
        debug_assert!(index.into_index() < self.capacity());

        match self.get_mut(index) {
            None => {
                let value_index = unsafe { I::from_index(self.values.len()) };
                self.values.push((index, value));
                unsafe {
                    *self.indices.get_unchecked_mut(index.into_index()) = Some(value_index);
                }

                None
            }
            Some(old) => Some(replace(old, value)),
        }
    }

    /// Removes entity and returns its value if exists.
    ///
    /// # Safety
    ///
    /// Caller must ensure that `index` is within valid range.
    unsafe fn remove(&mut self, index: I) -> Option<T> {
        debug_assert!(index.into_index() < self.capacity());

        let dense_target_slot = match *unsafe { self.indices.get_unchecked(index.into_index()) } {
            Some(i) => i,
            None => return None,
        };

        let dense_last_slot = self.values.len() - 1;
        let dense_last_entity = unsafe { self.values.get_unchecked(dense_last_slot) }.0;

        let (removed_entity, removed_value) =
            unsafe { swap_remove_unchecked(&mut self.values, dense_target_slot.into_index()) };

        debug_assert!(removed_entity == index);

        unsafe {
            *self
                .indices
                .get_unchecked_mut(dense_last_entity.into_index()) = Some(dense_target_slot);
            *self.indices.get_unchecked_mut(index.into_index()) = None;
        }

        Some(removed_value)
    }

    /// Immutable iterator over values only.
    fn iter(&self) -> impl Iterator<Item = &T> {
        self.values.iter().map(|(_, v)| v)
    }

    /// Mutable iterator over values only.
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.values.iter_mut().map(|(_, v)| v)
    }
}

#[cfg(test)]
mod pool_tests {
    use super::*;
    use crate::entity_fields::Id24;

    fn id(n: u32) -> Id24 {
        Id24::new(n).unwrap()
    }

    #[test]
    fn test_single_insert_remove() {
        let mut pool = Pool::<i32, Id24>::new(10);

        unsafe {
            assert_eq!(pool.insert(id(3), 99), None);
            assert_eq!(pool.get(id(3)), Some(&99));
            assert_eq!(pool.remove(id(3)), Some(99));
            assert_eq!(pool.get(id(3)), None);
        }

        assert!(pool.is_empty());
    }

    #[test]
    fn test_multiple_insert_remove() {
        let mut pool = Pool::<i32, Id24>::new(10);

        unsafe {
            for i in 0..10 {
                assert_eq!(pool.insert(id(i), i as i32), None);
            }

            for i in 0..10 {
                assert_eq!(pool.get(id(i)), Some(&(i as i32)));
            }

            for i in 0..10 {
                assert_eq!(pool.remove(id(i)), Some(i as i32));
            }
        }

        assert!(pool.is_empty());
    }

    #[test]
    fn test_full_capacity() {
        let mut pool = Pool::<i32, Id24>::new(5);

        unsafe {
            for i in 0..5 {
                assert_eq!(pool.insert(id(i), i as i32), None);
            }
        }

        assert!(pool.is_full());
        assert_eq!(pool.len(), 5);
    }

    #[test]
    fn test_mixed_insert_remove() {
        let mut pool = Pool::<i32, Id24>::new(10);

        unsafe {
            assert_eq!(pool.insert(id(1), 11), None);
            assert_eq!(pool.insert(id(2), 22), None);
            assert_eq!(pool.remove(id(1)), Some(11));
            assert_eq!(pool.insert(id(3), 33), None);
            assert_eq!(pool.insert(id(2), 222), Some(22));
            assert_eq!(pool.get(id(3)), Some(&33));
            assert_eq!(pool.get(id(2)), Some(&222));
            assert_eq!(pool.get(id(1)), None);
        }
    }

    #[test]
    fn test_is_empty_and_is_full() {
        let mut pool = Pool::<i32, Id24>::new(2);

        assert!(pool.is_empty());
        assert!(!pool.is_full());

        unsafe {
            pool.insert(id(0), 10);
        }

        assert!(!pool.is_empty());
        assert!(!pool.is_full());

        unsafe {
            pool.insert(id(1), 20);
        }

        assert!(pool.is_full());
    }

    #[test]
    fn test_contains() {
        let mut pool = Pool::<i32, Id24>::new(10);

        assert!(!pool.contains(id(5)));

        unsafe {
            pool.insert(id(5), 55);
        }

        assert!(pool.contains(id(5)));

        unsafe {
            pool.remove(id(5));
        }

        assert!(!pool.contains(id(5)));
    }

    #[test]
    fn test_iter() {
        let mut pool = Pool::<i32, Id24>::new(5);

        unsafe {
            pool.insert(id(0), 100);
            pool.insert(id(1), 200);
            pool.insert(id(2), 300);
        }

        let values: Vec<_> = pool.iter().cloned().collect();
        assert_eq!(values.len(), 3);
        assert!(values.contains(&100));
        assert!(values.contains(&200));
        assert!(values.contains(&300));
    }

    #[test]
    fn test_iter_mut() {
        let mut pool = Pool::<i32, Id24>::new(5);

        unsafe {
            pool.insert(id(0), 1);
            pool.insert(id(1), 2);
            pool.insert(id(2), 3);
        }

        for v in pool.iter_mut() {
            *v *= 10;
        }

        let values: Vec<_> = pool.iter().cloned().collect();
        assert!(values.contains(&10));
        assert!(values.contains(&20));
        assert!(values.contains(&30));
    }
}

/// Calculates block size based on value size for internal pool partitioning.
const fn block_size(size: usize) -> usize {
    const PAGE_SIZE: usize = 4096;
    const MIN_BLOCK_SIZE: usize = 8;
    const MAX_BLOCK_SIZE: usize = 64;

    let block_size = (PAGE_SIZE / size).next_power_of_two();
    if block_size < MIN_BLOCK_SIZE {
        MIN_BLOCK_SIZE
    } else if block_size > MAX_BLOCK_SIZE {
        MAX_BLOCK_SIZE
    } else {
        block_size
    }
}

/// A dynamic entity map structure for sparse entity-component mappings.
///
/// Internally uses multiple `Pool`s for efficient sparse storage.
pub struct EntityMap<E: Entity, T> {
    pools: Vec<Pool<(E, T), E::Id>>,
    len: usize,
}

impl<E: Entity, T> EntityMap<E, T> {
    const BLOCK_SIZE: usize = block_size(std::mem::size_of::<(E, T)>());

    #[inline]
    fn locate(entity: &E) -> (usize, E::Id) {
        let id = entity.id().into_index();
        (id / Self::BLOCK_SIZE, unsafe {
            E::Id::from_index(id % Self::BLOCK_SIZE)
        })
    }

    /// Creates a new empty entity map.
    pub fn new() -> Self {
        Self {
            pools: Vec::new(),
            len: 0,
        }
    }

    /// Returns the number of stored entities.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the map is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks whether entity exists in map.
    pub fn contains(&self, entity: E) -> bool {
        let (index, offset) = Self::locate(&entity);
        let found_pool = self.pools.get(index);

        match found_pool {
            None => false,
            Some(pool) => unsafe { pool.contains_unchecked(offset) },
        }
    }

    /// Gets immutable reference to component by entity.
    pub fn get(&self, entity: E) -> Option<&T> {
        if self.contains(entity) {
            Some(unsafe { self.get_unchecked(entity) })
        } else {
            None
        }
    }

    /// Gets immutable reference without safety checks.
    ///
    /// # Safety
    ///
    /// Caller must ensure entity exists.
    pub unsafe fn get_unchecked(&self, entity: E) -> &T {
        debug_assert!(self.contains(entity));

        let (index, offset) = Self::locate(&entity);
        let found_pool = self.pools.get(index);

        match found_pool {
            None => unsafe { unreachable_unchecked() },
            Some(pool) => unsafe { &pool.get_unchecked(offset).1 },
        }
    }

    /// Gets mutable reference to component by entity.
    pub fn get_mut(&mut self, entity: E) -> Option<&mut T> {
        if self.contains(entity) {
            Some(unsafe { self.get_unchecked_mut(entity) })
        } else {
            None
        }
    }

    /// Gets mutable reference without safety checks.
    ///
    /// # Safety
    ///
    /// Caller must ensure entity exists.
    pub unsafe fn get_unchecked_mut(&mut self, entity: E) -> &mut T {
        debug_assert!(self.contains(entity));

        let (index, offset) = Self::locate(&entity);
        let found_pool = self.pools.get_mut(index);

        match found_pool {
            None => unsafe { unreachable_unchecked() },
            Some(pool) => unsafe { &mut pool.get_unchecked_mut(offset).1 },
        }
    }

    /// Inserts entity and component.
    ///
    /// Returns old component if entity already exists.
    pub fn insert(&mut self, entity: E, value: T) -> Option<T> {
        let (index, offset) = Self::locate(&entity);

        let ret = match self.pools.get_mut(index) {
            Some(pool) => unsafe { pool.insert(offset, (entity, value)) },
            None => {
                self.pools
                    .resize_with(index + 1, || Pool::new(Self::BLOCK_SIZE));
                let pool = unsafe { self.pools.get_unchecked_mut(index) };
                unsafe { pool.insert(offset, (entity, value)) }
            }
        };

        match ret {
            Some((old_entity, old_value)) => {
                debug_assert!(entity == old_entity);
                Some(old_value)
            }
            None => {
                self.len += 1;
                None
            }
        }
    }

    /// Removes entity and returns component if exists.
    pub fn remove(&mut self, entity: E) -> Option<T> {
        let (index, offset) = Self::locate(&entity);
        let found_pool = self.pools.get_mut(index);

        match found_pool {
            None => None,
            Some(pool) => {
                let res = unsafe { pool.remove(offset) };
                if res.is_some() {
                    self.len -= 1;
                }
                res.map(|(_, v)| v)
            }
        }
    }

    /// Immutable iterator over all `(entity, component)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (E, &T)> {
        self.pools
            .iter()
            .flat_map(|pool| pool.iter().map(|(e, v)| (*e, v)))
    }

    /// Mutable iterator over all `(entity, component)` pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (E, &mut T)> {
        self.pools
            .iter_mut()
            .flat_map(|pool| pool.iter_mut().map(|(e, v)| (*e, v)))
    }

    /// Immutable iterator over entities only.
    pub fn entities(&self) -> impl Iterator<Item = E> {
        self.pools
            .iter()
            .flat_map(|pool| pool.iter().map(|(e, _)| *e))
    }

    /// Immutable iterator over values only.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.pools
            .iter()
            .flat_map(|pool| pool.iter().map(|(_, v)| v))
    }

    /// Mutable iterator over values only.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.pools
            .iter_mut()
            .flat_map(|pool| pool.iter_mut().map(|(_, v)| v))
    }
}

#[cfg(test)]
mod entity_map_tests {
    use super::*;
    use crate::entity::Entity32;
    use crate::entity_fields::Id24;

    fn id(n: u32) -> Id24 {
        Id24::new(n).unwrap()
    }

    #[test]
    fn test_single_insert_remove() {
        let mut map = EntityMap::<Entity32, i32>::new();
        let e = Entity32::new(id(1));

        assert_eq!(map.insert(e, 42), None);
        assert_eq!(map.get(e), Some(&42));
        assert_eq!(map.remove(e), Some(42));
        assert_eq!(map.get(e), None);
    }

    #[test]
    fn test_multiple_insert_remove() {
        let mut map = EntityMap::<Entity32, i32>::new();

        for i in 0..10 {
            let e = Entity32::new(id(i));
            assert_eq!(map.insert(e, i as i32), None);
        }

        for i in 0..10 {
            let e = Entity32::new(id(i));
            assert_eq!(map.get(e), Some(&(i as i32)));
            assert_eq!(map.remove(e), Some(i as i32));
        }

        assert!(map.is_empty());
    }

    #[test]
    fn test_mixed_insert_remove() {
        let mut map = EntityMap::<Entity32, i32>::new();

        for i in 0..5 {
            let e = Entity32::new(id(i));
            assert_eq!(map.insert(e, i as i32 * 10), None);
        }

        let e2 = Entity32::new(id(2));
        let e4 = Entity32::new(id(4));
        assert_eq!(map.remove(e2), Some(20));
        assert_eq!(map.remove(e4), Some(40));

        let e5 = Entity32::new(id(5));
        assert_eq!(map.insert(e5, 50), None);

        assert_eq!(map.get(e5), Some(&50));
        assert_eq!(map.get(e2), None);
    }

    #[test]
    fn test_contains_len_is_empty() {
        let mut map = EntityMap::<Entity32, i32>::new();
        let e = Entity32::new(id(42));

        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
        assert!(!map.contains(e));

        map.insert(e, 123);
        assert!(map.contains(e));
        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_iter() {
        let mut map = EntityMap::<Entity32, i32>::new();

        for i in 0..3 {
            let e = Entity32::new(id(i));
            map.insert(e, i as i32 * 10);
        }

        let mut keys = vec![];
        let mut values = vec![];

        for (k, v) in map.iter() {
            keys.push(k.id().into_index());
            values.push(*v);
        }

        keys.sort();
        values.sort();

        assert_eq!(keys, vec![0, 1, 2]);
        assert_eq!(values, vec![0, 10, 20]);
    }

    #[test]
    fn test_entities() {
        let mut map = EntityMap::<Entity32, i32>::new();
        let ents: Vec<_> = (0..5).map(|i| Entity32::new(id(i))).collect();

        for e in &ents {
            map.insert(*e, e.id().into_index() as i32);
        }

        let mut collected: Vec<_> = map.entities().collect();
        collected.sort_by_key(|e| e.id().into_index());
        assert_eq!(collected, ents);
    }

    #[test]
    fn test_values() {
        let mut map = EntityMap::<Entity32, i32>::new();

        for i in 0..3 {
            map.insert(Entity32::new(id(i)), ((i + 1) * 100) as i32);
        }

        let mut values: Vec<_> = map.values().cloned().collect();
        values.sort();
        assert_eq!(values, vec![100, 200, 300]);
    }

    #[test]
    fn test_values_mut() {
        let mut map = EntityMap::<Entity32, i32>::new();

        for i in 0..3 {
            map.insert(Entity32::new(id(i)), i as i32);
        }

        for v in map.values_mut() {
            *v += 10;
        }

        let values: Vec<_> = map.values().cloned().collect();
        assert!(values.contains(&10));
        assert!(values.contains(&11));
        assert!(values.contains(&12));
    }

    const STRIDE: u32 = 1024;

    #[test]
    fn test_insert_multiple_pools() {
        let mut map = EntityMap::<Entity32, i32>::new();

        for i in 0..5 {
            let ent = Entity32::new(id(i * STRIDE));
            assert_eq!(map.insert(ent, i as i32), None);
        }

        for i in 0..5 {
            let ent = Entity32::new(id(i * STRIDE));
            assert_eq!(map.get(ent), Some(&(i as i32)));
        }

        for i in 0..5 {
            let ent = Entity32::new(id(i * STRIDE));
            assert_eq!(map.remove(ent), Some(i as i32));
            assert_eq!(map.get(ent), None);
        }

        assert!(map.is_empty());
    }

    #[test]
    fn test_mixed_operations_multiple_pools() {
        let mut map = EntityMap::<Entity32, i32>::new();

        let ent_a = Entity32::new(id(0));
        let ent_b = Entity32::new(id(STRIDE));
        let ent_c = Entity32::new(id(STRIDE * 10));
        let ent_d = Entity32::new(id(STRIDE * 100));

        assert_eq!(map.insert(ent_a, 1), None);
        assert_eq!(map.insert(ent_b, 2), None);
        assert_eq!(map.insert(ent_c, 3), None);
        assert_eq!(map.insert(ent_d, 4), None);

        assert_eq!(map.remove(ent_b), Some(2));
        assert_eq!(map.remove(ent_c), Some(3));

        let ent_e = Entity32::new(id(STRIDE * 200));
        assert_eq!(map.insert(ent_e, 5), None);

        assert_eq!(map.get(ent_a), Some(&1));
        assert_eq!(map.get(ent_b), None);
        assert_eq!(map.get(ent_c), None);
        assert_eq!(map.get(ent_d), Some(&4));
        assert_eq!(map.get(ent_e), Some(&5));
    }

    #[test]
    fn test_iter_across_pools() {
        let mut map = EntityMap::<Entity32, i32>::new();

        let entities = [
            Entity32::new(id(0)),
            Entity32::new(id(STRIDE)),
            Entity32::new(id(STRIDE * 5)),
            Entity32::new(id(STRIDE * 10)),
        ];

        for (i, e) in entities.iter().enumerate() {
            map.insert(*e, i as i32 * 10);
        }

        let mut keys: Vec<_> = map.entities().map(|e| e.id().into_index() as u32).collect();
        keys.sort();

        let mut values: Vec<_> = map.values().cloned().collect();
        values.sort();

        assert_eq!(keys, vec![0, STRIDE, STRIDE * 5, STRIDE * 10]);
        assert_eq!(values, vec![0, 10, 20, 30]);
    }
}
