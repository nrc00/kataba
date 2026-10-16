//! `git` backend driver.
//!
//! Reads: gix in-process with a forced trust level (Helix pattern — we
//! discover ourselves, then force trust so repo-local `filter.*` clean/smudge
//! drivers never run untrusted).
//! Writes: `git` CLI always — we never reimplement rebase/merge/staging
//! semantics.
//!
//! Trust is caller-supplied (`Trust::Reduced` default), matching Helix's
//! workspace-trust model.

pub mod discovery;
pub mod status;
pub mod write;

pub use discovery::open_repo;
pub use status::repo_status;
pub use write::*;

use std::path::{Path, PathBuf};

use crate::{Error, Result};
use crate::Capabilities;

/// A git repository handle.
pub struct GitRepo {
    pub worktree: PathBuf,
    pub repo: gix::Repository,
    pub trust: Trust,
    pub capabilities: Capabilities,
}

impl GitRepo {
    /// Discover and open the repository nearest to `path` (walking up).
    ///
    /// `trust` decides whether repo-local config drivers are honored.
    pub fn open(path: &Path, trust: Trust) -> Result<Self> {
        let (git_dir, worktree) = discover(path)?;
        let ts_repo = open_repo(&git_dir, trust)?;
        // ThreadSafeRepository is the Send+Sync handle; convert to a plain
        // Repository for direct method access (single-threaded reads here).
        let repo: gix::Repository = ts_repo.into();
        Ok(Self {
            worktree,
            repo,
            trust,
            capabilities: Capabilities::git(),
        })
    }

    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    pub fn trust(&self) -> Trust {
        self.trust
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
}

/// Trust decision for opening a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trust {
    /// Honor no repo-local config drivers (safe default).
    #[default]
    Reduced,
    /// Honor full repo config (user explicitly trusted this workspace).
    Full,
}

/// Discover the `.git` directory and worktree by walking up from `path`.
///
/// Returns `(git_dir, worktree_root)`.
pub fn discover(path: &Path) -> Result<(PathBuf, PathBuf)> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    let mut cur = abs.clone();
    loop {
        let dot_git = cur.join(".git");
        if dot_git.is_dir() || dot_git.is_file() {
            return Ok((dot_git, cur));
        }
        if !cur.pop() {
            break;
        }
    }

    Err(Error::RepoNotFound { path: abs })
}
