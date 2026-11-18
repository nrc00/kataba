//! LRU eviction and root-collapse for the undo history DAG.
//!
//! Prunes unreferenced leaves (oldest first), then collapses root children,
//! then drops ghost links — never pruning the root or the current revision.

use crate::history::dag::{History, RevisionId};

/// Budget cost of one ghost parent link.
const GHOST_LINK_BYTES: usize = 32;

impl History {
    /// Prune until within budget: unreferenced leaves (oldest first), then
    /// root collapse, then ghost links (oldest first, degrades fallback precision).
    /// Never prunes the root or the current revision.
    pub(crate) fn maybe_evict(&mut self) {
        while self.allocated_bytes > self.undo_limit_bytes {
            let leaf = self
                .revisions
                .iter()
                .filter(|r| !r.id.is_root() && r.id != self.current && r.children.is_empty())
                .min_by_key(|r| r.lru_generation)
                .map(|r| r.id);
            if let Some(victim) = leaf {
                self.remove_leaf(victim);
                continue;
            }
            if self.collapse_oldest_root_child() {
                continue;
            }
            if !self.ghosts.is_empty() {
                self.ghosts.remove(0);
                self.allocated_bytes = self.allocated_bytes.saturating_sub(GHOST_LINK_BYTES);
                continue;
            }
            break;
        }
    }

    /// Removes a childless, non-current revision and unlinks it from its parent.
    /// Leaves a ghost parent link so external references still resolve.
    fn remove_leaf(&mut self, id: RevisionId) {
        let pos = match self.pos_of(id) {
            Some(p) => p,
            None => return,
        };
        let rev = self.revisions.remove(pos);
        self.allocated_bytes = self.allocated_bytes.saturating_sub(rev.approx_bytes);
        self.index.remove(&id);
        for (i, r) in self.revisions.iter().enumerate().skip(pos) {
            self.index.insert(r.id, i);
        }
        if let Some(parent_id) = rev.parent {
            if let Some(ppos) = self.pos_of(parent_id) {
                let parent = &mut self.revisions[ppos];
                parent.children.retain(|c| *c != id);
                if parent.last_child == Some(id) {
                    parent.last_child = parent.children.last().copied();
                }
            }
            self.ghosts.push((id, parent_id));
            self.allocated_bytes += GHOST_LINK_BYTES;
        }
    }

    /// Folds the oldest root child into the root (bounded linear history).
    ///
    /// The victim's children are reparented to the root; undo past the new
    /// root is lost, but all live ids stay stable and the current path stays
    /// navigable. Returns `false` when no foldable victim exists.
    fn collapse_oldest_root_child(&mut self) -> bool {
        let root_pos = match self.pos_of(RevisionId::ROOT) {
            Some(p) => p,
            None => return false,
        };
        let victim = self.revisions[root_pos]
            .children
            .iter()
            .copied()
            .filter(|&id| id != self.current)
            .filter_map(|id| self.get(id))
            .min_by_key(|r| r.lru_generation)
            .map(|r| r.id);
        let victim = match victim {
            Some(id) => id,
            None => return false,
        };
        let vpos = match self.pos_of(victim) {
            Some(p) => p,
            None => return false,
        };
        let (children, bytes) = {
            let rev = &self.revisions[vpos];
            (rev.children.clone(), rev.approx_bytes)
        };
        // Reparent the victim's children to the root.
        for child in children.iter() {
            if let Some(cpos) = self.pos_of(*child) {
                self.revisions[cpos].parent = Some(RevisionId::ROOT);
            }
        }
        self.revisions.remove(vpos);
        self.allocated_bytes = self.allocated_bytes.saturating_sub(bytes);
        self.index.remove(&victim);
        for (i, r) in self.revisions.iter().enumerate().skip(vpos.min(self.revisions.len())) {
            self.index.insert(r.id, i);
        }
        if let Some(new_root) = self.pos_of(RevisionId::ROOT) {
            let root = &mut self.revisions[new_root];
            root.children.retain(|c| *c != victim);
            for child in children.iter() {
                if !root.children.contains(child) {
                    root.children.push(*child);
                }
            }
            if root.last_child == Some(victim) {
                root.last_child = children.last().copied().or(root.children.last().copied());
            }
        }
        self.ghosts.push((victim, RevisionId::ROOT));
        self.allocated_bytes += GHOST_LINK_BYTES;
        true
    }
}
