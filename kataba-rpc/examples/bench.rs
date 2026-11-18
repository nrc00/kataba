//! Round-trip latency and throughput of [`kataba_rpc::Connection`] over an
//! in-process Unix socket pair.
//!
//! Run with: `cargo run --release --example bench`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use kataba_rpc::{Connection, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client_side, server_side) = std::os::unix::net::UnixStream::pair()?;
    let client = Connection::from_unix_stream(client_side)?;
    let server = Connection::from_unix_stream(server_side)?;
    server.on_request("echo", |request| Ok(Value::Array(request.params.clone())));

    for _ in 0..500 {
        client.call_timeout("echo", vec![Value::from(1)], Duration::from_secs(5))?;
    }

    let iterations = 20_000;
    let mut samples = Vec::with_capacity(iterations);
    for index in 0..iterations {
        let start = Instant::now();
        client.call_timeout(
            "echo",
            vec![Value::from(index as i64)],
            Duration::from_secs(5),
        )?;
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let percentile = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    let mean: u64 = samples.iter().sum::<u64>() / samples.len() as u64;
    println!(
        "small call roundtrip: mean {mean} ns ({} calls/s), p50 {} ns, p99 {} ns",
        1_000_000_000_u64.checked_div(mean).unwrap_or(0),
        percentile(0.50),
        percentile(0.99),
    );

    // Large payloads: 16 MiB out, 16 MiB directly back.
    server.on_request("echo_bytes", |request| match request.params.first() {
        Some(Value::Binary(_)) => Ok(request.params[0].clone()),
        _ => Ok(Value::Nil),
    });
    let payload = vec![0xAB_u8; 16 * 1024 * 1024];
    let start = Instant::now();
    let reply = client.call_timeout(
        "echo_bytes",
        vec![Value::from(payload)],
        Duration::from_secs(60),
    )?;
    let elapsed = start.elapsed();
    match reply {
        Value::Binary(bytes) => assert_eq!(bytes.len(), 16 * 1024 * 1024),
        other => panic!("expected binary, got {other:?}"),
    }
    println!(
        "16 MiB echo (32 MiB total): {elapsed:?}, {:.1} MiB/s",
        32.0 / elapsed.as_secs_f64()
    );

    client.close();
    server.close();
    client.join();
    server.join();
    Ok(())
}
