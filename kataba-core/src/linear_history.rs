//! Normal (linear) undo/redo history.
//!
//! Classic stack semantics for editors that don't need branches:
//! - `undo` steps back through applied revisions.
//! - `redo` steps forward again.
//! - Committing after an undo **discards** the redo tail (no branches kept).
//!
//! Pick this when you want predictable linear behavior; pick [`History`](crate::History)
//! (Neovim-style DAG, also preserved on the `neovim-style-undo` branch) when you
//! want branches, time travel, LRU budgets, and undofiles.

use crate::history::State;
use crate::transaction::Transaction;

/// One applied linear revision: forward change plus its pure inversion.
#[derive(Debug, Clone)]
pub struct LinearRevision {
    /// Forward transaction (re-applied on redo).
    pub transaction: Transaction,
    /// Pure inversion carrying the pre-edit selection (applied on undo).
    pub inversion: Transaction,
}

/// Normal linear undo/redo history.
#[derive(Debug, Default, Clone)]
pub struct LinearHistory {
    /// Applied revisions in order; `current` of them are active.
    revisions: Vec<LinearRevision>,
    /// Number of active revisions (`0` = pristine).
    current: usize,
}

impl LinearHistory {
    /// Creates an empty linear history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored revisions (active + undone tail).
    #[must_use]
    pub fn len(&self) -> usize {
        self.revisions.len()
    }

    /// `true` when no revision has ever been committed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.revisions.is_empty()
    }

    /// Number of currently active revisions.
    #[must_use]
    pub const fn current_index(&self) -> usize {
        self.current
    }

    /// `true` when nothing can be undone.
    #[must_use]
    pub const fn at_root(&self) -> bool {
        self.current == 0
    }

    /// `true` when [`LinearHistory::undo`] would return a transaction.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.current > 0
    }

    /// `true` when [`LinearHistory::redo`] would return a transaction.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.current < self.revisions.len()
    }

    /// Drops all revisions, resetting to pristine.
    pub fn clear(&mut self) {
        self.revisions.clear();
        self.current = 0;
    }

    /// Commits a transaction against its pre-state.
    ///
    /// Any undone redo tail is discarded (linear semantics). Missing deleted
    /// text is captured from `original.doc` so the stored inversion is pure.
    pub fn commit(&mut self, transaction: &Transaction, original: &State) {
        let mut stored = transaction.clone();
        if !stored.is_pure() {
            stored.capture_deleted(&original.doc);
        }
        let inversion = stored
            .invert()
            .map(|inv| inv.with_sel(original.selection.clone()))
            .unwrap_or_else(|| stored.invert_with_doc(&original.doc, &original.selection));
        self.revisions.truncate(self.current);
        self.revisions.push(LinearRevision {
            transaction: stored,
            inversion,
        });
        self.current += 1;
    }

    /// Undoes the latest active revision, returning its inversion.
    pub fn undo(&mut self) -> Option<&Transaction> {
        if self.current == 0 {
            return None;
        }
        self.current -= 1;
        Some(&self.revisions[self.current].inversion)
    }

    /// Redoes the next undone revision, returning its transaction.
    pub fn redo(&mut self) -> Option<&Transaction> {
        if self.current >= self.revisions.len() {
            return None;
        }
        let txn = &self.revisions[self.current].transaction;
        self.current += 1;
        Some(txn)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::rope::Rope;
    use crate::selection::Selection;

    fn commit_text(history: &mut LinearHistory, state: &mut State, pos: usize, text: &str) {
        let txn = Transaction::change(&state.doc, std::iter::once((pos, pos, Some(text.into()))));
        history.commit(&txn, state);
        txn.apply_doc(&mut state.doc);
        state.selection = state.selection.clone().map(txn.changes());
    }

    #[test]
    fn linear_undo_redo_round_trip() {
        let mut history = LinearHistory::new();
        let mut state = State {
            doc: Rope::from_str("hello"),
            selection: Selection::point(5),
        };
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        commit_text(&mut history, &mut state, 5, " world");
        commit_text(&mut history, &mut state, 11, "!");
        assert_eq!(state.doc.to_string(), "hello world!");
        assert_eq!(history.len(), 2);

        let undo = history.undo().expect("undo").clone();
        assert!(undo.apply_doc(&mut state.doc));
        assert_eq!(state.doc.to_string(), "hello world");
        assert!(history.can_redo());

        let undo = history.undo().expect("undo").clone();
        assert!(undo.apply_doc(&mut state.doc));
        assert_eq!(state.doc.to_string(), "hello");
        assert!(!history.can_undo());
        assert!(history.undo().is_none());

        let redo = history.redo().expect("redo").clone();
        assert!(redo.apply_doc(&mut state.doc));
        assert_eq!(state.doc.to_string(), "hello world");
        let redo = history.redo().expect("redo").clone();
        assert!(redo.apply_doc(&mut state.doc));
        assert_eq!(state.doc.to_string(), "hello world!");
        assert!(!history.can_redo());
        assert!(history.redo().is_none());
    }

    #[test]
    fn commit_after_undo_discards_redo_tail() {
        let mut history = LinearHistory::new();
        let mut state = State {
            doc: Rope::from_str("ab"),
            selection: Selection::point(2),
        };
        commit_text(&mut history, &mut state, 2, "c");
        assert_eq!(state.doc.to_string(), "abc");
        let undo = history.undo().expect("undo").clone();
        undo.apply(&mut state.doc, &mut state.selection);
        assert_eq!(state.doc.to_string(), "ab");
        assert!(history.can_redo());

        // New edit: the "abc" redo path is gone (linear, no branches).
        commit_text(&mut history, &mut state, 2, "d");
        assert_eq!(state.doc.to_string(), "abd");
        assert!(!history.can_redo());
        assert_eq!(history.len(), 1);

        let undo = history.undo().expect("undo").clone();
        undo.apply(&mut state.doc, &mut state.selection);
        assert_eq!(state.doc.to_string(), "ab");
    }

    #[test]
    fn undo_restores_cursor() {
        let mut history = LinearHistory::new();
        let mut state = State {
            doc: Rope::from_str("cdef"),
            selection: Selection::point(2),
        };
        let txn = Transaction::change(&state.doc, std::iter::once((0, 0, Some("XY".into()))));
        history.commit(&txn, &state);
        txn.apply(&mut state.doc, &mut state.selection);
        assert_eq!(state.selection.primary().head, 4);

        let undo = history.undo().expect("undo").clone();
        undo.apply(&mut state.doc, &mut state.selection);
        assert_eq!(state.doc.to_string(), "cdef");
        assert_eq!(state.selection.primary().head, 2);
    }
}
