//! Typed errors. Never `unwrap`/`panic` in library code; every failure path
//! is a value the caller must handle.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("repository not found at {path:?}")]
    RepoNotFound { path: PathBuf },

    #[error("trust required for mutation on untrusted workspace")]
    TrustRequired,

    #[error("index lock held; concurrent operation in progress")]
    IndexLocked,

    #[error("feature not supported by this backend: {0}")]
    NotSupported(&'static str),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation timed out")]
    Timeout,

    #[error("git CLI failed: {0}")]
    GitCli(String),

    #[error("jj CLI failed: {0}")]
    JjCli(String),

    #[error("gix error: {0}")]
    Gix(#[from] gix::Error),
}

// gix sub-error types do not implement Into<gix::Error>, so we convert them
// explicitly so the `?` operator works in the git driver.
impl From<gix::status::Error> for Error {
    fn from(e: gix::status::Error) -> Self {
        Error::GitCli(e.to_string())
    }
}
impl From<gix::status::into_iter::Error> for Error {
    fn from(e: gix::status::into_iter::Error) -> Self {
        Error::GitCli(e.to_string())
    }
}
impl From<bstr::Utf8Error> for Error {
    fn from(e: bstr::Utf8Error) -> Self {
        Error::GitCli(e.to_string())
    }
}
impl From<gix::reference::find::existing::Error> for Error {
    fn from(e: gix::reference::find::existing::Error) -> Self {
        Error::GitCli(e.to_string())
    }
}
impl From<gix::reference::head_commit::Error> for Error {
    fn from(e: gix::reference::head_commit::Error) -> Self {
        Error::GitCli(e.to_string())
    }
}
