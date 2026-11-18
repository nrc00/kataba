//! Hover feature module.
//!
//! Provides `textDocument/hover` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{Hover, HoverParams, TextDocumentIdentifier, WorkDoneProgressParams};

/// Requests hover documentation at `position` in the document identified by `uri`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<Hover>> {
    client.check_feature(LanguageServerFeature::Hover).await?;
    let params = HoverParams {
        text_document_position_params: lsp_types::TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client.send_request("textDocument/hover", params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(LanguageServerFeature::Hover.as_str(), "textDocument/hover");
    }
}
