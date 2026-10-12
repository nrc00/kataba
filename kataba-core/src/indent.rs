//! Indentation style for text editing.

/// Indentation style used by the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IndentStyle {
    /// Use tabs for indentation.
    #[default]
    Tabs,
    /// Use spaces for indentation.
    Spaces(usize),
}

impl IndentStyle {
    /// Returns the number of spaces per indent level.
    #[must_use]
    pub const fn tab_width(self) -> usize {
        match self {
            Self::Tabs => 1,
            Self::Spaces(n) => n,
        }
    }

    /// Returns `true` if this style uses tabs.
    #[must_use]
    pub const fn is_tabs(self) -> bool {
        matches!(self, Self::Tabs)
    }

    /// Returns `true` if this style uses spaces.
    #[must_use]
    pub const fn is_spaces(self) -> bool {
        matches!(self, Self::Spaces(_))
    }
}