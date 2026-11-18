//! Message shapes and codec.
//!
//! Adapted from Neovim's `src/nvim/msgpack_rpc/channel.c` (v0.12.6) and the
//! MessagePack-RPC specification:
//!
//! * Requests are `[0, msgid, method, params]` (`serialize_request` packs 4
//!   elements when a request id exists).
//! * Notifications are `[2, method, params]` (3 elements, no msgid).
//! * Responses are `[1, msgid, error, result]`; `error` is `nil` on success and
//!   `result` is `nil` on failure (`serialize_response`).
//! * Request/notification `params` must be an array; Neovim closes the channel
//!   otherwise (`parse_msgpack`).
//! * MessagePack is self-delimiting, so messages have no additional framing.

use std::io::{self, Read};

use rmpv::Value;
use thiserror::Error;

use crate::error::RpcError;

/// Message type tags from the MessagePack-RPC specification
/// (`kMessageType*` in Neovim).
pub mod message_type {
    /// Request tag: `[0, msgid, method, params]`.
    pub const REQUEST: i64 = 0;
    /// Response tag: `[1, msgid, error, result]`.
    pub const RESPONSE: i64 = 1;
    /// Notification tag: `[2, method, params]`.
    pub const NOTIFICATION: i64 = 2;
}

/// Maximum nesting depth accepted for a single message.
///
/// rmpv's default is 1024; 128 is far more than any editor message needs and
/// keeps hostile/degenerate input cheap to reject.
pub const MAX_DEPTH: usize = 128;

/// One msgpack-RPC message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A call that expects a [`Message::Response`] with the same `msgid`.
    Request {
        /// Request id, unique per connection, starting at 1.
        msgid: u32,
        /// Method name.
        method: String,
        /// Positional arguments (possibly empty).
        params: Vec<Value>,
    },
    /// The reply to a [`Message::Request`].
    Response {
        /// Id of the request being answered.
        msgid: u32,
        /// `Some` when the call failed; the wire form is `[code, message]`.
        error: Option<RpcError>,
        /// Return value; nil when `error` is set.
        result: Value,
    },
    /// A call that must not be answered.
    Notification {
        /// Method name.
        method: String,
        /// Positional arguments (possibly empty).
        params: Vec<Value>,
    },
}

/// Failure to encode a message.
///
/// Writing to an in-memory buffer cannot actually fail; the variant exists so
/// custom transports stay possible without a panic path.
#[derive(Debug, Error)]
#[error("failed to encode message: {0}")]
pub struct EncodeError(#[from] rmpv::encode::Error);

/// Failure to decode a message.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// The bytes are not a valid msgpack-RPC message.
    #[error("malformed message: {0}")]
    Malformed(String),
    /// The underlying reader failed (including truncation mid-message).
    #[error("i/o error while reading message: {0}")]
    Io(io::Error),
}

impl Message {
    /// Encodes the message into `out`.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), EncodeError> {
        rmpv::encode::write_value(out, &self.to_value())?;
        Ok(())
    }

    /// Encodes the message into a fresh buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut out = Vec::new();
        self.encode_into(&mut out)?;
        Ok(out)
    }

    /// The `Value` form of this message.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Request {
                msgid,
                method,
                params,
            } => Value::Array(vec![
                Value::from(message_type::REQUEST),
                Value::from(*msgid),
                Value::from(method.clone()),
                Value::Array(params.clone()),
            ]),
            Self::Response {
                msgid,
                error,
                result,
            } => {
                let (error, result) = match error {
                    Some(error) => (error.to_value(), Value::Nil),
                    None => (Value::Nil, result.clone()),
                };
                Value::Array(vec![
                    Value::from(message_type::RESPONSE),
                    Value::from(*msgid),
                    error,
                    result,
                ])
            }
            Self::Notification { method, params } => Value::Array(vec![
                Value::from(message_type::NOTIFICATION),
                Value::from(method.clone()),
                Value::Array(params.clone()),
            ]),
        }
    }

    /// Reads the next message from `reader`.
    ///
    /// `Ok(None)` means the peer closed the connection cleanly at a message
    /// boundary (EOF before any byte of the next message). Anything else that
    /// goes wrong — truncation mid-message, invalid msgpack, a wrong shape —
    /// is fatal for the connection, mirroring Neovim's unpacker.
    pub fn decode_next<R: Read>(reader: &mut R) -> Result<Option<Self>, DecodeError> {
        let Some(first) = read_first_byte(reader)? else {
            return Ok(None);
        };
        let mut chained = io::Cursor::new([first]).chain(reader);
        let value = rmpv::decode::read_value_with_max_depth(&mut chained, MAX_DEPTH)
            .map_err(map_decode_error)?;
        Self::from_value(value).map(Some)
    }

    /// Decodes one message value, enforcing the shapes Neovim accepts.
    fn from_value(value: Value) -> Result<Self, DecodeError> {
        let items = match value {
            Value::Array(items) => items,
            other => {
                return Err(DecodeError::Malformed(format!(
                    "expected a message array, got {}",
                    type_name(&other)
                )));
            }
        };
        let tag = match items.first() {
            Some(Value::Integer(tag)) => tag.as_i64(),
            Some(other) => {
                return Err(DecodeError::Malformed(format!(
                    "expected an integer message type, got {}",
                    type_name(other)
                )));
            }
            None => return Err(DecodeError::Malformed("empty message array".to_string())),
        };
        match tag {
            Some(message_type::REQUEST) => {
                let [_, msgid, method, params] = array4(items)?;
                Ok(Self::Request {
                    msgid: msgid_u32(&msgid)?,
                    method: string(&method, "method")?,
                    params: params_array(&params)?,
                })
            }
            Some(message_type::RESPONSE) => {
                let [_, msgid, error, result] = array4(items)?;
                let error = match error {
                    Value::Nil => None,
                    other => Some(RpcError::from_value(&other)),
                };
                Ok(Self::Response {
                    msgid: msgid_u32(&msgid)?,
                    error,
                    result,
                })
            }
            Some(message_type::NOTIFICATION) => {
                let [_, method, params] = array3(items)?;
                Ok(Self::Notification {
                    method: string(&method, "method")?,
                    params: params_array(&params)?,
                })
            }
            Some(other) => Err(DecodeError::Malformed(format!(
                "unknown message type {other}"
            ))),
            None => Err(DecodeError::Malformed(
                "message type out of range".to_string(),
            )),
        }
    }
}

fn read_first_byte<R: Read>(reader: &mut R) -> Result<Option<u8>, DecodeError> {
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(_) => return Ok(Some(byte[0])),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(DecodeError::Io(error)),
        }
    }
}

fn map_decode_error(error: rmpv::decode::Error) -> DecodeError {
    match error {
        rmpv::decode::Error::InvalidMarkerRead(io) | rmpv::decode::Error::InvalidDataRead(io) => {
            DecodeError::Io(io)
        }
        rmpv::decode::Error::DepthLimitExceeded => {
            DecodeError::Malformed(format!("nesting deeper than {MAX_DEPTH} levels"))
        }
    }
}

fn array4(items: Vec<Value>) -> Result<[Value; 4], DecodeError> {
    let len = items.len();
    <[Value; 4]>::try_from(items).map_err(|_| {
        DecodeError::Malformed(format!("expected a 4-element message array, got {len}"))
    })
}

fn array3(items: Vec<Value>) -> Result<[Value; 3], DecodeError> {
    let len = items.len();
    <[Value; 3]>::try_from(items).map_err(|_| {
        DecodeError::Malformed(format!("expected a 3-element message array, got {len}"))
    })
}

fn msgid_u32(value: &Value) -> Result<u32, DecodeError> {
    match value {
        Value::Integer(integer) => u32::try_from(integer.as_u64().unwrap_or(u64::MAX))
            .map_err(|_| DecodeError::Malformed(format!("msgid {integer} is not a u32"))),
        other => Err(DecodeError::Malformed(format!(
            "expected an integer msgid, got {}",
            type_name(other)
        ))),
    }
}

fn string(value: &Value, what: &str) -> Result<String, DecodeError> {
    match value {
        Value::String(string) => Ok(string.as_str().unwrap_or_default().to_string()),
        other => Err(DecodeError::Malformed(format!(
            "expected a string {what}, got {}",
            type_name(other)
        ))),
    }
}

fn params_array(value: &Value) -> Result<Vec<Value>, DecodeError> {
    match value {
        Value::Array(params) => Ok(params.clone()),
        other => Err(DecodeError::Malformed(format!(
            "params must be an array, got {}",
            type_name(other)
        ))),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Boolean(_) => "a boolean",
        Value::Integer(_) => "an integer",
        Value::F32(_) | Value::F64(_) => "a float",
        Value::String(_) => "a string",
        Value::Binary(_) => "binary data",
        Value::Array(_) => "an array",
        Value::Map(_) => "a map",
        Value::Ext(_, _) => "an extension",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{message_type, Message};
    use crate::error::RpcError;
    use rmpv::Value;

    fn decode_value(value: Value) -> Result<Option<Message>, super::DecodeError> {
        let mut bytes = Vec::new();
        rmpv::encode::write_value(&mut bytes, &value).unwrap();
        let mut cursor = bytes.as_slice();
        Message::decode_next(&mut cursor)
    }

    #[test]
    fn request_roundtrip() {
        let message = Message::Request {
            msgid: 1,
            method: "echo".to_string(),
            params: vec![Value::from("hi")],
        };
        let bytes = message.to_bytes().unwrap();
        let mut cursor = bytes.as_slice();
        assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
    }

    #[test]
    fn response_roundtrip_ok_and_error() {
        let ok = Message::Response {
            msgid: 7,
            error: None,
            result: Value::from(3),
        };
        let err = Message::Response {
            msgid: 8,
            error: Some(RpcError::validation("nope")),
            result: Value::Nil,
        };
        for message in [ok, err] {
            let bytes = message.to_bytes().unwrap();
            let mut cursor = bytes.as_slice();
            assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
        }
    }

    #[test]
    fn notification_roundtrip() {
        let message = Message::Notification {
            method: "log".to_string(),
            params: vec![],
        };
        let bytes = message.to_bytes().unwrap();
        let mut cursor = bytes.as_slice();
        assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
    }

    #[test]
    fn clean_eof_is_none() {
        let mut empty: &[u8] = &[];
        assert!(matches!(Message::decode_next(&mut empty), Ok(None)));
    }

    #[test]
    fn truncated_message_is_an_error() {
        let message = Message::Request {
            msgid: 1,
            method: "echo".to_string(),
            params: vec![],
        };
        let bytes = message.to_bytes().unwrap();
        for cut in 1..bytes.len() {
            let mut cursor = &bytes[..cut];
            assert!(Message::decode_next(&mut cursor).is_err(), "cut {cut}");
        }
    }

    #[test]
    fn wrong_shapes_are_rejected() {
        // Not an array.
        assert!(decode_value(Value::from(1)).is_err());
        // Empty array.
        assert!(decode_value(Value::Array(vec![])).is_err());
        // Unknown type tag.
        assert!(decode_value(Value::Array(vec![
            Value::from(9),
            Value::from(1),
            Value::from("m"),
            Value::Array(vec![]),
        ]))
        .is_err());
        // Wrong arity for a request.
        assert!(decode_value(Value::Array(vec![
            Value::from(message_type::REQUEST),
            Value::from(1),
        ]))
        .is_err());
        // msgid out of u32 range.
        assert!(decode_value(Value::Array(vec![
            Value::from(message_type::REQUEST),
            Value::from(u64::from(u32::MAX) + 1),
            Value::from("m"),
            Value::Array(vec![]),
        ]))
        .is_err());
        // Negative msgid.
        assert!(decode_value(Value::Array(vec![
            Value::from(message_type::REQUEST),
            Value::from(-1),
            Value::from("m"),
            Value::Array(vec![]),
        ]))
        .is_err());
        // Params must be an array (Neovim closes the channel otherwise).
        assert!(decode_value(Value::Array(vec![
            Value::from(message_type::REQUEST),
            Value::from(1),
            Value::from("m"),
            Value::from("not an array"),
        ]))
        .is_err());
        // Method must be a string.
        assert!(decode_value(Value::Array(vec![
            Value::from(message_type::NOTIFICATION),
            Value::from(3),
            Value::Array(vec![]),
        ]))
        .is_err());
    }

    #[test]
    fn error_shapes_follow_rpc_send_call() {
        // A bare string error is promoted to an exception.
        let decoded = decode_value(Value::Array(vec![
            Value::from(message_type::RESPONSE),
            Value::from(1),
            Value::from("channel closed"),
            Value::Nil,
        ]))
        .unwrap()
        .unwrap();
        match decoded {
            Message::Response { error, .. } => {
                let error = error.unwrap();
                assert_eq!(error.code, 0);
                assert_eq!(error.message, "channel closed");
            }
            other => panic!("expected response, got {other:?}"),
        }
        // A non-[code, message] error degrades to "unknown error".
        let decoded = decode_value(Value::Array(vec![
            Value::from(message_type::RESPONSE),
            Value::from(1),
            Value::Array(vec![Value::from(7), Value::from("custom")]),
            Value::Nil,
        ]))
        .unwrap()
        .unwrap();
        match decoded {
            Message::Response { error, .. } => {
                assert_eq!(error.unwrap().message, "unknown error");
            }
            other => panic!("expected response, got {other:?}"),
        }
    }
}
