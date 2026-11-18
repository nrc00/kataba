//! Signature help feature module.
//!
//! Provides `textDocument/signatureHelp` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    SignatureHelp, SignatureHelpParams, TextDocumentIdentifier, WorkDoneProgressParams,
};

/// Requests signature help for the parameter list at `position`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<SignatureHelp>> {
    client
        .check_feature(LanguageServerFeature::SignatureHelp)
        .await?;
    let params = SignatureHelpParams {
        text_document_position_params: lsp_types::TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        context: None,
    };
    client
        .send_request("textDocument/signatureHelp", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::SignatureHelp.as_str(),
            "textDocument/signatureHelp"
        );
    }
}
