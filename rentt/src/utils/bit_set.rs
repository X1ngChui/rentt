pub(crate) struct BitSet<T> {
    bits: T,
}

impl<T> BitSet<T> {
    pub(crate) fn new(bits: T) -> Self {
        Self { bits }
    }
}

pub(crate) struct BitSetIterator<T> {
    remain_bits: T,
}

macro_rules! impl_bit_set {
    ($t: ty) => {
        impl IntoIterator for BitSet<$t> {
            type Item = u32;
            type IntoIter = BitSetIterator<$t>;

            fn into_iter(self) -> Self::IntoIter {
                BitSetIterator {
                    remain_bits: self.bits,
                }
            }
        }

        impl Iterator for BitSetIterator<$t> {
            type Item = u32;

            fn next(&mut self) -> Option<Self::Item> {
                if self.remain_bits == 0 {
                    None
                } else {
                    let index = self.remain_bits.trailing_zeros();
                    self.remain_bits &= !(1 << index);
                    Some(index)
                }
            }
        }
    };
}

impl_bit_set!(u8);
impl_bit_set!(u16);
impl_bit_set!(u32);
impl_bit_set!(u64);
impl_bit_set!(u128);
impl_bit_set!(usize);
