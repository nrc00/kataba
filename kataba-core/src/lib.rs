//! kataba-core: A property-tested editor core library.
//!
//! This crate provides the fundamental building blocks for a text editor:
//! ropes, change sets, transactions, selections, ranges, and undo history.
//! It is designed to be correct, safe, and auditable.
//!
//! Undo history comes in two flavors: [`LinearHistory`] for normal linear
//! undo/redo (committing after undo discards the redo tail), and [`History`]
//! for Neovim-style branching undo with time travel, LRU budgets, and undofiles.

#![deny(unsafe_code)]
#![deny(unused_mut)]
#![deny(unused_variables)]
#![deny(dead_code)]

pub mod assoc;
pub mod change;
pub mod direction;
pub mod encoding;
pub mod error;
pub mod grapheme;
pub mod history;
pub mod identity;
pub mod indent;
pub mod linear_history;
pub mod motion;
pub mod point;
pub mod range;
pub mod rope;
pub mod selection;
pub mod transaction;

pub use assoc::{char_is_word, Assoc};
pub use change::{Change, ChangeIterator, ChangeSet, Deletion, Operation, Tendril};
pub use direction::Direction;
pub use encoding::PositionEncoding;
pub use error::Error;
pub use grapheme::Grapheme;
pub use history::{History, Revision, RevisionId, State, UndoKind, DEFAULT_UNDO_LIMIT_BYTES};
pub use identity::{BufferId, Version};
pub use indent::IndentStyle;
pub use linear_history::{LinearHistory, LinearRevision};
pub use point::Point;
pub use range::Range;
pub use rope::{Rope, RopeSlice};
pub use selection::Selection;
pub use transaction::Transaction;
