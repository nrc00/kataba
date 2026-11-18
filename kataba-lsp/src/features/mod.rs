//! Modular LSP 3.17 feature modules for kataba-lsp.
//!
//! Each module in this directory implements a distinct LSP feature area:
//! completion, code actions, hover, rename, formatting, etc.
//!
//! Design:
//! - Each feature is a self-contained module with its own request/response types.
//! - Features take `&LspClient` and perform capability gating internally.
//! - This keeps `client.rs` lean and lets frontends (TUI, FFI, tests) use only
//!   the features they need without pulling in the entire client surface.
//!
//! Pattern per feature:
//!   pub async fn request(client: &LspClient, params: Params) -> Result<Option<Response>>
//!   // internally: client.check_feature(Feature::X)?; client.send_request(...)

// Public feature modules.
pub mod call_hierarchy;
pub mod code_action;
pub mod completion;
pub mod definition;
pub mod document_color;
pub mod document_links;
pub mod file_operations;
pub mod folding;
pub mod formatting;
pub mod hover;
pub mod inlay_hints;
pub mod inline_values;
pub mod linked_editing;
pub mod moniker;
pub mod pull_diagnostics;
pub mod references;
pub mod rename;
pub mod selection;
pub mod semantic_tokens;
pub mod signature;
pub mod workspace;
