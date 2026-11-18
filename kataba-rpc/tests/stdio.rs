//! The stdout/stdin transport: a real child process speaking msgpack-RPC
//! over its standard streams.
//!
//! The child is the package's `kataba-rpc-daemon` binary (a byte-clean
//! stdout, unlike a test harness, whose own output would be indistinguishable
//! from protocol noise on the wire).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};
use std::time::Duration;

use kataba_rpc::{Connection, Value};

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn stdio_transport_talks_to_a_child_process() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kataba-rpc-daemon"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let connection = Connection::from_stream(stdout, stdin).unwrap();

    // Requests and notifications cross the pipes.
    let (notified_tx, notified_rx) = std::sync::mpsc::channel();
    connection.on_notification("anything", move |request| {
        let _ = notified_tx.send(request.params.first().and_then(Value::as_i64));
        Ok(())
    });

    let reply = connection
        .call_timeout(
            "echo",
            vec![Value::from("over the pipe"), Value::from(7)],
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(
        reply,
        Value::Array(vec![Value::from("over the pipe"), Value::from(7)])
    );

    // Pipelined calls work over pipes as well.
    let pendings: Vec<_> = (0..64)
        .map(|number| {
            connection
                .call_async("echo", vec![Value::from(number)])
                .unwrap()
        })
        .collect();
    for (number, pending) in pendings.into_iter().enumerate() {
        let reply = pending.wait_timeout(TIMEOUT).unwrap();
        assert_eq!(reply, Value::Array(vec![Value::from(number as i64)]));
    }

    let reply = connection.call_timeout("exit", vec![], TIMEOUT).unwrap();
    assert_eq!(reply, Value::from("bye"));

    // Closing our side drops the child's stdin, which the child observes as
    // a clean EOF, so it exits successfully.
    connection.close();
    connection.join();
    let status = child.wait().unwrap();
    assert!(status.success(), "child exited with {status}");

    // Optional notification channel stays quiet for this conversation.
    assert!(notified_rx.try_recv().is_err());
}
