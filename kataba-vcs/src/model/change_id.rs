//! Change ID / revision model placeholder for jj (M4).
//!
//! Kept here so the `jj` driver can share types with git without circular
//! deps. Git only uses `Oid` for SHA-1s.

use crate::model::Oid;

/// A jj change ID (hex, 20 bytes) — stable across ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChangeId([u8; 20]);

impl ChangeId {
    pub const EMPTY: Self = ChangeId([0; 20]);

    pub fn from_oid(oid: Oid) -> Self {
        ChangeId(oid.as_bytes())
    }

    pub fn as_oid(self) -> Oid {
        Oid(self.0)
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(40);
        for b in &self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}
