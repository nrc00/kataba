//! Parsers for machine-readable VCS output.
//!
//! All parsers are pure functions `&[u8] -> Result<Model>` — proptestable and
//! fuzzable in isolation, no process or filesystem access.

// Re-export sub-modules as they are added.
pub mod porcelain;
