//! Domain model: VCS-neutral types describing repo state.
//!
//! Pure data — no I/O, no async. Byte-exact: paths are `PathBuf` (bytes),
//! not UTF-8-assuming strings.

use std::path::PathBuf;

use smallvec::SmallVec;

/// A content-addressed object ID (git SHA-1, jj change ID hex).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Oid([u8; 20]);

impl Oid {
    pub const EMPTY: Self = Oid([0; 20]);

    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> [u8; 20] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(40);
        for b in &self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    pub fn short(&self, len: usize) -> String {
        let n = len.min(40);
        let mut s = String::with_capacity(n);
        for b in &self.0[..n / 2] {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 20]
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A single file change observed in a working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// Not tracked by the VCS.
    Untracked,
    /// Modified relative to the index/base.
    Modified,
    /// Conflict (unmerged).
    Conflict,
    /// Deleted.
    Deleted,
    /// Renamed: `from_path` → `to_path`.
    Renamed { from_path: PathBuf, to_path: PathBuf },
}

impl ChangeKind {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Untracked { .. } => unreachable!("Untracked carries no path here"),
            Self::Modified { .. } => unreachable!(),
            Self::Conflict { .. } => unreachable!(),
            Self::Deleted { .. } => unreachable!(),
            Self::Renamed { to_path, .. } => to_path,
        }
    }
}

/// One entry in a status listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

impl FileChange {
    pub fn untracked(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Untracked,
        }
    }

    pub fn modified(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Modified,
        }
    }

    pub fn deleted(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Deleted,
        }
    }

    pub fn conflict(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: ChangeKind::Conflict,
        }
    }

    pub fn renamed(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        let to = to.into();
        Self {
            path: to.clone(),
            kind: ChangeKind::Renamed {
                from_path: from.into(),
                to_path: to,
            },
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// Authoritative status snapshot for one repository at one moment.
#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub gen: u64,
    pub head: Option<String>,
    pub changes: SmallVec<[FileChange; 8]>,
}

impl StatusSnapshot {
    pub fn new(gen: u64, head: Option<String>) -> Self {
        Self {
            gen,
            head,
            changes: SmallVec::new(),
        }
    }

    pub fn is_clean(&self) -> bool {
        self.changes.is_empty()
    }
}

/// A line inside a unified diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

/// One line of a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// Raw line content without trailing newline.
    pub text: Vec<u8>,
    /// Old file line number, if any.
    pub old_lineno: Option<u32>,
    /// New file line number, if any.
    pub new_lineno: Option<u32>,
}

/// One diff hunk.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub header: Vec<u8>,
    pub lines: SmallVec<[DiffLine; 16]>,
}

/// A unified diff for one file (patch-ready).
#[derive(Debug, Clone)]
pub struct Patch {
    pub old_path: std::path::PathBuf,
    pub new_path: std::path::PathBuf,
    pub old_mode: Option<u32>,
    pub new_mode: Option<u32>,
    pub new_or_deleted: bool,
    pub hunks: SmallVec<[DiffHunk; 1]>,
}

/// A revision/commit entry in a log.
#[derive(Debug, Clone)]
pub struct Revision {
    pub id: Oid,
    pub summary: Vec<u8>,
    pub author: Vec<u8>,
    pub when: i64, // unix seconds
    pub parents: SmallVec<[Oid; 2]>,
}

/// A ref (branch / bookmark / tag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    Bookmark,
    Tag,
    Head,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub name: Vec<u8>,
    pub kind: RefKind,
    pub target: Oid,
}

/// A line of blame output.
#[derive(Debug, Clone)]
pub struct BlameLine {
    pub oid: Oid,
    pub orig_lineno: u32,
    pub lineno: u32,
    pub text: Vec<u8>,
}

/// A remote reference hint (for forge detection later).
#[derive(Debug, Clone)]
pub struct RemoteRef {
    pub name: Vec<u8>,
    pub url: Vec<u8>,
    pub kind: RemoteKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    Fetch,
    Push,
}

/// Range of lines within a file, for hunk staging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkRange {
    pub start_line: u32,
    pub end_line: u32,
}
