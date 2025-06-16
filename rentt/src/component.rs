//! # ECS Component System Core
//!
//! This module defines the core abstractions for ECS component storage and registration.
//!
//! It implements type-erased storage via `ComponentEntry`, allowing heterogeneous components
//! to be stored uniformly in the ECS world, while retaining type safety at higher levels.
//!
//! ## Key Concepts
//!
//! - `Component`: Marker trait for all component types.
//! - `ComponentHandle`: Unique handle assigned to each registered component type.
//! - `ComponentEntry`: Type-erased interface for inserting/removing components.
//! - `ComponentStorage`: Concrete storage for a specific component type `T`.
//! - `ComponentRegistry`: Global registry for constructing per-type storages.
//!
//! ## Safety Guarantees
//!
//! - Type-erasure only occurs internally; public APIs remain fully type-safe.
//! - `register::<T>()` must be called exactly once per type before any operations.
//! - Correct alignment, initialization, and casting is enforced by strict APIs.
//!
//! This system forms the backend for the high-performance, type-safe ECS world.

#![allow(dead_code)]

use crate::{entity::Entity, entity_map::EntityMap};
use std::{mem::MaybeUninit, ptr};

/// Type-erased immutable pointer to a component value.
///
/// Allows operating on component data without knowing its concrete type.
pub(crate) type ComponentPtr = *const u8;

/// Type-erased mutable pointer to a component value.
///
/// Allows obtaining mutable access to components during iteration or updates.
pub(crate) type ComponentPtrMut = *mut u8;

/// Type-erased mutable pointer to uninitialized memory for returning optional component values.
///
/// Used when inserting or removing components to write back replaced or removed values
/// into caller-provided scratch buffers.
///
/// # Safety
///
/// - Caller must ensure correct alignment and type matching.
/// - The pointer must reference a `MaybeUninit<Option<T>>` of the correct type.
/// - Caller must call `.assume_init()` after writing.
pub(crate) type UninitializedComponent = *mut MaybeUninit<Option<u8>>;

/// Type-erased interface for component storage.
///
/// This trait abstracts insertion and removal of components regardless of their underlying type.
/// Concrete implementations (like `ComponentStorage`) implement this interface for specific `T`.
///
/// # Safety
///
/// - All pointer arguments must point to valid data of correct type and alignment.
/// - The caller is responsible for ensuring type safety at runtime.
pub trait ComponentEntry<E: Entity> {
    /// Returns the number of entities currently holding this component.
    fn len(&self) -> usize;

    /// Returns an iterator over all `(entity, component pointer)` pairs in this storage.
    fn iter(&self) -> Box<dyn Iterator<Item = (E, ComponentPtr)> + '_>;

    /// Returns a mutable iterator over all `(entity, mutable component pointer)` pairs.
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (E, ComponentPtrMut)> + '_>;

    /// Inserts a component into storage, returning any previous value into `old_value`.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid instance of type `T`.
    /// - `old_value` must point to uninitialized `MaybeUninit<Option<T>>`.
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent);

    /// Inserts a component into storage, discarding any previous value.
    ///
    /// This is more efficient when the caller does not care about replaced values.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid instance of type `T`.
    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr);

    /// Removes a component from storage, writing the removed value into `removed`.
    ///
    /// # Safety
    ///
    /// - `removed` must point to uninitialized `MaybeUninit<Option<T>>`.
    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent);

    /// Removes a component from storage, discarding any removed value.
    fn remove_without_value(&mut self, entity: E);
}

/// Concrete storage for a specific component type `T`.
///
/// Internally uses an `EntityMap` to associate entities with component values.
pub(crate) struct ComponentStorage<E: Entity, T> {
    storage: EntityMap<E, T>,
}

impl<E: Entity, T> ComponentStorage<E, T> {
    /// Creates a new empty storage for type `T`.
    pub(crate) fn new() -> Self {
        Self {
            storage: EntityMap::new(),
        }
    }
}

impl<E: Entity, T> ComponentEntry<E> for ComponentStorage<E, T> {
    fn len(&self) -> usize {
        self.storage.len()
    }

    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent) {
        let pvalue = value as *const T;
        let value = unsafe { ptr::read(pvalue) };

        let old = self.storage.insert(entity, value);

        let pold_value = old_value as *mut MaybeUninit<Option<T>>;
        unsafe {
            ptr::write(pold_value, MaybeUninit::new(old));
        }
    }

    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr) {
        let pvalue = value as *const T;
        let value = unsafe { ptr::read(pvalue) };
        let _ = self.storage.insert(entity, value);
    }

    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent) {
        let removed_value = self.storage.remove(entity);
        let premoved = removed as *mut MaybeUninit<Option<T>>;
        unsafe {
            ptr::write(premoved, MaybeUninit::new(removed_value));
        }
    }

    fn remove_without_value(&mut self, entity: E) {
        let _ = self.storage.remove(entity);
    }

    fn iter(&self) -> Box<dyn Iterator<Item = (E, ComponentPtr)> + '_> {
        Box::new(
            self.storage
                .iter()
                .map(|(e, v)| (e, v as *const T as ComponentPtr)),
        )
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (E, ComponentPtrMut)> + '_> {
        Box::new(
            self.storage
                .iter_mut()
                .map(|(e, v)| (e, v as *mut T as ComponentPtrMut)),
        )
    }
}

/// Opaque handle uniquely identifying a registered component type.
///
/// Internally represented by a simple index into the component registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentHandle {
    index: usize,
}

impl ComponentHandle {
    /// Creates a new handle from raw index.
    pub(crate) fn new(index: usize) -> Self {
        Self { index }
    }

    /// Returns the underlying index used for tree addressing.
    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

/// Marker trait for types that may be used as ECS components.
///
/// Each component type must implement this trait, allowing the ECS world to retrieve
/// its associated handle at runtime. This enables type-safe storage access with zero runtime cost.
///
/// # Safety
///
/// This trait operates under the following strict rules:
///
/// - `set_handle()` will be called **once and only once** at registration time.
/// - `handle()` must always return the exact handle previously assigned.
/// - If violated, the ECS system may exhibit undefined behavior.
pub unsafe trait Component: 'static {
    /// Stores the assigned handle for this component type.
    ///
    /// # Safety
    ///
    /// Called exactly once for each type.
    unsafe fn set_handle(handle: ComponentHandle);

    /// Retrieves the assigned handle for this component type.
    ///
    /// # Safety
    ///
    /// - Only called after `set_handle()` has been called.
    unsafe fn handle() -> ComponentHandle;
}

/// Type alias for type-erased storage constructors.
///
/// Each component type registers a constructor that produces its `Box<dyn ComponentEntry>`.
#[allow(type_alias_bounds)]
pub(crate) type ComponentStorageConstructor<E: Entity> = fn() -> Box<dyn ComponentEntry<E>>;

/// Registry that stores all registered component constructors.
///
/// Acts as a global type factory to initialize type-erased storages dynamically.
pub(crate) struct ComponentRegistry<E: Entity> {
    constructors: Vec<ComponentStorageConstructor<E>>,
}

impl<E: Entity> ComponentRegistry<E> {
    /// Creates an empty component registry.
    pub(crate) fn new() -> Self {
        Self {
            constructors: Vec::new(),
        }
    }

    /// Registers a new component type and returns its assigned handle.
    ///
    /// # Safety
    ///
    /// - Must only be called once per component type.
    /// - Caller must ensure exclusive access during registration.
    pub(crate) unsafe fn register(
        &mut self,
        constructor: ComponentStorageConstructor<E>,
    ) -> ComponentHandle {
        let index = self.constructors.len();
        self.constructors.push(constructor);
        ComponentHandle::new(index)
    }

    #[cfg(test)]
    /// Clears all registered components (testing only).
    pub(crate) fn clear(&mut self) {
        self.constructors.clear();
    }

    /// Returns an iterator over all registered component constructors.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &ComponentStorageConstructor<E>> {
        self.constructors.iter()
    }

    /// Returns a mutable iterator over registered constructors.
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut ComponentStorageConstructor<E>> {
        self.constructors.iter_mut()
    }
}
