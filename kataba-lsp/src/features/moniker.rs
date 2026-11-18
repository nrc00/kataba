//! Moniker feature module.
//!
//! Provides `textDocument/moniker` requests for cross-repository symbol identifiers.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    Moniker, MonikerParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
};

/// Requests monikers (cross-repository symbol identifiers) at `position`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<Vec<Moniker>>> {
    client.check_feature(LanguageServerFeature::Moniker).await?;
    let params = MonikerParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client.send_request("textDocument/moniker", params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::Moniker.as_str(),
            "textDocument/moniker"
        );
    }
}
