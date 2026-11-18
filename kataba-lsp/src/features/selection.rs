//! Selection range feature module.
//!
//! Provides `textDocument/selectionRange` requests for smart expand/shrink selection.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    SelectionRange, SelectionRangeParams, TextDocumentIdentifier, WorkDoneProgressParams,
};

/// Requests selection ranges at the given `positions` in `uri`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    positions: Vec<lsp_types::Position>,
) -> Result<Option<Vec<SelectionRange>>> {
    client
        .check_feature(LanguageServerFeature::SelectionRange)
        .await?;
    let params = SelectionRangeParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        positions,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/selectionRange", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::SelectionRange.as_str(),
            "textDocument/selectionRange"
        );
    }
}
