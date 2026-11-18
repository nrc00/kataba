//! Linked editing range feature module.
//!
//! Provides `textDocument/linkedEditingRange` requests for simultaneous multiple tag editing.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    LinkedEditingRangeParams, LinkedEditingRanges, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

/// Requests linked editing ranges at `position` (e.g. matching HTML tags).
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<LinkedEditingRanges>> {
    client
        .check_feature(LanguageServerFeature::LinkedEditingRange)
        .await?;
    let params = LinkedEditingRangeParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client
        .send_request("textDocument/linkedEditingRange", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::LinkedEditingRange.as_str(),
            "textDocument/linkedEditingRange"
        );
    }
}
