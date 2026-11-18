//! Connects to a headless Neovim over msgpack-RPC and prints the reference
//! implementation's answers: API metadata, and the exact error for an
//! unknown method.
//!
//! Run with: `cargo run --example nvim_probe`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use kataba_rpc::{Connection, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = std::env::temp_dir().join(format!("kataba-rpc-probe-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let mut child = Command::new("nvim")
        .args(["--headless", "--clean", "--listen"])
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let deadline = Instant::now() + Duration::from_secs(10);
    let client = loop {
        match Connection::connect_unix(&socket) {
            Ok(connection) => break connection,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(error.into()),
        }
    };

    let info = client.call_timeout("nvim_get_api_info", vec![], Duration::from_secs(10))?;
    if let Value::Array(items) = &info {
        if let Some(Value::Map(metadata)) = items.get(1) {
            for (key, value) in metadata {
                if key.as_str() == Some("version") {
                    println!("nvim version: {value:?}");
                }
            }
        }
    }

    let error = client
        .call_timeout("nvim_not_a_method", vec![], Duration::from_secs(10))
        .unwrap_err();
    println!(
        "unknown method -> code {}, message: {}",
        error.code, error.message
    );

    client.close();
    client.join();
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&socket);
    Ok(())
}
