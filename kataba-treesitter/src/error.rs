//! Error types for the tree-sitter layer.

use std::ops::RangeInclusive;
use std::path::PathBuf;

use thiserror::Error;

/// Errors produced by grammar loading and parsing.
#[derive(Debug, Error)]
pub enum TreeError {
    /// The grammar shared library could not be opened.
    #[error("failed to load grammar library `{path}`: {source}")]
    DlOpen {
        /// Path of the library that failed to load.
        path: PathBuf,
        /// Underlying `dlopen`-style error.
        source: libloading::Error,
    },

    /// The library does not export the expected `tree_sitter_<name>` symbol.
    #[error("grammar library `{path}` does not export `{symbol}`: {source}")]
    DlSym {
        /// Path of the library.
        path: PathBuf,
        /// Symbol that was looked up.
        symbol: String,
        /// Underlying symbol error.
        source: libloading::Error,
    },

    /// The grammar's language function returned a null pointer.
    #[error("grammar `{name}` returned a null language pointer")]
    NullLanguage {
        /// Grammar name.
        name: String,
    },

    /// The grammar's ABI version is outside the window this build supports.
    #[error(
        "grammar `{name}` uses ABI {found}, but this build supports {supported:?}; \
         rebuild the grammar or update Kataba"
    )]
    IncompatibleAbi {
        /// Grammar name.
        name: String,
        /// ABI version reported by the grammar.
        found: usize,
        /// Window supported by the linked tree-sitter runtime.
        supported: RangeInclusive<usize>,
    },

    /// The parser rejected the language.
    #[error("parser rejected the language: {source}")]
    SetLanguage {
        /// Underlying tree-sitter error.
        source: tree_sitter::LanguageError,
    },

    /// tree-sitter returned no tree although parsing was not cancelled.
    ///
    /// This indicates an internal failure (for example, an allocation error)
    /// rather than a cancellation; callers should surface it as a bug.
    #[error("tree-sitter returned no tree without cancellation (internal failure)")]
    ParseFailed,
}
