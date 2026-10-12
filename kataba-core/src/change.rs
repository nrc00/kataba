//! Change set system for the editor core.
//!
//! Helix-parity OT algebra with a modernized builder API.
//! `ChangeSet` is the structural delta (Layer 1); see `transaction.rs`
//! for the committed unit (Layer 2) and `history.rs` for persistence.

use crate::assoc::Assoc;
use crate::range::Range;
use crate::rope::Rope;
use std::iter::once;

/// A tendril is a small, owned string used for insertions.
/// Inline storage for short fragments (Helix parity, no heap per keystroke).
pub type Tendril = smartstring::SmartString<smartstring::LazyCompact>;

/// (from, to, replacement) — a single change in document coordinates.
pub type Change = (usize, usize, Option<Tendril>);
/// (from, to) — a single deletion in document coordinates.
pub type Deletion = (usize, usize);

/// An operation in a change set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Retain `usize` characters.
    Retain(usize),
    /// Delete `usize` characters.
    Delete(usize),
    /// Insert a tendril of text.
    Insert(Tendril),
}

impl Operation {
    /// Returns the number of characters this operation affects in the
    /// original document.
    #[must_use]
    pub fn len_original(&self) -> usize {
        match self {
            Self::Retain(n) => *n,
            Self::Delete(n) => *n,
            Self::Insert(_) => 0,
        }
    }

    /// Returns the number of characters this operation produces in the
    /// new document.
    #[must_use]
    pub fn len_new(&self) -> usize {
        match self {
            Self::Retain(n) => *n,
            Self::Delete(_) => 0,
            Self::Insert(s) => s.chars().count(),
        }
    }

    /// Total characters touched (Helix `len_chars` parity).
    #[must_use]
    pub fn len_chars(&self) -> usize {
        match self {
            Self::Retain(n) | Self::Delete(n) => *n,
            Self::Insert(s) => s.chars().count(),
        }
    }
}

/// A change set represents a transformation from one document state to another.
///
/// It is a sequence of operations: Retain (keep), Delete (remove), Insert (add).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    /// The operations that make up this change set.
    pub changes: Vec<Operation>,
    /// The length of the original document.
    pub len: usize,
    /// The length of the new document.
    pub len_after: usize,
}

impl ChangeSet {
    /// Empty builder with capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            changes: Vec::with_capacity(capacity),
            len: 0,
            len_after: 0,
        }
    }

    /// Creates a new change set from a sequence of operations.
    #[must_use]
    pub fn new(changes: Vec<Operation>) -> Self {
        let mut len = 0;
        let mut len_after = 0;
        for op in &changes {
            len += op.len_original();
            len_after += op.len_new();
        }
        Self {
            changes,
            len,
            len_after,
        }
    }

    /// Creates an empty change set.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            changes: Vec::new(),
            len: 0,
            len_after: 0,
        }
    }

    /// Creates a change set from a document rope.
    #[must_use]
    pub fn from_doc(doc: &Rope) -> Self {
        let len = doc.len();
        Self {
            changes: vec![Operation::Retain(len)],
            len,
            len_after: len,
        }
    }

    /// Returns `true` if this change set is empty (no changes).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() || self.changes == [Operation::Retain(self.len)]
    }

    /// Returns the length of the original document.
    #[must_use]
    pub const fn original_len(&self) -> usize {
        self.len
    }

    /// Returns the length of the new document.
    #[must_use]
    pub const fn new_len(&self) -> usize {
        self.len_after
    }

    /// Borrowed operations (for LSP / inspection).
    #[must_use]
    pub fn changes(&self) -> &[Operation] {
        &self.changes
    }

    pub(crate) fn delete(&mut self, n: usize) {
        use Operation::*;
        if n == 0 {
            return;
        }
        self.len += n;
        if let Some(Delete(count)) = self.changes.last_mut() {
            *count += n;
        } else {
            self.changes.push(Delete(n));
        }
    }

    pub(crate) fn insert(&mut self, fragment: Tendril) {
        use Operation::*;
        if fragment.is_empty() {
            return;
        }
        self.len_after += fragment.chars().count();
        let new_last = match self.changes.as_mut_slice() {
            [.., Insert(prev)] | [.., Insert(prev), Delete(_)] => {
                prev.push_str(&fragment);
                return;
            }
            [.., last @ Delete(_)] => std::mem::replace(last, Insert(fragment)),
            _ => Insert(fragment),
        };
        self.changes.push(new_last);
    }

    pub(crate) fn retain(&mut self, n: usize) {
        use Operation::*;
        if n == 0 {
            return;
        }
        self.len += n;
        self.len_after += n;
        if let Some(Retain(count)) = self.changes.last_mut() {
            *count += n;
        } else {
            self.changes.push(Retain(n));
        }
    }

    /// Generate a `ChangeSet` from an iterator of changes against `doc`.
    pub fn from_changes<I>(doc: &Rope, changes: I) -> Self
    where
        I: Iterator<Item = Change>,
    {
        let len = doc.len();
        let (lower, upper) = changes.size_hint();
        let size = upper.unwrap_or(lower);
        let mut changeset = Self::with_capacity(2 * size + 1);
        let mut last = 0;
        for (from, to, tendril) in changes {
            debug_assert!(last <= from);
            debug_assert!(from <= to, "edit end must not precede start");
            changeset.retain(from - last);
            let span = to - from;
            match tendril {
                Some(text) => {
                    changeset.insert(text);
                    changeset.delete(span);
                }
                None => changeset.delete(span),
            }
            last = to;
        }
        changeset.retain(len - last);
        changeset
    }

    /// Single-change constructor.
    pub fn from_change(doc: &Rope, change: Change) -> Self {
        Self::from_changes(doc, std::iter::once(change))
    }
}

/// Creates a change set that replaces a range with the given text.
#[must_use]
pub fn replace_range(range: Range, text: &str, doc: &Rope) -> ChangeSet {
    ChangeSet::from_change(doc, (range.from(), range.to(), Some(text.into())))
}

/// Creates a change set that deletes a range.
#[must_use]
pub fn delete_range(range: Range, doc: &Rope) -> ChangeSet {
    ChangeSet::from_change(doc, (range.from(), range.to(), None))
}

/// Creates a change set that inserts text at a position.
#[must_use]
pub fn insert_at(pos: usize, text: &str) -> ChangeSet {
    let mut cs = ChangeSet::with_capacity(2);
    cs.retain(pos);
    cs.insert(text.into());
    cs
}

impl ChangeSet {
    /// Maps a single position through this change set with the given association.
    pub fn map_position(&self, pos: usize, assoc: Assoc) -> usize {
        self.map_pos(pos, assoc)
    }

    /// Helix-parity single position mapping.
    pub fn map_pos(&self, mut pos: usize, assoc: Assoc) -> usize {
        self.update_positions(once((&mut pos, assoc)));
        pos
    }

    /// Maps a list of (mostly sorted) positions in `O(N+M)`.
    pub fn update_positions<'a>(
        &self,
        positions: impl Iterator<Item = (&'a mut usize, Assoc)>,
    ) {
        use Operation::*;
        let mut positions = positions.peekable();
        let mut old_pos = 0usize;
        let mut new_pos = 0usize;
        let mut iter = self.changes.iter().enumerate().peekable();

        'outer: loop {
            macro_rules! map {
                ($map: expr, $i: expr) => {
                    loop {
                        let Some((pos, assoc)) = positions.peek_mut() else {
                            return;
                        };
                        if **pos < old_pos {
                            for (i, change) in self.changes[..$i].iter().enumerate().rev() {
                                match change {
                                    Retain(i) => {
                                        old_pos -= i;
                                        new_pos -= i;
                                    }
                                    Delete(i) => {
                                        old_pos -= i;
                                    }
                                    Insert(ins) => {
                                        new_pos -= ins.chars().count();
                                    }
                                }
                                if old_pos <= **pos {
                                    iter = self.changes[i..].iter().enumerate().peekable();
                                }
                            }
                            debug_assert!(old_pos <= **pos);
                            continue 'outer;
                        }
                        #[allow(clippy::redundant_closure_call)]
                        let Some(mapped) = $map(**pos, *assoc) else {
                            break;
                        };
                        **pos = mapped;
                        positions.next();
                    }
                };
            }

            let Some((i, change)) = iter.next() else {
                map!(
                    |pos, _| (old_pos == pos).then_some(new_pos),
                    self.changes.len()
                );
                break;
            };

            let len = match change {
                Delete(i) | Retain(i) => *i,
                Insert(_) => 0,
            };
            let mut old_end = old_pos + len;

            match change {
                Retain(_) => {
                    map!(
                        |pos, _| (old_end > pos).then_some(new_pos + (pos - old_pos)),
                        i
                    );
                    new_pos += len;
                }
                Delete(_) => {
                    map!(|pos, _| (old_end > pos).then_some(new_pos), i);
                }
                Insert(s) => {
                    if let Some((_, Delete(len))) = iter.peek() {
                        iter.next();
                        old_end = old_pos + len;
                        map!(
                            |pos, assoc: Assoc| (old_end > pos).then(|| {
                                if pos == old_pos && assoc.stay_at_gaps() {
                                    new_pos
                                } else {
                                    let ins = assoc.insert_offset(s);
                                    if *len == ins && assoc.is_sticky() {
                                        new_pos + (pos - old_pos)
                                    } else {
                                        new_pos + assoc.insert_offset(s)
                                    }
                                }
                            }),
                            i
                        );
                    } else {
                        map!(
                            |pos, assoc: Assoc| (old_pos == pos)
                                .then(|| new_pos + assoc.insert_offset(s)),
                            i
                        );
                    }
                    new_pos += s.chars().count();
                }
            }
            old_pos = old_end;
        }
        let out_of_bounds: Vec<_> = positions.collect();
        assert!(
            out_of_bounds.is_empty(),
            "positions {out_of_bounds:?} out of range for changeset len {old_pos}"
        );
    }

    /// Maps two positions through this change set with the given association.
    #[must_use]
    pub fn map_two_positions(&self, pos1: usize, pos2: usize, assoc: Assoc) -> (usize, usize) {
        (
            self.map_position(pos1, assoc),
            self.map_position(pos2, assoc),
        )
    }

    /// Maps a range through this change set with the given association.
    #[must_use]
    pub fn map_range(&self, range: Range, assoc: Assoc) -> Range {
        range.map_with_assoc(self, assoc)
    }

    /// Applies this change set to a rope, modifying it in place.
    pub fn apply(&self, doc: &mut Rope) -> bool {
        if doc.len() != self.len {
            return false;
        }
        let mut pos = 0usize;
        for op in &self.changes {
            match op {
                Operation::Retain(n) => {
                    pos += *n;
                }
                Operation::Delete(n) => {
                    doc.remove(pos, pos + *n);
                }
                Operation::Insert(s) => {
                    doc.insert(pos, s);
                    pos += s.chars().count();
                }
            }
        }
        true
    }

    /// Inverts this change set using the original document.
    pub fn invert(&self, original: &Rope) -> Self {
        assert_eq!(original.len(), self.len);
        let mut changes = Self::with_capacity(self.changes.len());
        let mut pos = 0usize;
        for op in &self.changes {
            match op {
                Operation::Retain(n) => {
                    changes.retain(*n);
                    pos += *n;
                }
                Operation::Delete(n) => {
                    changes.insert(original.slice_tendril(pos, pos + *n));
                    pos += *n;
                }
                Operation::Insert(s) => {
                    changes.delete(s.chars().count());
                }
            }
        }
        changes
    }

    /// Composes this change set with another, producing a change set
    /// equivalent to applying both sequentially.
    pub fn compose(self, other: Self) -> Self {
        assert_eq!(
            self.len_after, other.len,
            "compose length mismatch: {} != {}",
            self.len_after, other.len
        );
        if self.changes.is_empty() {
            return other;
        }
        if other.changes.is_empty() {
            return self;
        }
        let cap = self.changes.len().max(other.changes.len());
        let mut changes_a = self.changes.into_iter();
        let mut changes_b = other.changes.into_iter();
        let mut head_a = changes_a.next();
        let mut head_b = changes_b.next();
        let mut changes = Self::with_capacity(cap);
        let start_len = changes.len;
        let _ = start_len;

        loop {
            use std::cmp::Ordering;
            use Operation::*;
            match (head_a, head_b) {
                (None, None) => break,
                (Some(Delete(i)), b) => {
                    changes.delete(i);
                    head_a = changes_a.next();
                    head_b = b;
                }
                (a, Some(Insert(current))) => {
                    changes.insert(current);
                    head_a = a;
                    head_b = changes_b.next();
                }
                (None, val) | (val, None) => {
                    unreachable!("compose length mismatch, leftover: {val:?}")
                }
                (Some(Retain(i)), Some(Retain(j))) => match i.cmp(&j) {
                    Ordering::Less => {
                        changes.retain(i);
                        head_a = changes_a.next();
                        head_b = Some(Retain(j - i));
                    }
                    Ordering::Equal => {
                        changes.retain(i);
                        head_a = changes_a.next();
                        head_b = changes_b.next();
                    }
                    Ordering::Greater => {
                        changes.retain(j);
                        head_a = Some(Retain(i - j));
                        head_b = changes_b.next();
                    }
                },
                (Some(Insert(mut s)), Some(Delete(j))) => {
                    let len = s.chars().count();
                    match len.cmp(&j) {
                        Ordering::Less => {
                            head_a = changes_a.next();
                            head_b = Some(Delete(j - len));
                        }
                        Ordering::Equal => {
                            head_a = changes_a.next();
                            head_b = changes_b.next();
                        }
                        Ordering::Greater => {
                            let (pos, _) = s.char_indices().nth(j).unwrap_or((s.len(), ' '));
                            s.replace_range(0..pos, "");
                            head_a = Some(Insert(s));
                            head_b = changes_b.next();
                        }
                    }
                }
                (Some(Insert(s)), Some(Retain(j))) => {
                    let len = s.chars().count();
                    match len.cmp(&j) {
                        Ordering::Less => {
                            changes.insert(s);
                            head_a = changes_a.next();
                            head_b = Some(Retain(j - len));
                        }
                        Ordering::Equal => {
                            changes.insert(s);
                            head_a = changes_a.next();
                            head_b = changes_b.next();
                        }
                        Ordering::Greater => {
                            let (pos, _) = s.char_indices().nth(j).unwrap_or((s.len(), ' '));
                            let mut before = s;
                            let after = before.split_off(pos);
                            changes.insert(before);
                            head_a = Some(Insert(after));
                            head_b = changes_b.next();
                        }
                    }
                }
                (Some(Retain(i)), Some(Delete(j))) => match i.cmp(&j) {
                    Ordering::Less => {
                        changes.delete(i);
                        head_a = changes_a.next();
                        head_b = Some(Delete(j - i));
                    }
                    Ordering::Equal => {
                        changes.delete(j);
                        head_a = changes_a.next();
                        head_b = changes_b.next();
                    }
                    Ordering::Greater => {
                        changes.delete(j);
                        head_a = Some(Retain(i - j));
                        head_b = changes_b.next();
                    }
                },
            }
        }
        changes
    }

    /// Iterate over `(from, to, replacement)` changes.
    pub fn changes_iter(&self) -> ChangeIterator<'_> {
        ChangeIterator::new(self)
    }
}

/// Iterator over `(from, to, replacement)` triples.
pub struct ChangeIterator<'a> {
    iter: std::iter::Peekable<std::slice::Iter<'a, Operation>>,
    pos: usize,
}

impl<'a> ChangeIterator<'a> {
    fn new(changeset: &'a ChangeSet) -> Self {
        Self {
            iter: changeset.changes.iter().peekable(),
            pos: 0,
        }
    }
}

impl Iterator for ChangeIterator<'_> {
    type Item = Change;

    fn next(&mut self) -> Option<Self::Item> {
        use Operation::*;
        loop {
            match self.iter.next()? {
                Retain(len) => {
                    self.pos += len;
                }
                Delete(len) => {
                    let start = self.pos;
                    self.pos += len;
                    return Some((start, self.pos, None));
                }
                Insert(s) => {
                    let start = self.pos;
                    if let Some(Delete(len)) = self.iter.peek() {
                        self.iter.next();
                        self.pos += len;
                        return Some((start, self.pos, Some(s.clone())));
                    }
                    return Some((start, start, Some(s.clone())));
                }
            }
        }
    }
}
