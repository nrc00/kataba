//! Inlay hints feature module.
//!
//! Provides `textDocument/inlayHint` and `inlayHint/resolve` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{InlayHint, InlayHintParams, TextDocumentIdentifier, WorkDoneProgressParams};

/// Requests inlay hints (inline type and parameter annotations) for `range`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    range: lsp_types::Range,
) -> Result<Option<Vec<InlayHint>>> {
    client
        .check_feature(LanguageServerFeature::InlayHint)
        .await?;
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client.send_request("textDocument/inlayHint", params).await
}

/// Resolves an inlay hint via `inlayHint/resolve`.
pub async fn resolve(client: &LspClient, hint: InlayHint) -> Result<InlayHint> {
    client
        .check_feature(LanguageServerFeature::InlayHint)
        .await?;
    client.send_request("inlayHint/resolve", hint).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::InlayHint.as_str(),
            "textDocument/inlayHint"
        );
    }
}
