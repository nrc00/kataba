//! Point type for representing a position in a document using line and UTF-16 column.

use std::fmt;

/// A position in a document represented as a line number and UTF-16 column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Point {
    /// The line number (0-indexed).
    pub line: u32,
    /// The UTF-16 column offset within the line.
    pub col_utf16: u32,
}

impl Point {
    /// Creates a new point.
    #[must_use]
    pub const fn new(line: u32, col_utf16: u32) -> Self {
        Self { line, col_utf16 }
    }

    /// Returns `true` if this point is at the origin (0, 0).
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.line == 0 && self.col_utf16 == 0
    }

    /// Saturating addition of two points.
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            line: self.line.saturating_add(other.line),
            col_utf16: self.col_utf16.saturating_add(other.col_utf16),
        }
    }

    /// Saturating subtraction of two points.
    #[must_use]
    pub fn saturating_sub(self, other: Self) -> Self {
        Self {
            line: self.line.saturating_sub(other.line),
            col_utf16: self.col_utf16.saturating_sub(other.col_utf16),
        }
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col_utf16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_new() {
        let p = Point::new(5, 10);
        assert_eq!(p.line, 5);
        assert_eq!(p.col_utf16, 10);
    }

    #[test]
    fn test_point_is_zero() {
        assert!(Point::new(0, 0).is_zero());
        assert!(!Point::new(0, 1).is_zero());
        assert!(!Point::new(1, 0).is_zero());
    }

    #[test]
    fn test_point_saturating_add() {
        let p1 = Point::new(5, 10);
        let p2 = Point::new(3, 7);
        assert_eq!(p1.saturating_add(p2), Point::new(8, 17));

        let p1 = Point::new(u32::MAX, u32::MAX);
        let p2 = Point::new(1, 1);
        assert_eq!(p1.saturating_add(p2), Point::new(u32::MAX, u32::MAX));
    }

    #[test]
    fn test_point_saturating_sub() {
        let p1 = Point::new(10, 20);
        let p2 = Point::new(3, 7);
        assert_eq!(p1.saturating_sub(p2), Point::new(7, 13));

        let p1 = Point::new(5, 5);
        let p2 = Point::new(10, 10);
        assert_eq!(p1.saturating_sub(p2), Point::new(0, 0));
    }

    #[test]
    fn test_point_display() {
        assert_eq!(format!("{}", Point::new(0, 0)), "0:0");
        assert_eq!(format!("{}", Point::new(5, 10)), "5:10");
    }

    #[test]
    fn test_point_ord() {
        assert!(Point::new(0, 0) < Point::new(0, 1));
        assert!(Point::new(0, 1) < Point::new(1, 0));
        assert!(Point::new(1, 0) > Point::new(0, 10));
    }
}