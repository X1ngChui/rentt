//! ECS Entity composition module.
//!
//! Provides strongly typed entity handles composed of an `EntityId` and an `EntityVer`.
//! Entities are compactly packed into non-zero integer values for fast storage, comparisons,
//! and option optimizations.
//!
//! This implementation supports multiple entity formats with varying ID and version bit widths.

use crate::entity_fields::{EntityId, EntityVer, Id12, Id24, Id48, Ver4, Ver8, Ver16};
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

/// A trait representing a strongly typed entity handle.
///
/// Entities are uniquely identified by a pair of ID and version.
/// Implementations guarantee correct packing and unpacking of these fields.
pub trait Entity: Copy + Clone + PartialEq + Eq {
    /// The ID type associated with the entity.
    type Id: EntityId;
    /// The version type associated with the entity.
    type Ver: EntityVer;

    /// Creates a new entity with the given ID and default version (`Ver::MIN`).
    fn new(id: Self::Id) -> Self;

    /// Creates an entity from an explicit ID and version.
    fn combine(id: Self::Id, ver: Self::Ver) -> Self;

    /// Extracts the entity ID from this handle.
    fn id(&self) -> Self::Id;

    /// Extracts the entity version from this handle.
    fn ver(&self) -> Self::Ver;

    /// Advances the entity version, wrapping if necessary.
    fn next_ver(self) -> Self;
}

/// Macro to implement compact entity packing for specific ID and version formats.
macro_rules! impl_entity {
    ($t:ident, $r:ty, $id:ty, $ver:ty, $doc: literal) => {
        #[doc = $doc]
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        pub struct $t {
            raw: $r,
        }

        impl Entity for $t {
            type Id = $id;
            type Ver = $ver;

            #[inline]
            fn new(id: Self::Id) -> Self {
                Self::combine(id, Self::Ver::MIN)
            }

            #[inline]
            fn combine(id: Self::Id, ver: Self::Ver) -> Self {
                let id = id.get();
                let ver = ver.get() as <Self::Id as EntityId>::Base;
                let raw = (ver << Self::Id::BITS) | id;
                debug_assert!(raw != 0);
                Self {
                    raw: unsafe { <$r>::new_unchecked(raw) },
                }
            }

            #[inline]
            fn id(&self) -> Self::Id {
                let id = self.raw.get() & Self::Id::MAX.get();
                unsafe { Self::Id::new_unchecked(id) }
            }

            #[inline]
            fn ver(&self) -> Self::Ver {
                let ver = (self.raw.get() >> Self::Id::BITS) as <Self::Ver as EntityVer>::Base;
                unsafe { Self::Ver::new_unchecked(ver) }
            }

            #[inline]
            fn next_ver(self) -> Self {
                Self::combine(self.id(), self.ver().next())
            }
        }
    };
}

#[cfg(target_pointer_width = "64")]
impl_entity!(Entity64, NonZeroU64, Id48, Ver16, "64-bit Entity: 48 bits ID + 16 bits version.");

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_entity!(Entity32, NonZeroU32, Id24, Ver8, "32-bit Entity: 24 bits ID + 8 bits version.");

impl_entity!(Entity16, NonZeroU16, Id12, Ver4, "16-bit Entity: 12 bits ID + 4 bits version.");

/// Default entity type depending on platform word size.
#[cfg(target_pointer_width = "64")]
pub type DefaultEntity = Entity32;

#[cfg(target_pointer_width = "32")]
pub type DefaultEntity = Entity32;

#[cfg(target_pointer_width = "16")]
pub type DefaultEntity = Entity16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity16() {
        let id = Id12::new(0x0FFF).unwrap();
        let ver = Ver4::new(0x0F).unwrap();
        let e = Entity16::combine(id, ver);
        assert_eq!(e.id().get(), 0x0FFF);
        assert_eq!(e.ver().get(), 0x0F);
        let e2 = e.next_ver();
        assert_eq!(e2.ver().get(), 1);
    }

    #[test]
    fn test_entity32() {
        let id = Id24::new(0x00FF_FFFF).unwrap();
        let ver = Ver8::new(0xFF).unwrap();
        let e = Entity32::combine(id, ver);
        assert_eq!(e.id().get(), 0x00FF_FFFF);
        assert_eq!(e.ver().get(), 0xFF);
        let e2 = e.next_ver();
        assert_eq!(e2.ver().get(), 1);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_entity64() {
        let id = Id48::new(0x0000_FFFF_FFFF_FFFF).unwrap();
        let ver = Ver16::new(0xFFFF).unwrap();
        let e = Entity64::combine(id, ver);
        assert_eq!(e.id().get(), 0x0000_FFFF_FFFF_FFFF);
        assert_eq!(e.ver().get(), 0xFFFF);
        let e2 = e.next_ver();
        assert_eq!(e2.ver().get(), 1);
    }
}
