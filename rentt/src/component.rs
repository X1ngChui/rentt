#![allow(dead_code)]

use crate::{entity::Entity, entity_map::EntityMap};
use std::{mem::MaybeUninit, ptr};

/// Type-erased immutable pointer to a component value.
///
/// This allows passing component values as raw pointers without knowing their concrete type.
/// Used heavily inside the type-erased storage interface.
pub(crate) type ComponentPtr = *const u8;

/// Type-erased mutable pointer to uninitialized memory for returning optional component values.
///
/// Used by insertion and removal operations to write back previous values
/// into caller-provided scratch buffers.
///
/// # Safety
///
/// - The caller must ensure correct alignment and type matching.
/// - The pointer must reference a `MaybeUninit<Option<T>>` of the correct type `T`.
/// - The caller must call `.assume_init()` after writing.
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
pub(crate) trait ComponentEntry<E: Entity> {
    /// Inserts a component into the storage for the given entity.
    ///
    /// Writes any replaced component into `old_value` as `Option<T>`, or `None` if none existed.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid instance of type `T`.
    /// - `old_value` must point to uninitialized `MaybeUninit<Option<T>>`.
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent);

    /// Inserts a component into the storage, ignoring the previous value.
    ///
    /// This is more efficient when the caller does not care about replaced values.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid instance of type `T`.
    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr);

    /// Removes a component from the storage.
    ///
    /// Writes the removed value (if any) into `removed` as `Option<T>`.
    ///
    /// # Safety
    ///
    /// - `removed` must point to uninitialized `MaybeUninit<Option<T>>`.
    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent);

    /// Removes a component from the storage, discarding the removed value.
    ///
    /// More efficient when caller does not require removed value.
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
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent) {
        // Cast input pointer to correct type.
        let pvalue = value as *const T;
        let value = unsafe { ptr::read(pvalue) };

        // Insert into storage, returning old value.
        let old = self.storage.insert(entity, value);

        // Cast output pointer and store previous value.
        let pold_value = old_value as *mut MaybeUninit<Option<T>>;
        unsafe { ptr::write(pold_value, MaybeUninit::new(old)); }
    }

    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr) {
        let pvalue = value as *const T;
        let value = unsafe { ptr::read(pvalue) };
        let _ = self.storage.insert(entity, value);
    }

    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent) {
        let removed_value = self.storage.remove(entity);
        let premoved = removed as *mut MaybeUninit<Option<T>>;
        unsafe { ptr::write(premoved, MaybeUninit::new(removed_value)); }
    }

    fn remove_without_value(&mut self, entity: E) {
        let _ = self.storage.remove(entity);
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

    /// Returns the underlying index.
    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

/// Marker trait for types that may be used as ECS components.
///
/// All component types must implement this trait.
/// The implementor is responsible for associating the type with its corresponding `ComponentHandle`.
///
/// # Safety
///
/// This trait operates under the following strict guarantees:
///
/// - `set_handle()` will be called **exactly once** for each component type during registration.
/// - `handle()` will only be called **after** `set_handle()` has completed.
/// - `handle()` must always return the exact same `ComponentHandle` value that was previously assigned via `set_handle()`.
///
/// Violating these constraints will result in undefined behavior in the ECS internals.
pub unsafe trait Component: 'static {
    /// Stores the assigned handle for this component type.
    ///
    /// # Safety
    ///
    /// This function is guaranteed to be called exactly once for each component type.
    unsafe fn set_handle(handle: ComponentHandle);

    /// Retrieves the assigned handle for this component type.
    ///
    /// # Safety
    ///
    /// This function will only be called after `set_handle()` has been called.
    /// It must always return the same handle that was previously set.
    unsafe fn handle() -> ComponentHandle;
}


/// Type alias for type-erased storage constructors.
///
/// Each component type registers a constructor that produces its `Box<dyn ComponentEntry>`.
#[allow(type_alias_bounds)]
pub(crate) type ComponentStorageConstructor<E: Entity> = fn() -> Box<dyn ComponentEntry<E>>;

/// Registry that stores all registered component constructors.
///
/// Acts as a central type-erased storage factory.
pub(crate) struct ComponentRegistry<E: Entity> {
    constructors: Vec<ComponentStorageConstructor<E>>,
}

impl<E: Entity> ComponentRegistry<E> {
    /// Creates an empty registry.
    pub(crate) fn new() -> Self {
        Self {
            constructors: Vec::new(),
        }
    }

    /// Registers a new component type.
    ///
    /// # Safety
    ///
    /// - Must only be called once per component type.
    /// - Caller must ensure single-threaded access during registration.
    pub(crate) unsafe fn register(
        &mut self,
        constructor: ComponentStorageConstructor<E>,
    ) -> ComponentHandle {
        let index = self.constructors.len();
        self.constructors.push(constructor);

        ComponentHandle::new(index)
    }

    /// Clears all registered components.
    pub(crate) fn clear(&mut self) {
        self.constructors.clear();
    }

    /// Iterates over all registered constructors.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &ComponentStorageConstructor<E>> {
        self.constructors.iter()
    }

    /// Mutable iterator over registered constructors.
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut ComponentStorageConstructor<E>> {
        self.constructors.iter_mut()
    }
}
