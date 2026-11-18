use anyhow::{bail, Context, Result};
use gix::Repository;
use std::path::{Path, PathBuf};

/// A single file change, tagged with the section it belongs to.
#[derive(Debug, Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub mode: ChangeKind,
    pub section: Section,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

impl ChangeKind {
    pub fn letter(self) -> char {
        match self {
            ChangeKind::Added => 'A',
            ChangeKind::Modified => 'M',
            ChangeKind::Deleted => 'D',
        }
    }

    pub fn from_letter(c: char) -> Self {
        match c {
            'A' => ChangeKind::Added,
            'D' => ChangeKind::Deleted,
            _ => ChangeKind::Modified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Staged,
    Unstaged,
    Untracked,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Staged => "Staged changes",
            Section::Unstaged => "Unstaged changes",
            Section::Untracked => "Untracked files",
        }
    }

    pub fn idx(self) -> usize {
        match self {
            Section::Staged => 0,
            Section::Unstaged => 1,
            Section::Untracked => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
}

pub struct HeadInfo {
    pub branch: String,    // "(detached)" when detached
    pub oid: String,
    pub abbrev: String,
    pub commit_message: String,
    pub detached: bool,
}

/// A thin wrapper around a git repository. All git operations shell out to the
/// `git` executable — exactly like neogit/Magit do in the background — which
/// keeps the implementation simple and correct without fighting a changing
/// library API.
pub struct GitRepo {
    repo: Repository,
    worktree: PathBuf,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self> {
        let worktree = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        // Validate it's a git repo and keep a gix handle for reads.
        let repo = gix::open(&worktree).with_context(|| format!("failed to open git repository at {path}"))?;
        Ok(Self { repo, worktree })
    }

    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    pub fn head_info(&self) -> Result<HeadInfo> {
        let head = self.repo.head()?;
        let detached = head.is_detached();
        let branch = if detached {
            "(detached)".to_string()
        } else {
            Self::run_git(&self.worktree, &["symbolic-ref", "--short", "HEAD"])?
        };
        let id = self.repo.head_id()?;
        let oid = id.to_string();
        let abbrev = id.shorten_or_id().to_string();
        let commit_message = Self::run_git(&self.worktree, &["log", "-1", "--format=%s"])?;
        Ok(HeadInfo { branch, oid, abbrev, commit_message, detached })
    }

    /// Parse `git status --porcelain=v1` into staged / unstaged / untracked items.
    pub fn status(&self) -> Result<Vec<FileItem>> {
        let out = Self::run_git(
            &self.worktree,
            &["status", "--porcelain=v1", "-u", "--no-renames"],
        )?;
        let mut files: Vec<FileItem> = Vec::new();
        for line in out.lines() {
            if line.is_empty() {
                continue;
            }
            let bytes = line.as_bytes();
            if bytes.len() < 3 {
                continue;
            }
            let x = bytes[0] as char;
            let y = bytes[1] as char;
            // second char is always a space in porcelain v1
            let rest = &line[3..];
            // Handle quoted paths (start with ") and renames "X Y orig -> dest".
            let path = parse_porcelain_path(rest);
            if path.is_empty() {
                continue;
            }
            let path = PathBuf::from(path);

            // Staged part (X)
            if x != ' ' && x != '?' {
                files.push(FileItem {
                    path: path.clone(),
                    mode: ChangeKind::from_letter(x),
                    section: Section::Staged,
                });
            }
            // Unstaged part (Y)
            if y != ' ' && y != '?' {
                files.push(FileItem {
                    path: path.clone(),
                    mode: ChangeKind::from_letter(y),
                    section: Section::Unstaged,
                });
            }
            // Untracked (?? )
            if x == '?' && y == '?' {
                files.push(FileItem {
                    path,
                    mode: ChangeKind::Added,
                    section: Section::Untracked,
                });
            }
        }

        // Order: staged first, then unstaged, then untracked; alphabetical within each.
        files.sort_by(|a, b| {
            let sa = (a.section.idx(), a.path.clone());
            let sb = (b.section.idx(), b.path.clone());
            sa.cmp(&sb)
        });
        Ok(files)
    }

    /// Stage a file (or all with `.`).
    pub fn stage(&self, path: &Path) -> Result<()> {
        let out = std::process::Command::new("git")
            .args([
                "-C", self.worktree.to_str().unwrap(), "add", "--",
                path.to_string_lossy().as_ref(),
            ])
            .output()?;
        if !out.status.success() {
            bail!("git add failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    pub fn stage_all(&self) -> Result<()> {
        let out = std::process::Command::new("git")
            .args(["-C", self.worktree.to_str().unwrap(), "add", "."])
            .output()?;
        if !out.status.success() {
            bail!("git add . failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Unstage a file (reset index entry to HEAD).
    pub fn unstage(&self, path: &Path) -> Result<()> {
        let out = std::process::Command::new("git")
            .args([
                "-C", self.worktree.to_str().unwrap(), "restore", "--staged", "--",
                path.to_string_lossy().as_ref(),
            ])
            .output()?;
        if !out.status.success() {
            bail!("git restore --staged failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    pub fn unstage_all(&self) -> Result<()> {
        let out = std::process::Command::new("git")
            .args(["-C", self.worktree.to_str().unwrap(), "restore", "--staged", "."])
            .output()?;
        if !out.status.success() {
            bail!("git restore --staged . failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Discard worktree changes for a file (checkout from index).
    pub fn discard(&self, path: &Path) -> Result<()> {
        let out = std::process::Command::new("git")
            .args([
                "-C", self.worktree.to_str().unwrap(), "checkout", "--",
                path.to_string_lossy().as_ref(),
            ])
            .output()?;
        if !out.status.success() {
            bail!("git checkout failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Remove an untracked file.
    pub fn remove_untracked(&self, path: &Path) -> Result<()> {
        let full = self.worktree.join(path);
        std::fs::remove_file(full).with_context(|| format!("failed to remove {}", path.display()))?;
        Ok(())
    }

    pub fn commit(&self, message: &str) -> Result<()> {
        let out = std::process::Command::new("git")
            .args(["-C", self.worktree.to_str().unwrap(), "commit", "-m", message])
            .output()?;
        if !out.status.success() {
            bail!("git commit failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Unified diff for a single file. staged = index vs HEAD, else worktree vs index.
    pub fn diff(&self, path: &Path, staged: bool) -> Result<String> {
        let mut args = vec!["-C", self.worktree.to_str().unwrap(), "diff", "--no-color"];
        if staged {
            args.push("--staged");
        }
        args.push("--");
        let rel = path.to_string_lossy();
        args.push(rel.as_ref());
        let out = std::process::Command::new("git").args(&args).output()?;
        if !out.status.success() {
            bail!("git diff failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn log(&self) -> Result<Vec<CommitInfo>> {
        let out = Self::run_git(
            &self.worktree,
            &["log", "-n", "200", "--format=format:%H%x09%s%x09%an"],
        )?;
        let mut entries = Vec::new();
        for line in out.lines() {
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let id = parts.next().unwrap_or("").to_string();
            let summary = parts.next().unwrap_or("").to_string();
            let author = parts.next().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let short_id = id[..7.min(id.len())].to_string();
            entries.push(CommitInfo { id, short_id, summary, author });
        }
        Ok(entries)
    }

    pub fn branches(&self) -> Result<Vec<String>> {
        let out = Self::run_git(
            &self.worktree,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )?;
        let mut out2: Vec<String> = out.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();
        out2.sort();
        Ok(out2)
    }

    pub fn checkout(&self, branch: &str) -> Result<()> {
        let out = std::process::Command::new("git")
            .args(["-C", self.worktree.to_str().unwrap(), "checkout", branch])
            .output()?;
        if !out.status.success() {
            bail!("git checkout failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Run `git` in the worktree and capture stdout as a string.
    fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
        let out = std::process::Command::new("git")
            .args(["-C", cwd.to_str().context("invalid cwd")?])
            .args(args)
            .output()
            .with_context(|| format!("failed to run git {:?}", args))?;
        if !out.status.success() {
            bail!(
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

/// Turn a porcelain v1 path field into a clean path string.
/// Handles quoted paths and rename "orig -> dest" (returns the destination/worktree path).
fn parse_porcelain_path(rest: &str) -> String {
    let rest = rest.trim();
    if rest.is_empty() {
        return String::new();
    }
    // Renames/copies: "orig -> dest" -> we care about dest (current worktree path).
    if let Some((_orig, dest)) = rest.split_once(" -> ") {
        return unquote_path(dest.trim());
    }
    unquote_path(rest)
}

/// Remove surrounding git quotes if present.
fn unquote_path(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') {
        // git quoted style: unescape common escapes.
        let inner = &s[1..s.len() - 1];
        return inner.replace("\\n", "\n").replace("\\t", "\t").replace("\\\"", "\"").replace("\\\\", "\\");
    }
    s.to_string()
}
