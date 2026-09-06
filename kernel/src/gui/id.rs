//! Widget ID — Stable identifiers for immediate-mode widget state tracking.
//!
//! Inspired by egui's `Id` system: widgets need stable IDs so that state
//! (scroll position, text cursor, collapsed/expanded, etc.) persists across
//! frames even though widgets are rebuilt every frame.
//!
//! IDs are computed via FNV-1a hashing for speed and low collision rate.///
/// A unique identifier for a widget, computed from a hierarchy of salts.
/// Two widgets with the same `Id` share the same persistent state.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id(u64);

impl Id {
    /// The null / root ID.
    pub const NULL: Id = Id(0);

    /// Create an ID from a raw u64.
    #[inline]
    pub const fn new(value: u64) -> Self {
        Id(value)
    }

    /// Create an ID from a string label.
    pub fn from_str(s: &str) -> Self {
        Id(fnv1a_hash_bytes(s.as_bytes()))
    }

    /// Create an ID from an integer.
    #[inline]
    pub const fn from_u64(v: u64) -> Self {
        // Mix bits so sequential IDs spread well
        Id(v.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }

    /// Combine this ID with another salt to produce a child ID.
    /// This is how hierarchical IDs work: `parent.with("child_name")`.
    #[inline]
    pub fn with(self, salt: &str) -> Id {
        Id(fnv1a_hash_u64_and_bytes(self.0, salt.as_bytes()))
    }

    /// Combine with an integer salt.
    #[inline]
    pub fn with_index(self, index: usize) -> Id {
        let bytes = (index as u64).to_le_bytes();
        Id(fnv1a_hash_u64_and_bytes(self.0, &bytes))
    }

    /// Get the raw value.
    #[inline]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl core::fmt::Debug for Id {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Id({:#018x})", self.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FNV-1a Hash — fast, non-cryptographic hash for widget IDs
// ═══════════════════════════════════════════════════════════════════════

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001B3;

#[inline]
fn fnv1a_hash_bytes(data: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[inline]
fn fnv1a_hash_u64_and_bytes(seed: u64, data: &[u8]) -> u64 {
    let mut hash = seed ^ FNV_OFFSET;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
