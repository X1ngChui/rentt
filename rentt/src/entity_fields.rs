//! ECS Entity ID and Version types.
//!
//! This module provides strongly typed entity IDs and versions for an ECS (Entity Component System).
//! IDs and versions are constrained using `NonMax` and `NonZero` wrappers to ensure correctness at compile-time and runtime.
//!
//! - `EntityId`: A trait for entity identifiers, disallowing zero and enforcing bit width limits.
//! - `EntityVer`: A trait for entity versions, disallowing zero and supporting wrapping semantics.

use nonmax::{NonMaxU16, NonMaxU32, NonMaxU64};
use std::num::{NonZeroU8, NonZeroU16};

/// Trait for strongly typed entity identifiers.
///
/// Implementations of this trait enforce non-zero, bounded integer IDs.
/// This allows representing entity IDs with various bit widths safely.
pub trait EntityId: Clone + Copy + PartialEq + Eq {
    /// The underlying integer type.
    type Base;

    /// The number of bits used by this ID.
    const BITS: u32;

    /// The maximum allowed ID value.
    const MAX: Self;

    /// Creates a new ID if the value is valid.
    fn new(value: Self::Base) -> Option<Self>;

    /// Creates a new ID without checking preconditions.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `value` is not exceed `Self::MAX.get().
    unsafe fn new_unchecked(value: Self::Base) -> Self;

    /// Returns the underlying integer value.
    fn get(&self) -> Self::Base;

    /// Converts this ID into a `usize` index.
    ///
    /// This conversion is infallible.
    fn into_index(&self) -> usize;

    /// Creates an ID from a `usize` index without checking.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `index` is not exceed `Self::MAX.get()`.
    unsafe fn from_index(index: usize) -> Self;
}

/// Implements `EntityId` for the given type using a `NonMax` wrapper.
macro_rules! impl_entity_id_for_nonmax {
    ($t: ident, $r: ty, $b: ty, $bits: expr, $max: expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        /// Entity ID type with fixed bit width.
        pub struct $t {
            raw: $r,
        }

        impl EntityId for $t {
            type Base = $b;
            const BITS: u32 = $bits;
            const MAX: Self = Self {
                raw: unsafe { <$r>::new_unchecked($max) },
            };

            #[inline]
            fn new(value: Self::Base) -> Option<Self> {
                if value > Self::MAX.get() {
                    None
                } else {
                    Some(unsafe { Self::new_unchecked(value) })
                }
            }

            #[inline]
            unsafe fn new_unchecked(value: Self::Base) -> Self {
                debug_assert!(value <= Self::MAX.get());

                Self {
                    raw: unsafe { <$r>::new_unchecked(value) },
                }
            }

            #[inline]
            fn get(&self) -> Self::Base {
                self.raw.get()
            }

            #[inline]
            fn into_index(&self) -> usize {
                self.get() as usize
            }

            #[inline]
            unsafe fn from_index(index: usize) -> Self {
                debug_assert!(index <= Self::MAX.get() as usize);
                unsafe {
                    Self::new_unchecked(index as Self::Base)
                }
            }
        }
    };
}

// Implementations of various ID sizes.
impl_entity_id_for_nonmax!(Id48, NonMaxU64, u64, 48, 0x0000_FFFF_FFFF_FFFF);
impl_entity_id_for_nonmax!(Id24, NonMaxU32, u32, 24, 0x00FF_FFFF);
impl_entity_id_for_nonmax!(Id12, NonMaxU16, u16, 12, 0x0FFF);

/// Trait for strongly typed entity versions.
///
/// Versions are always non-zero and wrap around when reaching their maximum value.
pub trait EntityVer: Clone + Copy + PartialEq + Eq {
    /// The underlying integer type.
    type Base;

    /// The minimum allowed version value.
    const MIN: Self;

    /// The maximum allowed version value.
    const MAX: Self;

    /// Creates a new version if the value is valid (non-zero and within range).
    fn new(value: Self::Base) -> Option<Self>;

    /// Creates a new version without checking preconditions.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `value != 0` and `value <= MAX.get()`.
    unsafe fn new_unchecked(value: Self::Base) -> Self;

    /// Returns the underlying integer value.
    fn get(&self) -> Self::Base;

    /// Returns the next version, wrapping to `MIN` if exceeding `MAX`.
    fn next(self) -> Self;
}

/// Implements `EntityVer` for the given type using a `NonZero` wrapper.
macro_rules! impl_entity_ver_for_nonzero {
    ($t: ident, $r: ty, $b: ty, $max: expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        /// Entity version type with wrapping semantics.
        pub struct $t {
            raw: $r,
        }

        impl EntityVer for $t {
            type Base = $b;
            const MIN: Self = Self { raw: <$r>::MIN };
            const MAX: Self = Self {
                raw: unsafe { <$r>::new_unchecked($max) },
            };

            #[inline]
            fn new(value: Self::Base) -> Option<Self> {
                if value == 0 || value > Self::MAX.get() {
                    None
                } else {
                    Some(unsafe { Self::new_unchecked(value) })
                }
            }

            #[inline]
            unsafe fn new_unchecked(value: Self::Base) -> Self {
                debug_assert!(value != 0);
                debug_assert!(value <= Self::MAX.get());

                Self {
                    raw: unsafe { <$r>::new_unchecked(value) },
                }
            }

            #[inline]
            fn get(&self) -> Self::Base {
                self.raw.get()
            }

            #[inline]
            fn next(self) -> Self {
                match self.raw.checked_add(1) {
                    None => Self::MIN,
                    Some(n) => {
                        if n.get() > Self::MAX.get() {
                            Self::MIN
                        } else {
                            Self { raw: n }
                        }
                    }
                }
            }
        }
    };
}

// Implementations of various version sizes.
impl_entity_ver_for_nonzero!(Ver16, NonZeroU16, u16, 0xFFFF);
impl_entity_ver_for_nonzero!(Ver8, NonZeroU8, u8, 0xFF);
impl_entity_ver_for_nonzero!(Ver4, NonZeroU8, u8, 0x0F);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id12() {
        assert!(Id12::new(0).is_some());
        assert!(Id12::new(0x1000).is_none());
        let id = Id12::new(0x0FFF).unwrap();
        assert_eq!(id.get(), 0x0FFF);

        // index conversions
        let index = id.into_index();
        assert_eq!(index, 0x0FFF);
        let id2 = unsafe { Id12::from_index(index) };
        assert_eq!(id, id2);
    }

    #[test]
    fn test_id24() {
        assert!(Id24::new(0).is_some());
        assert!(Id24::new(0x0100_0000).is_none());
        let id = Id24::new(0x00FF_FFFF).unwrap();
        assert_eq!(id.get(), 0x00FF_FFFF);

        let index = id.into_index();
        assert_eq!(index, 0x00FF_FFFF);
        let id2 = unsafe { Id24::from_index(index) };
        assert_eq!(id, id2);
    }

    #[test]
    fn test_id48() {
        assert!(Id48::new(0).is_some());
        assert!(Id48::new(0x0001_0000_0000_0000).is_none());
        let id = Id48::new(0x0000_FFFF_FFFF_FFFF).unwrap();
        assert_eq!(id.get(), 0x0000_FFFF_FFFF_FFFF);

        let index = id.into_index();
        assert_eq!(index, 0x0000_FFFF_FFFF_FFFF);
        let id2 = unsafe { Id48::from_index(index) };
        assert_eq!(id, id2);
    }

    #[test]
    fn test_ver4() {
        assert!(Ver4::new(0).is_none());
        assert!(Ver4::new(16).is_none());
        let ver = Ver4::new(15).unwrap();
        assert_eq!(ver.get(), 15);
        assert_eq!(ver.next().get(), 1);
    }

    #[test]
    fn test_ver8() {
        assert!(Ver8::new(0).is_none());
        let ver = Ver8::new(255).unwrap();
        assert_eq!(ver.get(), 255);
        assert_eq!(ver.next().get(), 1);
    }

    #[test]
    fn test_ver16() {
        assert!(Ver16::new(0).is_none());
        let ver = Ver16::new(65535).unwrap();
        assert_eq!(ver.get(), 65535);
        assert_eq!(ver.next().get(), 1);
    }
}
