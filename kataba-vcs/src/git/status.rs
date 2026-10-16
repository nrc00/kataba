//! `git status --porcelain=v2 -z` via gix, parsed by `parse::porcelain`.
//!
//! Authority: porcelain output. gix is used only to locate the repo and,
//! where it adds value, to read base blobs (see `gutter.rs` later).
//!
//! Renames: we request rewrite detection so `R` entries appear.

use bstr::ByteSlice;
use gix::diff::Rewrites;
use gix::dir::entry::Status as DirStatus;
use gix::progress::Discard;
use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
use gix::status::{index_worktree::Item, UntrackedFiles};

use crate::exec::run_in;
use crate::Result;
use crate::model::{FileChange, StatusSnapshot};
use crate::parse::porcelain::parse_porcelain_v2;

/// Read the authoritative working-tree status for `worktree` via the `git`
/// CLI (`--porcelain=v2 -z`).
///
/// `gen` is a caller-supplied snapshot generation (actor layer V3).
pub fn repo_status(
    _repo: &gix::Repository,
    worktree: &std::path::Path,
    head: Option<String>,
    gen: u64,
) -> Result<StatusSnapshot> {
    // Prefer porcelain v2 -z as the single source of truth.
    let out = run_in(
        worktree,
        "git",
        ["status", "--porcelain=v2", "-z", "--branch"],
    )?;

    // Re-parse from raw bytes properly (two-line protocol).
    parse_porcelain_v2(out.stdout.as_slice(), head, gen)
}

/// Read status directly via gix status plumbing as a cross-check / future
/// default once conformance tests pass against porcelain.
#[allow(dead_code)]
pub fn repo_status_gix(
    repo: &gix::Repository,
    worktree: &std::path::Path,
    head: Option<String>,
    gen: u64,
) -> Result<StatusSnapshot> {
    let status_platform = repo
        .status(Discard)?
        .untracked_files(UntrackedFiles::Files)
        .index_worktree_rewrites(Some(Rewrites {
            copies: None,
            percentage: Some(0.5),
            limit: 1000,
            ..Default::default()
        }));

    let empty_patterns: Vec<gix::bstr::BString> = Vec::new();
    let mut changes = Vec::new();
    for item in status_platform.into_index_worktree_iter(empty_patterns)? {
        let item = item.map_err(|e| crate::Error::GitCli(format!("gix status: {e}")))?;
        let change = match item {
            Item::Modification {
                rela_path, status, ..
            } => {
                let path = worktree.join(&*rela_path.to_path()?);
                match status {
                    EntryStatus::Conflict { .. } => FileChange::conflict(path),
                    EntryStatus::Change(change_kind) => match change_kind {
                        Change::Removed => FileChange::deleted(path),
                        Change::Modification { .. } => FileChange::modified(path),
                        _ => continue,
                    },
                    EntryStatus::IntentToAdd => FileChange::untracked(path),
                    _ => continue,
                }
            }
            Item::DirectoryContents {
                entry, ..
            } if entry.status == DirStatus::Untracked => {
                FileChange::untracked(worktree.join(&*entry.rela_path.to_path()?))
            }
            Item::Rewrite {
                source,
                dirwalk_entry,
                ..
            } => FileChange::renamed(
                worktree.join(&*source.rela_path().to_path()?),
                worktree.join(&*dirwalk_entry.rela_path.to_path()?),
            ),
            _ => continue,
        };
        changes.push(change);
    }

    Ok(StatusSnapshot {
        gen,
        head,
        changes: changes.into(),
    })
}

/// Resolve the current HEAD name (branch or short hash when detached).
pub fn head_name(repo: &gix::Repository) -> Result<Option<String>> {
    let head_ref = repo.head_ref()?;
    let head_commit = repo.head_commit()?;
    let name = match head_ref {
        Some(reference) => reference.name().shorten().to_string(),
        None => head_commit.id.to_hex_with_len(8).to_string(),
    };
    Ok(Some(name))
}

/// List local branches.
pub fn branches(repo: &gix::Repository) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for name in repo.branch_names() {
        out.push(name.to_string());
    }
    Ok(out)
}

/// Convenience wrapper around `GitRepo` for status reads.
pub struct StatusReader;

impl StatusReader {
    pub fn status(repo: &crate::git::GitRepo) -> Result<StatusSnapshot> {
        let head = head_name(&repo.repo)?;
        repo_status(&repo.repo, &repo.worktree, head, 0)
    }
}
