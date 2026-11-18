//! Handler registry for editor integration callbacks.

use lsp_types::{
    ApplyWorkspaceEditParams, ApplyWorkspaceEditResponse, ConfigurationParams,
    DidChangeConfigurationParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
    ExecuteCommandParams, LogMessageParams, MessageActionItem, ProgressParams,
    PublishDiagnosticsParams, RegistrationParams, ShowDocumentParams, ShowDocumentResult,
    ShowMessageParams, ShowMessageRequestParams, UnregistrationParams,
    WorkDoneProgressCreateParams, WorkspaceFolder,
};

/// Error type for handler responses.
#[derive(Debug, Clone)]
pub struct HandlerError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Handler error (code {}): {}", self.code, self.message)
    }
}

impl std::error::Error for HandlerError {}

/// Trait for handling server-initiated LSP notifications and requests.
/// Implement this to integrate LSP events with your editor UI.
pub trait LspHandlers: Send + Sync {
    /// Called when the server publishes diagnostics for a document.
    fn on_diagnostics(&self, params: PublishDiagnosticsParams);

    /// Called when the server sends a progress notification ($/progress).
    fn on_progress(&self, params: ProgressParams);

    /// Called when the server requests a workspace edit (workspace/applyEdit).
    /// Return the result indicating whether the edit was applied.
    fn on_apply_edit(&self, params: ApplyWorkspaceEditParams) -> ApplyWorkspaceEditResponse;

    /// Called when the server registers a new capability dynamically.
    fn on_register_capability(&self, params: RegistrationParams);

    /// Called when the server unregisters a capability dynamically.
    fn on_unregister_capability(&self, params: UnregistrationParams);

    /// Called when workspace folders change.
    fn on_workspace_folders_change(&self, params: DidChangeWorkspaceFoldersParams);

    /// Called when configuration changes.
    fn on_config_change(&self, params: DidChangeConfigurationParams);

    /// Called when watched files change.
    fn on_watched_files_change(&self, params: DidChangeWatchedFilesParams);

    /// Called when the server sends a log message.
    fn on_log_message(&self, params: LogMessageParams);

    /// Called when the server requests a message to be shown to the user.
    fn on_show_message(&self, params: ShowMessageParams);

    /// Called when the server requests workspace configuration (workspace/configuration).
    /// Return the configuration values for the requested items.
    fn on_configuration(&self, params: ConfigurationParams) -> Vec<serde_json::Value>;

    /// Called when the server requests command execution (workspace/executeCommand).
    /// Return the command result or an error.
    fn on_execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>, HandlerError>;

    /// Called when the server requests creation of a work done progress token.
    fn on_work_done_progress_create(&self, _params: WorkDoneProgressCreateParams) {}

    /// Called when the server requests the current workspace folders.
    fn on_workspace_folders(&self) -> Vec<WorkspaceFolder> {
        Vec::new()
    }

    /// Called when the server requests showing a document in the editor.
    fn on_show_document(&self, _params: ShowDocumentParams) -> ShowDocumentResult {
        ShowDocumentResult { success: false }
    }

    /// Called when the server requests showing an interactive message with actions.
    fn on_show_message_request(
        &self,
        _params: ShowMessageRequestParams,
    ) -> Option<MessageActionItem> {
        None
    }

    /// Called when the server requests refreshing diagnostic reports.
    fn on_refresh_diagnostics(&self) {}

    /// Called when the server requests refreshing inlay hints.
    fn on_refresh_inlay_hints(&self) {}

    /// Called when the server requests refreshing code lenses.
    fn on_refresh_code_lenses(&self) {}

    /// Called when the server requests refreshing semantic tokens.
    fn on_refresh_semantic_tokens(&self) {}

    /// Called when the server requests refreshing inline values.
    fn on_refresh_inline_values(&self) {}
}

/// Default no-op implementation for empty handlers.
impl LspHandlers for () {
    fn on_diagnostics(&self, _: PublishDiagnosticsParams) {}
    fn on_progress(&self, _: ProgressParams) {}
    fn on_apply_edit(&self, _: ApplyWorkspaceEditParams) -> ApplyWorkspaceEditResponse {
        ApplyWorkspaceEditResponse {
            applied: false,
            failure_reason: None,
            failed_change: None,
        }
    }
    fn on_register_capability(&self, _: RegistrationParams) {}
    fn on_unregister_capability(&self, _: UnregistrationParams) {}
    fn on_workspace_folders_change(&self, _: DidChangeWorkspaceFoldersParams) {}
    fn on_config_change(&self, _: DidChangeConfigurationParams) {}
    fn on_watched_files_change(&self, _: DidChangeWatchedFilesParams) {}
    fn on_log_message(&self, _: LogMessageParams) {}
    fn on_show_message(&self, _: ShowMessageParams) {}
    fn on_configuration(&self, _: ConfigurationParams) -> Vec<serde_json::Value> {
        Vec::new()
    }
    fn on_execute_command(
        &self,
        _: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>, HandlerError> {
        Err(HandlerError {
            code: -32601, // MethodNotFound
            message: "workspace/executeCommand not implemented".to_string(),
            data: None,
        })
    }
}
/// Blanket implementation for Arc<dyn LspHandlers>.
impl LspHandlers for std::sync::Arc<dyn LspHandlers> {
    fn on_diagnostics(&self, params: PublishDiagnosticsParams) {
        (**self).on_diagnostics(params)
    }
    fn on_progress(&self, params: ProgressParams) {
        (**self).on_progress(params)
    }
    fn on_apply_edit(&self, params: ApplyWorkspaceEditParams) -> ApplyWorkspaceEditResponse {
        (**self).on_apply_edit(params)
    }
    fn on_register_capability(&self, params: RegistrationParams) {
        (**self).on_register_capability(params)
    }
    fn on_unregister_capability(&self, params: UnregistrationParams) {
        (**self).on_unregister_capability(params)
    }
    fn on_workspace_folders_change(&self, params: DidChangeWorkspaceFoldersParams) {
        (**self).on_workspace_folders_change(params)
    }
    fn on_config_change(&self, params: DidChangeConfigurationParams) {
        (**self).on_config_change(params)
    }
    fn on_watched_files_change(&self, params: DidChangeWatchedFilesParams) {
        (**self).on_watched_files_change(params)
    }
    fn on_log_message(&self, params: LogMessageParams) {
        (**self).on_log_message(params)
    }
    fn on_show_message(&self, params: ShowMessageParams) {
        (**self).on_show_message(params)
    }
    fn on_configuration(&self, params: ConfigurationParams) -> Vec<serde_json::Value> {
        (**self).on_configuration(params)
    }
    fn on_execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>, HandlerError> {
        (**self).on_execute_command(params)
    }
    fn on_work_done_progress_create(&self, params: WorkDoneProgressCreateParams) {
        (**self).on_work_done_progress_create(params)
    }
    fn on_workspace_folders(&self) -> Vec<WorkspaceFolder> {
        (**self).on_workspace_folders()
    }
    fn on_show_document(&self, params: ShowDocumentParams) -> ShowDocumentResult {
        (**self).on_show_document(params)
    }
    fn on_show_message_request(
        &self,
        params: ShowMessageRequestParams,
    ) -> Option<MessageActionItem> {
        (**self).on_show_message_request(params)
    }
    fn on_refresh_diagnostics(&self) {
        (**self).on_refresh_diagnostics()
    }
    fn on_refresh_inlay_hints(&self) {
        (**self).on_refresh_inlay_hints()
    }
    fn on_refresh_code_lenses(&self) {
        (**self).on_refresh_code_lenses()
    }
    fn on_refresh_semantic_tokens(&self) {
        (**self).on_refresh_semantic_tokens()
    }
    fn on_refresh_inline_values(&self) {
        (**self).on_refresh_inline_values()
    }
}

/// Blanket implementation for Box<dyn LspHandlers>.
impl LspHandlers for std::boxed::Box<dyn LspHandlers> {
    fn on_diagnostics(&self, params: PublishDiagnosticsParams) {
        (**self).on_diagnostics(params)
    }
    fn on_progress(&self, params: ProgressParams) {
        (**self).on_progress(params)
    }
    fn on_apply_edit(&self, params: ApplyWorkspaceEditParams) -> ApplyWorkspaceEditResponse {
        (**self).on_apply_edit(params)
    }
    fn on_register_capability(&self, params: RegistrationParams) {
        (**self).on_register_capability(params)
    }
    fn on_unregister_capability(&self, params: UnregistrationParams) {
        (**self).on_unregister_capability(params)
    }
    fn on_workspace_folders_change(&self, params: DidChangeWorkspaceFoldersParams) {
        (**self).on_workspace_folders_change(params)
    }
    fn on_config_change(&self, params: DidChangeConfigurationParams) {
        (**self).on_config_change(params)
    }
    fn on_watched_files_change(&self, params: DidChangeWatchedFilesParams) {
        (**self).on_watched_files_change(params)
    }
    fn on_log_message(&self, params: LogMessageParams) {
        (**self).on_log_message(params)
    }
    fn on_show_message(&self, params: ShowMessageParams) {
        (**self).on_show_message(params)
    }
    fn on_configuration(&self, params: ConfigurationParams) -> Vec<serde_json::Value> {
        (**self).on_configuration(params)
    }
    fn on_execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>, HandlerError> {
        (**self).on_execute_command(params)
    }
    fn on_work_done_progress_create(&self, params: WorkDoneProgressCreateParams) {
        (**self).on_work_done_progress_create(params)
    }
    fn on_workspace_folders(&self) -> Vec<WorkspaceFolder> {
        (**self).on_workspace_folders()
    }
    fn on_show_document(&self, params: ShowDocumentParams) -> ShowDocumentResult {
        (**self).on_show_document(params)
    }
    fn on_show_message_request(
        &self,
        params: ShowMessageRequestParams,
    ) -> Option<MessageActionItem> {
        (**self).on_show_message_request(params)
    }
    fn on_refresh_diagnostics(&self) {
        (**self).on_refresh_diagnostics()
    }
    fn on_refresh_inlay_hints(&self) {
        (**self).on_refresh_inlay_hints()
    }
    fn on_refresh_code_lenses(&self) {
        (**self).on_refresh_code_lenses()
    }
    fn on_refresh_semantic_tokens(&self) {
        (**self).on_refresh_semantic_tokens()
    }
    fn on_refresh_inline_values(&self) {
        (**self).on_refresh_inline_values()
    }
}
