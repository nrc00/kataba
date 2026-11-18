//! Server-side helpers: listening sockets and broadcast.
//!
//! Adapted from Neovim's `src/nvim/msgpack_rpc/server.c` (accepting sockets
//! for `--listen`) and `broadcast_event` in `channel.c` (serialize once,
//! deliver to every channel).

use std::io;
use std::os::unix::net::UnixListener;
use std::path::Path;

use rmpv::Value;

use crate::connection::Connection;
use crate::error::RpcError;
use crate::message::Message;

/// A Unix-domain-socket listener that accepts [`Connection`]s.
pub struct Server {
    listener: UnixListener,
}

impl Server {
    /// Binds a Unix-domain socket at `path`.
    ///
    /// Fails if the path already exists; remove a stale socket first if that
    /// is the intent.
    pub fn bind_unix(path: &Path) -> io::Result<Self> {
        Ok(Self {
            listener: UnixListener::bind(path)?,
        })
    }

    /// Blocks until a peer connects and returns its connection.
    pub fn accept(&self) -> io::Result<Connection> {
        let (stream, _address) = self.listener.accept()?;
        Connection::from_unix_stream(stream)
    }

    /// The underlying listener, for non-blocking accept loops.
    #[must_use]
    pub fn listener(&self) -> &UnixListener {
        &self.listener
    }
}

impl std::fmt::Debug for Server {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.listener.local_addr() {
            Ok(address) => write!(formatter, "Server({address:?})"),
            Err(error) => write!(formatter, "Server(<{error}>)"),
        }
    }
}

/// Sends one notification to many connections, encoding it once.
///
/// Adapted from `broadcast_event`: Neovim serializes the message once into a
/// refcounted buffer and appends it to every channel. Returns how many
/// connections accepted the message; closed connections are skipped.
pub fn broadcast(
    connections: &[Connection],
    method: &str,
    params: Vec<Value>,
) -> Result<usize, RpcError> {
    let bytes = Message::Notification {
        method: method.to_string(),
        params,
    }
    .to_bytes()
    .map_err(|error| RpcError::exception(format!("failed to encode broadcast: {error}")))?;
    let mut delivered = 0;
    for connection in connections {
        if connection.write_raw(bytes.clone()).is_ok() {
            delivered += 1;
        }
    }
    Ok(delivered)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Server;
    use crate::connection::Connection;
    use std::path::PathBuf;

    fn socket_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("kataba-rpc-{}-{name}.sock", std::process::id()))
    }

    #[test]
    fn accept_returns_a_connection() {
        let path = socket_path("accept");
        let _ = std::fs::remove_file(&path);
        let server = Server::bind_unix(&path).unwrap();

        let client_path = path.clone();
        let client = std::thread::spawn(move || Connection::connect_unix(&client_path));

        let server_side = server.accept().unwrap();
        let client_side = client.join().unwrap().unwrap();

        // Both directions work.
        server_side.on_request("ping", |_| Ok(rmpv::Value::from("pong")));
        let reply = client_side
            .call_timeout("ping", vec![], std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(reply, rmpv::Value::from("pong"));

        client_side.close();
        server_side.close();
        client_side.join();
        server_side.join();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn broadcast_reaches_every_connection() {
        let path_a = socket_path("broadcast-a");
        let path_b = socket_path("broadcast-b");
        for path in [&path_a, &path_b] {
            let _ = std::fs::remove_file(path);
        }
        let server_a = Server::bind_unix(&path_a).unwrap();
        let server_b = Server::bind_unix(&path_b).unwrap();

        let (received_tx, received_rx) = std::sync::mpsc::channel();
        let make_client = |path: PathBuf, received_tx: std::sync::mpsc::Sender<i64>| {
            std::thread::spawn(move || {
                let connection = Connection::connect_unix(&path).unwrap();
                connection.on_notification("event", move |request| {
                    let value = request
                        .params
                        .first()
                        .and_then(rmpv::Value::as_i64)
                        .unwrap_or(-1);
                    received_tx.send(value).unwrap();
                    Ok(())
                });
                connection
            })
        };
        let client_a = make_client(path_a.clone(), received_tx.clone());
        let client_b = make_client(path_b.clone(), received_tx);
        let server_a_side = server_a.accept().unwrap();
        let server_b_side = server_b.accept().unwrap();
        let client_a = client_a.join().unwrap();
        let client_b = client_b.join().unwrap();

        let delivered = super::broadcast(
            &[server_a_side.clone(), server_b_side.clone()],
            "event",
            vec![rmpv::Value::from(7)],
        )
        .unwrap();
        assert_eq!(delivered, 2);
        assert_eq!(
            received_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            7
        );
        assert_eq!(
            received_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            7
        );

        for connection in [&client_a, &client_b, &server_a_side, &server_b_side] {
            connection.close();
        }
        for connection in [&client_a, &client_b, &server_a_side, &server_b_side] {
            connection.join();
        }
        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }
}
