use std::hint::unreachable_unchecked;
use std::mem::replace;

#[derive(Debug)]
enum Cell<T> {
    Occupied(T),
    Free(Option<usize>),
}

#[derive(Debug )]
pub struct Block<T> {
    cells: Vec<Cell<T>>,
    free: Option<usize>,
    deleted: usize,
}

impl<T> Block<T> {
    pub fn new(size: usize) -> Self {
        Self {
            cells: Vec::with_capacity(size),
            free: None,
            deleted: 0,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        debug_assert!(self.cells.len() >= self.deleted);
        self.cells.len() - self.deleted
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == self.cells.capacity()
    }

    #[inline]
    pub fn insert(&mut self, value: T) -> usize {
        debug_assert!(!self.is_full());

        let cell = Cell::Occupied(value);

        if let Some(free_index) = self.free {
            debug_assert!(free_index < self.cells.len());

            match unsafe { self.cells.get_unchecked(free_index) } {
                Cell::Free(_) => {
                    let next = replace(unsafe { self.cells.get_unchecked_mut(free_index) }, cell);

                    match next {
                        Cell::Free(next_free) => {
                            self.free = next_free;
                            self.deleted -= 1;
                        }
                        _ => unsafe { unreachable_unchecked() }
                    }
                }
                _ => unsafe { unreachable_unchecked() },
            };

            free_index
        } else {
            let index = self.cells.len();
            self.cells.push(cell);
            index
        }
    }

    pub fn remove(&mut self, index: usize) -> T {
        debug_assert!(index < self.cells.len());

        match unsafe { self.cells.get_unchecked(index) } {
            Cell::Occupied(_) => {
                let cell = Cell::Free(self.free);
                self.free = Some(index);
                self.deleted += 1;
                let value = replace(unsafe { self.cells.get_unchecked_mut(index) }, cell);

                match value {
                    Cell::Occupied(value) => value,
                    _ => unsafe { unreachable_unchecked() },
                }
            }
            _ => unsafe { unreachable_unchecked() },
        }
    }

    pub unsafe fn get_unchecked(&self, index: usize) -> &T {
        debug_assert!(index < self.cells.len());
        match unsafe { self.cells.get_unchecked(index) } {
            Cell::Occupied(value) => value,
            Cell::Free(_) => unsafe { unreachable_unchecked() },
        }
    }

    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(index < self.cells.len());
        match unsafe { self.cells.get_unchecked_mut(index) } {
            Cell::Occupied(value) => value,
            Cell::Free(_) => unsafe { unreachable_unchecked() },
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter { inner: self.cells.iter() }
    }
}

pub struct Iter<'a, T> {
    inner: std::slice::Iter<'a, Cell<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(cell) = self.inner.next() {
            if let Cell::Occupied(v) = cell {
                return Some(v);
            }
        }
        None
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut block = Block::new(4);

        let a = block.insert(10);
        let b = block.insert(20);
        let c = block.insert(30);

        unsafe {
            assert_eq!(*block.get_unchecked(a), 10);
            assert_eq!(*block.get_unchecked(b), 20);
            assert_eq!(*block.get_unchecked(c), 30);
        }

        assert_eq!(block.len(), 3);
        assert!(!block.is_empty());
    }

    #[test]
    fn test_remove_and_reuse() {
        let mut block = Block::new(4);

        let a = block.insert(1);
        let b = block.insert(2);
        let c = block.insert(3);

        let val = block.remove(b);
        assert_eq!(val, 2);
        assert_eq!(block.deleted, 1);
        assert_eq!(block.len(), 2);

        let d = block.insert(42);
        assert_eq!(d, b);
        assert_eq!(block.deleted, 0);
        assert_eq!(block.len(), 3);

        unsafe {
            assert_eq!(*block.get_unchecked(a), 1);
            assert_eq!(*block.get_unchecked(c), 3);
            assert_eq!(*block.get_unchecked(d), 42);
        }
    }

    #[test]
    fn test_iter() {
        let mut block = Block::new(5);
        let a = block.insert(100);
        let b = block.insert(200);
        let c = block.insert(300);

        block.remove(b);

        let collected: Vec<_> = block.iter().copied().collect();
        assert_eq!(collected, vec![100, 300]);

        unsafe {
            assert_eq!(*block.get_unchecked(a), 100);
            assert_eq!(*block.get_unchecked(c), 300);
        }
    }
}
