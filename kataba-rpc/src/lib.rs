//! MessagePack-RPC for the Kataba editor, adapted from Neovim's `msgpack_rpc`.
//!
//! This crate is a standalone protocol layer: it has no Kataba dependencies
//! and knows nothing about buffers, windows, or trees. It provides
//!
//! * the wire shapes ([`Message`]) exactly as the MessagePack-RPC
//!   specification defines them (`[0, msgid, method, params]`,
//!   `[1, msgid, error, result]`, `[2, method, params]`), with the same
//!   strictness Neovim applies while unpacking;
//! * [`Connection`], a symmetric endpoint with a reader thread, a writer
//!   thread, and one in-order dispatch thread, carrying
//!   [`Connection::call`], [`Connection::notify`], and handler registration;
//! * transports for stdin/stdout ([`Connection::from_stdio`]) and Unix domain
//!   sockets ([`Connection::connect_unix`], [`Connection::from_unix_stream`],
//!   [`Server`]).
//!
//! The reference implementation for the adaptation was Neovim `v0.12.6`
//! (`src/nvim/msgpack_rpc/channel.c`, `channel_defs.h`, `packer.c`,
//! `unpacker.c`, `server.c`, `src/nvim/api/private/defs.h`); every message
//! shape, error encoding, and close rule cites its source in the module
//! docs. Intentional divergences are listed on [`Connection`].
//!
//! # Example
//!
//! ```
//! use kataba_rpc::{Connection, Value};
//!
//! # fn main() -> std::io::Result<()> {
//! let (client_side, server_side) = std::os::unix::net::UnixStream::pair()?;
//! let client = Connection::from_unix_stream(client_side)?;
//! let server = Connection::from_unix_stream(server_side)?;
//!
//! server.on_request("add", |request| {
//!     let a = request.params.first().and_then(Value::as_i64).unwrap_or(0);
//!     let b = request.params.get(1).and_then(Value::as_i64).unwrap_or(0);
//!     Ok(Value::from(a + b))
//! });
//!
//! let sum = client.call_timeout("add", vec![Value::from(2), Value::from(3)],
//!     std::time::Duration::from_secs(5)).unwrap_or(Value::Nil);
//! assert_eq!(sum, Value::from(5));
//!
//! client.close();
//! server.close();
//! client.join();
//! server.join();
//! # Ok(())
//! # }
//! ```

pub mod connection;
pub mod error;
pub mod message;
pub mod server;

pub use connection::{CloseReason, Connection, PendingReply, Request, ERROR_EVENT};
pub use error::{codes, RpcError};
pub use message::{message_type, DecodeError, Message};
pub use server::{broadcast, Server};

/// The MessagePack value type used for method arguments and results.
pub use rmpv::Value;
