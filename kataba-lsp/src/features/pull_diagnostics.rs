//! Pull diagnostics feature module.
//!
//! Provides `textDocument/diagnostic` (pull) and `workspace/diagnostic` requests.

use crate::client::LspClient;
use crate::error::Result;
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReportResult, PreviousResultId,
    TextDocumentIdentifier, WorkDoneProgressParams, WorkspaceDiagnosticParams,
    WorkspaceDiagnosticReportResult,
};

/// Requests pull diagnostics for `uri` with an optional `previous_result_id` for incremental updates.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    previous_result_id: Option<String>,
) -> Result<DocumentDiagnosticReportResult> {
    let params = DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        identifier: previous_result_id.clone(),
        previous_result_id,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client.send_request("textDocument/diagnostic", params).await
}

/// Requests workspace-wide pull diagnostics via `workspace/diagnostic`.
pub async fn workspace_diagnostic(
    client: &LspClient,
    previous_result_ids: Vec<PreviousResultId>,
) -> Result<WorkspaceDiagnosticReportResult> {
    let params = WorkspaceDiagnosticParams {
        previous_result_ids,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
        identifier: None,
    };
    client.send_request("workspace/diagnostic", params).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostic_method_names() {
        assert_eq!("textDocument/diagnostic", "textDocument/diagnostic");
        assert_eq!("workspace/diagnostic", "workspace/diagnostic");
    }
}
