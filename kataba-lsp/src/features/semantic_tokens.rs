//! Semantic tokens feature module.
//!
//! Provides `textDocument/semanticTokens/full`, `/range`, and `/full/delta` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    SemanticTokens, SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensParams,
    SemanticTokensRangeParams, SemanticTokensRangeResult, TextDocumentIdentifier,
    WorkDoneProgressParams,
};

/// Requests full semantic tokens for syntax highlighting.
pub async fn full(client: &LspClient, uri: &lsp_types::Url) -> Result<Option<SemanticTokens>> {
    client
        .check_feature(LanguageServerFeature::SemanticTokens)
        .await?;
    let params = SemanticTokensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/semanticTokens/full", params)
        .await
}

/// Requests semantic tokens for a sub-range.
pub async fn range(
    client: &LspClient,
    uri: &lsp_types::Url,
    range: lsp_types::Range,
) -> Result<Option<SemanticTokensRangeResult>> {
    client
        .check_feature(LanguageServerFeature::SemanticTokens)
        .await?;
    let params = SemanticTokensRangeParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/semanticTokens/range", params)
        .await
}

/// Requests a semantic tokens delta from `previous_result_id`.
pub async fn delta(
    client: &LspClient,
    uri: &lsp_types::Url,
    previous_result_id: String,
) -> Result<Option<SemanticTokensFullDeltaResult>> {
    client
        .check_feature(LanguageServerFeature::SemanticTokens)
        .await?;
    let params = lsp_types::SemanticTokensDeltaParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        previous_result_id,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/semanticTokens/full/delta", params)
        .await
}

/// Type alias for a semantic tokens edit (used in delta responses).
pub type TokenEdit = SemanticTokensEdit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::SemanticTokens.as_str(),
            "textDocument/semanticTokens"
        );
    }
}
