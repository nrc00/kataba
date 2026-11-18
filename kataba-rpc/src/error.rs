//! RPC error values and their wire encoding.
//!
//! Adapted from Neovim's `src/nvim/msgpack_rpc/channel.c` (v0.12.6):
//!
//! * `serialize_response` encodes a failed response as `[1, msgid, [code, message], nil]`
//!   — the error object is a two-element array of the integer error type and the
//!   message string.
//! * `rpc_send_call` decodes an error field by accepting either a bare string
//!   (transport-level failures) or a `[code, message]` array whose code is one
//!   of the two known error types; anything else becomes `"unknown error"`.
//! * `api/private/defs.h` defines the error type values.

use rmpv::Value;

/// Error type codes.
///
/// [`EXCEPTION`](codes::EXCEPTION) and [`VALIDATION`](codes::VALIDATION) are
/// the two values Neovim puts on the wire (`kErrorTypeException`,
/// `kErrorTypeValidation`). The negative codes are Kataba-local: they never
/// travel on the wire, because a peer that cannot answer is not there to
/// encode them.
pub mod codes {
    /// A general error (`kErrorTypeException`).
    pub const EXCEPTION: i64 = 0;
    /// Invalid arguments or values (`kErrorTypeValidation`).
    pub const VALIDATION: i64 = 1;
    /// Local only: the connection is closed (or closed while waiting).
    pub const CLOSED: i64 = -1;
    /// Local only: no reply arrived within the caller's timeout.
    pub const TIMEOUT: i64 = -2;
}

/// An RPC error: a code plus a human-readable message.
///
/// On the wire this is encoded as `[code, message]` (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RpcError {
    /// Error type code; see [`codes`].
    pub code: i64,
    /// Human-readable description.
    pub message: String,
}

impl RpcError {
    /// A general error (`kErrorTypeException`).
    pub fn exception(message: impl Into<String>) -> Self {
        Self {
            code: codes::EXCEPTION,
            message: message.into(),
        }
    }

    /// An invalid-arguments error (`kErrorTypeValidation`).
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            code: codes::VALIDATION,
            message: message.into(),
        }
    }

    /// A local connection-closed error. Never sent to a peer.
    pub fn closed(message: impl Into<String>) -> Self {
        Self {
            code: codes::CLOSED,
            message: message.into(),
        }
    }

    /// A local timeout error. Never sent to a peer.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: codes::TIMEOUT,
            message: message.into(),
        }
    }

    /// Whether this is a local connection-closed error.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.code == codes::CLOSED
    }

    /// Whether this is a local timeout error.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        self.code == codes::TIMEOUT
    }

    /// The wire form used inside a response: `[code, message]`.
    ///
    /// Mirrors `serialize_response` in `channel.c` (the error branch).
    pub(crate) fn to_value(&self) -> Value {
        Value::Array(vec![
            Value::from(self.code),
            Value::from(self.message.clone()),
        ])
    }

    /// Decodes a non-nil error field.
    ///
    /// Mirrors `rpc_send_call` in `channel.c`: a string is promoted to an
    /// exception, a `[0|1, string]` array keeps its code, and anything else
    /// degrades to the message `"unknown error"`.
    pub(crate) fn from_value(value: &Value) -> Self {
        match value {
            Value::String(string) => Self::exception(string.as_str().unwrap_or_default()),
            Value::Array(items) if items.len() == 2 => match (&items[0], &items[1]) {
                (Value::Integer(code), Value::String(message)) => match code.as_i64() {
                    Some(code @ (codes::EXCEPTION | codes::VALIDATION)) => Self {
                        code,
                        message: message.as_str().unwrap_or_default().to_string(),
                    },
                    _ => Self::exception("unknown error"),
                },
                _ => Self::exception("unknown error"),
            },
            _ => Self::exception("unknown error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{codes, RpcError};
    use rmpv::Value;

    #[test]
    fn wire_form_is_code_message_pair() {
        let error = RpcError::validation("bad argument");
        assert_eq!(
            error.to_value(),
            Value::Array(vec![Value::from(1), Value::from("bad argument")])
        );
    }

    #[test]
    fn known_codes_roundtrip() {
        for original in [
            RpcError::exception("boom"),
            RpcError {
                code: codes::VALIDATION,
                message: "nope".to_string(),
            },
        ] {
            let decoded = RpcError::from_value(&original.to_value());
            assert_eq!(decoded, original);
        }
    }

    #[test]
    fn string_error_is_promoted_to_exception() {
        // channel.c rpc_send_call accepts a bare string error, e.g. when the
        // channel was closed while a call was in flight.
        let decoded = RpcError::from_value(&Value::from("ch 1 was closed by the peer"));
        assert!(!decoded.is_closed());
        assert_eq!(decoded.code, codes::EXCEPTION);
        assert_eq!(decoded.message, "ch 1 was closed by the peer");
    }

    #[test]
    fn unknown_shapes_degrade_to_unknown_error() {
        for weird in [
            Value::from(42),
            Value::Array(vec![Value::from(7), Value::from("custom")]),
            Value::Array(vec![Value::from(0)]),
            Value::Map(vec![(Value::from("code"), Value::from(0))]),
        ] {
            let decoded = RpcError::from_value(&weird);
            assert_eq!(decoded.code, codes::EXCEPTION);
            assert_eq!(decoded.message, "unknown error");
        }
    }
}
