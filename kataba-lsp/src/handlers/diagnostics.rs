//! Diagnostic storage and caching for LSP client.

use lsp_types::{Diagnostic, PublishDiagnosticsParams, Url};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe in-memory cache of file diagnostics keyed by URL.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticStore {
    store: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
}

impl DiagnosticStore {
    /// Creates a new diagnostic store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Updates the cached diagnostics for the given document URI.
    pub async fn update(&self, params: PublishDiagnosticsParams) {
        let mut map = self.store.write().await;
        if params.diagnostics.is_empty() {
            map.remove(&params.uri);
        } else {
            map.insert(params.uri, params.diagnostics);
        }
    }

    /// Retrieves a cloned list of diagnostics for the given document URI.
    pub async fn get(&self, uri: &Url) -> Vec<Diagnostic> {
        let map = self.store.read().await;
        map.get(uri).cloned().unwrap_or_default()
    }

    /// Clears cached diagnostics for a closed document URI.
    pub async fn clear(&self, uri: &Url) {
        let mut map = self.store.write().await;
        map.remove(uri);
    }

    /// Clears all cached diagnostics across the entire workspace.
    pub async fn clear_all(&self) {
        let mut map = self.store.write().await;
        map.clear();
    }
}
