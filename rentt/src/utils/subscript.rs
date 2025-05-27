use std::ops::Div;

pub unsafe trait Subscript: Copy + PartialEq + Eq {
    const MAX: Self;
    fn from_subscript(subscript: usize) -> Self;
    fn into_subscript(self) -> usize;
}

macro_rules! impl_unsigned {
    ($t: ty) => {
        unsafe impl Subscript for $t {
            const MAX: Self = <$t>::MAX;

            #[inline]
            fn from_subscript(subscript: usize) -> Self {
                subscript as Self
            }

            #[inline]
            fn into_subscript(self) -> usize {
                self as usize
            }
        }
    };
}

impl_unsigned!(usize);
impl_unsigned!(u8);
impl_unsigned!(u16);

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_unsigned!(u32);

#[cfg(target_pointer_width = "64")]
impl_unsigned!(u64);
