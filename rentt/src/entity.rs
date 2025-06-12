//! ECS Entity Composition Module
//!
//! This module defines strongly typed entity handles for an Entity Component System (ECS).
//!
//! Each entity handle is composed of two fields:
//! - An `EntityId` (identifier).
//! - An `EntityVer` (version counter).
//!
//! The ID and version are compactly packed into non-zero integer types (`NonZeroU16`, `NonZeroU32`, `NonZeroU64`)
//! for efficient storage, comparisons, and optimizations such as `Option<Entity>` representation with no extra overhead.
//!
//! # Supported Entity Formats
//!
//! | Entity Type | Total Size | ID Bits | Version Bits | Max Live Entities   | Platforms      |
//! | ----------- | ---------- | ------- | ------------ | ------------------- | -------------- |
//! | `Entity16`  | 16-bit     | 12      | 4            | 4,096               | All            |
//! | `Entity32`  | 32-bit     | 24      | 8            | 16,777,216          | 32-bit, 64-bit |
//! | `Entity64`  | 64-bit     | 48      | 16           | 281,474,976,710,656 | 64-bit only    |

//! 
//!
//! > **Note**: Due to platform-specific constraints, some types are conditionally available based on `target_pointer_width`.
//!
//! # Packing and Bit Layout
//!
//! The entity value is encoded as a single non-zero integer as follows:
//!
//! ```text
//! [ version bits | id bits ]
//! ```
//!
//! - The lower `Id::BITS` bits store the entity ID.
//! - The upper remaining bits store the entity version.
//!
//! This allows fast extraction of both fields via simple bitwise operations, while maintaining strong typing.

use crate::entity_fields::{EntityId, EntityVer, Id12, Id24, Id48, Ver4, Ver8, Ver16};
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

/// Trait representing a strongly typed ECS entity handle.
///
/// Each entity is uniquely identified by a combination of an ID and a version.
/// Implementations define how the ID and version are packed into a single compact value.
pub trait Entity: Copy + Clone + PartialEq + Eq {
    /// The ID type used by this entity.
    type Id: EntityId;
    /// The version type used by this entity.
    type Ver: EntityVer;

    /// Creates a new entity with the given ID and default version (`Ver::MIN`).
    fn new(id: Self::Id) -> Self;

    /// Creates an entity from an explicit ID and version.
    fn combine(id: Self::Id, ver: Self::Ver) -> Self;

    /// Extracts the entity ID.
    fn id(&self) -> Self::Id;

    /// Extracts the entity version.
    fn ver(&self) -> Self::Ver;

    /// Returns a new entity with the same ID but advanced version (`ver.next()`).
    fn next_ver(self) -> Self;
}

/// Macro to implement compact entity packing for specific ID and version combinations.
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
                let id = self.raw.get() & Self::Id::MASK;
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

// 64-bit entity: 48-bit ID + 16-bit version (available only on 64-bit platforms)
#[cfg(target_pointer_width = "64")]
impl_entity!(
    Entity64,
    NonZeroU64,
    Id48,
    Ver16,
    "64-bit Entity: 48 bits ID + 16 bits version (only available on 64-bit platforms)."
);

// 32-bit entity: 24-bit ID + 8-bit version (available on both 32-bit and 64-bit platforms)
#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_entity!(
    Entity32,
    NonZeroU32,
    Id24,
    Ver8,
    "32-bit Entity: 24 bits ID + 8 bits version (available on 32-bit and 64-bit platforms)."
);

// 16-bit entity: 12-bit ID + 4-bit version (always available)
impl_entity!(
    Entity16,
    NonZeroU16,
    Id12,
    Ver4,
    "16-bit Entity: 12 bits ID + 4 bits version (always available on all platforms)."
);

/// Default entity type depending on target platform.
///
/// This alias provides a convenient default entity type optimized for most use cases:
///
/// | Platform | DefaultEntity |
/// |----------|----------------|
/// | 64-bit   | `Entity32`     |
/// | 32-bit   | `Entity32`     |
/// | 16-bit   | `Entity16`     |
///
/// > Note: `Entity64` is not selected by default on 64-bit platforms for better memory efficiency.
/// > You can opt-in to `Entity64` if your application needs very large entity capacity.

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
