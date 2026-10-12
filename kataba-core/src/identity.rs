//! Identity types for buffers and document versions.

use std::sync::atomic::{AtomicU64, Ordering};

/// A unique identifier for a buffer.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct BufferId(pub u64);

impl BufferId {
    /// Creates a new unique buffer ID using a static atomic counter.
    #[must_use]
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Creates a buffer ID from a raw u64 value.
    #[must_use]
    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw u64 value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates a buffer ID from a URI string by hashing it with FNV-1a.
    ///
    /// Returns `None` only if the URI is empty.
    #[must_use]
    pub fn from_uri(uri: &str) -> Option<Self> {
        if uri.is_empty() {
            return None;
        }

        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in uri.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }

        Some(Self(hash))
    }
}

impl fmt::Display for BufferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BufferId({})", self.0)
    }
}

/// A document version number.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version(pub u32);

impl Version {
    /// Creates a new version starting at 1.
    #[must_use]
    pub const fn new() -> Self {
        Self(1)
    }

    /// Returns the version as a u32.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Returns the next version.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Version({})", self.0)
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_id_new() {
        let id1 = BufferId::new();
        let id2 = BufferId::new();
        assert_ne!(id1, id2);
        assert!(id1.as_u64() >= 1);
        assert!(id2.as_u64() >= 1);
    }

    #[test]
    fn test_buffer_id_from_raw() {
        let id = BufferId::from_raw(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_buffer_id_display() {
        let id = BufferId::from_raw(123);
        assert_eq!(format!("{}", id), "BufferId(123)");
    }

    #[test]
    fn test_version_new() {
        let v = Version::new();
        assert_eq!(v.as_u32(), 1);
    }

    #[test]
    fn test_version_next() {
        let v = Version::new();
        let v2 = v.next();
        assert_eq!(v2.as_u32(), 2);
        assert_eq!(v2.next().as_u32(), 3);
    }

    #[test]
    fn test_version_display() {
        let v = Version(5);
        assert_eq!(format!("{}", v), "Version(5)");
    }

    #[test]
    fn test_version_ord() {
        assert!(Version(1) < Version(2));
        assert!(Version(5) > Version(3));
    }
}