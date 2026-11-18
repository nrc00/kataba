//! kataba-lsp: Language Server Protocol client implementation for the Kataba editor.
//!
//! This crate provides a high-performance, asynchronous LSP 3.17 client
//! that interfaces external language servers (such as `rust-analyzer`, `clangd`,
//! `gopls`, and `pyright`) with the `kataba-core` text editing engine.

#![forbid(unsafe_code)]

pub mod capabilities;
pub mod client;
pub mod conversion;
pub mod error;
pub mod event_loop;
pub mod features;
pub mod framing;
pub mod handlers;
pub mod manager;
pub mod rpc;
pub mod state;
pub mod sync;
pub mod transport;
pub mod watcher;

pub use capabilities::default_client_capabilities;
pub use client::LspClient;
pub use error::{LspError, Result};
pub use framing::LspCodec;
pub use handlers::{DiagnosticStore, LspHandlers, ProgressStore, ResolvedWorkspaceEdit};
pub use manager::{LanguageServerManager, ServerKey};
pub use rpc::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId};
pub use state::{CapabilityStore, ClientState, DocumentVersionTracker, LanguageServerFeature};
pub use transport::{ProcessTransport, ServerConfig};
pub use watcher::{FileWatcher, WatchConfig, WatchRegistrationBuilder};
