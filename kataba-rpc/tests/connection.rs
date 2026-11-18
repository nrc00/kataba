//! Integration tests over in-process Unix socket pairs.
//!
//! These exercise the semantics adapted from Neovim's `msgpack_rpc/channel.c`:
//! calls and call frames, notification dispatch order, close propagation,
//! protocol-violation closes, nested calls from handlers, and the Kataba
//! extensions (reply timeouts).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use kataba_rpc::{codes, CloseReason, Connection, Message, RpcError, Value, ERROR_EVENT};

const TIMEOUT: Duration = Duration::from_secs(5);

fn pair() -> (Connection, Connection) {
    let (client_side, server_side) = UnixStream::pair().unwrap();
    (
        Connection::from_unix_stream(client_side).unwrap(),
        Connection::from_unix_stream(server_side).unwrap(),
    )
}

fn close_both(a: &Connection, b: &Connection) {
    a.close();
    b.close();
    a.join();
    b.join();
}

#[test]
fn call_and_response_roundtrip() {
    let (a, b) = pair();
    b.on_request("echo", |request| Ok(Value::Array(request.params.clone())));
    let reply = a
        .call_timeout("echo", vec![Value::from("hi"), Value::from(41)], TIMEOUT)
        .unwrap();
    assert_eq!(
        reply,
        Value::Array(vec![Value::from("hi"), Value::from(41)])
    );
    close_both(&a, &b);
}

#[test]
fn notifications_dispatch_in_arrival_order() {
    let (a, b) = pair();
    let seen: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    b.on_notification("tick", move |request| {
        let number = request.params.first().and_then(Value::as_i64).unwrap_or(-1);
        sink.lock().unwrap().push(number);
        Ok(())
    });
    for number in 0..200 {
        a.notify("tick", vec![Value::from(number)]).unwrap();
    }
    // The last notification is an ordering barrier behind the first 200.
    let finished = Arc::new(Mutex::new(None));
    let marker = Arc::clone(&finished);
    b.on_notification("done", move |_| {
        *marker.lock().unwrap() = Some(());
        Ok(())
    });
    a.notify("done", vec![]).unwrap();
    let deadline = std::time::Instant::now() + TIMEOUT;
    while finished.lock().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "notifications never finished"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, (0..200).collect::<Vec<_>>());
    close_both(&a, &b);
}

#[test]
fn unknown_method_is_an_exception_error() {
    let (a, b) = pair();
    let error = a.call_timeout("nope", vec![], TIMEOUT).unwrap_err();
    assert_eq!(error.code, codes::EXCEPTION);
    // Message text matches the reference implementation exactly.
    assert!(error.message.contains("Invalid method: nope"), "{error}");
    close_both(&a, &b);
}

#[test]
fn handler_errors_propagate_with_their_code() {
    let (a, b) = pair();
    b.on_request("strict", |_| Err(RpcError::validation("bad argument")));
    let error = a.call_timeout("strict", vec![], TIMEOUT).unwrap_err();
    assert_eq!(error.code, codes::VALIDATION);
    assert_eq!(error.message, "bad argument");
    close_both(&a, &b);
}

#[test]
fn handlers_can_call_back_through_their_connection() {
    let (a, b) = pair();
    a.on_request("signal", |_| Ok(Value::from("ok")));
    b.on_request("outer", |request| {
        let inner = request.connection.call_timeout("signal", vec![], TIMEOUT)?;
        Ok(Value::Array(vec![Value::from("outer"), inner]))
    });
    let reply = a.call_timeout("outer", vec![], TIMEOUT).unwrap();
    assert_eq!(
        reply,
        Value::Array(vec![Value::from("outer"), Value::from("ok")])
    );
    close_both(&a, &b);
}

#[test]
fn pipelined_calls_from_many_threads_all_resolve() {
    let (a, b) = pair();
    b.on_request("square", |request| {
        let number = request.params.first().and_then(Value::as_i64).unwrap_or(0);
        Ok(Value::from(number * number))
    });
    let workers: Vec<_> = (0..8)
        .map(|worker| {
            let a = a.clone();
            thread::spawn(move || {
                for step in 0..25 {
                    let number = worker * 25 + step;
                    let reply = a
                        .call_timeout("square", vec![Value::from(number)], TIMEOUT)
                        .unwrap();
                    assert_eq!(reply, Value::from(number * number));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    close_both(&a, &b);
}

#[test]
fn reply_timeout_is_local_and_leaves_the_connection_usable() {
    let (a, b) = pair();
    b.on_request("slow", |_| {
        thread::sleep(Duration::from_millis(300));
        Ok(Value::from("late"))
    });
    b.on_request("echo", |_| Ok(Value::from("hi")));

    let error = a
        .call_timeout("slow", vec![], Duration::from_millis(20))
        .unwrap_err();
    assert!(error.is_timeout(), "{error}");

    // The connection stays usable: the late reply for `slow` is orphaned and
    // dropped, not treated as an unknown-request-id protocol violation.
    let reply = a.call_timeout("echo", vec![], TIMEOUT).unwrap();
    assert_eq!(reply, Value::from("hi"));
    thread::sleep(Duration::from_millis(400));
    assert!(!a.is_closed());
    let reply = a.call_timeout("echo", vec![], TIMEOUT).unwrap();
    assert_eq!(reply, Value::from("hi"));
    close_both(&a, &b);
}

#[test]
fn large_payloads_roundtrip_intact() {
    let (a, b) = pair();
    b.on_request("store", |request| {
        let length = match request.params.first() {
            Some(Value::Binary(bytes)) => bytes.len(),
            _ => 0,
        };
        Ok(Value::from(length as i64))
    });
    let payload = vec![0xAB_u8; 1024 * 1024];
    let reply = a
        .call_timeout("store", vec![Value::from(payload.clone())], TIMEOUT)
        .unwrap();
    assert_eq!(reply, Value::from(1024_i64 * 1024));

    b.on_request("fetch", move |_| {
        Ok(Value::from(vec![0xCD_u8; 1024 * 1024]))
    });
    let reply = a.call_timeout("fetch", vec![], TIMEOUT).unwrap();
    match reply {
        Value::Binary(bytes) => assert_eq!(bytes.len(), 1024 * 1024),
        other => panic!("expected binary, got {other:?}"),
    }
    close_both(&a, &b);
}

#[test]
fn raw_writes_bypass_encoding() {
    let (a, b) = pair();
    let (seen_tx, seen_rx) = mpsc::channel();
    b.on_notification("raw", move |request| {
        let _ = seen_tx.send(request.params.first().and_then(Value::as_i64));
        Ok(())
    });
    let bytes = Message::Notification {
        method: "raw".to_string(),
        params: vec![Value::from(9)],
    }
    .to_bytes()
    .unwrap();
    a.write_raw(bytes).unwrap();
    assert_eq!(seen_rx.recv_timeout(TIMEOUT).unwrap(), Some(9));
    close_both(&a, &b);
}

#[test]
fn notification_handler_failure_sends_the_error_event() {
    let (a, b) = pair();
    b.on_notification("boom", |_| Err(RpcError::validation("bad input")));
    let (event_tx, event_rx) = mpsc::channel();
    a.on_notification(ERROR_EVENT, move |request| {
        let code = request.params.first().and_then(Value::as_i64);
        let message = request
            .params
            .get(1)
            .and_then(Value::as_str)
            .map(str::to_string);
        let _ = event_tx.send((code, message));
        Ok(())
    });
    a.notify("boom", vec![]).unwrap();
    let (code, message) = event_rx.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(code, Some(codes::VALIDATION));
    assert_eq!(message.as_deref(), Some("bad input"));
    close_both(&a, &b);
}

#[test]
fn peer_hangup_fails_pending_calls_and_reports_the_reason() {
    let (a, b) = pair();
    let (close_tx, close_rx) = mpsc::channel();
    a.on_close(move |reason| {
        let _ = close_tx.send(reason);
    });
    let (started_tx, started_rx) = mpsc::channel();
    b.on_request("slow", move |_| {
        let _ = started_tx.send(());
        thread::sleep(Duration::from_millis(200));
        Ok(Value::Nil)
    });
    let pending = a.call_async("slow", vec![]).unwrap();
    // Wait until the peer has consumed the request, so the hangup below is
    // unambiguously a read-side EOF rather than a write-side race.
    started_rx.recv_timeout(TIMEOUT).unwrap();
    b.close();
    let error = pending.wait().unwrap_err();
    assert!(error.is_closed(), "{error}");
    assert!(a.is_closed());
    let reason = close_rx.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(reason, CloseReason::PeerClosed);
    // Further calls fail immediately.
    assert!(a
        .call_timeout("anything", vec![], TIMEOUT)
        .unwrap_err()
        .is_closed());
    a.join();
    b.join();
}

#[test]
fn response_to_unknown_request_id_is_a_protocol_violation() {
    let (our_side, peer_side) = UnixStream::pair().unwrap();
    let ours = Connection::from_unix_stream(our_side).unwrap();
    let (close_tx, close_rx) = mpsc::channel();
    ours.on_close(move |reason| {
        let _ = close_tx.send(reason);
    });
    let peer = thread::spawn(move || {
        let mut reader = BufReader::new(peer_side.try_clone().unwrap());
        let mut writer = peer_side;
        let request = Message::decode_next(&mut reader).unwrap().unwrap();
        assert!(matches!(request, Message::Request { .. }));
        // Answer with the wrong request id.
        let bogus = Message::Response {
            msgid: 999_999,
            error: None,
            result: Value::from("desync"),
        };
        writer.write_all(&bogus.to_bytes().unwrap()).unwrap();
        writer.flush().unwrap();
    });
    let error = ours.call_timeout("anything", vec![], TIMEOUT).unwrap_err();
    assert!(error.is_closed(), "{error}");
    let reason = close_rx.recv_timeout(TIMEOUT).unwrap();
    assert!(
        matches!(reason, CloseReason::ProtocolViolation(ref message) if message.contains("unknown request id")),
        "{reason}"
    );
    peer.join().unwrap();
    ours.join();
}

#[test]
fn garbage_bytes_close_the_connection() {
    let (our_side, mut peer_side) = UnixStream::pair().unwrap();
    let ours = Connection::from_unix_stream(our_side).unwrap();
    let (close_tx, close_rx) = mpsc::channel();
    ours.on_close(move |reason| {
        let _ = close_tx.send(reason);
    });
    // 0xC1 is a reserved, never-used MessagePack marker byte.
    peer_side.write_all(&[0xC1]).unwrap();
    peer_side.flush().unwrap();
    let reason = close_rx.recv_timeout(TIMEOUT).unwrap();
    assert!(
        matches!(reason, CloseReason::ProtocolViolation(_)),
        "{reason}"
    );
    ours.join();
}

#[test]
fn close_handlers_run_once() {
    let (a, b) = pair();
    let calls: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let counter = Arc::clone(&calls);
    a.on_close(move |_| {
        *counter.lock().unwrap() += 1;
    });
    a.close();
    a.close();
    b.close();
    a.join();
    b.join();
    assert_eq!(*calls.lock().unwrap(), 1);
}
