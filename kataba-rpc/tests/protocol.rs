//! Byte-exact conformance vectors for the MessagePack-RPC wire shapes.
//!
//! The expected byte sequences are the canonical MessagePack encodings of the
//! three message forms from the MessagePack-RPC specification, which Neovim's
//! `serialize_request`/`serialize_response` produce as well.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kataba_rpc::{message_type, Message, RpcError, Value};

#[test]
fn request_vector_is_byte_exact() {
    let message = Message::Request {
        msgid: 1,
        method: "echo".to_string(),
        params: vec![Value::from("hi")],
    };
    #[rustfmt::skip]
    let expected = [
        0x94,             // array(4)
        0x00,             // 0: request
        0x01,             // msgid 1
        0xa4, b'e', b'c', b'h', b'o', // "echo"
        0x91, 0xa2, b'h', b'i',       // ["hi"]
    ];
    assert_eq!(message.to_bytes().unwrap(), expected);
    let mut cursor = expected.as_slice();
    assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
}

#[test]
fn notification_vector_is_byte_exact() {
    let message = Message::Notification {
        method: "ping".to_string(),
        params: vec![],
    };
    #[rustfmt::skip]
    let expected = [
        0x93,                          // array(3)
        0x02,                          // 2: notification
        0xa4, b'p', b'i', b'n', b'g',  // "ping"
        0x90,                          // []
    ];
    assert_eq!(message.to_bytes().unwrap(), expected);
    let mut cursor = expected.as_slice();
    assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
}

#[test]
fn successful_response_vector_is_byte_exact() {
    let message = Message::Response {
        msgid: 1,
        error: None,
        result: Value::from("ok"),
    };
    #[rustfmt::skip]
    let expected = [
        0x94,              // array(4)
        0x01,              // 1: response
        0x01,              // msgid 1
        0xc0,              // nil (no error)
        0xa2, b'o', b'k',  // "ok"
    ];
    assert_eq!(message.to_bytes().unwrap(), expected);
    let mut cursor = expected.as_slice();
    assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
}

#[test]
fn error_response_vector_is_byte_exact() {
    // Neovim encodes a failed response as [1, msgid, [code, message], nil].
    let message = Message::Response {
        msgid: 1,
        error: Some(RpcError::exception("boom")),
        result: Value::Nil,
    };
    #[rustfmt::skip]
    let expected = [
        0x94,                              // array(4)
        0x01,                              // 1: response
        0x01,                              // msgid 1
        0x92, 0x00, 0xa4, b'b', b'o', b'o', b'm', // [0, "boom"]
        0xc0,                              // nil result
    ];
    assert_eq!(message.to_bytes().unwrap(), expected);
    let mut cursor = expected.as_slice();
    assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
}

#[test]
fn consecutive_messages_decode_from_one_stream() {
    let messages = [
        Message::Request {
            msgid: 1,
            method: "a".to_string(),
            params: vec![Value::from(1)],
        },
        Message::Notification {
            method: "b".to_string(),
            params: vec![],
        },
        Message::Response {
            msgid: 1,
            error: None,
            result: Value::Nil,
        },
    ];
    let mut stream = Vec::new();
    for message in &messages {
        message.encode_into(&mut stream).unwrap();
    }
    let mut cursor = stream.as_slice();
    for message in &messages {
        assert_eq!(
            Message::decode_next(&mut cursor).unwrap().as_ref(),
            Some(message)
        );
    }
    assert_eq!(Message::decode_next(&mut cursor).unwrap(), None);
}

#[test]
fn msgid_extremes_roundtrip() {
    for msgid in [0, 1, u32::MAX] {
        let message = Message::Request {
            msgid,
            method: "m".to_string(),
            params: vec![],
        };
        let bytes = message.to_bytes().unwrap();
        let mut cursor = bytes.as_slice();
        assert_eq!(Message::decode_next(&mut cursor).unwrap(), Some(message));
    }
}

#[test]
fn message_type_constants_match_the_spec() {
    assert_eq!(message_type::REQUEST, 0);
    assert_eq!(message_type::RESPONSE, 1);
    assert_eq!(message_type::NOTIFICATION, 2);
}
