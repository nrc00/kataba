//! Repository discovery with forced trust.
//!
//! Port of Helix's `helix-vcs/src/git.rs::open_repo`: gix's own discovery
//! re-derives trust from `.git` ownership and panics on `GIT_DIR`, so we
//! split discovery (ours, simple) from opening (gix, forced trust).

use gix::open::Permissions;
use gix::sec::trust::DefaultForLevel;
use gix::sec::Trust as GixTrust;
use gix::ThreadSafeRepository;

use crate::Result;
use super::Trust;


/// Open the discovered repository with a forced trust level.
pub fn open_repo(git_dir: &std::path::Path, trust: Trust) -> Result<ThreadSafeRepository> {
    let gix_trust = match trust {
        Trust::Reduced => GixTrust::Reduced,
        Trust::Full => GixTrust::Full,
    };

    // On Windows various configuration options are bundled as part of the git
    // installation; only look them up there.
    let config = gix::open::permissions::Config {
        system: true,
        git: true,
        user: true,
        env: true,
        includes: true,
        git_binary: cfg!(windows),
    };

    let permissions = Permissions {
        config,
        ..Permissions::default_for_level(gix_trust)
    };

    let options = gix::open::Options::default()
        .permissions(permissions)
        .open_path_as_is(true)
        .with(gix_trust);

    ThreadSafeRepository::open_opts(git_dir, options)
        .map_err(|e| crate::Error::GitCli(format!("open repository: {e}")))
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn open_requires_dot_git() {
        let tmp = tempfile::tempdir().unwrap();
        // No .git -> RepoNotFound
        let err = crate::git::discover(tmp.path()).unwrap_err();
        assert!(matches!(err, crate::Error::RepoNotFound { .. }));
    }

    #[test]
    fn open_finds_nested_repo() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("sub/.git")).unwrap();
        let (_git_dir, worktree) = crate::git::discover(&tmp.path().join("sub")).unwrap();
        assert_eq!(worktree, tmp.path().join("sub"));
    }
}
