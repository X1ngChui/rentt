//! Strongly Typed ECS Entity IDs and Versions
//!
//! This module defines strongly typed entity identifiers and versions for use in an ECS (Entity Component System).
//!
//! IDs and versions are wrapped using `NonMax` and `NonZero` integer types to ensure correctness both at compile-time and runtime.
//!
//! # Entity IDs
//!
//! Each entity ID type represents a bounded, non-negative integer with fixed bit width.
//! This allows for compact storage, safe indexing, and efficient packing in composite storage formats.
//!
//! ## Available ID Types
//!
//! | Type  | Bit Width | Platforms |
//! |-------|-----------|-----------|
//! | `Id12` | 12 bits   | All       |
//! | `Id24` | 24 bits   | 32-bit, 64-bit |
//! | `Id48` | 48 bits   | 64-bit only |
//!
//! ## About `MASK`
//!
//! Each `EntityId` implementation defines a `MASK` constant, which serves **two purposes**:
//!
//! 1. **Maximum valid value**: any ID greater than `MASK` is invalid.
//! 2. **Bit mask for extraction**: when IDs are stored inside larger packed integers (e.g. when combining ID and version),
//!    `MASK` can be used to extract the ID portion via bitwise AND.
//!
//! ### Bit-Packing Example
//!
//! ```ignore
//! let packed: NonZeroU32 = ...;
//! let id_bits = packed.get() & Id24::MASK;
//! let id = Id24::new(id_bits).unwrap();
//! ```
//!
//! # Entity Versions
//!
//! Versions are used for generational indices, preventing accidental reuse of stale entity handles.
//! Versions are always non-zero and automatically wrap back to their minimum value after reaching the maximum.
//!
//! ## Available Version Types
//!
//! | Type  | Bit Width | Range |
//! |-------|-----------|-------|
//! | `Ver4`  | 4 bits | 1..=15 |
//! | `Ver8`  | 8 bits | 1..=255 |
//! | `Ver16` | 16 bits | 1..=65535 |

use nonmax::{NonMaxU16, NonMaxU32, NonMaxU64};
use std::num::{NonZeroU8, NonZeroU16};

/// Trait for strongly typed entity identifiers.
///
/// Implementations enforce non-zero, bounded integer IDs with a defined maximum range.
pub trait EntityId: Clone + Copy + PartialEq + Eq {
    /// The underlying primitive integer type.
    type Base;

    /// The number of bits used for this ID.
    const BITS: u32;

    /// The maximum allowed ID value.
    ///
    /// Also serves as a bitmask for extracting ID bits from packed integer representations.
    const MASK: Self::Base;

    /// Creates a new ID if the provided value is valid (i.e., `value <= MASK`).
    fn new(value: Self::Base) -> Option<Self>;

    /// Creates a new ID without checking validity.
    ///
    /// # Safety
    ///
    /// Caller must ensure that `value <= MASK`.
    unsafe fn new_unchecked(value: Self::Base) -> Self;

    /// Returns the underlying raw integer value.
    fn get(&self) -> Self::Base;

    /// Converts this ID into a `usize` index.
    fn into_index(&self) -> usize;

    /// Creates an ID from a `usize` index without checking.
    ///
    /// # Safety
    ///
    /// Caller must ensure that `index <= MASK as usize`.
    unsafe fn from_index(index: usize) -> Self;
}

/// Internal macro for implementing `EntityId` using a `NonMax` wrapper.
macro_rules! impl_entity_id_for_nonmax {
    ($t:ident, $r:ty, $b:ty, $bits:expr, $mask:expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        /// Entity ID type with fixed bit width.
        pub struct $t {
            raw: $r,
        }

        impl EntityId for $t {
            type Base = $b;
            const BITS: u32 = $bits;
            const MASK: Self::Base = $mask;

            #[inline]
            fn new(value: Self::Base) -> Option<Self> {
                if value > Self::MASK {
                    None
                } else {
                    Some(unsafe { Self::new_unchecked(value) })
                }
            }

            #[inline]
            unsafe fn new_unchecked(value: Self::Base) -> Self {
                debug_assert!(value <= Self::MASK);
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
                debug_assert!(index <= Self::MASK as usize);
                unsafe { Self::new_unchecked(index as Self::Base) }
            }
        }
    };
}

// ID Implementations:

#[cfg(target_pointer_width = "64")]
impl_entity_id_for_nonmax!(Id48, NonMaxU64, u64, 48, 0x0000_FFFF_FFFF_FFFF);

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_entity_id_for_nonmax!(Id24, NonMaxU32, u32, 24, 0x00FF_FFFF);

impl_entity_id_for_nonmax!(Id12, NonMaxU16, u16, 12, 0x0FFF);

/// Trait for strongly typed entity versions.
///
/// Versions are always non-zero, bounded integers that wrap to `MIN` after reaching `MAX`.
pub trait EntityVer: Clone + Copy + PartialEq + Eq {
    /// The underlying primitive integer type.
    type Base;

    /// Minimum valid version (always non-zero).
    const MIN: Self;

    /// Maximum allowed version.
    const MAX: Self;

    /// Creates a new version if value is valid (`value != 0 && value <= MAX`).
    fn new(value: Self::Base) -> Option<Self>;

    /// Creates a version without checking validity.
    ///
    /// # Safety
    ///
    /// Caller must ensure `value != 0 && value <= MAX`.
    unsafe fn new_unchecked(value: Self::Base) -> Self;

    /// Returns the raw integer value.
    fn get(&self) -> Self::Base;

    /// Returns the next version, wrapping if necessary.
    fn next(self) -> Self;
}

/// Internal macro for implementing `EntityVer` using `NonZero` wrapper.
macro_rules! impl_entity_ver_for_nonzero {
    ($t:ident, $r:ty, $b:ty, $max:expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        /// Entity version type with wrapping behavior.
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
                debug_assert!(value != 0 && value <= Self::MAX.get());
                Self {
                    raw: unsafe { <$r>::new_unchecked(value) },
                }
            }

            #[inline]
            fn get(&self) -> Self::Base {
                self.raw.get()
            }

            #[inline]
            #[allow(unused_comparisons)]
            fn next(self) -> Self {
                match self.raw.checked_add(1) {
                    Some(n) if n.get() <= Self::MAX.get() => Self { raw: n },
                    _ => Self::MIN,
                }
            }
        }
    };
}

// Version Implementations:

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
