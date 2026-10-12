//! kataba-lsp: Language Server Protocol implementation for Kataba editor.
//!
//! This crate provides the LSP server implementation that interfaces with
//! the kataba-core editor library.

#![deny(unsafe_code)]
#![deny(unused_mut)]
#![deny(unused_variables)]
#![deny(dead_code)]

pub use server::KatabaLanguageServer;

mod server {
    use tower_lsp::{Client, LanguageServer};
    use kataba_core::Rope;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    pub struct KatabaLanguageServer {
        _client: Client,
        documents: Arc<RwLock<HashMap<String, Rope>>>,
    }

    #[tower_lsp::async_trait]
    impl LanguageServer for KatabaLanguageServer {
        async fn initialize(&self, _: tower_lsp::lsp_types::InitializeParams) -> tower_lsp::jsonrpc::Result<tower_lsp::lsp_types::InitializeResult> {
            Ok(tower_lsp::lsp_types::InitializeResult::default())
        }

        async fn initialized(&self, _: tower_lsp::lsp_types::InitializedParams) {}

        async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
            Ok(())
        }

        async fn did_open(&self, params: tower_lsp::lsp_types::DidOpenTextDocumentParams) {
            let mut docs = self.documents.write().await;
            docs.insert(params.text_document.uri.to_string(), Rope::from_str(&params.text_document.text));
        }

        async fn did_change(&self, params: tower_lsp::lsp_types::DidChangeTextDocumentParams) {
            let mut docs = self.documents.write().await;
            if let Some(_rope) = docs.get_mut(&params.text_document.uri.to_string()) {
                for _change in params.content_changes {
                    // Apply changes to rope
                    // This is a simplified implementation
                }
            }
        }
    }

    impl KatabaLanguageServer {
        pub fn new(client: Client) -> Self {
            Self {
                _client: client,
                documents: Default::default(),
            }
        }
    }
}