//! Transaction type: one undoable unit of changes.
//!
//! Layer 2 of the edit taxonomy: `ChangeSet` + preserved deleted text +
//! post `Selection`.
//!
//! Deleted text is captured at construction time (against the pre-document),
//! so [`Transaction::invert`] is pure: no document needed (Vision A4, Total
//! Text Preservation). Transactions built without a document (legacy
//! [`Transaction::new`]/`From<ChangeSet>`) are impure until
//! [`Transaction::capture_deleted`] fills the store; pure [`Transaction::invert`]
//! returns `None` for those.

use crate::change::{Change, ChangeSet, Deletion, Operation, Tendril};
use crate::range::Range;
use crate::rope::Rope;
use crate::selection::Selection;
use smallvec::SmallVec;

/// A transaction is a change set with preserved deletions and an optional selection.
///
/// Transactions are the unit of work in the editor core. They can be applied,
/// purely inverted, and composed to support undo/redo.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Transaction {
    /// The change set for this transaction.
    pub changes: ChangeSet,
    /// Deleted text per `Delete` op in [`Transaction::changes`], in order.
    ///
    /// `deleted.len()` equals the number of `Delete` ops when pure;
    /// empty (or short) means impure — [`Transaction::invert`] returns `None`.
    pub deleted: Vec<Tendril>,
    /// The optional selection associated with this transaction.
    pub selection: Option<Selection>,
}

/// Take the whole current self entry (self Deletes are never split in compose).
fn take_self_full(del: &[Tendril], si: &mut usize, n: usize, out: &mut Vec<Tendril>) -> bool {
    match del.get(*si) {
        Some(e) if e.chars().count() == n => {
            out.push(e.clone());
            *si += 1;
            true
        }
        _ => false,
    }
}

/// Take a prefix of `n` chars from the current other entry into `out` (split-aware).
fn take_other_prefix(
    del: &[Tendril],
    oi: &mut usize,
    ooff: &mut usize,
    n: usize,
    out: &mut Vec<Tendril>,
) -> bool {
    let entry = match del.get(*oi) {
        Some(e) => e,
        None => return false,
    };
    let remain = entry.chars().count().saturating_sub(*ooff);
    if remain < n {
        return false;
    }
    out.push(entry.chars().skip(*ooff).take(n).collect());
    if remain == n {
        *oi += 1;
        *ooff = 0;
    } else {
        *ooff += n;
    }
    true
}

/// Drop a prefix of `n` chars from the current other entry (annihilation).
fn drop_other_prefix(del: &[Tendril], oi: &mut usize, ooff: &mut usize, n: usize) -> bool {
    let entry = match del.get(*oi) {
        Some(e) => e,
        None => return false,
    };
    let remain = entry.chars().count().saturating_sub(*ooff);
    if remain < n {
        return false;
    }
    if remain == n {
        *oi += 1;
        *ooff = 0;
    } else {
        *ooff += n;
    }
    true
}

/// Collect deleted spans of `changes` applied to `doc`.
fn collect_deleted(doc: &Rope, changes: &ChangeSet) -> Vec<Tendril> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    for op in changes.changes() {
        match op {
            Operation::Retain(n) => pos += *n,
            Operation::Delete(n) => {
                out.push(doc.slice_tendril(pos, pos + *n));
                pos += *n;
            }
            Operation::Insert(_) => {}
        }
    }
    out
}

impl Transaction {
    /// Creates a new transaction from a change set (legacy, impure).
    ///
    /// Pure [`Transaction::invert`] returns `None` until
    /// [`Transaction::capture_deleted`] is called, unless there are no deletes.
    #[must_use]
    pub fn new(changes: ChangeSet) -> Self {
        Self {
            changes,
            deleted: Vec::new(),
            selection: None,
        }
    }

    /// Creates an empty transaction against `doc` (Helix parity, pure).
    #[must_use]
    pub fn new_empty(doc: &Rope) -> Self {
        Self {
            changes: ChangeSet::from_doc(doc),
            deleted: Vec::new(),
            selection: None,
        }
    }

    /// Creates a new transaction with a change set and selection (impure).
    #[must_use]
    pub fn with_selection(changes: ChangeSet, selection: Selection) -> Self {
        Self {
            changes,
            deleted: Vec::new(),
            selection: Some(selection),
        }
    }

    /// Builder-style selection setter (Helix parity).
    #[must_use]
    pub fn with_sel(mut self, selection: Selection) -> Self {
        self.selection = Some(selection);
        self
    }

    /// Creates an empty transaction.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(ChangeSet::empty())
    }

    /// Borrowed changes.
    #[must_use]
    pub fn changes(&self) -> &ChangeSet {
        &self.changes
    }

    /// `true` when every `Delete` op has preserved text.
    #[must_use]
    pub fn is_pure(&self) -> bool {
        let mut di = 0usize;
        for op in self.changes.changes() {
            if let Operation::Delete(n) = op {
                match self.deleted.get(di) {
                    Some(t) if t.chars().count() == *n => {}
                    _ => return false,
                }
                di += 1;
            }
        }
        di == self.deleted.len()
    }

    /// Fill the deleted-text store from the pre-document (makes pure).
    pub fn capture_deleted(&mut self, doc: &Rope) {
        self.deleted = collect_deleted(doc, &self.changes);
    }

    /// Applies to a document only (Helix parity). Empty is a no-op success.
    pub fn apply_doc(&self, doc: &mut Rope) -> bool {
        if self.changes.is_empty() {
            return true;
        }
        self.changes.apply(doc)
    }

    /// Applies this transaction to a rope and maps the selection through the changes.
    pub fn apply(&self, doc: &mut Rope, sel: &mut Selection) -> bool {
        let result = if self.changes.is_empty() {
            true
        } else {
            self.changes.apply(doc)
        };
        if let Some(ref txn_sel) = self.selection {
            *sel = txn_sel.clone();
        } else {
            *sel = sel.clone().map(&self.changes);
        }
        result
    }

    /// Pure inversion using preserved deleted text (Vision A4).
    ///
    /// Returns `None` when the transaction is impure (missing deletes).
    /// The result carries the inverse deletions so double-inversion round-trips;
    /// selection is `None` — the caller (History) attaches the pre-selection.
    #[must_use]
    pub fn invert(&self) -> Option<Self> {
        let mut inv = ChangeSet::with_capacity(self.changes.changes().len());
        let mut inv_deleted = Vec::new();
        let mut di = 0usize;
        for op in self.changes.changes() {
            match op {
                Operation::Retain(n) => inv.retain(*n),
                Operation::Delete(n) => {
                    let text = self.deleted.get(di)?;
                    if text.chars().count() != *n {
                        return None;
                    }
                    inv.insert(text.clone());
                    di += 1;
                }
                Operation::Insert(s) => {
                    inv.delete(s.chars().count());
                    inv_deleted.push(s.clone());
                }
            }
        }
        if di != self.deleted.len() {
            return None;
        }
        Some(Self {
            changes: inv,
            deleted: inv_deleted,
            selection: None,
        })
    }

    /// Inverts using the original document (compat: captures, then purely inverts).
    ///
    /// The inverted transaction carries the *raw* pre-selection so undo
    /// restores the cursor (Helix parity).
    #[must_use]
    pub fn invert_with_doc(&self, original: &Rope, original_sel: &Selection) -> Self {
        let mut this = self.clone();
        this.capture_deleted(original);
        this.invert()
            .map(|inv| inv.with_sel(original_sel.clone()))
            .unwrap_or_else(|| Self {
                changes: self.changes.invert(original),
                deleted: Vec::new(),
                selection: Some(original_sel.clone()),
            })
    }

    /// Legacy inversion `(original, selection)` — same as [`Transaction::invert_with_doc`].
    #[must_use]
    pub fn invert_legacy(&self, original: &Rope, original_sel: &Selection) -> Self {
        self.invert_with_doc(original, original_sel)
    }

    /// Inverts using the original document and original selection.
    #[must_use]
    pub fn invert_with_selection(&self, original: &Rope, original_sel: &Selection) -> Self {
        self.invert_with_doc(original, original_sel)
    }

    /// Inverts using the original document (Helix parity: selection dropped).
    #[must_use]
    pub fn invert_doc(&self, original: &Rope) -> Self {
        let mut this = self.clone();
        this.capture_deleted(original);
        this.invert().unwrap_or_else(|| Self {
            changes: self.changes.invert(original),
            deleted: Vec::new(),
            selection: None,
        })
    }

    /// Composes this transaction with another, preserving deleted text.
    ///
    /// The result is pure when both sides are pure. Carries the other's
    /// selection (other wins).
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        use std::cmp::Ordering;
        use Operation::*;
        let selection = other.selection.clone().or(self.selection.clone());
        let composed_changes = self.changes.clone().compose(other.changes.clone());
        if !(self.is_pure() && other.is_pure()) {
            return Self {
                changes: composed_changes,
                deleted: Vec::new(),
                selection,
            };
        }

        // Merge deleted stores alongside owned op vectors with in-place
        // remainder splitting (mirrors ChangeSet::compose arms exactly).
        let mut self_ops = self.changes.changes().to_vec();
        let mut other_ops = other.changes.changes().to_vec();
        let (mut i, mut j) = (0usize, 0usize);
        let mut si = 0usize;
        let mut oi = 0usize;
        let mut ooff = 0usize;
        let self_del = self.deleted.clone();
        let other_del = other.deleted.clone();
        let mut out: Vec<Tendril> = Vec::new();
        let mut ok = true;

        while i < self_ops.len() || j < other_ops.len() {
            if !ok {
                break;
            }
            let a = self_ops.get(i).cloned();
            let b = other_ops.get(j).cloned();
            match (a, b) {
                (None, None) => break,
                (Some(Delete(n)), _) => {
                    ok = take_self_full(&self_del, &mut si, n, &mut out);
                    i += 1;
                }
                (_, Some(Insert(_))) => {
                    j += 1;
                }
                (None, _) | (_, None) => {
                    ok = false;
                    break;
                }
                (Some(Retain(x)), Some(Retain(y))) => {
                    let m = x.min(y);
                    if m == x {
                        i += 1;
                    } else {
                        self_ops[i] = Retain(x - m);
                    }
                    if m == y {
                        j += 1;
                    } else {
                        other_ops[j] = Retain(y - m);
                    }
                }
                (Some(Insert(s)), Some(Delete(d))) => {
                    let len = s.chars().count();
                    match len.cmp(&d) {
                        Ordering::Less => {
                            ok = drop_other_prefix(&other_del, &mut oi, &mut ooff, len);
                            other_ops[j] = Delete(d - len);
                            i += 1;
                        }
                        Ordering::Equal => {
                            ok = drop_other_prefix(&other_del, &mut oi, &mut ooff, d);
                            i += 1;
                            j += 1;
                        }
                        Ordering::Greater => {
                            ok = drop_other_prefix(&other_del, &mut oi, &mut ooff, d);
                            let rest: Tendril = s.chars().skip(d).collect();
                            self_ops[i] = Insert(rest);
                            j += 1;
                        }
                    }
                }
                (Some(Insert(s)), Some(Retain(n))) => {
                    let len = s.chars().count();
                    if len <= n {
                        i += 1;
                        if len == n {
                            j += 1;
                        } else {
                            other_ops[j] = Retain(n - len);
                        }
                    } else {
                        let rest: Tendril = s.chars().skip(n).collect();
                        self_ops[i] = Insert(rest);
                        j += 1;
                    }
                }
                (Some(Retain(x)), Some(Delete(d))) => {
                    let m = x.min(d);
                    ok = take_other_prefix(&other_del, &mut oi, &mut ooff, m, &mut out);
                    if m == x {
                        i += 1;
                    } else {
                        self_ops[i] = Retain(x - m);
                    }
                    if m == d {
                        j += 1;
                    } else {
                        other_ops[j] = Delete(d - m);
                    }
                }
            }
        }

        if !ok || si != self_del.len() || oi != other_del.len() {
            return Self {
                changes: composed_changes,
                deleted: Vec::new(),
                selection,
            };
        }
        // Validate against composed Delete ops.
        // The builder canonicalizes Insert-before-Delete and coalesces adjacent
        // Deletes, so walk-order spans are concatenated then re-split along the
        // composed Delete ops.
        let flat: String = out.iter().flat_map(|s| s.chars()).collect();
        let mut chars = flat.chars();
        let mut merged: Vec<Tendril> = Vec::new();
        for op in composed_changes.changes() {
            if let Operation::Delete(n) = op {
                let s: Tendril = chars.by_ref().take(*n).collect();
                if s.chars().count() != *n {
                    return Self {
                        changes: composed_changes,
                        deleted: Vec::new(),
                        selection,
                    };
                }
                merged.push(s);
            }
        }
        if chars.next().is_some() {
            return Self {
                changes: composed_changes,
                deleted: Vec::new(),
                selection,
            };
        }
        Self {
            changes: composed_changes,
            deleted: merged,
            selection,
        }
    }

    /// Returns the selection associated with this transaction, if any.
    #[must_use]
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Iterate `(from, to, replacement)` changes.
    pub fn changes_iter(&self) -> impl Iterator<Item = Change> + '_ {
        self.changes.changes_iter()
    }

    /// Generate from an iterator of changes against `doc` (pure).
    pub fn change<I>(doc: &Rope, changes: I) -> Self
    where
        I: Iterator<Item = Change>,
    {
        let owned: Vec<Change> = changes.collect();
        let changeset = ChangeSet::from_changes(doc, owned.clone().into_iter());
        let deleted = collect_deleted(doc, &changeset);
        Self {
            changes: changeset,
            deleted,
            selection: None,
        }
    }

    /// Generate from deletions (merging overlaps) against `doc` (pure).
    pub fn delete<I>(doc: &Rope, deletions: I) -> Self
    where
        I: Iterator<Item = Deletion>,
    {
        let len = doc.len();
        let (lower, upper) = deletions.size_hint();
        let size = upper.unwrap_or(lower);
        let mut changeset = ChangeSet::with_capacity(2 * size + 1);
        let mut last = 0usize;
        for (mut from, to) in deletions {
            if last > to {
                continue;
            }
            if last > from {
                from = last;
            }
            debug_assert!(from <= to);
            changeset.retain(from - last);
            changeset.delete(to - from);
            last = to;
        }
        changeset.retain(len - last);
        let deleted = collect_deleted(doc, &changeset);
        Self {
            changes: changeset,
            deleted,
            selection: None,
        }
    }

    /// Generate with a change per selection range (pure).
    pub fn change_by_selection<F>(doc: &Rope, selection: &Selection, f: F) -> Self
    where
        F: FnMut(&Range) -> Change,
    {
        Self::change(doc, selection.iter().map(f))
    }

    /// Delete per selection range (merging overlaps, pure).
    pub fn delete_by_selection<F>(doc: &Rope, selection: &Selection, f: F) -> Self
    where
        F: FnMut(&Range) -> Deletion,
    {
        Self::delete(doc, selection.iter().map(f))
    }

    /// Insert text at each selection head (pure).
    pub fn insert(doc: &Rope, selection: &Selection, text: Tendril) -> Self {
        Self::change_by_selection(doc, selection, |range| {
            (range.head, range.head, Some(text.clone()))
        })
    }

    /// Change per selection range, producing a new post selection (pure).
    pub fn change_by_and_with_selection<F>(doc: &Rope, selection: &Selection, mut f: F) -> Self
    where
        F: FnMut(&Range) -> (Change, Option<Range>),
    {
        let mut end_ranges: SmallVec<[Range; 1]> = SmallVec::with_capacity(selection.len());
        let mut offset: isize = 0;
        let transaction = Self::change_by_selection(doc, selection, |start_range| {
            let ((from, to, replacement), end_range) = f(start_range);
            let mut change_size = to as isize - from as isize;
            if let Some(ref text) = replacement {
                change_size = text.chars().count() as isize - change_size;
            } else {
                change_size = -change_size;
            }
            let new_range = end_range.unwrap_or_else(|| {
                ChangeSet::from_change(doc, (from, to, replacement.clone()))
                    .map_pos_range(start_range)
            });
            let (na, nh) = (
                (new_range.anchor as isize + offset) as usize,
                (new_range.head as isize + offset) as usize,
            );
            end_ranges.push(Range::new(na, nh));
            offset += change_size;
            (from, to, replacement)
        });
        transaction.with_sel(Selection::with_primary(end_ranges, selection.primary_index()))
    }
}

impl From<ChangeSet> for Transaction {
    fn from(changes: ChangeSet) -> Self {
        Self {
            changes,
            deleted: Vec::new(),
            selection: None,
        }
    }
}

// Helper: map a single range with Helix sticky semantics via ChangeSet.
trait MapRange {
    fn map_pos_range(&self, range: &Range) -> Range;
}

impl MapRange for ChangeSet {
    fn map_pos_range(&self, range: &Range) -> Range {
        (*range).map(self)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn doc(s: &str) -> Rope {
        Rope::from_str(s)
    }

    #[test]
    fn pure_invert_round_trip() {
        let d0 = doc("hello world");
        let txn = Transaction::change(&d0, vec![(6, 11, Some("void".into()))].into_iter());
        assert!(txn.is_pure());
        let inv = txn.invert().expect("pure invert");
        let mut d1 = d0.clone();
        assert!(txn.apply_doc(&mut d1));
        assert_eq!(d1.len(), "hello void".len());
        assert!(inv.apply_doc(&mut d1));
        assert_eq!(d1, d0);
        // Double inversion restores the original transaction.
        let again = inv.invert().expect("double invert");
        assert_eq!(again.changes, txn.changes);
    }

    #[test]
    fn pure_invert_delete_only() {
        let d0 = doc("abcdef");
        let txn = Transaction::delete(&d0, vec![(1, 3)].into_iter());
        assert!(txn.is_pure());
        let inv = txn.invert().expect("pure invert");
        let mut d1 = d0.clone();
        assert!(txn.apply_doc(&mut d1));
        assert_eq!(d1.len(), 4);
        assert!(inv.apply_doc(&mut d1));
        assert_eq!(d1, d0);
    }

    #[test]
    fn impure_invert_returns_none() {
        let cs = ChangeSet::new(vec![
            Operation::Retain(2),
            Operation::Delete(3),
            Operation::Retain(1),
        ]);
        let txn = Transaction::new(cs);
        assert!(!txn.is_pure());
        assert!(txn.invert().is_none());
    }

    #[test]
    fn compose_preserves_purity_and_inverts() {
        let d0 = doc("abcdefgh");
        let t1 = Transaction::change(&d0, vec![(2, 4, Some("XY".into()))].into_iter());
        let mut d1 = d0.clone();
        assert!(t1.apply_doc(&mut d1));
        let t2 = Transaction::change(&d1, vec![(0, 2, None)].into_iter());
        let mut d2 = d1.clone();
        assert!(t2.apply_doc(&mut d2));
        let composed = t1.compose(t2);
        assert!(composed.is_pure());
        let mut dc = d0.clone();
        assert!(composed.apply_doc(&mut dc));
        assert_eq!(dc, d2);
        let inv = composed.invert().expect("composed pure invert");
        assert!(inv.apply_doc(&mut dc));
        assert_eq!(dc, d0);
    }
}
