#![allow(dead_code)]

use crate::bit_set::BitSet;
use crate::component::{ComponentEntry, ComponentPtr, UninitializedComponent};
use crate::entity::Entity;
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
    entry: Box<dyn ComponentEntry<E>>,
    /// Per-entity bit set indicating which subtrees contain active components.
    bit_signs: EntityMap<E, BitSet>,
}

impl<E: Entity> ComponentNode<E> {
    /// Tree branching factor (equals to number of bits in `BitSet`).
    const ORDER: usize = BitSet::BITS;

    /// Creates a new component node with the given storage backend.
    #[inline]
    fn new(entry: Box<dyn ComponentEntry<E>>) -> Self {
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
        unsafe {
            self.entry.insert(entity, value, old_value);
        }
    }

    /// Inserts without retrieving old value.
    ///
    /// # Safety
    /// - `value` must point to a valid component instance.
    #[inline]
    unsafe fn insert_without_value(&mut self, entity: E, value: ComponentPtr) {
        unsafe {
            self.entry.insert_without_value(entity, value);
        }
    }

    /// Removes component value for the given entity, writing previous value into `removed`.
    ///
    /// # Safety
    /// - `removed` must point to uninitialized memory for receiving old value.
    #[inline]
    unsafe fn remove(&mut self, entity: E, removed: UninitializedComponent) {
        unsafe {
            self.entry.remove(entity, removed);
        }
    }

    /// Removes component value for the given entity, ignoring old value.
    #[inline]
    fn remove_without_value(&mut self, entity: E) {
        self.entry.remove_without_value(entity);
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

/// Opaque handle representing a registered component inside the component tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentHadle {
    index: usize,
}

impl ComponentHadle {
    /// Creates a new handle for internal use.
    fn new(index: usize) -> Self {
        Self { index }
    }

    /// Returns internal index of the component.
    fn index(&self) -> usize {
        self.index
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

impl<E: Entity> Default for RawWorld<E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Entity> RawWorld<E> {
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

    /// Registers a new component type and returns its handle.
    #[inline]
    fn register_component(&mut self, entry: Box<dyn ComponentEntry<E>>) -> ComponentHadle {
        let handle = ComponentHadle::new(self.components.len());
        self.components.push(ComponentNode::new(entry));
        handle
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
        component_handle: ComponentHadle,
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
        component_handle: ComponentHadle,
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
mod tests {
    use super::*;
    use crate::component::{ComponentPtr, ComponentStorage, UninitializedComponent};
    use crate::entity::Entity32;
    use std::mem::MaybeUninit;

    #[test]
    fn test_raw_world_insert_and_remove() {
        let mut world = RawWorld::<Entity32>::new();

        // Register component storage
        let storage = ComponentStorage::<Entity32, i32>::new();
        let handle = world.register_component(Box::new(storage));

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

        let handle_a = world.register_component(Box::new(storage_a));
        let handle_b = world.register_component(Box::new(storage_b));

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
        for _ in 0..COMPONENT_COUNT {
            let storage = ComponentStorage::<Entity32, usize>::new();
            let handle = world.register_component(Box::new(storage));
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
