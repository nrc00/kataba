//! Capability gate. Mirrors kataba-lsp's `L3 (Capability Precondition)`:
//! no feature request before the backend confirms it can serve it.
//!
//! Git: has_index, has_staging, stable_change_ids, blame.
//! Jujutsu: has_op_log, has_bookmarks, stable_change_ids, no index.
//!
//! A UI keyed off these capabilities never fakes an index for jj, and never
//! offers a branch menu to a backend that only knows change IDs.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// The backend exposes a mutable index / staging area.
    pub has_index: bool,
    /// Patch-style hunk staging is available (implies has_index).
    pub has_staging: bool,
    /// Revision IDs are stable and human-meaningful for display.
    pub stable_change_ids: bool,
    /// An operation log exists (undo/redo across sessions).
    pub has_op_log: bool,
    /// Movable refs (bookmarks in jj, branches in git) exist.
    pub has_bookmarks: bool,
    /// Line blame is supported.
    pub blame: bool,
    /// Minimum CLI version required, if the backend shells out.
    pub min_cli_version: Option<(u32, u32, u32)>,
}

impl Capabilities {
    /// git (M0–M3)
    pub const fn git() -> Self {
        Self {
            has_index: true,
            has_staging: true,
            stable_change_ids: false, // SHA-1s are stable but not human-meaningful
            has_op_log: false,
            has_bookmarks: true,
            blame: true,
            min_cli_version: None,
        }
    }

    /// jujutsu (M4)
    pub const fn jj() -> Self {
        Self {
            has_index: false,
            has_staging: false,
            stable_change_ids: true,
            has_op_log: true,
            has_bookmarks: true,
            blame: false, // not yet
            min_cli_version: Some((0, 13, 0)),
        }
    }
}
