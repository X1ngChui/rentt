use crate::entity::Entity;
use crate::entity_fields::EntityId;
use std::marker::PhantomData;
use std::{hint::unreachable_unchecked, ptr};

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

#[derive(Debug)]
struct Pool<T, I: EntityId> {
    indices: Box<[Option<I>]>,
    values: Vec<T>,
}

impl<T, I: EntityId> Pool<T, I> {
    fn new(capacity: I) -> Self {
        Self {
            indices: vec![None; capacity.into_usize()].into_boxed_slice(),
            values: Vec::new(),
        }
    }

    fn contains(&self, index: usize) -> bool {
        if index >= self.indices.len() {
            false
        } else {
            unsafe { self.contains_unchecked(index) }
        }
    }

    unsafe fn contains_unchecked(&self, index: usize) -> bool {
        debug_assert!(index < self.indices.len());
        match unsafe { self.indices.get_unchecked(index) } {
            None => false,
            Some(_) => true,
        }
    }

    fn get(&self, index: usize) -> Option<&T> {
        if self.contains(index) {
            Some(unsafe { self.get_unchecked(index) })
        } else {
            None
        }
    }

    unsafe fn get_unchecked(&self, index: usize) -> &T {
        debug_assert!(unsafe { self.contains_unchecked(index) });

        let index = match unsafe { self.indices.get_unchecked(index) } {
            None => unsafe { unreachable_unchecked() },
            Some(i) => *i,
        };

        unsafe { self.values.get_unchecked(index.into_usize()) }
    }
}

#[cfg(test)]
mod tests {}
