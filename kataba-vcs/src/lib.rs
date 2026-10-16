//! `kataba-vcs`: an independent, capability-gated version-control module.
//!
//! Zero Kataba dependencies. No UI, no keymaps, no global state.
//! Consumers: `kataba` daemon (`kataba.vcs.*` RPC), a standalone CLI bin,
//! tests, agents, and any future frontend.
//!
//! Three layers:
//!   1. Pure domain (`model`, `parse`, `patch`, `diff`) — sync, no I/O,
//!      `#![forbid(unsafe_code)]`, proptestable.
//!   2. Drivers (`git`, `jj`) — process management + gix reads.
//!   3. Optional actor shell (`actor`, `watcher`, `cache`) — tokio layer.
//!
//! See `KATABA_VCS.md` for axioms V1–V6 and the full roadmap.

#![forbid(unsafe_code)]
#![deny(unused_mut)]
#![deny(unused_variables)]
#![deny(dead_code)]

pub mod capabilities;
pub mod error;
pub mod exec;
pub mod model;
pub mod parse;

#[cfg(feature = "git-backend")]
pub mod git;

pub use capabilities::Capabilities;
pub use error::Error;
pub use model::*;

pub type Result<T> = std::result::Result<T, Error>;
