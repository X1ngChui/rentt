use crate::utils::Subscript;
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

pub trait EntityOps: Copy {
    type Ver: Subscript;
    type Id: Subscript;

    fn new(ver: Self::Ver, id: Self::Id) -> Option<Self>;
    unsafe fn new_unchecked(ver: Self::Ver, id: Self::Id) -> Self;
    fn ver(&self) -> Self::Ver;
    fn id(&self) -> Self::Id;
    fn next_ver(self) -> Self;
    fn locate<const N: usize>(&self) -> (Self::Id, Self::Id);
}

macro_rules! impl_entity {
    ($name:ident, $nonzero:ty, $ver_type:ty, $id_type:ty, $ver_shift:expr, $id_mask:expr) => {
        #[repr(transparent)]
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        pub struct $name {
            raw: $nonzero,
        }

        impl $name {
            const VER_SHIFT: u8 = $ver_shift;
            const ID_MASK: $id_type = $id_mask;
        }

        impl EntityOps for $name {
            type Ver = $ver_type;
            type Id = $id_type;

            #[inline]
            fn new(ver: Self::Ver, id: Self::Id) -> Option<Self> {
                if ver == 0 || id > Self::ID_MASK {
                    None
                } else {
                    let raw = (ver as $id_type) << Self::VER_SHIFT | id;
                    debug_assert!(raw != 0);
                    Some(Self {
                        raw: unsafe { <$nonzero>::new_unchecked(raw) },
                    })
                }
            }

            #[inline]
            unsafe fn new_unchecked(ver: Self::Ver, id: Self::Id) -> Self {
                debug_assert!(ver != 0);
                debug_assert!(id <= Self::ID_MASK);

                let raw = (ver as $id_type) << Self::VER_SHIFT | id;
                Self {
                    raw: unsafe { <$nonzero>::new_unchecked(raw) },
                }
            }

            #[inline]
            fn ver(&self) -> Self::Ver {
                (self.raw.get() >> Self::VER_SHIFT) as Self::Ver
            }

            #[inline]
            fn id(&self) -> Self::Id {
                self.raw.get() & Self::ID_MASK
            }

            #[inline]
            fn next_ver(self) -> Self {
                let new_ver = match self.ver().wrapping_add(1) {
                    0 => 1,
                    n => n,
                };

                let id = self.id();

                unsafe { Self::new_unchecked(new_ver, id) }
            }

            #[inline]
            fn locate<const N: usize>(&self) -> (Self::Id, Self::Id) {
                let subscript = self.id().into_subscript();
                let index = (subscript / N) as Self::Id;
                let offset = (subscript % N) as Self::Id;
                (index, offset)
            }
        }
    };
}

impl_entity!(Entity64, NonZeroU64, u16, u64, 48, 0x0000_FFFF_FFFF_FFFF);
impl_entity!(Entity32, NonZeroU32, u8, u32, 24, 0x00FF_FFFF);

pub type Entity = Entity32;
