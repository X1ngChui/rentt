use std::fmt::Debug;

pub(crate) trait Subscript: Debug + From<usize> + Into<usize> + Copy + Clone + PartialEq + Eq {
    type Base;
    const MAX: Self;
}

// Macro to implement wrapper types for unsigned integers
macro_rules! impl_subscript {
    ($($t:ty => $wrapper:ident),*) => {
        $(
            // Define the wrapper struct
            #[derive(Debug, Copy, Clone, PartialEq, Eq)]
            pub(crate) struct $wrapper($t);

            impl From<usize> for $wrapper {
                fn from(value: usize) -> Self {
                    $wrapper(value as $t)
                }
            }

            impl Into<usize> for $wrapper {
                fn into(self) -> usize {
                    self.0 as usize
                }
            }

            impl Subscript for $wrapper {
                type Base = $t;
                const MAX: Self = $wrapper(<$t>::MAX);
            }
        )*
    };
}

#[cfg(target_pointer_width="64")]
impl_subscript!(
    u64 => U64Subscript,
    u32 => U32Subscript,
    u16 => U16Subscript,
    u8 => U8Subscript
);

#[cfg(target_pointer_width="32")]
impl_subscript!(
    u32 => U32Subscript,
    u16 => U16Subscript,
    u8 => U8Subscript
);

#[cfg(target_pointer_width="16")]
impl_subscript!(
    u16 => U16Subscript,
    u8 => U8Subscript
);