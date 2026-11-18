//! Code action feature module.
//!
//! Provides `textDocument/codeAction` and `codeAction/resolve` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    CodeAction, CodeActionContext, CodeActionParams, CodeActionResponse, TextDocumentIdentifier,
    WorkDoneProgressParams,
};

/// Requests available code actions for `range` in the document identified by `uri`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    range: lsp_types::Range,
    context: CodeActionContext,
) -> Result<Option<CodeActionResponse>> {
    client
        .check_feature(LanguageServerFeature::CodeAction)
        .await?;
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        context,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client.send_request("textDocument/codeAction", params).await
}

/// Resolves additional information for a single `CodeAction` via `codeAction/resolve`.
pub async fn resolve(client: &LspClient, action: CodeAction) -> Result<CodeAction> {
    client
        .check_feature(LanguageServerFeature::CodeAction)
        .await?;
    client.send_request("codeAction/resolve", action).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::CodeAction.as_str(),
            "textDocument/codeAction"
        );
    }
}
