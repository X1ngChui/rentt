#![allow(dead_code)]

use crate::{entity::Entity, entity_map::EntityMap};
use std::{mem::MaybeUninit, ptr};

/// Type-erased immutable pointer to a component value.
///
/// This pointer represents a component instance without exposing its concrete type.
///
/// # Safety
///
/// - The caller must know the actual type, size, and alignment of the data.
/// - The pointer must be valid for reads of the full size of the erased type.
pub(crate) type ComponentPtr = *const u8;

/// Type-erased mutable pointer to uninitialized memory for returning `Option<T>` values.
///
/// This is used to return insertion/removal results into caller-provided buffers.
///
/// # Safety
///
/// - Must point to properly aligned, uninitialized `MaybeUninit<Option<T>>` memory.
/// - Caller must call `.assume_init()` after the function writes into it.
/// - The actual type `T` must match the erased type stored internally.
pub(crate) type UninitializedComponent = *mut MaybeUninit<Option<u8>>;

/// Erased interface for component storage.
///
/// Provides a common abstraction for inserting and removing components across
/// heterogeneous types using raw pointers.
///
/// # Safety
///
/// All methods require unsafe code:
/// - Pointers must have correct type, alignment, and lifetime.
/// - The caller is responsible for ensuring type consistency.
pub(crate) trait ComponentEntry<E: Entity> {
    /// Insert a component for the given entity.
    ///
    /// If a previous value exists, it will be returned via `old_value`.
    /// Otherwise, `None` is written.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid instance of erased type `T`.
    /// - `old_value` must point to uninitialized `MaybeUninit<Option<T>>`.
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent);

    /// Insert a component for the given entity, ignoring the previous value.
    ///
    /// The previous value is silently discarded.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid instance of erased type `T`.
    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr);

    /// Remove a component from the entity.
    ///
    /// If a value existed, it is written into `removed`; otherwise `None` is written.
    ///
    /// # Safety
    ///
    /// - `removed` must point to uninitialized `MaybeUninit<Option<T>>`.
    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent);

    /// Remove a component from the entity, discarding the removed value.
    fn remove_without_value(&mut self, entity: E);
}

/// Concrete storage for a specific component type `T`.
///
/// Internally backed by an `EntityMap` which maps entities to component values.
pub(crate) struct ComponentStorage<E: Entity, T> {
    storage: EntityMap<E, T>,
}

impl<E: Entity, T> ComponentStorage<E, T> {
    /// Creates a new, empty component storage.
    pub(crate) fn new() -> Self {
        Self {
            storage: EntityMap::new(),
        }
    }
}

impl<E: Entity, T> ComponentEntry<E> for ComponentStorage<E, T> {
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent) {
        // Cast raw input pointer to correct type
        let pvalue = value as *const T;
        let value = unsafe { ptr::read(pvalue) };

        // Insert into storage and get previous value
        let old = self.storage.insert(entity, value);

        // Cast output pointer and write previous value into buffer
        let pold_value = old_value as *mut MaybeUninit<Option<T>>;
        unsafe { ptr::write(pold_value, MaybeUninit::new(old)) };
    }

    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr) {
        let pvalue = value as *const T;
        let value = unsafe { ptr::read(pvalue) };
        let _ = self.storage.insert(entity, value);
    }

    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent) {
        let removed_value = self.storage.remove(entity);
        let premoved = removed as *mut MaybeUninit<Option<T>>;
        unsafe { ptr::write(premoved, MaybeUninit::new(removed_value)) };
    }

    fn remove_without_value(&mut self, entity: E) {
        let _ = self.storage.remove(entity);
    }
}
