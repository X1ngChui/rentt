#![allow(dead_code)]

//! # Entity-Component-System (ECS) World Implementation
//!
//! This module provides a basic ECS world implementation with automatic component registration.
//! It supports various entity types (`Entity16`, `Entity32`, `Entity64`) and components that do not
//! contain references. The implementation uses a tree-based structure for efficient component
//! storage and retrieval.
//!
//! ## Available World Types
//!
//! | World Type | Entity Type | Max Live Entities   | Platforms      |
//! | ---------- | ----------- | ------------------- | -------------- |
//! | `World16`  | `Entity16`  | 4,096               | All platforms  |
//! | `World32`  | `Entity32`  | 16,777,216          | 32-bit & 64-bit|
//! | `World64`  | `Entity64`  | 281,474,976,710,656 | 64-bit only    |
//!
//! ## Default World Type
//!
//! The default `World` type, aliased as `DefaultWorld`, is selected based on the target platform:
//!
//! - On 64-bit platforms, `DefaultWorld` is `World32`.
//! - On 32-bit platforms, `DefaultWorld` is `World32`.
//! - On 16-bit platforms, `DefaultWorld` is `World16`.

use std::mem::MaybeUninit;
use std::sync::{OnceLock, RwLock};

use crate::bit_set::BitSet;
use crate::component::{
    Component, ComponentEntry, ComponentHandle, ComponentPtr, ComponentPtrMut, ComponentRegistry,
    ComponentStorage, ComponentStorageConstructor, UninitializedComponent,
};
use crate::entity::{Entity, Entity16, Entity32, Entity64};
use crate::entity_fields::EntityId;
use crate::entity_map::EntityMap;

/// Internal node in the component tree, managing component storage and subtree presence.
///
/// This struct is part of a multi-way complete tree structure for organizing component storages:
/// - The root is at level 0.
/// - Each node can have up to `ORDER` children, where `ORDER` is the bit width of `BitSet`.
/// - Per-entity `BitSet`s track which subtrees contain components, enabling efficient iteration
///   by skipping irrelevant branches.
///
/// # Type Parameters
///
/// - `E`: The entity identifier type, implementing the `Entity` trait.
struct ComponentNode<E: Entity> {
    /// Type-erased storage backend for components at this node.
    entry: Box<dyn ComponentEntry<E>>,
    /// Map of entities to bit sets, indicating active subtrees for each entity.
    bit_signs: EntityMap<E, BitSet>,
}

impl<E: Entity> ComponentNode<E> {
    /// The branching factor of the tree, equal to the number of bits in `BitSet`.
    const ORDER: usize = BitSet::BITS;

    /// Creates a new `ComponentNode` with the specified storage backend.
    #[inline]
    fn new(entry: Box<dyn ComponentEntry<E>>) -> Self {
        Self {
            entry,
            bit_signs: EntityMap::new(),
        }
    }

    /// Inserts a component for an entity, storing the previous value (if any) in `old_value`.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid component instance.
    /// - `old_value` must point to uninitialized memory capable of receiving the previous value.
    #[inline]
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent) {
        unsafe {
            self.entry.insert(entity, value, old_value);
        }
    }

    /// Inserts a component for an entity without retrieving the previous value.
    ///
    /// # Safety
    ///
    /// - `value` must point to a valid component instance.
    #[inline]
    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr) {
        unsafe {
            self.entry.insert_without_value(entity, value);
        }
    }

    /// Removes a component for an entity, storing it in `removed`.
    ///
    /// # Safety
    ///
    /// - `removed` must point to uninitialized memory capable of receiving the removed value.
    #[inline]
    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent) {
        unsafe {
            self.entry.remove(entity, removed);
        }
    }

    /// Removes a component for an entity without retrieving the value.
    #[inline]
    fn remove_without_value(&mut self, entity: E) {
        self.entry.remove_without_value(entity);
    }

    /// Marks an entity as present in a child subtree by setting a bit in its `BitSet`.
    ///
    /// # Safety
    ///
    /// - `sign` must be in the range `0..Self::ORDER`.
    #[inline]
    unsafe fn insert_sign(&mut self, entity: E, sign: usize) {
        debug_assert!(sign < Self::ORDER);

        match self.bit_signs.get_mut(entity) {
            None => {
                let signs = BitSet::new(1 << sign);
                self.bit_signs.insert(entity, signs);
            }
            Some(signs) => unsafe { signs.insert(sign) },
        }
    }

    /// Clears an entity's presence from a child subtree by unsetting a bit in its `BitSet`.
    ///
    /// # Safety
    ///
    /// - `sign` must be in the range `0..Self::ORDER`.
    #[inline]
    unsafe fn remove_sign(&mut self, entity: E, sign: usize) {
        debug_assert!(sign < Self::ORDER);

        if let Some(signs) = self.bit_signs.get_mut(entity) {
            unsafe { signs.remove(sign) };
        }
    }

    /// Returns the number of entities currently holding this component.
    #[inline]
    fn len(&self) -> usize {
        self.entry.len()
    }

    /// Returns an iterator over all components stored directly in this node.
    ///
    /// This iterator yields `(entity, component_ptr)` pairs for all entities that have
    /// components stored at the current level of the tree.
    ///
    /// # Returns
    ///
    /// A boxed iterator yielding:
    /// - `E`: The entity identifier.
    /// - `ComponentPtr`: A type-erased immutable pointer to the component data.
    #[inline]
    fn iter(&self) -> Box<dyn Iterator<Item = (E, ComponentPtr)> + '_> {
        self.entry.iter()
    }

    /// Returns a mutable iterator over all components stored directly in this node.
    ///
    /// This iterator yields `(entity, component_ptr_mut)` pairs for all entities that have
    /// components stored at the current level of the tree, allowing in-place modification
    /// of the component data.
    ///
    /// # Returns
    ///
    /// A boxed iterator yielding:
    /// - `E`: The entity identifier.
    /// - `ComponentPtrMut`: A type-erased mutable pointer to the component data.
    #[inline]
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (E, ComponentPtrMut)> + '_> {
        self.entry.iter_mut()
    }
}

/// Internal ECS world implementation handling type-erased data.
///
/// Manages:
/// - Entity ID allocation and reuse.
/// - A tree of component storages.
/// - Low-level component binding and unbinding.
///
/// This struct operates on type-erased data, unaware of specific component types.
macro_rules! impl_raw_world {
    ($t: ident, $e: ty, $reg: ident) => {
        static $reg: OnceLock<RwLock<ComponentRegistry<$e>>> = OnceLock::new();

        /// Internal ECS world implementation handling type-erased data.
        pub struct $t {
            /// Vector of component nodes forming the complete tree of storages.
            components: Vec<ComponentNode<$e>>,
            /// Next entity ID to allocate, if available.
            next_entity_id: Option<<$e as Entity>::Id>,
            /// Pool of reusable entity IDs from removed entities.
            removed_entities: Vec<$e>,
        }

        impl Default for $t {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $t {
            /// Branching factor of the component tree.
            const ORDER: usize = ComponentNode::<$e>::ORDER;

            /// Retrieves or initializes the global component registry.
            fn get_registry() -> &'static RwLock<ComponentRegistry<$e>> {
                $reg.get_or_init(|| RwLock::new(ComponentRegistry::new()))
            }

            /// Registers a component storage constructor, returning its handle.
            ///
            /// # Safety
            ///
            /// - Caller must ensure the constructor is valid and unique registration is safe.
            pub unsafe fn register(constructor: ComponentStorageConstructor<$e>) -> ComponentHandle {
                unsafe { Self::get_registry().write().unwrap().register(constructor) }
            }

            /// Gets a reference to the component node for a handle, extending the tree if needed.
            fn get_node(&mut self, handle: ComponentHandle) -> &ComponentNode<$e> {
                let index = handle.index();

                if index >= self.components.len() {
                    let registry = Self::get_registry().read().unwrap();
                    let mut additional_nodes = Vec::new();
                    for constructor in registry.iter().skip(self.components.len()) {
                        additional_nodes.push(ComponentNode::new(constructor()));
                    }
                    self.components.extend(additional_nodes);
                }

                debug_assert!(index < self.components.len());
                unsafe { self.components.get_unchecked(index) }
            }

            /// Gets a mutable reference to the component node for a handle, extending the tree if needed.
            fn get_node_mut(&mut self, handle: ComponentHandle) -> &mut ComponentNode<$e> {
                let index = handle.index();

                if index >= self.components.len() {
                    let registry = Self::get_registry().read().unwrap();
                    let mut additional_nodes = Vec::new();
                    for constructor in registry.iter().skip(self.components.len()) {
                        additional_nodes.push(ComponentNode::new(constructor()));
                    }
                    self.components.extend(additional_nodes);
                }

                debug_assert!(index < self.components.len());
                unsafe { self.components.get_unchecked_mut(index) }
            }

            /// Creates a new, empty ECS world.
            #[inline]
            pub fn new() -> Self {
                Self {
                    components: Vec::new(),
                    next_entity_id: Some(<<$e as Entity>::Id as EntityId>::MIN),
                    removed_entities: Vec::new(),
                }
            }

            /// Allocates a new entity, reusing IDs if available or generating a new one.
            ///
            /// Returns `Some(entity)` on success, `None` if the ID pool is exhausted.
            pub fn new_entity(&mut self) -> Option<$e> {
                if let Some(entity) = self.removed_entities.pop() {
                    return Some(entity.next_ver());
                }

                self.next_entity_id.map(|now| {
                    let entity = <$e>::new(now);
                    self.next_entity_id = now.next();
                    entity
                })
            }

            /// Recursively removes all components for an entity from the tree.
            fn remove_entity_helper(&mut self, entity: $e, root: usize) {
                if root >= self.components.len() {
                    return;
                }

                let component_node = unsafe { self.components.get_unchecked_mut(root) };
                component_node.remove_without_value(entity);

                let subtrees = component_node.bit_signs.remove(entity);
                if let Some(subtrees) = subtrees {
                    for subtree in subtrees.iter() {
                        let next = root * Self::ORDER + subtree + 1;
                        self.remove_entity_helper(entity, next);
                    }
                }
            }

            /// Removes an entity and all its components, recycling its ID.
            #[inline]
            pub fn remove_entity(&mut self, entity: $e) {
                self.removed_entities.push(entity);
                self.remove_entity_helper(entity, 0);
            }

            /// Binds a component to an entity, updating the tree structure.
            ///
            /// # Safety
            ///
            /// - `value` must point to a valid component instance.
            /// - `old_value` must point to uninitialized memory for the previous value.
            #[inline]
            pub unsafe fn bind_component(
                &mut self,
                entity: $e,
                component_handle: ComponentHandle,
                value: ComponentPtr,
                old_value: UninitializedComponent,
            ) {
                let component_node = self.get_node_mut(component_handle);

                unsafe {
                    component_node.insert(entity, value, old_value);
                }

                // Propagate bit signs up the tree
                let mut now = component_handle.index();
                while now > 0 {
                    let parent_index = (now - 1) / Self::ORDER;
                    let subtree_index = (now - 1) % Self::ORDER;

                    let parent_node = unsafe { self.components.get_unchecked_mut(parent_index) };
                    unsafe {
                        parent_node.insert_sign(entity, subtree_index);
                    }

                    now = parent_index;
                }
            }

            /// Unbinds a component from an entity, leaving bit signs unchanged.
            ///
            /// # Safety
            ///
            /// - `old_value` must point to uninitialized memory for the removed value.
            #[inline]
            pub unsafe fn unbind_component(
                &mut self,
                entity: $e,
                component_handle: ComponentHandle,
                old_value: UninitializedComponent,
            ) {
                let component_node = self.get_node_mut(component_handle);

                unsafe {
                    component_node.remove(entity, old_value);
                }
            }

            /// Returns the number of entities currently holding this component.
            #[inline]
            fn len(&mut self, handle: ComponentHandle) -> usize {
                let component_node = self.get_node(handle);
                component_node.len()
            }

            /// Returns an iterator over all components of a specific type.
            ///
            /// This method provides type-erased, immutable access to all components bound
            /// to entities for the given `component_handle`. It only iterates over components
            /// stored directly at the storage node corresponding to the handle.
            ///
            /// # Parameters
            ///
            /// - `handle`: The component handle obtained during registration.
            ///
            /// # Returns
            ///
            /// A boxed iterator yielding `(entity, component_ptr)` pairs, where:
            /// - `entity` is the entity ID.
            /// - `component_ptr` is a type-erased immutable pointer to the component data.
            ///
            /// # Notes
            ///
            /// - The iteration order is not specified and should not be relied upon.
            /// - Returned `ComponentPtr` must be safely cast to the actual component type
            ///   by the caller.
            #[inline]
            pub fn iter(&mut self, handle: ComponentHandle) -> Box<dyn Iterator<Item = ($e, ComponentPtr)> + '_> {
                let component_node = self.get_node(handle);
                component_node.iter()
            }

            /// Returns a mutable iterator over all components of a specific type.
            ///
            /// This method provides type-erased, mutable access to all components bound
            /// to entities for the given `component_handle`. It allows in-place modification
            /// of component values.
            ///
            /// # Parameters
            ///
            /// - `handle`: The component handle obtained during registration.
            ///
            /// # Returns
            ///
            /// A boxed iterator yielding `(entity, component_ptr_mut)` pairs, where:
            /// - `entity` is the entity ID.
            /// - `component_ptr_mut` is a type-erased mutable pointer to the component data.
            ///
            /// # Safety Notes
            ///
            /// - The iteration order is not specified and should not be relied upon.
            /// - Returned `ComponentPtrMut` must be safely cast to the actual component type
            ///   by the caller.
            #[inline]
            pub fn iter_mut(&mut self, handle: ComponentHandle) -> Box<dyn Iterator<Item = ($e, ComponentPtrMut)> + '_> {
                let component_node = self.get_node_mut(handle);
                component_node.iter_mut()
            }
        }
    };
}

#[cfg(target_pointer_width = "64")]
impl_raw_world!(RawWorld64, Entity64, __REGISTRY64);

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_raw_world!(RawWorld32, Entity32, __REGISTRY32);

impl_raw_world!(RawWorld16, Entity16, __REGISTRY16);

#[cfg(test)]
mod raw_world_tests {
    use super::*;
    use crate::component::{ComponentPtr, ComponentStorage, UninitializedComponent};
    use crate::entity::Entity32;
    use std::mem::MaybeUninit;

    #[test]
    fn test_raw_world_insert_and_remove() {
        RawWorld32::get_registry().write().unwrap().clear();
        let mut world = RawWorld32::new();
        let handle =
            unsafe { RawWorld32::register(|| Box::new(ComponentStorage::<Entity32, i32>::new())) };

        // Allocate entity
        let entity = world.new_entity().unwrap();

        // Insert component for the entity
        let value: i32 = 42;
        let value_ptr: ComponentPtr = &value as *const i32 as ComponentPtr;

        let mut old_value_buf: MaybeUninit<Option<i32>> = MaybeUninit::uninit();
        let old_value_ptr: UninitializedComponent =
            &mut old_value_buf as *mut _ as UninitializedComponent;

        unsafe {
            world.bind_component(entity, handle, value_ptr, old_value_ptr);
        }

        // Ensure no previous value existed
        assert!(unsafe { old_value_buf.assume_init() }.is_none());

        // Remove the component
        let mut removed_value_buf: MaybeUninit<Option<i32>> = MaybeUninit::uninit();
        let removed_value_ptr: UninitializedComponent =
            &mut removed_value_buf as *mut _ as UninitializedComponent;

        unsafe {
            world.unbind_component(entity, handle, removed_value_ptr);
        }

        let removed_value = unsafe { removed_value_buf.assume_init() };
        assert_eq!(removed_value, Some(42));

        // Remove the entity itself
        world.remove_entity(entity);

        // Verify entity ID reuse logic
        let recycled = world.new_entity().unwrap();
        assert_eq!(recycled.id(), entity.id());
        assert_ne!(recycled.ver(), entity.ver());
    }

    #[test]
    fn test_multiple_components_and_entities() {
        RawWorld32::get_registry().write().unwrap().clear();
        let mut world = RawWorld32::new();
        let handle_a =
            unsafe { RawWorld32::register(|| Box::new(ComponentStorage::<Entity32, i32>::new())) };
        let handle_b =
            unsafe { RawWorld32::register(|| Box::new(ComponentStorage::<Entity32, u64>::new())) };

        let entity1 = world.new_entity().unwrap();
        let entity2 = world.new_entity().unwrap();

        // Bind component A to entity1
        let value_a1: i32 = 100;
        let value_a1_ptr: ComponentPtr = &value_a1 as *const i32 as ComponentPtr;
        let mut old_a1_buf: MaybeUninit<Option<i32>> = MaybeUninit::uninit();
        let old_a1_ptr: UninitializedComponent =
            &mut old_a1_buf as *mut _ as UninitializedComponent;

        unsafe {
            world.bind_component(entity1, handle_a, value_a1_ptr, old_a1_ptr);
        }

        assert!(unsafe { old_a1_buf.assume_init() }.is_none());

        // Bind component B to entity2
        let value_b2: u64 = 200;
        let value_b2_ptr: ComponentPtr = &value_b2 as *const u64 as *const u8;
        let mut old_b2_buf: MaybeUninit<Option<u64>> = MaybeUninit::uninit();
        let old_b2_ptr: UninitializedComponent =
            &mut old_b2_buf as *mut _ as UninitializedComponent;

        unsafe {
            world.bind_component(entity2, handle_b, value_b2_ptr, old_b2_ptr);
        }

        assert!(unsafe { old_b2_buf.assume_init() }.is_none());

        // Unbind component A from entity1
        let mut removed_a1_buf: MaybeUninit<Option<i32>> = MaybeUninit::uninit();
        let removed_a1_ptr: UninitializedComponent =
            &mut removed_a1_buf as *mut _ as UninitializedComponent;

        unsafe {
            world.unbind_component(entity1, handle_a, removed_a1_ptr);
        }
        let removed_a1 = unsafe { removed_a1_buf.assume_init() };
        assert_eq!(removed_a1, Some(100));

        // Unbind component B from entity2
        let mut removed_b2_buf: MaybeUninit<Option<u64>> = MaybeUninit::uninit();
        let removed_b2_ptr: UninitializedComponent =
            &mut removed_b2_buf as *mut _ as UninitializedComponent;

        unsafe {
            world.unbind_component(entity2, handle_b, removed_b2_ptr);
        }
        let removed_b2 = unsafe { removed_b2_buf.assume_init() };
        assert_eq!(removed_b2, Some(200));
    }

    #[test]
    fn test_raw_world_deep_tree_stress() {
        const COMPONENT_COUNT: usize = 1024; // Ensure deep multi-level tree structure
        RawWorld32::get_registry().write().unwrap().clear();
        let mut world = RawWorld32::new();

        // Dynamically register a large number of components
        let mut handles = Vec::new();
        for _ in 0..COMPONENT_COUNT {
            let handle = unsafe {
                RawWorld32::register(|| Box::new(ComponentStorage::<Entity32, usize>::new()))
            };
            handles.push(handle);
        }

        // Create multiple entities
        let entities: Vec<_> = (0..10).map(|_| world.new_entity().unwrap()).collect();

        // Bind all components to each entity (test bind_component with deep tree traversal)
        for &entity in &entities {
            for (comp_id, handle) in handles.iter().enumerate() {
                let value: usize = comp_id;
                let value_ptr: ComponentPtr = &value as *const usize as ComponentPtr;

                let mut old_value_buf: MaybeUninit<Option<usize>> = MaybeUninit::uninit();
                let old_value_ptr: UninitializedComponent =
                    &mut old_value_buf as *mut _ as UninitializedComponent;

                unsafe {
                    world.bind_component(entity, *handle, value_ptr, old_value_ptr);
                }

                assert!(unsafe { old_value_buf.assume_init() }.is_none());
            }
        }

        // Unbind all components from each entity (test unbind_component and tree cleanup)
        for &entity in &entities {
            for (comp_id, handle) in handles.iter().enumerate() {
                let mut removed_buf: MaybeUninit<Option<usize>> = MaybeUninit::uninit();
                let removed_ptr: UninitializedComponent =
                    &mut removed_buf as *mut _ as UninitializedComponent;

                unsafe {
                    world.unbind_component(entity, *handle, removed_ptr);
                }

                let removed_value = unsafe { removed_buf.assume_init() };
                assert_eq!(removed_value, Some(comp_id));
            }
        }

        // Remove all entities
        for &entity in &entities {
            world.remove_entity(entity);
        }

        // Verify entity ID reuse after removal
        for _ in 0..entities.len() {
            let recycled = world.new_entity().unwrap();
            assert!(entities.iter().any(|&e| e.id() == recycled.id()));
            assert!(entities.iter().all(|&e| e.ver() != recycled.ver()));
        }
    }
}

/// Public ECS world interface for managing entities and components.
///
/// Provides type-safe methods for entity and component management, wrapping a `RawWorld`.
macro_rules! impl_world {
    ($t: ident, $r: ty, $e: ty) => {
        /// The ECS world, managing entities and their components.
        pub struct $t {
            raw: $r,
        }

        impl Default for $t {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $t {
            /// Registers a component type for use in this world.
            ///
            /// Must be called exactly once for a given component type before binding any components of that type.
            ///
            /// # Safety
            ///
            /// - Caller must ensure the component type’s handle is set correctly and safely.
            /// - This method must be called exactly once for the component type `T` before any components of type `T` are inserted using `bind_component`.
            #[inline]
            unsafe fn register<T: Component>() {
                let handle = unsafe { <$r>::register(|| Box::new(ComponentStorage::<$e, T>::new()) ) };
                unsafe { T::set_handle(handle) };
                debug_assert!(unsafe { T::handle() } == handle);
            }

            /// Creates a new, empty ECS world.
            #[inline]
            pub fn new() -> Self {
                Self {
                    raw: <$r>::new(),
                }
            }

            /// Allocates a new entity.
            ///
            /// Returns `Some(entity)` on success, `None` if the entity pool is exhausted.
            #[inline]
            pub fn new_entity(&mut self) -> Option<$e> {
                self.raw.new_entity()
            }

            /// Removes an entity and all its associated components.
            #[inline]
            pub fn remove_entity(&mut self, entity: $e) {
                self.raw.remove_entity(entity);
            }

            /// Binds a component of type `T` to an entity.
            ///
            /// If a component of type `T` already exists, it is replaced and returned.
            pub fn bind_component<T: Component>(&mut self, entity: $e, value: T) -> Option<T> {
                let handle = unsafe { T::handle() };

                let mut old_value = MaybeUninit::<Option<T>>::uninit();
                unsafe {
                    self.raw.bind_component(
                        entity,
                        handle,
                        &value as *const T as ComponentPtr,
                        old_value.as_mut_ptr() as UninitializedComponent,
                    );
                    old_value.assume_init()
                }
            }

            /// Unbinds and removes a component of type `T` from an entity.
            ///
            /// Returns the removed component if it existed, otherwise `None`.
            pub fn unbind_component<T: Component>(&mut self, entity: $e) -> Option<T> {
                let handle = unsafe { T::handle() };

                let mut old_value = MaybeUninit::<Option<T>>::uninit();
                unsafe {
                    self.raw.unbind_component(
                        entity,
                        handle,
                        old_value.as_mut_ptr() as UninitializedComponent,
                    );
                    old_value.assume_init()
                }
            }

            /// Returns the number of entities currently holding this component.
            #[inline]
            pub fn len<T: Component>(&mut self) -> usize {
                let handle = unsafe { T::handle() };
                self.raw.len(handle)
            }

            /// Returns an iterator over all components of type `T`.
            ///
            /// Yields `(entity, &T)` pairs for every entity that has a component of type `T` bound.
            ///
            /// # Safety
            ///
            /// This method internally performs type-erased pointer casting. Safety relies on:
            /// - The `register::<T>()` function being correctly called exactly once before use.
            ///
            /// # Notes
            ///
            /// - The iteration order is not specified and should not be relied upon.
            #[inline]
            pub fn iter<T: Component>(&mut self) -> impl Iterator<Item = ($e, &T)> {
                let handle = unsafe { T::handle() };
                self.raw.iter(handle).map(|(e, p)| {
                    let pv = p as *const T;
                    (e, unsafe { &*pv })
                })
            }

            /// Returns a mutable iterator over all components of type `T`.
            ///
            /// Yields `(entity, &mut T)` pairs for every entity that has a component of type `T` bound,
            /// allowing in-place modification of the components.
            ///
            /// # Safety
            ///
            /// This method internally performs type-erased mutable pointer casting. Safety relies on:
            /// - The `register::<T>()` function being correctly called exactly once before use.
            ///
            /// # Notes
            ///
            /// - The iteration order is not specified and should not be relied upon.
            #[inline]
            pub fn iter_mut<T: Component>(&mut self) -> impl Iterator<Item = ($e, &mut T)> {
                let handle = unsafe { T::handle() };
                self.raw.iter_mut(handle).map(|(e, p)| {
                    let pv = p as *mut T;
                    (e, unsafe { &mut *pv })
                })
            }
        }
    };
}

#[cfg(target_pointer_width = "64")]
impl_world!(World64, RawWorld64, Entity64);

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_world!(World32, RawWorld32, Entity32);

impl_world!(World16, RawWorld16, Entity16);

/// Alias for the default world type based on the target platform.
///
/// - 64-bit: `World32`
/// - 32-bit: `World32`
/// - 16-bit: `World16`
///
/// `World64` is available on 64-bit platforms but not used by default due to its larger size.
#[cfg(target_pointer_width = "64")]
pub type DefaultWorld = World32;

#[cfg(target_pointer_width = "32")]
pub type DefaultWorld = World32;

#[cfg(target_pointer_width = "16")]
pub type DefaultWorld = World16;

#[cfg(test)]
mod world_tests {
    use std::collections::HashSet;

    use super::*;
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct Position(i32, i32);

    static mut __HANDLE_POSITION: MaybeUninit<ComponentHandle> = MaybeUninit::uninit();

    unsafe impl Component for Position {
        #[allow(static_mut_refs)]
        unsafe fn set_handle(handle: ComponentHandle) {
            unsafe { __HANDLE_POSITION.write(handle) };
        }

        unsafe fn handle() -> ComponentHandle {
            unsafe { __HANDLE_POSITION.assume_init() }
        }
    }

    #[test]
    fn test_entity_creation_and_removal() {
        let mut world = World32::new();
        let entity = world.new_entity().expect("Failed to create entity");
        world.remove_entity(entity);
        let recyled_entity = world.new_entity().unwrap();
        assert_ne!(entity, recyled_entity);
        assert_eq!(entity.id(), recyled_entity.id());
        assert_ne!(entity.ver(), recyled_entity.ver());
    }

    #[test]
    fn test_bind_and_unbind_component() {
        RawWorld32::get_registry().write().unwrap().clear();
        let mut world = World32::new();
        unsafe {
            World32::register::<Position>();
        }
        let entity = world.new_entity().expect("Failed to create entity");

        // Bind component
        let old = world.bind_component(entity, Position(10, 20));
        assert!(old.is_none());

        // Overwrite component
        let replaced = world.bind_component(entity, Position(30, 40));
        assert_eq!(replaced, Some(Position(10, 20)));

        // Unbind component
        let removed = world.unbind_component::<Position>(entity);
        assert_eq!(removed, Some(Position(30, 40)));

        // Unbind again (should be None)
        let removed_again = world.unbind_component::<Position>(entity);
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_iter() {
        RawWorld32::get_registry().write().unwrap().clear();
        let mut world = World32::new();
        unsafe {
            World32::register::<Position>();
        }

        const POSITION_COUNT: usize = 1024;
        let mut expected = HashSet::new();
        for i in 0..POSITION_COUNT {
            let entity = world.new_entity().unwrap();
            let pos = Position(i as i32, (i * 2) as i32);
            world.bind_component(entity, pos);
            expected.insert((entity, pos));
        }
        assert_eq!(world.len::<Position>(), POSITION_COUNT);

        let result: HashSet<_> = world.iter::<Position>().map(|(e, p)| (e, *p)).collect();

        assert_eq!(expected, result);
    }

    #[test]
    fn test_iter_mut() {
        RawWorld32::get_registry().write().unwrap().clear();
        let mut world = World32::new();
        unsafe {
            World32::register::<Position>();
        }

        const POSITION_COUNT: usize = 1024;
        let mut initial = HashSet::new();
        for i in 0..POSITION_COUNT {
            let entity = world.new_entity().unwrap();
            let pos = Position(i as i32, (i * 2) as i32);
            world.bind_component(entity, pos);
            initial.insert((entity, pos));
        }
        assert_eq!(world.len::<Position>(), POSITION_COUNT);

        for (_entity, pos) in world.iter_mut::<Position>() {
            pos.0 += 1;
            pos.1 += 1;
        }

        let expected: HashSet<_> = initial
            .into_iter()
            .map(|(e, Position(x, y))| (e, Position(x + 1, y + 1)))
            .collect();

        let result: HashSet<_> = world.iter::<Position>().map(|(e, p)| (e, *p)).collect();

        assert_eq!(expected, result);
    }
}
