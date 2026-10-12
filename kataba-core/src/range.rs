//! Range type for representing a selection within a document.

use crate::assoc::Assoc;
use crate::change::ChangeSet;
use crate::direction::Direction;

/// A range within a document, defined by an anchor and a head.
///
/// Anchor/head use gap indexing (positions between chars) and may be in
/// either order; `from()`/`to()` give the ordered span. Direction is derived
/// from the ordering (`head < anchor` is backward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// The anchor position (side that doesn't move on extend).
    pub anchor: usize,
    /// The head position (side moved on extend).
    pub head: usize,
}

impl Range {
    /// Creates a new range.
    #[must_use]
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// Creates a zero-width range at the given position.
    #[must_use]
    pub const fn point(pos: usize) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    /// Returns the start of the range (the smaller of anchor and head).
    #[must_use]
    #[inline]
    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// Returns the end of the range (the larger of anchor and head).
    #[must_use]
    #[inline]
    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }

    /// Returns the length of the range in characters.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.head.abs_diff(self.anchor)
    }

    /// Returns `true` if this range is empty (zero-width).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// `Direction::Backward` when `head < anchor`, `Forward` otherwise.
    #[must_use]
    #[inline]
    pub fn direction(&self) -> Direction {
        if self.head < self.anchor {
            Direction::Backward
        } else {
            Direction::Forward
        }
    }

    /// Returns `true` if this range is forward.
    #[must_use]
    #[inline]
    pub fn is_forward(&self) -> bool {
        self.direction().is_forward()
    }

    /// Returns `true` if this range is backward.
    #[must_use]
    #[inline]
    pub fn is_backward(&self) -> bool {
        self.direction().is_backward()
    }

    /// Flips the direction of this range.
    #[must_use]
    pub const fn flip(&self) -> Self {
        Self {
            anchor: self.head,
            head: self.anchor,
        }
    }

    /// Swaps anchor and head in place.
    #[inline]
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.anchor, &mut self.head);
    }

    /// Returns the selection if it goes in `direction`, flipped otherwise.
    #[must_use]
    pub fn with_direction(self, direction: Direction) -> Self {
        if self.direction() == direction {
            self
        } else {
            self.flip()
        }
    }

    /// Returns `true` if this range overlaps with another range.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.from() == other.from() || (self.to() > other.from() && other.to() > self.from())
    }

    /// Returns `true` if this range contains a position (inclusive left, exclusive right).
    #[must_use]
    pub fn contains(&self, pos: usize) -> bool {
        self.from() <= pos && pos < self.to()
    }

    /// Merges this range with another range.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        if self.is_backward() && other.is_backward() {
            Self {
                anchor: self.anchor.max(other.anchor),
                head: self.head.min(other.head),
            }
        } else {
            Self {
                anchor: self.from().min(other.from()),
                head: self.to().max(other.to()),
            }
        }
    }

    /// Maps this range through a change set (Helix sticky-pair semantics).
    #[must_use]
    pub fn map(mut self, changes: &ChangeSet) -> Self {
        if changes.is_empty() {
            return self;
        }
        let (anchor, head) = (self.anchor, self.head);
        let (mapped_anchor, mapped_head) = match anchor.cmp(&head) {
            std::cmp::Ordering::Equal => {
                let m = changes.map_pos(anchor, Assoc::AfterSticky);
                (m, m)
            }
            std::cmp::Ordering::Less => (
                changes.map_pos(anchor, Assoc::AfterSticky),
                changes.map_pos(head, Assoc::BeforeSticky),
            ),
            std::cmp::Ordering::Greater => {
                let mapped_head = changes.map_pos(head, Assoc::AfterSticky);
                let mapped_anchor = changes.map_pos(anchor, Assoc::BeforeSticky);
                (mapped_anchor, mapped_head)
            }
        };
        self.anchor = mapped_anchor;
        self.head = mapped_head;
        self
    }

    /// Maps this range through a change set with the given association.
    #[must_use]
    pub fn map_with_assoc(&self, changes: &ChangeSet, assoc: Assoc) -> Self {
        let (anchor, head) = changes.map_two_positions(self.anchor, self.head, assoc);
        Self::new(anchor, head)
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{} {:?}", self.from(), self.to(), self.direction())
    }
}

impl From<(usize, usize)> for Range {
    fn from((anchor, head): (usize, usize)) -> Self {
        Self::new(anchor, head)
    }
}
