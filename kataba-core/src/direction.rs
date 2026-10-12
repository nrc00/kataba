//! Direction of movement or selection.

/// Direction of cursor movement or range orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Forward (left to right, top to bottom).
    Forward,
    /// Backward (right to left, bottom to top).
    Backward,
}

impl Direction {
    /// Returns `true` if this direction is forward.
    #[must_use]
    pub const fn is_forward(self) -> bool {
        matches!(self, Self::Forward)
    }

    /// Returns `true` if this direction is backward.
    #[must_use]
    pub const fn is_backward(self) -> bool {
        matches!(self, Self::Backward)
    }

    /// Returns the opposite direction.
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
        }
    }
}