//! Git write operations via the `git` CLI.
//!
//! We never reimplement rebase/merge/staging semantics — `git` CLI owns
//! them. Every mutation is followed by a caller-side re-read (axiom V1).

use std::path::Path;

use crate::{Error, Result};
use crate::exec::run_in;
use crate::model::HunkRange;

/// Stage whole file(s): `git add -- <paths>`.
pub fn stage_file(cwd: &Path, paths: &[&Path]) -> Result<()> {
    let mut args = Vec::with_capacity(2 + paths.len());
    args.push("add");
    args.push("--");
    for p in paths {
        args.push(p.to_str().ok_or_else(|| Error::Parse("non-UTF8 path".into()))?);
    }
    run_in(cwd, "git", args)?;
    Ok(())
}

/// Unstage whole file(s): `git restore --staged -- <paths>`.
pub fn unstage_file(cwd: &Path, paths: &[&Path]) -> Result<()> {
    let mut args = Vec::with_capacity(3 + paths.len());
    args.push("restore");
    args.push("--staged");
    args.push("--");
    for p in paths {
        args.push(p.to_str().ok_or_else(|| Error::Parse("non-UTF8 path".into()))?);
    }
    run_in(cwd, "git", args)?;
    Ok(())
}

/// Discard working-tree changes: `git restore -- <paths>`.
pub fn discard(cwd: &Path, paths: &[&Path]) -> Result<()> {
    let mut args = Vec::with_capacity(2 + paths.len());
    args.push("restore");
    args.push("--");
    for p in paths {
        args.push(p.to_str().ok_or_else(|| Error::Parse("non-UTF8 path".into()))?);
    }
    run_in(cwd, "git", args)?;
    Ok(())
}

/// Commit with message: `git commit -m <msg>`.
pub fn commit(cwd: &Path, message: &str) -> Result<()> {
    run_in(cwd, "git", ["commit", "-m", message])?;
    Ok(())
}

/// Amend: `git commit --amend -m <msg> --no-edit`.
pub fn amend(cwd: &Path, message: &str) -> Result<()> {
    run_in(cwd, "git", ["commit", "--amend", "-m", message, "--no-edit"])?;
    Ok(())
}

/// Create branch: `git branch <name>`.
pub fn branch_create(cwd: &Path, name: &str) -> Result<()> {
    run_in(cwd, "git", ["branch", name])?;
    Ok(())
}

/// Switch branch: `git switch <name>`.
pub fn branch_switch(cwd: &Path, name: &str) -> Result<()> {
    run_in(cwd, "git", ["switch", name])?;
    Ok(())
}

/// Stash push: `git stash push [-m <msg>] [-- <paths>...]`
pub fn stash_push(cwd: &Path, message: Option<&str>, paths: Option<&[&Path]>) -> Result<()> {
    let mut args = Vec::with_capacity(4);
    args.push("stash");
    args.push("push");
    if let Some(msg) = message {
        args.push("-m");
        args.push(msg);
    }
    if let Some(ps) = paths {
        args.push("--");
        for p in ps {
            args.push(p.to_str().ok_or_else(|| Error::Parse("non-UTF8 path".into()))?);
        }
    }
    run_in(cwd, "git", args)?;
    Ok(())
}

/// Stage selected hunks via a constructed unified patch → `git apply --cached`.
pub fn stage_hunks(_cwd: &Path, _file: &Path, _ranges: &[HunkRange]) -> Result<()> {
    Err(Error::NotSupported("hunk staging"))
}

/// Unstage selected hunks → `git apply --cached --reverse`.
pub fn unstage_hunks(_cwd: &Path, _file: &Path, _ranges: &[HunkRange]) -> Result<()> {
    Err(Error::NotSupported("hunk staging"))
}
