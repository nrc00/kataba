//! Interop tests against a real Neovim instance.
//!
//! Neovim is the reference implementation this crate was adapted from, so the
//! strongest conformance check is to speak to it directly: `nvim --headless
//! --listen <socket>` exposes msgpack-RPC on that socket. Each test is
//! skipped with a note when `nvim` is not installed or cannot start.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use kataba_rpc::{codes, Connection, Value};

const TIMEOUT: Duration = Duration::from_secs(10);

struct NvimProcess {
    child: Child,
    socket: PathBuf,
}

impl Drop for NvimProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Starts a headless Neovim and connects to its RPC socket.
///
/// Returns `None` (after printing a note) when nvim is unavailable.
fn connect_nvim(test: &str) -> Option<(NvimProcess, Connection)> {
    let socket = std::env::temp_dir().join(format!(
        "kataba-rpc-nvim-{}-{test}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket);
    let child = Command::new("nvim")
        .args(["--headless", "--clean", "--listen"])
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let process = NvimProcess {
        child,
        socket: socket.clone(),
    };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match Connection::connect_unix(&socket) {
            Ok(connection) => return Some((process, connection)),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    }
}

macro_rules! nvim_or_skip {
    ($test:expr) => {
        match connect_nvim($test) {
            Some(pair) => pair,
            None => {
                eprintln!("skipping `{}`: nvim --headless is unavailable", $test);
                return;
            }
        }
    };
}

#[test]
fn api_info_request_roundtrips_against_nvim() {
    let (_nvim, connection) = nvim_or_skip!("api-info");
    let reply = connection
        .call_timeout("nvim_get_api_info", vec![], TIMEOUT)
        .unwrap();
    let items = match reply {
        Value::Array(items) => items,
        other => panic!("expected array, got {other:?}"),
    };
    assert_eq!(
        items.len(),
        2,
        "nvim_get_api_info returns [channel_id, metadata]"
    );
    assert!(matches!(items[0], Value::Integer(_)), "{:?}", items[0]);
    assert!(matches!(items[1], Value::Map(_)), "{:?}", items[1]);
    connection.close();
    connection.join();
}

#[test]
fn eval_result_roundtrips_against_nvim() {
    let (_nvim, connection) = nvim_or_skip!("eval");
    let reply = connection
        .call_timeout("nvim_eval", vec![Value::from("40 + 2")], TIMEOUT)
        .unwrap();
    assert_eq!(reply, Value::from(42));
    connection.close();
    connection.join();
}

#[test]
fn unknown_method_error_matches_reference_semantics() {
    let (_nvim, connection) = nvim_or_skip!("unknown-method");
    let error = connection
        .call_timeout("nvim_definitely_not_a_method", vec![], TIMEOUT)
        .unwrap_err();
    // Neovim reports unknown methods as exception-typed errors (kErrorTypeException).
    assert_eq!(error.code, codes::EXCEPTION, "{error}");
    assert!(
        error.message.contains("nvim_definitely_not_a_method"),
        "reference message should name the method: {error}"
    );
    connection.close();
    connection.join();
}

#[test]
fn nvim_can_notify_us() {
    let (_nvim, connection) = nvim_or_skip!("notify");
    let (seen_tx, seen_rx) = mpsc::channel();
    connection.on_notification("kataba_ping", move |request| {
        let number = request.params.first().and_then(Value::as_i64);
        let _ = seen_tx.send(number);
        Ok(())
    });
    // Channel 0 means "the current channel" from Lua's point of view.
    connection
        .call_timeout(
            "nvim_exec_lua",
            vec![
                Value::from("vim.rpcnotify(0, 'kataba_ping', 42)"),
                Value::Array(vec![]),
            ],
            TIMEOUT,
        )
        .unwrap();
    let number = seen_rx.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(number, Some(42));
    connection.close();
    connection.join();
}

#[test]
fn pipelined_calls_against_nvim_all_resolve() {
    let (_nvim, connection) = nvim_or_skip!("pipeline");
    let pendings: Vec<_> = (0..128)
        .map(|number| {
            connection
                .call_async("nvim_eval", vec![Value::from(format!("{number} * 2"))])
                .unwrap()
        })
        .collect();
    for (number, pending) in pendings.into_iter().enumerate() {
        let reply = pending.wait_timeout(TIMEOUT).unwrap();
        assert_eq!(reply, Value::from(number as i64 * 2));
    }
    connection.close();
    connection.join();
}
