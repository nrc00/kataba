//! Error types for the editor core.

/// Errors that can occur in the editor core.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Position is outside the valid range of the document.
    #[error("position {pos} is out of bounds (length {len})")]
    OutOfBounds {
        /// The invalid position.
        pos: usize,
        /// The document length.
        len: usize,
    },

    /// A selection is invalid (e.g., empty or out of bounds).
    #[error("invalid selection: {reason}")]
    InvalidSelection {
        /// Explanation of why the selection is invalid.
        reason: &'static str,
    },

    /// A change operation is invalid.
    #[error("invalid change: {reason}")]
    InvalidChange {
        /// Explanation of why the change is invalid.
        reason: &'static str,
    },

    /// A transaction does not match the expected state.
    #[error("transaction mismatch: {reason}")]
    TransactionMismatch {
        /// Explanation of the mismatch.
        reason: &'static str,
    },

    /// An error in the history/undo system.
    #[error("history error: {reason}")]
    HistoryError {
        /// Explanation of the history error.
        reason: &'static str,
    },

    /// Serialized history data is corrupt or fails integrity checks.
    #[error("corrupt history: {message}")]
    Corrupt {
        /// Explanation of the corruption.
        message: String,
    },
}

impl Error {
    /// Create an out-of-bounds error.
    pub fn out_of_bounds(pos: usize, len: usize) -> Self {
        Self::OutOfBounds { pos, len }
    }

    /// Create an invalid selection error.
    pub fn invalid_selection(reason: &'static str) -> Self {
        Self::InvalidSelection { reason }
    }

    /// Create an invalid change error.
    pub fn invalid_change(reason: &'static str) -> Self {
        Self::InvalidChange { reason }
    }

    /// Create a transaction mismatch error.
    pub fn transaction_mismatch(reason: &'static str) -> Self {
        Self::TransactionMismatch { reason }
    }

    /// Create a history error.
    pub fn history_error(reason: &'static str) -> Self {
        Self::HistoryError { reason }
    }

    /// Create a corruption error for serialized histories.
    pub fn corrupt(message: String) -> Self {
        Self::Corrupt { message }
    }
}

/// Result type alias for the editor core.
pub type Result<T> = std::result::Result<T, Error>;