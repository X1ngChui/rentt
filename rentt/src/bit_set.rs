#![allow(dead_code)]

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BitSet {
    bits: usize,
}

impl BitSet {
    pub(crate) const BITS: usize = usize::BITS as usize;

    #[inline]
    pub(crate) fn new(bits: usize) -> Self {
        Self { bits }
    }

    #[inline]
    pub(crate) unsafe fn insert(&mut self, value: usize) {
        debug_assert!(value < Self::BITS);
        self.bits |= 1 << value;
    }

    #[inline]
    pub(crate) unsafe fn remove(&mut self, value: usize) {
        debug_assert!(value < Self::BITS);
        self.bits &= !(1 << value);
    }

    #[inline]
    pub(crate) fn contains(&self, value: usize) -> bool {
        debug_assert!(value < Self::BITS);
        (self.bits & (1 << value)) != 0
    }

    #[inline]
    pub(crate) fn iter(&self) -> BitSetIter {
        BitSetIter {
            remaining: self.bits,
        }
    }
}

pub(crate) struct BitSetIter {
    remaining: usize,
}

impl Iterator for BitSetIter {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let bit = self.remaining.trailing_zeros() as usize;
        self.remaining &= self.remaining - 1;
        Some(bit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_contains() {
        let mut bs = BitSet::default();
        unsafe {
            bs.insert(1);
            bs.insert(3);
            bs.insert(62);
        }

        assert!(bs.contains(1));
        assert!(bs.contains(3));
        assert!(bs.contains(62));
        assert!(!bs.contains(0));
        assert!(!bs.contains(5));
    }

    #[test]
    fn test_remove() {
        let mut bs = BitSet::default();
        unsafe {
            bs.insert(1);
            bs.insert(3);
            bs.insert(62);
            bs.remove(3);
        }

        assert!(bs.contains(1));
        assert!(!bs.contains(3));
        assert!(bs.contains(62));
    }

    #[test]
    fn test_iteration() {
        let mut bs = BitSet::default();
        unsafe {
            bs.insert(1);
            bs.insert(3);
            bs.insert(62);
        }

        let mut items: Vec<usize> = bs.iter().collect();
        items.sort(); // because order is not guaranteed

        assert_eq!(items, vec![1, 3, 62]);
    }

    #[test]
    fn test_empty() {
        let bs = BitSet::default();
        let items: Vec<usize> = bs.iter().collect();
        assert!(items.is_empty());
    }
}
