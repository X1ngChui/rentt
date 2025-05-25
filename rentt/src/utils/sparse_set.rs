#[allow(dead_code)]
struct Pool<K: From<usize> + Into<usize> + Copy, V, const S: usize> {
    keys: Box<[Option<K>; S]>,
    values: Vec<(K, V)>,
}

#[allow(dead_code)]
impl<K: From<usize> + Into<usize> + Copy, V, const S: usize> Pool<K, V, S> {
    fn new() -> Self {
        Self {
            keys: Box::new([None; S]),
            values: Vec::new(),
        }
    }

    fn insert(&mut self, key: K, value: V) {
        let index = K::from(self.values.len());
        self.values.push((index, value));
        unsafe {
            *self.keys.get_unchecked_mut(key.into()) = Some(index);
        }
    }
}
