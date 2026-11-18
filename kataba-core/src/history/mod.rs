//! History/undo system: stable generational DAG + persistent undofiles.
//!
//! Revisions carry monotonic [`RevisionId`]s that never shift during root
//! collapses or LRU leaf pruning. Memory is bounded by a byte budget
//! ([`History::set_undo_limit_bytes`]); unreferenced leaves with the lowest
//! `lru_generation` are pruned first. External references (marks, jump lists)
//! must query [`History::is_alive`] before jumping.
//!
//! Persistence is pure bytes: [`History::encode`] snapshots the DAG with a
//! SHA-256 integrity header (Neovim-style quarantine on mismatch) and
//! [`History::decode`] restores it. File I/O stays in the outer shell.

pub mod dag;
pub mod lru;
pub mod undofile;

pub use dag::{
    DEFAULT_UNDO_LIMIT_BYTES, History, Revision, RevisionId, State, UndoKind,
};
