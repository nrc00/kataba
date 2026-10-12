//! Association of a position with a character boundary.

/// How a position should be associated with a character boundary
/// when mapping through changes.
///
/// This is used to determine what happens to a position that falls
/// exactly on a boundary when a change is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Assoc {
    /// Position is associated with the character before it.
    Before,
    /// Position is associated with the character after it.
    After,
    /// Position is associated with the start of the current word.
    BeforeWord,
    /// Position is associated with the end of the current word.
    AfterWord,
    /// Position is sticky — it stays at the same offset even if characters
    /// are inserted or deleted at that exact position.
    BeforeSticky,
    /// Position is sticky — it stays at the same offset even if characters
    /// are inserted or deleted at that exact position.
    AfterSticky,
}

impl Assoc {
    /// Returns `true` if this association is sticky.
    #[must_use]
    pub const fn is_sticky(self) -> bool {
        matches!(self, Self::BeforeSticky | Self::AfterSticky)
    }

    /// Returns `true` if this association is before-oriented.
    #[must_use]
    pub const fn is_before(self) -> bool {
        matches!(self, Self::Before | Self::BeforeWord | Self::BeforeSticky)
    }

    /// Returns `true` if this association is after-oriented.
    #[must_use]
    pub const fn is_after(self) -> bool {
        matches!(self, Self::After | Self::AfterWord | Self::AfterSticky)
    }

    /// Whether to stick to gaps (all except word-oriented assocs).
    #[must_use]
    pub const fn stay_at_gaps(self) -> bool {
        !matches!(self, Self::BeforeWord | Self::AfterWord)
    }

    /// Offset into inserted text for a position exactly at the insert point.
    #[must_use]
    pub fn insert_offset(self, s: &str) -> usize {
        let chars = s.chars().count();
        match self {
            Self::After | Self::AfterSticky => chars,
            Self::AfterWord => s.chars().take_while(|&c| char_is_word(c)).count(),
            Self::Before | Self::BeforeSticky => 0,
            Self::BeforeWord => {
                chars - s.chars().rev().take_while(|&c| char_is_word(c)).count()
            }
        }
    }
}

/// Whether a character counts as a word character (Helix parity, pure Rust).
#[inline]
#[must_use]
pub fn char_is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}