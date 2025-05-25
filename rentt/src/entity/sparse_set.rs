use super::{Entt, Subscript};

#[allow(dead_code)]
struct Pool<K: Entt, V, I: Subscript, const S: usize> {
    keys: Box<[I; S]>,
    values: Vec<(K, V)>,
}

#[allow(dead_code)]
impl<K: Entt, V, I: Subscript, const S: usize> Pool<K, V, I, S> {
    const NULL: I = I::MAX;

    fn new() -> Self {
        Self {
            keys: Box::new([Self::NULL; S]),
            values: Vec::new(),
        }
    }

    unsafe fn insert(&mut self, key: K, value: V) {
        let index = self.values.len();
        debug_assert!(index != Self::NULL.into());

        self.values.push((key, value));
        unsafe {
            *self.keys.get_unchecked_mut(key.index()) = I::from(index);
        }
    }
}
