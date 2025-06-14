#![allow(dead_code)]

//! ECS World implementation.
//!
//! Provides basic entity-component management with automatic component registration.
//!
//! This implementation supports any `Entity` type defined by the crate
//! (e.g., `Entity16`, `Entity32`, `Entity64`) and any `Component` type
//! that does not contain references.

use std::mem::{MaybeUninit, replace};

use crate::bit_set::BitSet;
use crate::component::{
    Component, ComponentEntry, ComponentHandle, ComponentPtr, ComponentStorage,
    UninitializedComponent,
};
use crate::entity::{Entity, EntityInternal};
use crate::entity_fields::EntityId;
use crate::entity_map::EntityMap;

/// Internal node representing a component storage entry in the component tree.
///
/// `ComponentNode` forms part of a multi-way complete tree structure that organizes
/// all component storages hierarchically.
///
/// The tree structure works as:
/// - Root node is at level 0.
/// - Each node has up to `ORDER` child nodes, where `ORDER` equals the bit width of `BitSet`.
/// - Each entity maintains its presence in subtrees through per-node BitSets.
///
/// This tree allows efficiently skipping entire subtrees during iteration when the entity
/// does not have any components under certain branches.
///
/// # Type parameters
/// - `E`: The entity identifier type.
struct ComponentNode<E: Entity> {
    /// Type-erased component storage backend for this node.
    entry: Option<Box<dyn ComponentEntry<E>>>,
    /// Per-entity bit set indicating which subtrees contain active components.
    bit_signs: EntityMap<E, BitSet>,
}

impl<E: Entity> Default for ComponentNode<E> {
    fn default() -> Self {
        Self {
            entry: None,
            bit_signs: EntityMap::new(),
        }
    }
}

impl<E: Entity> ComponentNode<E> {
    /// Tree branching factor (equals to number of bits in `BitSet`).
    const ORDER: usize = BitSet::BITS;

    /// Creates a new component node with the given storage backend.
    #[inline]
    fn new(entry: Option<Box<dyn ComponentEntry<E>>>) -> Self {
        Self {
            entry,
            bit_signs: EntityMap::new(),
        }
    }

    /// Inserts a component value into the storage, returning previous value into `old_value`.
    ///
    /// # Safety
    /// - `value` must point to a valid component instance.
    /// - `old_value` must point to uninitialized storage able to receive the previous value.
    #[inline]
    unsafe fn insert(&mut self, entity: E, value: ComponentPtr, old_value: UninitializedComponent) {
        if let Some(entry) = &mut self.entry {
            unsafe {
                entry.insert(entity, value, old_value);
            }
        }
    }

    /// Inserts without retrieving old value.
    ///
    /// # Safety
    /// - `value` must point to a valid component instance.
    #[inline]
    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr) {
        if let Some(entry) = &mut self.entry {
            unsafe {
                entry.insert_without_value(entity, value);
            }
        }
    }

    /// Removes component value for the given entity, writing previous value into `removed`.
    ///
    /// # Safety
    /// - `removed` must point to uninitialized memory for receiving old value.
    #[inline]
    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent) {
        if let Some(entry) = &mut self.entry {
            unsafe {
                entry.remove(entity, removed);
            }
        }
    }

    /// Removes component value for the given entity, ignoring old value.
    #[inline]
    fn remove_without_value(&mut self, entity: E) {
        if let Some(entry) = &mut self.entry {
            entry.remove_without_value(entity);
        }
    }

    /// Sets presence of the entity in a specific child subtree by inserting the given sign bit.
    ///
    /// # Safety
    /// - `sign` must be in `0..Self::ORDER`.
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

    /// Clears presence of the entity from a specific child subtree.
    ///
    /// # Safety
    /// - `sign` must be in `0..Self::ORDER`.
    #[inline]
    unsafe fn remove_sign(&mut self, entity: E, sign: usize) {
        debug_assert!(sign < Self::ORDER);

        if let Some(signs) = self.bit_signs.get_mut(entity) {
            unsafe { signs.remove(sign) };
        }
    }
}

/// Core storage implementation of the entity-component system, handling only type-erased data.
///
/// `RawWorld` maintains:
/// - Entity ID allocation and reuse.
/// - The component tree structure.
/// - Binding/unbinding components to entities at low-level.
///
/// This layer has no knowledge of concrete component types.
struct RawWorld<E: Entity> {
    /// Type-erased complete tree of component storages.
    components: Vec<ComponentNode<E>>,

    /// Next available entity id for allocation.
    next_entity_id: Option<E::Id>,

    /// Pool of previously removed entity ids for reuse.
    removed_entities: Vec<E>,
}

impl<E: Entity> Default for RawWorld<E>
where
    E: EntityInternal,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Entity> RawWorld<E>
where
    E: EntityInternal,
{
    /// The branching factor of the tree.
    const ORDER: usize = ComponentNode::<E>::ORDER;

    /// Constructs a new empty world.
    #[inline]
    fn new() -> Self {
        Self {
            components: Vec::new(),
            next_entity_id: Some(<E::Id as EntityId>::MIN),
            removed_entities: Vec::new(),
        }
    }

    /// Returns true if the component type has been registered.
    #[inline]
    fn is_registered(&self, component_handle: ComponentHandle) -> bool {
        match self.components.get(component_handle.index()) {
            None => false,
            Some(node) => node.entry.is_some(),
        }
    }

    /// Registers (or replaces) a component entry for the specified handle, returning any previous entry.
    #[inline]
    fn register_component(
        &mut self,
        component_handle: ComponentHandle,
        entry: Box<dyn ComponentEntry<E>>,
    ) -> Option<Box<dyn ComponentEntry<E>>> {
        let component_index = component_handle.index();
        let new_node = ComponentNode::new(Some(entry));

        match self.components.get_mut(component_index) {
            None => {
                // If the index is out of bounds, we need to grow the components vector.

                // Resize the vector up to component_index (exclusive), filling with default ComponentNodes.
                self.components
                    .resize_with(component_index, || ComponentNode::default());

                // After resize, append the new node to occupy the exact component_index slot.
                self.components.push(new_node);

                // No previous entry existed at this index, return None.
                None
            }
            Some(target) => {
                // The index is within bounds; replace the existing node with the new one.
                let old_node = replace(target, new_node);

                // Return the previous entry (if any).
                old_node.entry
            }
        }
    }

    /// Allocates a new entity.
    ///
    /// Reuses recycled IDs first, then allocates sequentially.
    fn new_entity(&mut self) -> Option<E> {
        if let Some(entity) = self.removed_entities.pop() {
            return Some(entity.next_ver());
        }

        self.next_entity_id.map(|now| {
            let entity = E::new(now);
            self.next_entity_id = now.next();
            entity
        })
    }

    /// Internal recursive helper to remove all components for the entity.
    fn remove_entity_helper(&mut self, entity: E, root: usize) {
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

    /// Removes an entity entirely from the world.
    #[inline]
    fn remove_entity(&mut self, entity: E) {
        self.removed_entities.push(entity);
        self.remove_entity_helper(entity, 0);
    }

    /// Binds a component instance to an entity.
    ///
    /// Inserts value and updates tree upwards to track which branches are active.
    #[inline]
    unsafe fn bind_component(
        &mut self,
        entity: E,
        component_handle: ComponentHandle,
        value: ComponentPtr,
        old_value: UninitializedComponent,
    ) {
        debug_assert!(component_handle.index() < self.components.len());
        let component_index = component_handle.index();
        let component_node = unsafe { self.components.get_unchecked_mut(component_index) };

        unsafe {
            component_node.insert(entity, value, old_value);
        }

        // Propagate bit signs up the tree
        let mut now = component_index;
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

    /// Unbinds component instance from entity without updating bit signs.
    ///
    /// This leaves stale bits which can be cleaned lazily or ignored.
    #[inline]
    unsafe fn unbind_component(
        &mut self,
        entity: E,
        component_handle: ComponentHandle,
        old_value: UninitializedComponent,
    ) {
        debug_assert!(component_handle.index() < self.components.len());
        let component_index = component_handle.index();
        let component_node = unsafe { self.components.get_unchecked_mut(component_index) };

        unsafe {
            component_node.remove(entity, old_value);
        }
    }
}

#[cfg(test)]
mod raw_world_tests {
    use super::*;
    use crate::component::{ComponentPtr, ComponentStorage, UninitializedComponent};
    use crate::entity::Entity32;
    use std::mem::MaybeUninit;

    #[test]
    fn test_raw_world_insert_and_remove() {
        let mut world = RawWorld::<Entity32>::new();

        // Register component storage
        let storage = ComponentStorage::<Entity32, i32>::new();
        let handle = ComponentHandle::new(0);
        world.register_component(handle, Box::new(storage));

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
        let mut world = RawWorld::<Entity32>::new();

        let storage_a = ComponentStorage::<Entity32, i32>::new();
        let storage_b = ComponentStorage::<Entity32, u64>::new();

        let handle_a = ComponentHandle::new(1);
        let handle_b = ComponentHandle::new(10);
        assert!(
            world
                .register_component(handle_a, Box::new(storage_a))
                .is_none()
        );
        assert!(
            world
                .register_component(handle_b, Box::new(storage_b))
                .is_none()
        );

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
        let mut world = RawWorld::<Entity32>::new();

        // Dynamically register a large number of components
        let mut handles = Vec::new();
        for i in 0..COMPONENT_COUNT {
            let storage = ComponentStorage::<Entity32, usize>::new();
            let handle = ComponentHandle::new(i);
            world.register_component(handle, Box::new(storage));
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

/// Represents the ECS world, managing entities and their components.
///
/// The generic parameter `E` must implement the public `Entity` trait.
///
/// Note: Although `Entity` is publicly implementable, only the built-in entity types
/// (`Entity16`, `Entity32`, `Entity64`) also implement the private `EntityInternal` trait.
/// This means that only these types can be used with `World` to access its full functionality.
pub struct World<E: Entity> {
    raw: RawWorld<E>,
}

impl<E: Entity> Default for World<E>
where
    E: EntityInternal,
{
    fn default() -> Self {
        Self::new()
    }
}

/// A container for managing entities and components.
///
/// Each `World` owns a set of entities and their associated components.
/// Components are automatically registered upon first use.
#[allow(private_bounds)]
impl<E: Entity> World<E>
where
    E: EntityInternal,
{
    /// Creates a new empty world.
    #[inline]
    pub fn new() -> Self {
        Self {
            raw: RawWorld::new(),
        }
    }

    /// Allocates a new entity.
    ///
    /// Returns `Some(entity)` if successful, or `None` if the entity pool is exhausted.
    #[inline]
    pub fn new_entity(&mut self) -> Option<E> {
        self.raw.new_entity()
    }

    /// Removes an existing entity and all its associated components.
    #[inline]
    pub fn remove_entity(&mut self, entity: E) {
        self.raw.remove_entity(entity);
    }

    /// Lazily registers the component type `T` if it hasn't been registered yet.
    ///
    /// Returns the corresponding `ComponentHandle`.
    #[inline]
    fn try_register<T: Component>(&mut self) -> ComponentHandle {
        let handle = T::handle();
        if !self.raw.is_registered(handle) {
            let entry = Box::new(ComponentStorage::<E, T>::new());
            self.raw.register_component(handle, entry);
        }
        handle
    }

    /// Binds a component instance of type `T` to the given entity.
    ///
    /// If the entity already has a component of type `T`, it will be replaced and returned.
    pub fn bind_component<T: Component>(&mut self, entity: E, value: T) -> Option<T> {
        let handle = self.try_register::<T>();

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

    /// Unbinds and removes a component of type `T` from the given entity.
    ///
    /// Returns the removed component if it existed.
    pub fn unbind_component<T: Component>(&mut self, entity: E) -> Option<T> {
        let handle = self.try_register::<T>();

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
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::entity::Entity32;

    #[derive(Debug, PartialEq)]
    struct Position(i32, i32);

    impl Component for Position {}

    #[test]
    fn test_entity_creation_and_removal() {
        let mut world = World::<Entity32>::new();
        let entity = world.new_entity().expect("Failed to create entity");
        world.remove_entity(entity);
    }

    #[test]
    fn test_bind_and_unbind_component() {
        let mut world = World::<Entity32>::new();
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
}
