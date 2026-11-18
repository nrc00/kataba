//! References feature module.
//!
//! Provides `textDocument/references` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    Location, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

/// Requests all references to the symbol at `position`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
    include_declaration: bool,
) -> Result<Option<Vec<Location>>> {
    client
        .check_feature(LanguageServerFeature::References)
        .await?;
    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration,
        },
    };
    client.send_request("textDocument/references", params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::References.as_str(),
            "textDocument/references"
        );
    }
}
