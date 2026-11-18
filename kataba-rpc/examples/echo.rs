//! Minimal end-to-end demo: a server exposing `add`/`log` and a client that
//! calls them over a Unix socket.
//!
//! Run with: `cargo run --example echo`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::time::Duration;

use kataba_rpc::{Connection, Server, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path: PathBuf =
        std::env::temp_dir().join(format!("kataba-rpc-echo-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let server = Server::bind_unix(&path)?;

    let server_thread = std::thread::spawn(move || -> Result<(), String> {
        let connection = server.accept().map_err(|error| error.to_string())?;
        connection.on_request("add", |request| {
            let a = request.params.first().and_then(Value::as_i64).unwrap_or(0);
            let b = request.params.get(1).and_then(Value::as_i64).unwrap_or(0);
            Ok(Value::from(a + b))
        });
        connection.on_notification("log", |request| {
            println!("  [server] log: {:?}", request.params);
            Ok(())
        });
        // Serve until the client hangs up.
        while !connection.is_closed() {
            std::thread::sleep(Duration::from_millis(10));
        }
        connection.join();
        Ok(())
    });

    let client = Connection::connect_unix(&path)?;
    let sum = client.call_timeout(
        "add",
        vec![Value::from(20), Value::from(22)],
        Duration::from_secs(5),
    )?;
    println!("client: add(20, 22) = {sum}");
    client.notify("log", vec![Value::from("hello from the client")])?;
    std::thread::sleep(Duration::from_millis(50));

    client.close();
    client.join();
    server_thread.join().unwrap().unwrap();
    let _ = std::fs::remove_file(&path);
    Ok(())
}
