//! Position encoding types for LSP compatibility.

/// The encoding used for position columns in a document.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum PositionEncoding {
    /// UTF-8 encoding (byte offsets).
    Utf8,
    /// UTF-16 encoding (code unit offsets, used by LSP 3.x).
    #[default]
    Utf16,
    /// UTF-32 encoding (code point offsets).
    Utf32,
}

impl PositionEncoding {
    /// Returns the default position encoding (UTF-16).
    #[must_use]
    pub const fn default() -> Self {
        Self::Utf16
    }

    /// Returns `true` if this encoding is UTF-16.
    #[must_use]
    pub const fn is_utf16(self) -> bool {
        matches!(self, Self::Utf16)
    }

    /// Returns `true` if this encoding is UTF-8.
    #[must_use]
    pub const fn is_utf8(self) -> bool {
        matches!(self, Self::Utf8)
    }

    /// Returns `true` if this encoding is UTF-32.
    #[must_use]
    pub const fn is_utf32(self) -> bool {
        matches!(self, Self::Utf32)
    }
}

impl fmt::Display for PositionEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf8 => write!(f, "utf-8"),
            Self::Utf16 => write!(f, "utf-16"),
            Self::Utf32 => write!(f, "utf-32"),
        }
    }
}

impl std::str::FromStr for PositionEncoding {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "utf-8" | "utf8" => Ok(Self::Utf8),
            "utf-16" | "utf16" => Ok(Self::Utf16),
            "utf-32" | "utf32" => Ok(Self::Utf32),
            _ => Err(()),
        }
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        assert_eq!(PositionEncoding::default(), PositionEncoding::Utf16);
    }

    #[test]
    fn test_is_utf16() {
        assert!(PositionEncoding::Utf16.is_utf16());
        assert!(!PositionEncoding::Utf8.is_utf16());
        assert!(!PositionEncoding::Utf32.is_utf16());
    }

    #[test]
    fn test_is_utf8() {
        assert!(PositionEncoding::Utf8.is_utf8());
        assert!(!PositionEncoding::Utf16.is_utf8());
        assert!(!PositionEncoding::Utf32.is_utf8());
    }

    #[test]
    fn test_is_utf32() {
        assert!(PositionEncoding::Utf32.is_utf32());
        assert!(!PositionEncoding::Utf8.is_utf32());
        assert!(!PositionEncoding::Utf16.is_utf32());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", PositionEncoding::Utf8), "utf-8");
        assert_eq!(format!("{}", PositionEncoding::Utf16), "utf-16");
        assert_eq!(format!("{}", PositionEncoding::Utf32), "utf-32");
    }

    #[test]
    fn test_from_str() {
        use std::str::FromStr;
        assert_eq!(PositionEncoding::from_str("utf-8"), Ok(PositionEncoding::Utf8));
        assert_eq!(PositionEncoding::from_str("utf8"), Ok(PositionEncoding::Utf8));
        assert_eq!(PositionEncoding::from_str("utf-16"), Ok(PositionEncoding::Utf16));
        assert_eq!(PositionEncoding::from_str("utf16"), Ok(PositionEncoding::Utf16));
        assert_eq!(PositionEncoding::from_str("utf-32"), Ok(PositionEncoding::Utf32));
        assert_eq!(PositionEncoding::from_str("utf32"), Ok(PositionEncoding::Utf32));
        assert!(PositionEncoding::from_str("invalid").is_err());
    }
}