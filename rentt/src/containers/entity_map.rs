use crate::entity::EntityOps;
use crate::utils::Subscript;
use crate::utils::vec_unchecked::swap_remove_unchecked;
use std::hint::unreachable_unchecked;

/// A fixed-size pool for storing values of type `T`, indexed by `I`.
///
/// The pool uses a boxed array of size `N` to map indices to positions in the `values` vector.
/// The `values` vector stores tuples containing the original index and the associated value.
///
/// # Constraints
///
/// The index type `I` must satisfy `I::MAX >= N`. This ensures that the `NULL`
/// constant (`I::MAX`) is a valid sentinel value that does not conflict with any valid index in
/// the `value_indices` array. For example:
/// - If `I` is `u8`, then `I::MAX` is 255, so `N` must be <= 255.
/// - If `I` is `u16`, then `I::MAX` is 65,535, so `N` must be <= 65,535.
/// - If `I` is `u32`, then `I::MAX` is 2^32 - 1, so `N` must be <= 2^32 - 1. 
/// - If `I` is `u64`, then `I::MAX` is 2^64 - 1, so `N` must be <= 2^64 - 1. 
/// 
/// This constraint is critical to ensure that `NULL` can reliably indicate an empty or invalid slot.
#[derive(Debug)]
struct Pool<T, I: Subscript, const N: usize> {
    value_indices: Box<[I; N]>,
    values: Vec<(I, T)>,
}

impl<T, I: Subscript, const N: usize> Pool<T, I, N> {
    /// A constant representing an invalid or null index, set to the maximum value of `I`.
    const NULL: I = I::MAX;

    /// Creates a new, empty pool.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `I::MAX >= N`, as asserted in the method.
    #[inline]
    unsafe fn new() -> Self {
        debug_assert!(Self::NULL.into_subscript() >= N);

        Self {
            value_indices: Box::new([Self::NULL; N]),
            values: Vec::new(),
        }
    }

    /// Checks if the pool contains a value at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `index < N`.
    #[inline]
    unsafe fn contains(&self, index: I) -> bool {
        debug_assert!(index.into_subscript() < N);
        let value_index = *unsafe { self.value_indices.get_unchecked(index.into_subscript()) };
        value_index != Self::NULL
    }

    /// Retrieves an immutable reference to the value at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pool actually contains given index.
    #[inline]
    unsafe fn get(&self, index: I) -> &T {
        debug_assert!(unsafe { self.contains(index) });

        let value_index = *unsafe { self.value_indices.get_unchecked(index.into_subscript()) };
        debug_assert!(value_index.into_subscript() < self.values.len());
        let pair = unsafe { self.values.get_unchecked(value_index.into_subscript()) };
        &pair.1
    }

    /// Retrieves a mutable reference to the value at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pool actually contains given index.
    #[inline]
    unsafe fn get_mut(&mut self, index: I) -> &mut T {
        debug_assert!(unsafe { self.contains(index) });

        let value_index = *unsafe { self.value_indices.get_unchecked(index.into_subscript()) };
        debug_assert!(value_index.into_subscript() < self.values.len());
        let pair = unsafe { self.values.get_unchecked_mut(value_index.into_subscript()) };
        debug_assert!(pair.0 == index);
        &mut pair.1
    }

    /// Inserts a value at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pool does NOT contains given index.
    #[inline]
    unsafe fn insert(&mut self, index: I, value: T) {
        debug_assert!(unsafe { !self.contains(index) });

        let value_index = I::from_subscript(self.values.len());
        self.values.push((index, value)); // Fixed to store (index, value)
        unsafe {
            *self.value_indices.get_unchecked_mut(index.into_subscript()) = value_index;
        }
    }

    /// Removes the value at the given index and returns it.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pool actually contains given index.
    #[inline]
    unsafe fn remove(&mut self, index: I) -> T {
        debug_assert!(unsafe { self.contains(index) });

        let last_index = self.values.len() - 1;
        let last_value_index = unsafe { self.values.get_unchecked_mut(last_index).0 };
        let target_value_index =
            *unsafe { self.value_indices.get_unchecked(index.into_subscript()) };

        let (new_last_index, value) =
            unsafe { swap_remove_unchecked(&mut self.values, target_value_index.into_subscript()) };
        debug_assert!(new_last_index == index);

        unsafe {
            *self
                .value_indices
                .get_unchecked_mut(last_value_index.into_subscript()) = new_last_index;
            *self.value_indices.get_unchecked_mut(index.into_subscript()) = Self::NULL;
        }

        value
    }
}

/// An iterator over the values in the pool.
struct PoolIter<'a, T, I: Subscript> {
    inner: std::slice::Iter<'a, (I, T)>,
}

/// A mutable iterator over the values in the pool.
struct PoolIterMut<'a, T, I: Subscript> {
    inner: std::slice::IterMut<'a, (I, T)>,
}

impl<'a, T, I: Subscript> Default for PoolIter<'a, T, I> {
    fn default() -> Self {
        Self { inner: [].iter() }
    }
}

impl<'a, T, I: Subscript> Default for PoolIterMut<'a, T, I> {
    fn default() -> Self {
        Self {
            inner: [].iter_mut(),
        }
    }
}

impl<'a, T, I: Subscript> Iterator for PoolIter<'a, T, I> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, t)| t)
    }
}

impl<'a, T, I: Subscript> Iterator for PoolIterMut<'a, T, I> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, t)| t)
    }
}

impl<T, I: Subscript, const N: usize> Pool<T, I, N> {
    /// Returns an iterator over the values in the pool.
    #[inline]
    fn iter(&self) -> PoolIter<'_, T, I> {
        PoolIter {
            inner: self.values.iter(),
        }
    }

    /// Returns a mutable iterator over the values in the pool.
    #[inline]
    fn iter_mut(&mut self) -> PoolIterMut<'_, T, I> {
        PoolIterMut {
            inner: self.values.iter_mut(),
        }
    }
}

/// The size of each pool block in the entity map.
const BLOCK_SIZE: usize = 256;

/// A map for storing values associated with entities, using a collection of pools.
///
/// Each pool manages a fixed-size block of entities, enabling efficient storage and access.
/// The entity type `E` must implement `EntityOps` to provide location information.
#[derive(Debug)]
pub(crate) struct EntityMap<E: EntityOps, V> {
    pools: Vec<Pool<V, E::Id, BLOCK_SIZE>>,
}

impl<E: EntityOps, V> Default for EntityMap<E, V> {
    fn default() -> Self {
        Self { pools: Vec::new() }
    }
}

impl<E: EntityOps, V> EntityMap<E, V> {
    /// Creates a new, empty entity map.
    #[inline]
    pub(crate) fn new() -> Self {
        Self { pools: Vec::new() }
    }

    /// Checks if the map contains a value for the given entity.
    #[inline]
    pub(crate) fn contains(&self, entity: E) -> bool {
        let (index, offset) = entity.locate::<BLOCK_SIZE>();
        let found_pool = self.pools.get(index.into_subscript());

        match found_pool {
            None => false,
            Some(pool) => unsafe { pool.contains(offset) },
        }
    }

    /// Retrieves an immutable reference to the value associated with the given entity.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the map actually contains the give entity.
    #[inline]
    pub(crate) unsafe fn get(&self, entity: E) -> &V {
        debug_assert!(self.contains(entity));

        let (index, offset) = entity.locate::<BLOCK_SIZE>();
        let found_pool = self.pools.get(index.into_subscript());

        match found_pool {
            None => unsafe { unreachable_unchecked() },
            Some(pool) => unsafe { pool.get(offset) },
        }
    }

    /// Retrieves a mutable reference to the value associated with the given entity.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the map actually contains the give entity.
    #[inline]
    pub(crate) unsafe fn get_mut(&mut self, entity: E) -> &mut V {
        debug_assert!(self.contains(entity));

        let (index, offset) = entity.locate::<BLOCK_SIZE>();
        let found_pool = self.pools.get_mut(index.into_subscript());

        match found_pool {
            None => unsafe { unreachable_unchecked() },
            Some(pool) => unsafe { pool.get_mut(offset) },
        }
    }

    /// Inserts a value for the given entity.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the map does NOT contains the give entity.
    #[inline]
    pub(crate) unsafe fn insert(&mut self, entity: E, value: V) {
        debug_assert!(!self.contains(entity));

        let (index, offset) = entity.locate::<BLOCK_SIZE>();
        let found_pool = self.pools.get_mut(index.into_subscript());

        match found_pool {
            None => {
                self.pools
                    .resize_with(index.into_subscript() + 1, || unsafe { Pool::new() });
                unsafe {
                    self.pools
                        .get_unchecked_mut(index.into_subscript())
                        .insert(offset, value);
                }
            }
            Some(pool) => unsafe { pool.insert(offset, value) },
        }
    }

    /// Removes the value associated with the given entity and returns it.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the map actually contains the give entity.
    #[inline]
    pub(crate) unsafe fn remove(&mut self, entity: E) -> V {
        debug_assert!(self.contains(entity));

        let (index, offset) = entity.locate::<BLOCK_SIZE>();
        let found_pool = self.pools.get_mut(index.into_subscript());

        match found_pool {
            None => unsafe { unreachable_unchecked() },
            Some(pool) => unsafe { pool.remove(offset) },
        }
    }
}

/// An iterator over the values in the entity map.
struct MapIter<'a, E: EntityOps, V> {
    pools_iter: std::slice::Iter<'a, Pool<V, E::Id, BLOCK_SIZE>>,
    pairs_iter: PoolIter<'a, V, E::Id>,
}

/// A mutable iterator over the values in the entity map.
struct MapIterMut<'a, E: EntityOps, V> {
    pools_iter: std::slice::IterMut<'a, Pool<V, E::Id, BLOCK_SIZE>>,
    pairs_iter: PoolIterMut<'a, V, E::Id>,
}

impl<'a, E: EntityOps, V> Iterator for MapIter<'a, E, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(v) = self.pairs_iter.next() {
                return Some(v);
            }

            match self.pools_iter.next() {
                Some(next_pool) => {
                    self.pairs_iter = next_pool.iter();
                }
                None => return None,
            }
        }
    }
}

impl<'a, E: EntityOps, V> Iterator for MapIterMut<'a, E, V> {
    type Item = &'a mut V;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(v) = self.pairs_iter.next() {
                return Some(v);
            }

            if let Some(next_pool) = self.pools_iter.next() {
                self.pairs_iter = next_pool.iter_mut();
            } else {
                return None;
            }
        }
    }
}

impl<E: EntityOps, V> EntityMap<E, V> {
    /// Returns an iterator over the values in the entity map.
    #[inline]
    fn iter(&self) -> MapIter<'_, E, V> {
        MapIter {
            pools_iter: self.pools.iter(),
            pairs_iter: Default::default(),
        }
    }

    /// Returns a mutable iterator over the values in the entity map.
    #[inline]
    fn iter_mut(&mut self) -> MapIterMut<'_, E, V> {
        MapIterMut {
            pools_iter: self.pools.iter_mut(),
            pairs_iter: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{self, Entity};

    #[test]
    fn test_pool_insert_get_remove_once() {
        unsafe {
            let mut pool = Pool::<String, u8, 1>::new();
            pool.insert(0, "Hello".to_string());
            assert!(pool.contains(0));
            assert_eq!(pool.get(0), "Hello");
            let value = pool.remove(0);
            assert_eq!(value, "Hello");
            assert!(!pool.contains(0));
        }
    }

    #[test]
    fn test_pool_insert_get_remove_multiple() {
        const N: usize = 1024;

        unsafe {
            let mut pool = Pool::<String, usize, N>::new();

            for i in 0..N {
                pool.insert(i, format!("{}", i));
                assert!(pool.contains(i));
                assert_eq!(pool.get(i), &format!("{}", i));
            }

            for i in 0..N {
                let value = pool.remove(i);
                assert!(!pool.contains(i));
                assert_eq!(value, format!("{}", i));
            }
        }
    }

    #[test]
    fn test_empty_pool_iter() {
        unsafe {
            let mut pool = Pool::<String, usize, 8>::new();

            let mut cnt = 0;
            for _ in pool.iter() {
                cnt += 1;
            }
            assert_eq!(cnt, 0);

            let mut cnt_mut = 0;
            for _ in pool.iter_mut() {
                cnt_mut += 1;
            }
            assert_eq!(cnt_mut, 0);
        }
    }

    #[test]
    fn test_pool_iter() {
        const N: usize = 1024;

        unsafe {
            let mut pool = Pool::<String, usize, N>::new();

            for i in 0..N {
                pool.insert(i, format!("{}", i));
                assert!(pool.contains(i));
                assert_eq!(pool.get(i), &format!("{}", i));
            }

            for value in pool.iter() {
                let index: usize = value.parse().unwrap();
                assert!(pool.contains(index));
                assert_eq!(pool.get(index), value);
                assert_eq!(value, &format!("{}", index));
            }

            for value in pool.iter_mut() {
                *value = "modified".to_string();
            }

            let mut cnt = 0;
            for value in pool.iter() {
                assert_eq!(value, "modified");
                cnt += 1;
            }
            assert_eq!(cnt, N);
        }
    }

    #[test]
    fn test_entity_map_insert_get_remove_once() {
        let mut map = EntityMap::<Entity, String>::default();

        unsafe {
            let entity = Entity::new_unchecked(1, 0);
            map.insert(entity, "name".to_string());
            assert!(map.contains(entity));
            assert_eq!(map.get(entity), "name");

            let value = map.remove(entity);
            assert_eq!(value, "name");
            assert!(!map.contains(entity));
        }
    }

    #[test]
    fn test_entity_map_insert_get_remove_multiple() {
        const N: usize = 1024;
        let mut map = EntityMap::<Entity, String>::new();

        unsafe {
            for i in 0..N {
                let entity =
                    Entity::new_unchecked(1, i as <Entity as entity::entity::EntityOps>::Id);
                map.insert(entity, format!("{}", i));
                assert!(map.contains(entity));
                assert_eq!(map.get(entity), &format!("{}", i));
            }

            for i in 0..N {
                let entity =
                    Entity::new_unchecked(1, i as <Entity as entity::entity::EntityOps>::Id);
                let value = map.remove(entity);
                assert_eq!(value, format!("{}", i));
                assert!(!map.contains(entity));
            }
        }
    }

    #[test]
    fn test_entitiy_map_iter() {
        const N: usize = 1024;

        unsafe {
            let mut map = EntityMap::<Entity, String>::new();

            for i in 0..N {
                let entity =
                    Entity::new_unchecked(1, i as <Entity as entity::entity::EntityOps>::Id);
                map.insert(entity, format!("{}", i));
                assert!(map.contains(entity));
                assert_eq!(map.get(entity), &format!("{}", i));
            }

            for value in map.iter() {
                let index: usize = value.parse().unwrap();
                let entity =
                    Entity::new_unchecked(1, index as <Entity as entity::entity::EntityOps>::Id);
                assert!(map.contains(entity));
                assert_eq!(map.get(entity), value);
                assert_eq!(value, &format!("{}", index));
            }

            for value in map.iter_mut() {
                *value = "modified".to_string();
            }

            let mut cnt = 0;
            for value in map.iter() {
                assert_eq!(value, "modified");
                cnt += 1;
            }
            assert_eq!(cnt, N);
        }
    }

    #[test]
    fn test_empty_entity_map_iter() {
        let mut map = EntityMap::<Entity, String>::new();

        let mut cnt = 0;
        for _ in map.iter() {
            cnt += 1;
        }
        assert_eq!(cnt, 0);

        let mut cnt_mut = 0;
        for _ in map.iter_mut() {
            cnt_mut += 1;
        }
        assert_eq!(cnt_mut, 0);
    }
}