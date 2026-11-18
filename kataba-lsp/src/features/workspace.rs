//! Workspace management feature module.
//!
//! Provides workspace folders, configuration, and execute command support.

use crate::client::LspClient;
use crate::error::{LspError, Result};
use crate::state::LanguageServerFeature;
use lsp_types::{
    ConfigurationParams, DidChangeConfigurationParams, DidChangeWorkspaceFoldersParams,
    ExecuteCommandParams, WorkDoneProgressParams, WorkspaceFolder,
};
use serde_json::Value;

/// Requests the list of workspace folders (`workspace/workspaceFolders`).
pub async fn workspace_folders(client: &LspClient) -> Result<Option<Vec<WorkspaceFolder>>> {
    client
        .check_feature(LanguageServerFeature::WorkspaceFolders)
        .await?;
    client.send_request("workspace/workspaceFolders", ()).await
}

/// Notifies the server of added or removed workspace root folders.
pub async fn did_change_workspace_folders(
    client: &LspClient,
    added: Vec<WorkspaceFolder>,
    removed: Vec<WorkspaceFolder>,
) -> Result<()> {
    let params = DidChangeWorkspaceFoldersParams {
        event: lsp_types::WorkspaceFoldersChangeEvent { added, removed },
    };
    let params_val = serde_json::to_value(params).map_err(LspError::Json)?;
    client
        .send_notification_params("workspace/didChangeWorkspaceFolders", Some(params_val))
        .await
}

/// Notifies the server of editor configuration changes.
pub async fn did_change_configuration(client: &LspClient, settings: Value) -> Result<()> {
    let params = DidChangeConfigurationParams { settings };
    let params_val = serde_json::to_value(params).map_err(LspError::Json)?;
    client
        .send_notification_params("workspace/didChangeConfiguration", Some(params_val))
        .await
}

/// Requests workspace configuration sections (`workspace/configuration`).
pub async fn configuration(client: &LspClient, params: ConfigurationParams) -> Result<Vec<Value>> {
    client.send_request("workspace/configuration", params).await
}

/// Executes a custom workspace command on the language server.
pub async fn execute_command(
    client: &LspClient,
    command: String,
    arguments: Vec<Value>,
) -> Result<Option<Value>> {
    client
        .check_feature(LanguageServerFeature::ExecuteCommand)
        .await?;
    let params = ExecuteCommandParams {
        command,
        arguments,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client
        .send_request("workspace/executeCommand", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_names_match_spec() {
        assert_eq!(
            LanguageServerFeature::ExecuteCommand.as_str(),
            "workspace/executeCommand"
        );
        assert_eq!(
            LanguageServerFeature::WorkspaceFolders.as_str(),
            "workspace/workspaceFolders"
        );
    }
}
