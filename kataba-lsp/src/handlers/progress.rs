//! Progress notification tracking ($/progress).

use lsp_types::{NumberOrString, ProgressParams, ProgressParamsValue};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Key for tracking progress tokens.
pub type ProgressToken = NumberOrString;

/// In-memory store for tracking active progress tokens from the language server.
#[derive(Debug, Default, Clone)]
pub struct ProgressStore {
    tokens: Arc<RwLock<HashMap<ProgressToken, serde_json::Value>>>,
}

impl ProgressStore {
    /// Creates a new progress store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Updates or completes a progress notification.
    pub async fn handle(&self, params: ProgressParams) {
        let mut map = self.tokens.write().await;
        match params.value {
            ProgressParamsValue::WorkDone(work_done) => match work_done {
                lsp_types::WorkDoneProgress::End(_) => {
                    map.remove(&params.token);
                }
                _ => {
                    if let Ok(val) = serde_json::to_value(&work_done) {
                        map.insert(params.token, val);
                    }
                }
            },
        }
    }
}
