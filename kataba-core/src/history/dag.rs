//! Revision DAG core: `RevisionId`, `Revision`, `History` structure, commit,
//! undo/redo, and time-based navigation.
//!
//! Eviction (`maybe_evict`, `remove_leaf`, `collapse_oldest_root_child`) lives
//! in [`crate::history::lru`]; persistence (`encode`, `decode`) lives in
//! [`crate::history::undofile`].

use crate::assoc::Assoc;
use crate::change::Operation;
use crate::range::Range;
use crate::rope::Rope;
use crate::selection::Selection;
use crate::transaction::Transaction;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Stable generational revision identifier. Never reused, never shifted.
/// `RevisionId(0)` is always the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RevisionId(pub u64);

impl RevisionId {
    /// The root revision.
    pub const ROOT: Self = Self(0);

    /// `true` for the root revision.
    #[must_use]
    pub const fn is_root(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for RevisionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rev{}", self.0)
    }
}

/// Document + selection snapshot used at commit time.
#[derive(Debug, Clone)]
pub struct State {
    /// Document text.
    pub doc: Rope,
    /// Current selection.
    pub selection: Selection,
}

/// Whether to undo by a number of edits or a duration of time.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum UndoKind {
    /// Jump N revisions.
    Steps(usize),
    /// Jump to revision nearest `Duration` away.
    TimePeriod(Duration),
}

/// Default undo byte budget (50 MiB).
pub const DEFAULT_UNDO_LIMIT_BYTES: usize = 50 * 1024 * 1024;

/// A revision in the undo DAG.
#[derive(Debug, Clone)]
pub struct Revision {
    /// Stable generational id.
    pub id: RevisionId,
    /// Parent revision (`None` for the root).
    pub parent: Option<RevisionId>,
    /// Child revisions.
    pub children: SmallVec<[RevisionId; 2]>,
    /// Most recently committed child (redo target).
    pub last_child: Option<RevisionId>,
    /// The transaction for this revision.
    pub transaction: Transaction,
    /// The inversion of this transaction (with pre-selection).
    pub inversion: Transaction,
    /// Commit timestamp (monotonic, in-memory navigation).
    pub timestamp: Instant,
    /// Millis since history birth (serialized ordering).
    pub stamp_ms: u64,
    /// LRU generation (bumped per commit).
    pub lru_generation: u64,
    /// Approximate memory footprint in bytes.
    pub approx_bytes: usize,
}

impl Revision {
    /// Creates a new revision.
    #[must_use]
    pub fn new(
        id: RevisionId,
        parent: Option<RevisionId>,
        transaction: Transaction,
        inversion: Transaction,
        stamp_ms: u64,
        lru_generation: u64,
    ) -> Self {
        let approx_bytes = transaction_bytes(&transaction) + transaction_bytes(&inversion);
        Self {
            id,
            parent,
            children: SmallVec::new(),
            last_child: None,
            transaction,
            inversion,
            timestamp: Instant::now(),
            stamp_ms,
            lru_generation,
            approx_bytes,
        }
    }
}

/// Heuristic memory footprint of a transaction.
fn transaction_bytes(t: &Transaction) -> usize {
    let mut n = 128usize;
    for op in t.changes.changes() {
        n += 16;
        if let Operation::Insert(s) = op {
            n += s.len();
        }
    }
    for d in &t.deleted {
        n += d.len() + 8;
    }
    if let Some(sel) = &t.selection {
        n += sel.len() * 24 + 16;
    }
    n
}

/// The undo history for a document: an acyclic revision DAG.
#[derive(Debug)]
pub struct History {
    /// Revisions in creation (id) order; timestamps stay sorted for search.
    pub(crate) revisions: Vec<Revision>,
    /// Stable id → position in `revisions`.
    pub(crate) index: HashMap<RevisionId, usize>,
    /// Currently displayed revision.
    pub(crate) current: RevisionId,
    /// Next fresh id.
    pub(crate) next_id: u64,
    /// LRU clock.
    pub(crate) lru_generation: u64,
    /// Sum of `approx_bytes` over live revisions plus ghost links.
    pub(crate) allocated_bytes: usize,
    /// Byte budget; pruning keeps usage at or below it.
    pub(crate) undo_limit_bytes: usize,
    /// Birth instant for serializable timestamps.
    pub(crate) birth: Instant,
    /// Pruned child → parent links for precise ancestor fallback.
    pub(crate) ghosts: Vec<(RevisionId, RevisionId)>,
}

impl Clone for History {
    fn clone(&self) -> Self {
        Self {
            revisions: self.revisions.clone(),
            index: self.index.clone(),
            current: self.current,
            next_id: self.next_id,
            lru_generation: self.lru_generation,
            allocated_bytes: self.allocated_bytes,
            undo_limit_bytes: self.undo_limit_bytes,
            birth: self.birth,
            ghosts: self.ghosts.clone(),
        }
    }
}

impl History {
    /// Creates a new history with an initial empty root revision.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of live revisions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.revisions.len()
    }

    /// Returns `true` if no revisions are live (never for a fresh history).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.revisions.is_empty()
    }

    /// Current revision id.
    #[must_use]
    pub fn current_revision(&self) -> RevisionId {
        self.current
    }

    /// `true` at the root revision.
    #[must_use]
    pub fn at_root(&self) -> bool {
        self.current.is_root()
    }

    /// `true` when `id` names a live revision (external-reference guard).
    #[must_use]
    pub fn is_alive(&self, id: RevisionId) -> bool {
        self.index.contains_key(&id)
    }

    /// Borrow a live revision.
    #[must_use]
    pub fn get(&self, id: RevisionId) -> Option<&Revision> {
        self.index.get(&id).map(|&pos| &self.revisions[pos])
    }

    /// Current byte budget.
    #[must_use]
    pub const fn undo_limit_bytes(&self) -> usize {
        self.undo_limit_bytes
    }

    /// Bytes currently held by live revisions.
    #[must_use]
    pub const fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }

    /// Sets the byte budget (prunes immediately when over budget).
    pub fn set_undo_limit_bytes(&mut self, limit: usize) {
        self.undo_limit_bytes = limit;
        self.maybe_evict();
    }

    /// Commits a new revision (legacy signature: explicit doc + selection).
    pub fn commit_revision(
        &mut self,
        transaction: Transaction,
        original: &Rope,
        original_sel: &Selection,
    ) {
        let state = State {
            doc: original.clone(),
            selection: original_sel.clone(),
        };
        self.commit_revision_state(&transaction, &state);
    }

    /// Commits a new revision from a transaction + pre-state.
    pub fn commit_revision_state(&mut self, transaction: &Transaction, original: &State) {
        self.commit_revision_at_timestamp(transaction, original, Instant::now());
    }

    /// Commit with an explicit timestamp (for tests / `:earlier`).
    pub fn commit_revision_at_timestamp(
        &mut self,
        transaction: &Transaction,
        original: &State,
        timestamp: Instant,
    ) {
        // Ensure the stored transaction is pure (capture deletes from pre-doc).
        let mut stored = transaction.clone();
        if !stored.is_pure() {
            stored.capture_deleted(&original.doc);
        }
        // Pure inversion with the raw pre-selection (undo restores cursor).
        let inversion = stored
            .invert()
            .map(|inv| inv.with_sel(original.selection.clone()))
            .unwrap_or_else(|| stored.invert_with_doc(&original.doc, &original.selection));
        let id = RevisionId(self.next_id);
        self.next_id += 1;
        self.lru_generation += 1;
        let stamp_ms = timestamp
            .duration_since(self.birth)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let mut rev = Revision::new(
            id,
            Some(self.current),
            stored,
            inversion,
            stamp_ms,
            self.lru_generation,
        );
        rev.timestamp = timestamp;
        self.allocated_bytes += rev.approx_bytes;
        if let Some(parent) = self.pos_of(self.current) {
            let parent_rev = &mut self.revisions[parent];
            if !parent_rev.children.contains(&id) {
                parent_rev.children.push(id);
            }
            parent_rev.last_child = Some(id);
        }
        self.index.insert(id, self.revisions.len());
        self.revisions.push(rev);
        self.current = id;
        self.maybe_evict();
    }

    /// Position of a live revision in storage order.
    pub(crate) fn pos_of(&self, id: RevisionId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// Undoes the current revision, returning its inversion.
    pub fn undo(&mut self) -> Option<&Transaction> {
        if self.at_root() {
            return None;
        }
        let curr = self.current;
        let pos = self.pos_of(curr)?;
        let parent = self.revisions[pos].parent?;
        self.current = parent;
        // Touch current for LRU recency.
        self.lru_generation += 1;
        if let Some(ppos) = self.pos_of(parent) {
            self.revisions[ppos].lru_generation = self.lru_generation;
        }
        Some(&self.revisions[pos].inversion)
    }

    /// Redoes the last-child revision, returning its transaction.
    pub fn redo(&mut self) -> Option<&Transaction> {
        let pos = self.pos_of(self.current)?;
        let child = self.revisions[pos].last_child?;
        if !self.is_alive(child) {
            self.revisions[pos].last_child = self.revisions[pos].children.last().copied();
            let child = self.revisions[pos].last_child?;
            self.current = child;
            let cpos = self.pos_of(child)?;
            return Some(&self.revisions[cpos].transaction);
        }
        self.current = child;
        self.lru_generation += 1;
        let cpos = self.pos_of(child)?;
        self.revisions[cpos].lru_generation = self.lru_generation;
        Some(&self.revisions[cpos].transaction)
    }

    /// Changes since `revision` composed into one transaction, if any.
    pub fn changes_since(&self, revision: RevisionId) -> Option<Transaction> {
        if !self.is_alive(revision) || !self.is_alive(self.current) {
            return None;
        }
        let lca = self.lowest_common_ancestor(revision, self.current);
        let up = self.path_up(revision, lca);
        let down = self.path_up(self.current, lca);
        let up_txns = up
            .iter()
            .rev()
            .map(|&n| self.get(n).map(|r| r.inversion.clone()))
            .collect::<Option<Vec<_>>>()?;
        let down_txns = down
            .iter()
            .map(|&n| self.get(n).map(|r| r.transaction.clone()))
            .collect::<Option<Vec<_>>>()?;
        down_txns
            .into_iter()
            .chain(up_txns)
            .reduce(|acc, tx| tx.compose(acc))
    }

    /// Position of the last edit (primary of inversion mapped forward).
    pub fn last_edit_pos(&self) -> Option<usize> {
        if self.at_root() {
            return None;
        }
        let rev = self.get(self.current)?;
        let primary = *rev.inversion.selection()?.primary();
        let (_from, to, _frag) = rev
            .transaction
            .changes_iter()
            .find(|(from, to, _)| {
                Range::new(*from, *to).overlaps(&primary)
            })
            .or_else(|| rev.transaction.changes_iter().next())?;
        Some(rev.transaction.changes().map_pos(to, Assoc::After))
    }

    /// Nearest live ancestor of `id` (itself when alive). External guard fallback.
    ///
    /// Follows ghost parent links left behind by pruning, so a pruned leaf
    /// resolves to its live parent instead of collapsing all the way to root.
    #[must_use]
    pub fn nearest_live_ancestor(&self, mut id: RevisionId) -> RevisionId {
        let mut guard = 0usize;
        while !self.is_alive(id) {
            match self.ghosts.iter().find(|(g, _)| *g == id) {
                Some((_, parent)) => id = *parent,
                None => return RevisionId::ROOT,
            }
            guard += 1;
            if guard > self.ghosts.len() + 8 {
                return RevisionId::ROOT;
            }
        }
        id
    }

    fn lowest_common_ancestor(&self, mut a: RevisionId, mut b: RevisionId) -> RevisionId {
        let mut a_set = HashSet::new();
        let mut b_set = HashSet::new();
        loop {
            a_set.insert(a);
            b_set.insert(b);
            if a_set.contains(&b) {
                return b;
            }
            if b_set.contains(&a) {
                return a;
            }
            // Root's parent is itself for termination (root is never pruned).
            a = self.get(a).and_then(|r| r.parent).unwrap_or(RevisionId::ROOT);
            b = self.get(b).and_then(|r| r.parent).unwrap_or(RevisionId::ROOT);
        }
    }

    fn path_up(&self, mut n: RevisionId, a: RevisionId) -> Vec<RevisionId> {
        let mut path = Vec::new();
        while n != a {
            path.push(n);
            match self.get(n).and_then(|r| r.parent) {
                Some(p) => n = p,
                None => break,
            }
        }
        path
    }

    fn jump_to(&mut self, to: RevisionId) -> Vec<Transaction> {
        let to = self.nearest_live_ancestor(to);
        let lca = self.lowest_common_ancestor(self.current, to);
        let up = self.path_up(self.current, lca);
        let down = self.path_up(to, lca);
        self.current = to;
        let up_txns = up
            .iter()
            .filter_map(|&n| self.get(n).map(|r| r.inversion.clone()));
        let down_txns = down
            .iter()
            .rev()
            .filter_map(|&n| self.get(n).map(|r| r.transaction.clone()));
        up_txns.chain(down_txns).collect()
    }

    fn clamp_alive(&self, id: u64) -> RevisionId {
        let mut id = RevisionId(id);
        while !self.is_alive(id) && !id.is_root() {
            id = RevisionId(id.0.saturating_sub(1));
        }
        id
    }

    fn max_alive(&self) -> RevisionId {
        self.revisions.last().map(|r| r.id).unwrap_or(RevisionId::ROOT)
    }

    fn revision_closer_to_instant(&self, i: usize, instant: Instant) -> usize {
        let prev = self.revisions[i - 1].timestamp;
        let curr = self.revisions[i].timestamp;
        if instant.duration_since(prev) < curr.duration_since(instant) {
            i - 1
        } else {
            i
        }
    }

    fn jump_instant(&mut self, instant: Instant) -> Vec<Transaction> {
        let revision = match self
            .revisions
            .binary_search_by(|rev| rev.timestamp.cmp(&instant))
        {
            Ok(pos) => self.revisions[pos].id,
            Err(0) => RevisionId::ROOT,
            Err(n) if n == self.revisions.len() => self.max_alive(),
            Err(i) => {
                let closer = self.revision_closer_to_instant(i, instant);
                self.revisions[closer].id
            }
        };
        self.jump_to(revision)
    }

    fn jump_duration_backward(&mut self, duration: Duration) -> Vec<Transaction> {
        let pos = match self.pos_of(self.current) {
            Some(p) => p,
            None => return self.jump_to(RevisionId::ROOT),
        };
        match self.revisions[pos].timestamp.checked_sub(duration) {
            Some(instant) => self.jump_instant(instant),
            None => self.jump_to(RevisionId::ROOT),
        }
    }

    fn jump_duration_forward(&mut self, duration: Duration) -> Vec<Transaction> {
        let pos = match self.pos_of(self.current) {
            Some(p) => p,
            None => return self.jump_to(self.max_alive()),
        };
        match self.revisions[pos].timestamp.checked_add(duration) {
            Some(instant) => self.jump_instant(instant),
            None => self.jump_to(self.max_alive()),
        }
    }

    /// Undo navigation (steps or time).
    pub fn earlier(&mut self, uk: UndoKind) -> Vec<Transaction> {
        match uk {
            UndoKind::Steps(n) => self.jump_backward(n),
            UndoKind::TimePeriod(d) => self.jump_duration_backward(d),
        }
    }

    /// Redo navigation (steps or time).
    pub fn later(&mut self, uk: UndoKind) -> Vec<Transaction> {
        match uk {
            UndoKind::Steps(n) => self.jump_forward(n),
            UndoKind::TimePeriod(d) => self.jump_duration_forward(d),
        }
    }

    fn jump_backward(&mut self, delta: usize) -> Vec<Transaction> {
        let target = self.current.0.saturating_sub(delta as u64);
        self.jump_to(self.clamp_alive(target))
    }

    fn jump_forward(&mut self, delta: usize) -> Vec<Transaction> {
        let target = self.current.0.saturating_add(delta as u64);
        let max = self.max_alive().0;
        self.jump_to(self.clamp_alive(target.min(max)))
    }
}

impl Default for History {
    fn default() -> Self {
        let birth = Instant::now();
        let root = Revision {
            id: RevisionId::ROOT,
            parent: None,
            children: SmallVec::new(),
            last_child: None,
            transaction: Transaction::empty(),
            inversion: Transaction::empty(),
            timestamp: birth,
            stamp_ms: 0,
            lru_generation: 0,
            approx_bytes: 256,
        };
        let mut index = HashMap::new();
        index.insert(RevisionId::ROOT, 0);
        Self {
            revisions: vec![root],
            index,
            current: RevisionId::ROOT,
            next_id: 1,
            lru_generation: 0,
            allocated_bytes: 256,
            undo_limit_bytes: DEFAULT_UNDO_LIMIT_BYTES,
            birth,
            ghosts: Vec::new(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::change::ChangeSet;
    use crate::change::Tendril;
    use crate::range::Range as DocRange;

    fn commit_str(history: &mut History, state: &mut State, from: usize, to: usize, text: &str) {
        let rep = if text.is_empty() {
            None
        } else {
            Some(text.into())
        };
        let txn = Transaction::change(&state.doc, std::iter::once((from, to, rep)));
        history.commit_revision_state(&txn, state);
        txn.apply_doc(&mut state.doc);
    }

    #[test]
    fn test_history_undo_redo_linear() {
        let mut history = History::new();
        let mut doc = Rope::from_str("hello");
        let sel = Selection::new(DocRange::point(5));

        let c1 = ChangeSet::new(vec![
            Operation::Retain(5),
            Operation::Insert(" world".into()),
        ]);
        let t1 = Transaction::new(c1);
        let orig_doc = doc.clone();
        assert!(t1.changes.apply(&mut doc));
        history.commit_revision(t1, &orig_doc, &sel);

        assert_eq!(doc.len(), 11);
        assert_eq!(history.len(), 2);
        assert_eq!(history.current_revision(), RevisionId(1));

        let undo_txn = history.undo().expect("should undo revision 1").clone();
        assert!(undo_txn.changes.apply(&mut doc));
        assert_eq!(doc.len(), 5);
        assert!(history.at_root());

        assert!(history.undo().is_none());

        let redo_txn = history.redo().expect("should redo revision 1").clone();
        assert!(redo_txn.changes.apply(&mut doc));
        assert_eq!(doc.len(), 11);
        assert_eq!(history.current_revision(), RevisionId(1));

        assert!(history.redo().is_none());
    }

    #[test]
    fn test_history_branch_earlier_later() {
        let mut history = History::default();
        let mut state = State {
            doc: Rope::from_str("a\n"),
            selection: Selection::point(0),
        };
        let commit = |history: &mut History, state: &mut State, change: (usize, usize, Option<Tendril>)| {
            let txn = Transaction::change(&state.doc, std::iter::once(change));
            history.commit_revision_state(&txn, state);
            txn.apply_doc(&mut state.doc);
        };
        commit(&mut history, &mut state, (1, 1, Some(" b".into())));
        commit(&mut history, &mut state, (3, 3, Some(" c".into())));
        assert_eq!(state.doc.len(), 6);
        let back = history.earlier(UndoKind::Steps(1));
        for txn in back {
            txn.apply_doc(&mut state.doc);
        }
        assert_eq!(state.doc.len(), 4);
        let fwd = history.later(UndoKind::Steps(1));
        for txn in fwd {
            txn.apply_doc(&mut state.doc);
        }
        assert_eq!(state.doc.len(), 6);
    }

    #[test]
    fn test_revision_ids_stable_across_branching() {
        let mut history = History::default();
        let mut state = State {
            doc: Rope::from_str("ab"),
            selection: Selection::point(2),
        };
        commit_str(&mut history, &mut state, 2, 2, "c");
        let tip1 = history.current_revision();
        history.undo().expect("undo1").clone().apply_doc(&mut state.doc);
        commit_str(&mut history, &mut state, 2, 2, "d");
        let tip2 = history.current_revision();
        assert_ne!(tip1, tip2);
        // Both branches keep stable ids; parents resolve.
        assert!(history.is_alive(tip1));
        assert!(history.is_alive(tip2));
        assert_eq!(history.get(tip1).expect("tip1").parent, Some(RevisionId::ROOT));
        assert_eq!(history.get(tip2).expect("tip2").parent, Some(RevisionId::ROOT));
        // Root has two children (branching preserved).
        assert_eq!(
            history.get(RevisionId::ROOT).expect("root").children.len(),
            2
        );
    }

    #[test]
    fn test_lru_eviction_bounds_memory() {
        let mut history = History::default();
        history.set_undo_limit_bytes(4096);
        let mut state = State {
            doc: Rope::from_str(""),
            selection: Selection::point(0),
        };
        for i in 0..50 {
            let pos = state.doc.len();
            commit_str(&mut history, &mut state, pos, pos, &format!("line{i:02}\n"));
        }
        assert!(history.allocated_bytes() <= history.undo_limit_bytes());
        assert!(history.is_alive(history.current_revision()));
        assert!(history.is_alive(RevisionId::ROOT));
        // Current path still navigable after eviction.
        assert!(history.undo().is_some());
        assert!(history.redo().is_some());
    }

    #[test]
    fn test_pruned_leaf_falls_back_to_live_parent() {
        let mut history = History::default();
        let mut state = State {
            doc: Rope::from_str("a"),
            selection: Selection::point(1),
        };
        // Branch A: root -> r1 -> r2. Current: root -> r3.
        commit_str(&mut history, &mut state, 1, 1, "b");
        let r1 = history.current_revision();
        commit_str(&mut history, &mut state, 2, 2, "c");
        let r2 = history.current_revision();
        let r2bytes = history.get(r2).expect("r2").approx_bytes;
        for txn in history.earlier(UndoKind::Steps(10)) {
            txn.apply_doc(&mut state.doc);
        }
        commit_str(&mut history, &mut state, 1, 1, "z");
        // Budget room for everything except orphan leaf r2 (plus ghost links).
        history.set_undo_limit_bytes(history.allocated_bytes() - r2bytes + 64);
        assert!(!history.is_alive(r2));
        assert!(history.is_alive(r1));
        assert_eq!(history.nearest_live_ancestor(r2), r1);
    }
}
