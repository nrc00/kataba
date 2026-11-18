//! Rope-backed parsing with incremental updates and cooperative budgets.
//!
//! [`SyntaxParser`] is a thin wrapper around [`tree_sitter::Parser`] that:
//!
//! * reads document text straight out of a [`Rope`] in chunks — no copy of the
//!   document is made per parse;
//! * owns the incremental update ritual — callers hand in the old text, the
//!   old tree, the new text, and the change set that connects them, and never
//!   touch [`tree_sitter::InputEdit`]s directly;
//! * can cancel long parses cooperatively through tree-sitter's progress
//!   callback, so a pathological document can never pin a thread forever.

use std::cell::Cell;
use std::time::{Duration, Instant};

use kataba_core::{ChangeSet, Rope};
use tree_sitter::{Language, ParseOptions, ParseState, Parser, Point, Tree};

use crate::edits::generate_edits;
use crate::error::TreeError;

/// Result of a parse that ran under a cooperative time budget.
#[derive(Debug)]
pub enum ParseOutcome {
    /// The parse ran to completion; the tree reflects the parsed text.
    Parsed(Tree),
    /// The parse was cancelled after exceeding its budget. The previous tree
    /// should be retained; a later attempt with a larger budget may succeed.
    BudgetExceeded,
}

/// A tree-sitter parser bound to one language.
///
/// The parser holds no text and no tree state: callers keep the [`Tree`] and
/// feed it back in through [`SyntaxParser::parse_incremental`].
pub struct SyntaxParser {
    inner: Parser,
}

impl SyntaxParser {
    /// Creates a parser for `language`.
    pub fn new(language: &Language) -> Result<Self, TreeError> {
        let mut inner = Parser::new();
        inner
            .set_language(language)
            .map_err(|source| TreeError::SetLanguage { source })?;
        Ok(Self { inner })
    }

    /// Parses `text` from scratch.
    ///
    /// With no old tree and no budget there is no cancellation path, so this
    /// returns the [`Tree`] directly.
    pub fn parse(&mut self, text: &Rope) -> Result<Tree, TreeError> {
        self.run_unbudgeted(text, None)
    }

    /// Incrementally parses `new_text`.
    ///
    /// `old_tree` must be the tree produced from `old_text`, and `changeset`
    /// must transform `old_text` into `new_text`. The tree is cloned and
    /// edited internally, so callers never manage input edits by hand.
    ///
    /// With `budget` set, parsing is cancelled cooperatively once the wall
    /// clock exceeds it and [`ParseOutcome::BudgetExceeded`] is returned
    /// instead of a tree.
    pub fn parse_incremental(
        &mut self,
        old_text: &Rope,
        old_tree: &Tree,
        new_text: &Rope,
        changeset: &ChangeSet,
        budget: Option<Duration>,
    ) -> Result<ParseOutcome, TreeError> {
        let mut edited = old_tree.clone();
        // Edits carry coordinates in the old document, so they must be applied
        // last-first: a forward pass would let an early edit shift the
        // positions an early one describes (the same rule tree-house applies
        // for Helix).
        for edit in generate_edits(old_text, changeset).iter().rev() {
            edited.edit(edit);
        }
        let Some(budget) = budget else {
            return self
                .run_unbudgeted(new_text, Some(&edited))
                .map(ParseOutcome::Parsed);
        };
        self.run_budgeted(new_text, Some(&edited), budget)
    }

    /// Parses without a progress callback; the only failure mode is an
    /// internal tree-sitter failure.
    fn run_unbudgeted(&mut self, text: &Rope, old_tree: Option<&Tree>) -> Result<Tree, TreeError> {
        let mut reader = move |byte: usize, _position: Point| read_chunk(text, byte);
        self.inner
            .parse_with_options(&mut reader, old_tree, None)
            .ok_or(TreeError::ParseFailed)
    }

    /// Parses under a wall-clock budget, cancelling cooperatively through
    /// tree-sitter's progress callback.
    fn run_budgeted(
        &mut self,
        text: &Rope,
        old_tree: Option<&Tree>,
        budget: Duration,
    ) -> Result<ParseOutcome, TreeError> {
        let mut reader = move |byte: usize, _position: Point| read_chunk(text, byte);
        let deadline = Instant::now().checked_add(budget);
        let cancelled = Cell::new(false);
        let mut progress = |_state: &ParseState| match deadline {
            Some(deadline) if Instant::now() >= deadline => {
                cancelled.set(true);
                true
            }
            _ => false,
        };
        let options = ParseOptions::new().progress_callback(&mut progress);
        let parsed = self
            .inner
            .parse_with_options(&mut reader, old_tree, Some(options));
        match (parsed, cancelled.get()) {
            (Some(tree), _) => Ok(ParseOutcome::Parsed(tree)),
            (None, true) => Ok(ParseOutcome::BudgetExceeded),
            (None, false) => Err(TreeError::ParseFailed),
        }
    }
}

/// Fetches the chunk of `text` starting at `byte`, for tree-sitter's chunked
/// input callback.
///
/// Returns `""` at (or past) the end of the document, per the callback
/// contract. tree-sitter always requests char-boundary offsets; the defensive
/// walk below only guards against a callback contract violation.
fn read_chunk(text: &Rope, byte: usize) -> &str {
    if byte >= text.byte_len() {
        return "";
    }
    let (chunk, chunk_start, _char_start, _line_start) = text.chunk_at_byte(byte);
    let mut offset = byte - chunk_start;
    while offset > 0 && !chunk.is_char_boundary(offset) {
        offset -= 1;
    }
    &chunk[offset..]
}
