use std::ptr;

use super::Subscript;

#[allow(dead_code)]
struct Pool<V, I: Subscript, const S: usize> {
    keys: Box<[I; S]>,
    values: Vec<(I, V)>,
}

#[allow(dead_code)]
impl<V, I: Subscript, const S: usize> Pool<V, I, S> {
    const NULL: I = I::MAX;

    fn new() -> Self {
        debug_assert!(Self::NULL.into() >= S);
        Self {
            keys: Box::new([Self::NULL; S]),
            values: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    unsafe fn contain(&self, index: I) -> bool {
        debug_assert!(index.into() < S);

        unsafe {
            *self.keys.get_unchecked(index.into()) != Self::NULL
        }

    }

    unsafe fn insert(&mut self, index: I, value: V) {
        debug_assert!(index.into() < S);
        debug_assert!(unsafe { !self.contain(index) });

        let value_index = self.values.len();
        self.values.push((index, value));

        unsafe {
            *self.keys.get_unchecked_mut(index.into()) = I::from(value_index);
        }
    }

    unsafe fn get_unchecked(&self, index: I) -> &V {
        debug_assert!(index.into() < S);

        let value_index = unsafe { *self.keys.get_unchecked(index.into()) };
        debug_assert_ne!(value_index, Self::NULL);

        let pair = unsafe { self.values.get_unchecked(value_index.into()) };
        debug_assert_eq!(pair.0, index);

        &pair.1
    }

    unsafe fn get_unchecked_mut(&mut self, index: I) -> &mut V {
        debug_assert!(index.into() < S);

        let value_index = unsafe { *self.keys.get_unchecked(index.into()) };
        debug_assert_ne!(value_index, Self::NULL);

        let pair = unsafe { self.values.get_unchecked_mut(value_index.into()) };
        debug_assert_eq!(pair.0, index);

        &mut pair.1
    }

    unsafe fn remove(&mut self, index: I) -> V {
        debug_assert!(index.into() < S);
        debug_assert!(unsafe { self.contain(index) });

        let itarget_value_index = unsafe { *self.keys.get_unchecked_mut(index.into()) };
        debug_assert_ne!(itarget_value_index, Self::NULL);
        let target_value_index = itarget_value_index.into();

        // Swap and remove without bound check
        let len = self.values.len();
        unsafe {
            let base = self.values.as_mut_ptr();

            let (index_from_target, target_value) = ptr::read(base.add(target_value_index));
            debug_assert_eq!(index, index_from_target);

            ptr::copy(base.add(len - 1), base.add(target_value_index.into()), 1);
            let back_index = self.values.get_unchecked(target_value_index).0;
            self.values.set_len(len - 1);

            *self.keys.get_unchecked_mut(back_index.into()) = itarget_value_index;
            *self.keys.get_unchecked_mut(index.into()) = Self::NULL;
            
            target_value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::U8Subscript;

    #[test]
    fn test_pool_new() {
        let _ = Pool::<String, U8Subscript, 255>::new();
    }

    #[test]
    fn test_pool_insert() {
        let mut pool = Pool::<String, U8Subscript, 255>::new();

        let subscript = U8Subscript::from(42);
        unsafe {
            pool.insert(subscript, "42".to_string());
            assert!(pool.contain(subscript));
            assert_eq!(pool.get_unchecked(subscript), "42");
        }
    }

    #[test]
    fn test_pool_remove() {
        let mut pool = Pool::<String, U8Subscript, 255>::new();

        let subscript = U8Subscript::from(42);
        unsafe {
            pool.insert(subscript, "42".to_string());
            assert!(pool.contain(subscript));
            assert_eq!(pool.get_unchecked(subscript), "42");

            let value = pool.remove(subscript);
            assert_eq!(value, "42");
            assert!(!pool.contain(subscript));
        }
    }

    #[test]
    fn test_pool_insert_remove_complex() {
        let mut pool = Pool::<String, U8Subscript, 255>::new();

        // Step 1: Insert multiple elements
        let keys = [
            U8Subscript::from(10),
            U8Subscript::from(20),
            U8Subscript::from(30),
            U8Subscript::from(40),
            U8Subscript::from(254), // Near capacity
        ];
        let values = ["ten", "twenty", "thirty", "forty", "near_max"];

        unsafe {
            for (key, value) in keys.iter().zip(values.iter()) {
                pool.insert(*key, value.to_string());
                assert!(pool.contain(*key));
                assert_eq!(pool.get_unchecked(*key), *value);
            }

            // Verify all keys are present and correct
            assert_eq!(pool.len(), 5, "Pool should contain 5 elements");
            for (key, value) in keys.iter().zip(values.iter()) {
                assert!(pool.contain(*key));
                assert_eq!(pool.get_unchecked(*key), *value);
            }

            // Step 2: Remove elements in non-sequential order (middle, first, last)
            let remove_order = [30, 10, 254];
            let expected_removed = ["thirty", "ten", "near_max"];
            for (key, expected) in remove_order.iter().zip(expected_removed.iter()) {
                let key = U8Subscript::from(*key);
                let value = pool.remove(key);
                assert_eq!(value, *expected);
                assert!(!pool.contain(key));
            }

            // Verify remaining elements (keys 20 and 40)
            assert_eq!(pool.len(), 2, "Pool should contain 2 elements after removals");
            assert!(pool.contain(U8Subscript::from(20)), "Key 20 should remain");
            assert!(pool.contain(U8Subscript::from(40)), "Key 40 should remain");
            assert_eq!(pool.get_unchecked(U8Subscript::from(20)), "twenty");
            assert_eq!(pool.get_unchecked(U8Subscript::from(40)), "forty");

            // Step 3: Re-insert into previously used keys
            pool.insert(U8Subscript::from(10), "ten_again".to_string());
            assert!(pool.contain(U8Subscript::from(10)));
            assert_eq!(pool.get_unchecked(U8Subscript::from(10)), "ten_again");

            // Step 4: Test near capacity
            pool.insert(U8Subscript::from(254), "max_again".to_string());
            assert!(pool.contain(U8Subscript::from(254)));
            assert_eq!(pool.get_unchecked(U8Subscript::from(254)), "max_again");
        }
    }
}