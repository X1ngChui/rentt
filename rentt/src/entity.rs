//! ECS Entity Composition Module
//!
//! This module provides strongly typed entity handles for an Entity Component System (ECS).
//!
//! Each entity handle consists of two components:
//! - An `EntityId` (unique identifier).
//! - An `EntityVer` (version counter).
//!
//! The ID and version are compactly packed into non-zero integer types (`NonZeroU16`, `NonZeroU32`, `NonZeroU64`)
//! to enable efficient storage, fast comparisons, and zero-cost `Option<Entity>` optimizations.
//!
//! # Supported Entity Formats
//!
//! | Entity Type | Total Size | ID Bits | Version Bits | Max Live Entities   | Platforms      |
//! | ----------- | ---------- | ------- | ------------ | ------------------- | -------------- |
//! | `Entity16`  | 16-bit     | 12      | 4            | 4,096               | All platforms  |
//! | `Entity32`  | 32-bit     | 24      | 8            | 16,777,216          | 32-bit & 64-bit|
//! | `Entity64`  | 64-bit     | 48      | 16           | 281,474,976,710,656 | 64-bit only    |
//!
//! > **Note**: Availability of certain entity types depends on `target_pointer_width` due to platform constraints.
//!
//! # Bit Packing Layout
//!
//! Entities are stored as a single non-zero integer with the following layout:
//!
//! ```text
//! [ version bits | id bits ]
//! ```
//!
//! - The lower bits (width = `Id::BITS`) represent the entity ID.
//! - The upper bits represent the entity version.
//!
//! This enables fast extraction and combination of ID and version via simple bitwise operations.
//!
//! # Traits
//!
//! - `Entity`: Public trait exposing only safe accessors `id()` and `ver()`.
//! - `EntityInternal`: Crate-private trait that includes internal construction and version update methods.
//!   This trait extends `Entity` but is not exposed publicly to enforce encapsulation.
//!
//! # Implementation Details
//!
//! The `impl_entity!` macro generates concrete entity structs (e.g., `Entity16`, `Entity32`, `Entity64`)
//! with packed storage and implements both `Entity` and `EntityInternal` for them.
//!
//! This design cleanly separates public API from internal implementation details, ensuring
//! that only safe methods are accessible externally.
//!
//! # Default Entity Type Alias
//!
//! To simplify usage, a `DefaultEntity` alias is provided depending on the platform:
//!
//! | Platform | DefaultEntity |
//! |----------|---------------|
//! | 64-bit   | `Entity32`    |
//! | 32-bit   | `Entity32`    |
//! | 16-bit   | `Entity16`    |
//!
//! `Entity64` is excluded from the default alias due to its large size, but can be opted-in explicitly if needed.

use crate::entity_fields::{EntityId, EntityVer, Id12, Id24, Id48, Ver4, Ver8, Ver16};
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

/// Public trait representing a strongly typed ECS entity handle.
///
/// Provides read-only access to entity ID and version.
///
/// Implementations should pack ID and version compactly but expose only safe, immutable accessors.
pub trait Entity: Copy + Clone + PartialEq + Eq {
    /// The ID type used by this entity.
    type Id: EntityId;
    /// The version type used by this entity.
    type Ver: EntityVer;

    /// Returns the entity's unique ID.
    fn id(&self) -> Self::Id;

    /// Returns the entity's version.
    fn ver(&self) -> Self::Ver;
}


/// Internal trait extending `Entity` with entity construction and version management methods.
///
/// This trait is crate-private and intended for internal use only, hiding implementation details from public API.
///
/// It provides methods to:
/// - Create new entities with a default version.
/// - Construct entities from explicit ID and version.
/// - Advance the entity version.
pub(crate) trait EntityInternal: Entity {
    /// Creates a new entity with the given ID and the minimum version.
    fn new(id: Self::Id) -> Self;

    /// Creates an entity from explicit ID and version.
    fn combine(id: Self::Id, ver: Self::Ver) -> Self;

    /// Returns a new entity with the same ID and the next version.
    fn next_ver(self) -> Self;
}

/// Macro to implement a concrete entity type with compact ID and version packing.
///
/// This macro generates:
/// - A struct with packed `raw` storage of non-zero integer type.
/// - Implementation of the public `Entity` trait exposing `id()` and `ver()`.
/// - Implementation of the internal `EntityInternal` trait for construction and version advancement.
///
/// Parameters:
/// - `$t`: Entity struct name (e.g., `Entity32`).
/// - `$r`: Raw storage type (e.g., `NonZeroU32`).
/// - `$id`: ID type implementing `EntityId` (e.g., `Id24`).
/// - `$ver`: Version type implementing `EntityVer` (e.g., `Ver8`).
/// - `$doc`: Documentation string for the entity type.
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
            fn id(&self) -> Self::Id {
                let id = self.raw.get() & Self::Id::MASK;
                unsafe { Self::Id::new_unchecked(id) }
            }

            #[inline]
            fn ver(&self) -> Self::Ver {
                let ver = (self.raw.get() >> Self::Id::BITS) as <Self::Ver as EntityVer>::Base;
                unsafe { Self::Ver::new_unchecked(ver) }
            }
        }

        impl EntityInternal for $t {
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
            fn next_ver(self) -> Self {
                Self::combine(self.id(), self.ver().next())
            }
        }
    };
}

// 64-bit entity: 48-bit ID + 16-bit version (64-bit platforms only)
#[cfg(target_pointer_width = "64")]
impl_entity!(
    Entity64,
    NonZeroU64,
    Id48,
    Ver16,
    "64-bit Entity: 48 bits ID + 16 bits version (only available on 64-bit platforms)."
);

// 32-bit entity: 24-bit ID + 8-bit version (32-bit and 64-bit platforms)
#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl_entity!(
    Entity32,
    NonZeroU32,
    Id24,
    Ver8,
    "32-bit Entity: 24 bits ID + 8 bits version (available on 32-bit and 64-bit platforms)."
);

// 16-bit entity: 12-bit ID + 4-bit version (all platforms)
impl_entity!(
    Entity16,
    NonZeroU16,
    Id12,
    Ver4,
    "16-bit Entity: 12 bits ID + 4 bits version (always available on all platforms)."
);

/// Default entity type alias based on platform.
///
/// This alias selects an entity optimized for typical usage on the target platform.
/// `Entity64` is excluded by default due to its larger size but can be opted-in explicitly.
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
