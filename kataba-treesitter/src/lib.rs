//! kataba-treesitter: tree-sitter bindings for the Kataba editor.
//!
//! This crate is the engine-facing half of Kataba's syntax stack. It owns:
//!
//! - **Edit translation** ([`edits`]): converts kataba-core's character-indexed
//!   [`kataba_core::ChangeSet`] into the byte-and-point
//!   [`tree_sitter::InputEdit`]s tree-sitter needs for incremental parsing.
//! - **Grammar loading** ([`load`]): dynamic loading of compiled grammars
//!   (`.so`/`.dylib`/`.dll`) with ABI gating. This module contains the crate's
//!   single deliberate `unsafe` island.
//! - **Parsing** ([`parser`]): a rope-backed [`SyntaxParser`] with full and
//!   incremental entry points and cooperative time budgets.
//!
//! Grammar acquisition (fetching, building, installing, updating, removing)
//! lives in the separate `kataba-store` module; this crate only loads
//! artifacts that already exist on disk.
//!
//! # Invariants
//!
//! * **Incremental equivalence**: applying a [`kataba_core::ChangeSet`] to a
//!   tree and reparsing yields a tree structurally identical to a full
//!   reparse — same node kinds, byte ranges, and positions.
//! * **ABI gating**: a grammar is never handed to a parser unless its ABI
//!   version falls inside the window reported by the linked tree-sitter
//!   runtime (never hardcoded).
//! * **Zero API-level `unsafe`**: all `unsafe` code is contained in [`load`].

#![deny(unsafe_code)]
#![deny(unused_mut)]
#![deny(unused_variables)]
#![deny(dead_code)]

pub mod edits;
pub mod error;
pub mod load;
pub mod parser;

pub use edits::generate_edits;
pub use error::TreeError;
pub use load::{is_abi_supported, load_language, probe_abi, supported_abi_range, AbiInfo, Grammar};
pub use parser::{ParseOutcome, SyntaxParser};
