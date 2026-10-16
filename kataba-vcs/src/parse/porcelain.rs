//! `git status --porcelain=v2 -z` parser.
//!
//! Machine-readable, NUL-terminated, stable across git versions. This is the
//! authoritative status source for kataba-vcs (gix status remains a
//! benchmarked promotion candidate, gated on conformance tests).
//!
//! Format (from `git status --help`):
//!   <xy> <m> <m> <score> <sub> <from> NUL <to> NUL
//!   <xy> <m> <m> <score> <sub> NUL <path> NUL
//!
//! `<xy>` is exactly two bytes: index status then worktree status. The path
//! (and, for renames, the source path) lives on the NUL-terminated segment(s)
//! *after* the `<xy>` line.

use std::path::PathBuf;

use smallvec::SmallVec;

use crate::{Error, Result};
use crate::model::{FileChange, StatusSnapshot};

/// Parse `--porcelain=v2 -z` output into a status listing.
///
/// `head` is resolved separately (HEAD may be detached/unborn).
pub fn parse_porcelain_v2(
    bytes: &[u8],
    head: Option<String>,
    gen: u64,
) -> Result<StatusSnapshot> {
    let mut changes = SmallVec::new();
    let mut i = 0;

    while i < bytes.len() {
        // Read the `<xy>` line up to its NUL terminator.
        let line_end = find_nul(bytes, i)?;
        let line = &bytes[i..line_end];
        i = line_end + 1;

        if line.is_empty() || line.len() < 2 {
            continue;
        }

        // `<xy>` is the first two bytes: index status, then worktree status.
        let x = line[0];
        let y = line[1];

        let change = match (x, y) {
            (b'?', b'?') => {
                // Untracked: path on the next NUL segment.
                let path_end = find_nul(bytes, i)?;
                let path = &bytes[i..path_end];
                i = path_end + 1;
                FileChange::untracked(PathBuf::from(path_lossy(path)))
            }
            (b'A', b' ') | (b'M', b' ') | (b'M', b'M') | (b'A', b'M') => {
                // Staged (possibly also modified in worktree): path next.
                let path_end = find_nul(bytes, i)?;
                let path = &bytes[i..path_end];
                i = path_end + 1;
                FileChange::modified(PathBuf::from(path_lossy(path)))
            }
            (b' ', b'M') => {
                // Worktree-modified, index clean: path next.
                let path_end = find_nul(bytes, i)?;
                let path = &bytes[i..path_end];
                i = path_end + 1;
                FileChange::modified(PathBuf::from(path_lossy(path)))
            }
            (b' ', b'D') | (b'D', b' ') => {
                // Deleted (worktree or index): path next.
                let path_end = find_nul(bytes, i)?;
                let path = &bytes[i..path_end];
                i = path_end + 1;
                FileChange::deleted(PathBuf::from(path_lossy(path)))
            }
            (b'R', b' ') | (b'R', b'M') => {
                // Renamed: from on next segment, to on the one after.
                let from_end = find_nul(bytes, i)?;
                let from_path = &bytes[i..from_end];
                i = from_end + 1;
                let to_end = find_nul(bytes, i)?;
                let to_path = &bytes[i..to_end];
                i = to_end + 1;
                FileChange::renamed(
                    PathBuf::from(path_lossy(from_path)),
                    PathBuf::from(path_lossy(to_path)),
                )
            }
            (b'U', b' ') | (b'A', b'U') | (b'U', b'A') | (b'U', b'U') => {
                // Conflict: path next.
                let path_end = find_nul(bytes, i)?;
                let path = &bytes[i..path_end];
                i = path_end + 1;
                FileChange::conflict(PathBuf::from(path_lossy(path)))
            }
            _ => {
                // Unknown status combination: skip the path segment(s) and
                // keep scanning. For safety, skip one segment.
                if i < bytes.len() {
                    let _ = find_nul(bytes, i)?;
                    i = line_end + 1;
                }
                continue;
            }
        };

        changes.push(change);
    }

    Ok(StatusSnapshot {
        gen,
        head,
        changes,
    })
}

/// Find the next NUL terminator starting at `start`.
fn find_nul(bytes: &[u8], start: usize) -> Result<usize> {
    bytes[start..]
        .iter()
        .position(|b| *b == b'\0')
        .map(|p| start + p)
        .ok_or_else(|| Error::Parse("unterminated porcelain entry".into()))
}

fn path_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChangeKind;

    #[test]
    fn untracked_entry() {
        let mut input = Vec::new();
        input.extend_from_slice(b"?? ...... - .\0");
        input.extend_from_slice(b"new_file.rs\0");
        let snap = parse_porcelain_v2(&input, None, 1).unwrap();
        assert_eq!(snap.changes.len(), 1);
        assert!(matches!(snap.changes[0].kind, ChangeKind::Untracked));
        assert_eq!(snap.changes[0].path, PathBuf::from("new_file.rs"));
    }

    #[test]
    fn modified_entry() {
        let mut input = Vec::new();
        input.extend_from_slice(b" M ...... - .\0");
        input.extend_from_slice(b"modified.rs\0");
        let snap = parse_porcelain_v2(&input, None, 1).unwrap();
        assert_eq!(snap.changes.len(), 1);
        assert!(matches!(snap.changes[0].kind, ChangeKind::Modified));
    }

    #[test]
    fn renamed_entry() {
        let mut input = Vec::new();
        input.extend_from_slice(b"R 100644 100644 100% .\0");
        input.extend_from_slice(b"old_name.rs\0");
        input.extend_from_slice(b"new_name.rs\0");
        let snap = parse_porcelain_v2(&input, None, 1).unwrap();
        assert_eq!(snap.changes.len(), 1);
        match &snap.changes[0].kind {
            ChangeKind::Renamed { from_path, to_path } => {
                assert_eq!(*from_path, PathBuf::from("old_name.rs"));
                assert_eq!(*to_path, PathBuf::from("new_name.rs"));
            }
            other => panic!("expected renamed, got {other:?}"),
        }
    }

    #[test]
    fn deleted_entry() {
        let mut input = Vec::new();
        input.extend_from_slice(b" D ...... - .\0");
        input.extend_from_slice(b"gone.rs\0");
        let snap = parse_porcelain_v2(&input, None, 1).unwrap();
        assert_eq!(snap.changes.len(), 1);
        assert!(matches!(snap.changes[0].kind, ChangeKind::Deleted));
    }

    #[test]
    fn multiple_entries() {
        let mut input = Vec::new();
        input.extend_from_slice(b" M ...... - .\0");
        input.extend_from_slice(b"a.rs\0");
        input.extend_from_slice(b"?? ...... - .\0");
        input.extend_from_slice(b"b.rs\0");
        input.extend_from_slice(b" D ...... - .\0");
        input.extend_from_slice(b"c.rs\0");
        let snap = parse_porcelain_v2(&input, None, 1).unwrap();
        assert_eq!(snap.changes.len(), 3);
    }

    #[test]
    fn worktree_modified_index_clean() {
        let mut input = Vec::new();
        input.extend_from_slice(b" M ...... - .\0");
        input.extend_from_slice(b"edited.rs\0");
        let snap = parse_porcelain_v2(&input, None, 1).unwrap();
        assert_eq!(snap.changes.len(), 1);
        assert!(matches!(snap.changes[0].kind, ChangeKind::Modified));
    }
}
