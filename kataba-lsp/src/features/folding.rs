//! Folding range feature module.
//!
//! Provides `textDocument/foldingRange` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{FoldingRange, FoldingRangeParams, TextDocumentIdentifier, WorkDoneProgressParams};

/// Requests folding ranges for code structure in `uri`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
) -> Result<Option<Vec<FoldingRange>>> {
    client
        .check_feature(LanguageServerFeature::FoldingRange)
        .await?;
    let params = FoldingRangeParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/foldingRange", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::FoldingRange.as_str(),
            "textDocument/foldingRange"
        );
    }
}
