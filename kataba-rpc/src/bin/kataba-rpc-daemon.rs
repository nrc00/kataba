//! Minimal stdio daemon: msgpack-RPC over stdin/stdout.
//!
//! This is the deployment shape the Kataba engine will use — a frontend
//! (for example the future `kataba-tui`) spawns the daemon and speaks the
//! protocol over the pipes. It exposes two demo methods:
//!
//! * `echo` — returns its params array unchanged;
//! * `exit` — replies `"bye"` and then closes the connection.
//!
//! **Constraint:** a process speaking msgpack-RPC over stdout must keep
//! stdout byte-clean. No banners, no logging, no progress output — the same
//! rule Neovim's `--embed` and every stdio LSP server live under.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use kataba_rpc::{Connection, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::from_stdio()?;
    let quit = Arc::new(AtomicBool::new(false));

    connection.on_request("echo", |request| Ok(Value::Array(request.params.clone())));
    let quit_flag = Arc::clone(&quit);
    connection.on_request("exit", move |_| {
        quit_flag.store(true, Ordering::SeqCst);
        Ok(Value::from("bye"))
    });

    while !connection.is_closed() && !quit.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(5));
    }
    // The writer thread drains queued messages (including the `exit` reply)
    // after the queue closes, so the reply always precedes the hang-up.
    connection.close();
    connection.join();
    Ok(())
}
