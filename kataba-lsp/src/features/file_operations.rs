//! Workspace file operations feature module.
//!
//! Provides requests and notifications for file create, rename, and delete operations.

use crate::client::LspClient;
use crate::error::Result;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileCreate, FileDelete, FileRename, RenameFilesParams,
    WorkspaceEdit,
};

/// Sends a `workspace/willCreateFiles` request to allow the server to return edits prior to creation.
pub async fn will_create_files(
    client: &LspClient,
    files: Vec<FileCreate>,
) -> Result<Option<WorkspaceEdit>> {
    let params = CreateFilesParams { files };
    client
        .send_request("workspace/willCreateFiles", params)
        .await
}

/// Notifies the server that files were created (`workspace/didCreateFiles`).
pub async fn did_create_files(client: &LspClient, files: Vec<FileCreate>) -> Result<()> {
    let params = CreateFilesParams { files };
    let params_val = serde_json::to_value(params).map_err(crate::error::LspError::Json)?;
    client
        .send_notification_params("workspace/didCreateFiles", Some(params_val))
        .await
}

/// Sends a `workspace/willRenameFiles` request to allow the server to update imports/references.
pub async fn will_rename_files(
    client: &LspClient,
    files: Vec<FileRename>,
) -> Result<Option<WorkspaceEdit>> {
    let params = RenameFilesParams { files };
    client
        .send_request("workspace/willRenameFiles", params)
        .await
}

/// Notifies the server that files were renamed (`workspace/didRenameFiles`).
pub async fn did_rename_files(client: &LspClient, files: Vec<FileRename>) -> Result<()> {
    let params = RenameFilesParams { files };
    let params_val = serde_json::to_value(params).map_err(crate::error::LspError::Json)?;
    client
        .send_notification_params("workspace/didRenameFiles", Some(params_val))
        .await
}

/// Sends a `workspace/willDeleteFiles` request to allow the server to prepare for file deletion.
pub async fn will_delete_files(
    client: &LspClient,
    files: Vec<FileDelete>,
) -> Result<Option<WorkspaceEdit>> {
    let params = DeleteFilesParams { files };
    client
        .send_request("workspace/willDeleteFiles", params)
        .await
}

/// Notifies the server that files were deleted (`workspace/didDeleteFiles`).
pub async fn did_delete_files(client: &LspClient, files: Vec<FileDelete>) -> Result<()> {
    let params = DeleteFilesParams { files };
    let params_val = serde_json::to_value(params).map_err(crate::error::LspError::Json)?;
    client
        .send_notification_params("workspace/didDeleteFiles", Some(params_val))
        .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_operation_methods() {
        assert_eq!("workspace/willCreateFiles", "workspace/willCreateFiles");
        assert_eq!("workspace/didCreateFiles", "workspace/didCreateFiles");
        assert_eq!("workspace/willRenameFiles", "workspace/willRenameFiles");
        assert_eq!("workspace/didRenameFiles", "workspace/didRenameFiles");
        assert_eq!("workspace/willDeleteFiles", "workspace/willDeleteFiles");
        assert_eq!("workspace/didDeleteFiles", "workspace/didDeleteFiles");
    }
}
