//! Error types for the Kataba LSP client module.

use thiserror::Error;

/// The error type for all LSP client operations.
#[derive(Debug, Error)]
pub enum LspError {
    /// An underlying I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A JSON serialization or deserialization error occurred.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// An LSP wire framing or encoding error occurred.
    #[error("Framing error: {0}")]
    Framing(String),

    /// The server returned a JSON-RPC error response.
    #[error("Protocol error (code {code}): {message}")]
    Protocol {
        /// JSON-RPC error code.
        code: i64,
        /// Human-readable error message.
        message: String,
        /// Optional additional data payload.
        data: Option<serde_json::Value>,
    },

    /// The request was cancelled by the client or server.
    #[error("Request was cancelled")]
    Cancelled,

    /// A feature request was attempted for a capability the server does not support.
    #[error("Server capability not supported: {0}")]
    CapabilityNotSupported(&'static str),

    /// An invalid protocol state transition was attempted.
    #[error("Invalid state transition: expected {expected}, found {actual}")]
    InvalidState {
        /// Expected state description.
        expected: &'static str,
        /// Actual state description.
        actual: &'static str,
    },

    /// An internal actor or response channel was closed unexpectedly.
    #[error("Internal channel closed unexpectedly")]
    ChannelClosed,

    /// An operation timed out waiting for a server response.
    #[error("LSP request timed out")]
    Timeout,

    /// A document coordinate was out of bounds for the active buffer.
    #[error("Coordinate out of bounds: line {line}, column {col}")]
    OutOfBounds {
        /// Line index (0-based).
        line: u32,
        /// Column index.
        col: u32,
    },

    /// Document URI is not currently tracked by the client.
    #[error("Document not tracked: {0}")]
    DocumentNotTracked(String),
}

/// Convenience result alias for LSP operations.
pub type Result<T> = std::result::Result<T, LspError>;
