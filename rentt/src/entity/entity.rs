#[allow(unused_imports)]
use std::num::{NonZeroU16, NonZeroU32};

pub(crate) trait Entt: Copy + Clone + PartialEq + Eq {
    unsafe fn new(ver: usize, idx: usize) -> Self;
    fn ver(&self) -> usize;
    fn index(&self) -> usize;
}

#[cfg(any(target_pointer_width="32", target_pointer_width="64"))]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Entity {
    raw: NonZeroU32,
}

#[cfg(any(target_pointer_width="32", target_pointer_width="64"))]
impl Entity {
    const VER_SHIFT: usize = 24;
    const VER_MASK: usize = 0x0000_0000_FF00_0000;
    const IDX_MASK: usize = 0x0000_0000_00FF_FFFF;
}

#[cfg(any(target_pointer_width="32", target_pointer_width="64"))]
impl Entt for Entity {
    unsafe fn new(ver: usize, idx: usize) -> Self {
        debug_assert!(ver != 0);
        debug_assert!(ver <= (Self::VER_MASK >> Self::VER_SHIFT));
        debug_assert!(idx & Self::IDX_MASK == idx);

        let ver = ver as u32;
        let idx = idx as u32;

        let raw = unsafe {
            NonZeroU32::new_unchecked(ver << Self::VER_SHIFT | idx)
        };

        Self { raw }
    }

    #[inline]
    fn ver(&self) -> usize {
        (self.raw.get() >> Self::VER_SHIFT) as usize
    }

    #[inline]
    fn index(&self) -> usize {
        (self.raw.get() & (Self::IDX_MASK as u32)) as usize
    }
}

#[cfg(target_pointer_width="16")]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Entity {
    raw: NonZeroU16,
}

#[cfg(target_pointer_width="16")]
impl Entity {
    const VER_SHIFT: usize = 12;
    const VER_MASK: usize = 0x0000_0000_0000_F000;
    const IDX_MASK: usize = 0x0000_0000_0000_0FFF;
}

#[cfg(target_pointer_width="16")]
impl Entt for Entity {
    unsafe fn new(ver: usize, idx: usize) -> Self {
        debug_assert!(ver != 0);
        debug_assert!(ver <= (Self::VER_MASK >> Self::VER_SHIFT));
        debug_assert!(idx & Self::IDX_MASK == idx);

        let ver = ver as u16;
        let idx = idx as u16;

        let raw = unsafe {
            NonZeroU16::new_unchecked(ver << Self::VER_SHIFT | idx)
        };

        Self { raw }
    }

    #[inline]
    fn ver(&self) -> usize {
        (self.raw.get() >> Self::VER_SHIFT) as usize
    }

    #[inline]
    fn index(&self) -> usize {
        (self.raw.get() & (Self::IDX_MASK as u16)) as usize
    }
}