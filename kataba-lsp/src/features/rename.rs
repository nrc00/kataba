//! Rename feature module.
//!
//! Provides `textDocument/prepareRename` and `textDocument/rename` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    PrepareRenameResponse, RenameParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
};

/// Prepares rename validation at `position` in the document identified by `uri`.
pub async fn prepare(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<PrepareRenameResponse>> {
    client.check_feature(LanguageServerFeature::Rename).await?;
    let params = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        position,
    };
    client
        .send_request("textDocument/prepareRename", params)
        .await
}

/// Requests a symbol rename across the workspace.
pub async fn rename(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
    new_name: String,
) -> Result<Option<lsp_types::WorkspaceEdit>> {
    client.check_feature(LanguageServerFeature::Rename).await?;
    let params = RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        new_name,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client.send_request("textDocument/rename", params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::Rename.as_str(),
            "textDocument/rename"
        );
    }
}
