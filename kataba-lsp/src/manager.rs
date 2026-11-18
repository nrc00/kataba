//! Multi-server manager and workspace routing orchestrator.

use lsp_types::{FileEvent, Url, WorkspaceFolder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::client::LspClient;
use crate::error::{LspError, Result};
use crate::handlers::LspHandlers;
use crate::transport::ServerConfig;

/// Key uniquely identifying a running server instance by language and workspace root.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerKey {
    /// The language ID (e.g. "rust", "c", "python").
    pub language_id: String,
    /// Canonical workspace root directory.
    pub root_dir: PathBuf,
}

/// Orchestrates multiple active language servers across different languages and project roots.
pub struct LanguageServerManager {
    /// Registered server configurations by language ID.
    configs: HashMap<String, ServerConfig>,
    /// Active running client instances.
    clients: HashMap<ServerKey, Arc<LspClient>>,
    /// Handler callbacks for LSP events.
    handlers: Arc<dyn LspHandlers>,
}

impl LanguageServerManager {
    /// Creates a new, empty language server manager.
    #[must_use]
    pub fn new(handlers: Arc<dyn LspHandlers>) -> Self {
        Self {
            configs: HashMap::new(),
            clients: HashMap::new(),
            handlers,
        }
    }

    /// Registers a server configuration for a given language ID.
    pub fn register_config(&mut self, language_id: impl Into<String>, config: ServerConfig) {
        self.configs.insert(language_id.into(), config);
    }

    /// Retrieves an active client, or spawns and initializes a new instance for `(language_id, root_dir)`.
    pub async fn get_or_spawn(
        &mut self,
        language_id: &str,
        root_dir: &Path,
    ) -> Result<Arc<LspClient>> {
        let key = ServerKey {
            language_id: language_id.to_string(),
            root_dir: root_dir.to_path_buf(),
        };

        if let Some(client) = self.clients.get(&key) {
            return Ok(Arc::clone(client));
        }

        let config = self
            .configs
            .get(language_id)
            .ok_or(LspError::CapabilityNotSupported(
                "No server configured for language",
            ))?;

        let mut custom_config = config.clone();
        custom_config.current_dir = Some(root_dir.to_path_buf());

        let client = Arc::new(LspClient::start(&custom_config, self.handlers.clone())?);

        let root_uri = Url::from_directory_path(root_dir).ok();
        let workspace_folders = root_uri.clone().map(|uri| {
            vec![WorkspaceFolder {
                uri,
                name: root_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace")
                    .to_string(),
            }]
        });

        // Initialize server handshake with config options
        client
            .initialize(
                root_uri,
                workspace_folders,
                config.init_options.clone(),
                config.settings.clone(),
            )
            .await?;

        self.clients.insert(key, Arc::clone(&client));
        Ok(client)
    }

    /// Returns a reference to an active client if already spawned.
    #[must_use]
    pub fn get_client(&self, language_id: &str, root_dir: &Path) -> Option<Arc<LspClient>> {
        let key = ServerKey {
            language_id: language_id.to_string(),
            root_dir: root_dir.to_path_buf(),
        };
        self.clients.get(&key).map(Arc::clone)
    }

    /// Broadcasts workspace folder changes across all active language servers.
    pub async fn broadcast_workspace_folders_change(
        &self,
        added: Vec<WorkspaceFolder>,
        removed: Vec<WorkspaceFolder>,
    ) -> Result<()> {
        for client in self.clients.values() {
            let _ = client
                .did_change_workspace_folders(added.clone(), removed.clone())
                .await;
        }
        Ok(())
    }

    /// Broadcasts watched file change events across all active language servers.
    pub async fn broadcast_watched_files(&self, changes: Vec<FileEvent>) -> Result<()> {
        for client in self.clients.values() {
            let _ = client.did_change_watched_files(changes.clone()).await;
        }
        Ok(())
    }

    /// Shuts down all active language servers cleanly.
    pub async fn shutdown_all(&mut self) -> Result<()> {
        for (_, client) in self.clients.drain() {
            let _ = client.shutdown().await;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_configuration_registration() {
        let handlers = Arc::new(());
        let mut manager = LanguageServerManager::new(handlers);
        let config = ServerConfig::new("rust-analyzer").arg("--version");
        manager.register_config("rust", config);

        assert!(manager.configs.contains_key("rust"));
        assert_eq!(manager.configs["rust"].command, "rust-analyzer");
        assert_eq!(manager.configs["rust"].args, vec!["--version"]);
    }
}
