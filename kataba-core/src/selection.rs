//! Selection type for representing multiple ranges in a document.

use crate::assoc::Assoc;
use crate::change::ChangeSet;
use crate::range::Range;
use smallvec::SmallVec;

/// A selection is a collection of ranges in a document.
///
/// One range is designated as the "primary" range (the cursor position).
/// Invariant: never empty (always at least the primary range).
#[derive(Debug, Clone)]
pub struct Selection {
    /// The ranges in this selection.
    pub ranges: SmallVec<[Range; 1]>,
    /// The index of the primary range.
    pub primary_index: usize,
    /// Preserved visual display column for the active primary head.
    /// Preserved across vertical motions; reset on horizontal motions.
    pub desired_col: Option<u32>,
}

impl Selection {
    /// Creates a new selection with a single range.
    #[must_use]
    pub fn new(range: Range) -> Self {
        Self {
            ranges: SmallVec::from_vec(vec![range]),
            primary_index: 0,
            desired_col: None,
        }
    }

    /// Creates a normalized selection from ranges + primary index (Helix parity).
    #[must_use]
    pub fn with_primary(ranges: SmallVec<[Range; 1]>, primary_index: usize) -> Self {
        assert!(!ranges.is_empty());
        debug_assert!(primary_index < ranges.len());
        Self {
            ranges,
            primary_index,
            desired_col: None,
        }
        .normalized()
    }

    /// Creates a new selection from a vector of ranges.
    #[must_use]
    pub fn from_ranges(ranges: SmallVec<[Range; 1]>) -> Self {
        Self::with_primary(ranges, 0)
    }

    /// Single range at (anchor, head).
    #[must_use]
    pub fn single(anchor: usize, head: usize) -> Self {
        Self::new(Range::new(anchor, head))
    }

    /// Zero-width cursor.
    #[must_use]
    pub fn point(pos: usize) -> Self {
        Self::new(Range::point(pos))
    }

    /// Returns the primary range.
    #[must_use]
    pub fn primary(&self) -> &Range {
        &self.ranges[self.primary_index]
    }

    /// Returns the primary index.
    #[must_use]
    pub fn primary_index(&self) -> usize {
        self.primary_index
    }

    /// Borrowed ranges.
    #[must_use]
    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    /// Returns the number of ranges in this selection.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Returns `true` if this selection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Adds a range to this selection and makes it the primary.
    pub fn push(&mut self, range: Range) {
        self.primary_index = self.ranges.len();
        self.ranges.push(range);
        self.normalize();
    }

    /// Removes a range from this selection.
    pub fn remove(&mut self, index: usize) {
        assert!(
            self.ranges.len() > 1,
            "can't remove the last range from a selection"
        );
        self.ranges.remove(index);
        if index < self.primary_index || self.primary_index == self.ranges.len() {
            self.primary_index -= 1;
        }
    }

    /// Maps all ranges through a change set (Helix sticky-pair batch mapping).
    #[must_use]
    pub fn map(&self, changes: &ChangeSet) -> Self {
        self.clone().map_no_normalize(changes).normalized()
    }

    /// Maps without normalizing.
    #[must_use]
    pub fn map_no_normalize(mut self, changes: &ChangeSet) -> Self {
        if changes.is_empty() {
            return self;
        }
        // Batch all endpoints through one O(N+M) pass (Helix parity).
        let positions_to_map = self.ranges.iter_mut().flat_map(|range| {
            use std::cmp::Ordering;
            match range.anchor.cmp(&range.head) {
                Ordering::Equal => [
                    (&mut range.anchor, Assoc::AfterSticky),
                    (&mut range.head, Assoc::AfterSticky),
                ],
                Ordering::Less => [
                    (&mut range.anchor, Assoc::AfterSticky),
                    (&mut range.head, Assoc::BeforeSticky),
                ],
                Ordering::Greater => [
                    (&mut range.head, Assoc::AfterSticky),
                    (&mut range.anchor, Assoc::BeforeSticky),
                ],
            }
        });
        changes.update_positions(positions_to_map);
        self
    }

    /// Maps all ranges through a change set with the given association.
    #[must_use]
    pub fn map_with_assoc(&self, changes: &ChangeSet, assoc: Assoc) -> Self {
        let mut result = Self {
            ranges: self
                .ranges
                .iter()
                .map(|r| r.map_with_assoc(changes, assoc))
                .collect(),
            primary_index: self.primary_index,
            desired_col: self.desired_col,
        };
        result.normalize();
        result
    }

    /// Desired visual column for vertical motion (`None` = derive from primary head).
    #[must_use]
    pub const fn desired_col(&self) -> Option<u32> {
        self.desired_col
    }

    /// Sets the remembered visual column (used by vertical motions).
    pub fn set_desired_col(&mut self, col: u32) {
        self.desired_col = Some(col);
    }

    /// Clears the remembered visual column (used by horizontal motions).
    pub fn reset_desired_col(&mut self) {
        self.desired_col = None;
    }

    fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    /// Normalizes this selection by sorting and merging overlapping ranges.
    pub fn normalize(&mut self) {
        if self.ranges.len() <= 1 {
            return;
        }
        let mut primary = self.ranges.get(self.primary_index).copied().unwrap_or(self.ranges[0]);
        self.ranges.sort_by_key(|r| r.from());

        let mut merged: SmallVec<[Range; 1]> = SmallVec::new();
        for range in &self.ranges {
            if let Some(last) = merged.last_mut() {
                if last.overlaps(range) {
                    let new_range = last.merge(range);
                    if last == &primary || range == &primary {
                        primary = new_range;
                    }
                    *last = new_range;
                } else {
                    merged.push(*range);
                }
            } else {
                merged.push(*range);
            }
        }

        self.primary_index = merged.iter().position(|r| r == &primary).unwrap_or(0);
        self.ranges = merged;
    }

    /// Merge all ranges into one spanning first..last.
    #[must_use]
    pub fn merge_ranges(self) -> Self {
        let first = self.ranges.first().copied().unwrap_or(Range::point(0));
        let last = self.ranges.last().copied().unwrap_or(Range::point(0));
        Self::new(first.merge(&last))
    }

    /// Returns an iterator over the ranges in this selection.
    pub fn iter(&self) -> impl Iterator<Item = &Range> + '_ {
        self.ranges.iter()
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new(Range::point(0))
    }
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.ranges == other.ranges
            && self.primary_index == other.primary_index
            && self.desired_col == other.desired_col
    }
}

impl Eq for Selection {}

impl From<Range> for Selection {
    fn from(range: Range) -> Self {
        Self::new(range)
    }
}

impl FromIterator<Range> for Selection {
    fn from_iter<T: IntoIterator<Item = Range>>(ranges: T) -> Self {
        Self::from_ranges(ranges.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[test]
    fn test_selection_normalize_merges_and_tracks_primary() {
        let mut sel = Selection {
            ranges: smallvec![
                Range::new(5, 10),
                Range::new(8, 15),
                Range::new(0, 3),
            ],
            primary_index: 0,
            desired_col: None,
        };

        sel.normalize();

        assert_eq!(sel.ranges.len(), 2);
        assert_eq!(sel.ranges[0], Range::new(0, 3));
        assert_eq!(sel.ranges[1], Range::new(5, 15));
        assert_eq!(sel.primary_index, 1);
        assert_eq!(sel.primary(), &Range::new(5, 15));
    }
}
