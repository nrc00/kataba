//! LSP Framing: Content-Length header codec for JSON-RPC 2.0 messages.
//!
//! Language Server Protocol framing uses HTTP-style header blocks:
//! ```text
//! Content-Length: <length in bytes>\r\n
//! \r\n
//! <JSON payload>
//! ```

use crate::error::{LspError, Result};
use bytes::{Buf, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";
const CONTENT_LENGTH_PREFIX: &str = "content-length:";

/// A Tokio codec that encodes and decodes LSP `Content-Length` framed frames.
#[derive(Debug, Default, Clone)]
pub struct LspCodec;

impl LspCodec {
    /// Creates a new `LspCodec`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for LspCodec {
    type Item = Bytes;
    type Error = LspError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        // Search for the end of the header block (\r\n\r\n)
        let header_end = match src
            .windows(4)
            .position(|window| window == HEADER_TERMINATOR)
        {
            Some(pos) => pos,
            None => return Ok(None),
        };

        // Extract and parse the header block as a string
        let header_bytes = &src[..header_end];
        let header_str = match std::str::from_utf8(header_bytes) {
            Ok(s) => s,
            Err(_) => {
                return Err(LspError::Framing(
                    "Header block contains invalid UTF-8".to_string(),
                ))
            }
        };

        // Parse Content-Length from headers
        let mut content_length: Option<usize> = None;
        for line in header_str.lines() {
            let trimmed = line.trim();
            if trimmed
                .to_ascii_lowercase()
                .starts_with(CONTENT_LENGTH_PREFIX)
            {
                let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let len_str = parts[1].trim();
                    match len_str.parse::<usize>() {
                        Ok(len) => {
                            content_length = Some(len);
                            break;
                        }
                        Err(_) => {
                            return Err(LspError::Framing(format!(
                                "Invalid Content-Length value: '{len_str}'"
                            )));
                        }
                    }
                }
            }
        }

        let content_length = match content_length {
            Some(len) => len,
            None => {
                return Err(LspError::Framing(
                    "Missing Content-Length header".to_string(),
                ))
            }
        };

        let total_frame_len = header_end + HEADER_TERMINATOR.len() + content_length;

        // Check if we have received the complete payload
        if src.len() < total_frame_len {
            src.reserve(total_frame_len - src.len());
            return Ok(None);
        }

        // Consume headers
        src.advance(header_end + HEADER_TERMINATOR.len());

        // Split off the payload bytes
        let payload = src.split_to(content_length).freeze();
        Ok(Some(payload))
    }
}

impl Encoder<String> for LspCodec {
    type Error = LspError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<()> {
        self.encode(item.as_bytes(), dst)
    }
}

impl Encoder<&String> for LspCodec {
    type Error = LspError;

    fn encode(&mut self, item: &String, dst: &mut BytesMut) -> Result<()> {
        self.encode(item.as_bytes(), dst)
    }
}

impl Encoder<&str> for LspCodec {
    type Error = LspError;

    fn encode(&mut self, item: &str, dst: &mut BytesMut) -> Result<()> {
        self.encode(item.as_bytes(), dst)
    }
}

impl Encoder<&[u8]> for LspCodec {
    type Error = LspError;

    fn encode(&mut self, item: &[u8], dst: &mut BytesMut) -> Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", item.len());
        dst.reserve(header.len() + item.len());
        dst.extend_from_slice(header.as_bytes());
        dst.extend_from_slice(item);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_single_message_encode_decode() {
        let mut codec = LspCodec::new();
        let mut buf = BytesMut::new();
        let payload = r#"{"jsonrpc":"2.0","method":"test","params":{}}"#;

        codec.encode(payload.to_string(), &mut buf).unwrap();
        let expected_header = format!("Content-Length: {}\r\n\r\n", payload.len());
        assert!(buf.starts_with(expected_header.as_bytes()));

        let decoded = codec.decode(&mut buf).unwrap().expect("expected frame");
        assert_eq!(decoded.as_ref(), payload.as_bytes());
        assert!(buf.is_empty());
    }

    #[test]
    fn test_multiple_messages_in_single_buffer() {
        let mut codec = LspCodec::new();
        let mut buf = BytesMut::new();
        let msg1 = r#"{"msg":1}"#;
        let msg2 = r#"{"msg":2}"#;

        codec.encode(msg1.to_string(), &mut buf).unwrap();
        codec.encode(msg2.to_string(), &mut buf).unwrap();

        let decoded1 = codec.decode(&mut buf).unwrap().expect("msg1");
        assert_eq!(decoded1.as_ref(), msg1.as_bytes());

        let decoded2 = codec.decode(&mut buf).unwrap().expect("msg2");
        assert_eq!(decoded2.as_ref(), msg2.as_bytes());

        assert!(buf.is_empty());
    }

    #[test]
    fn test_fragmented_delivery() {
        let mut codec = LspCodec::new();
        let mut buf = BytesMut::new();
        let payload = r#"{"jsonrpc":"2.0","result":"success"}"#;

        let mut full = BytesMut::new();
        codec.encode(payload.to_string(), &mut full).unwrap();

        // Feed byte-by-byte until complete
        let mut result = None;
        for byte in full {
            buf.extend_from_slice(&[byte]);
            if let Some(msg) = codec.decode(&mut buf).unwrap() {
                result = Some(msg);
                break;
            }
        }

        let decoded = result.expect("should decode complete message");
        assert_eq!(decoded.as_ref(), payload.as_bytes());
        assert!(buf.is_empty());
    }

    #[test]
    fn test_case_insensitive_header() {
        let mut codec = LspCodec::new();
        let mut buf = BytesMut::new();
        let raw = b"content-length: 7\r\n\r\nhello!!";
        buf.extend_from_slice(raw);

        let decoded = codec.decode(&mut buf).unwrap().expect("should decode");
        assert_eq!(decoded.as_ref(), b"hello!!");
    }

    #[test]
    fn test_missing_content_length_error() {
        let mut codec = LspCodec::new();
        let mut buf = BytesMut::new();
        buf.extend_from_slice(b"Content-Type: application/json\r\n\r\n{}");

        let err = codec.decode(&mut buf).unwrap_err();
        assert!(matches!(err, LspError::Framing(_)));
    }
}
