//! Document color feature module.
//!
//! Provides `textDocument/documentColor` and `textDocument/colorPresentation` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    ColorInformation, ColorPresentation, ColorPresentationParams, DocumentColorParams,
    TextDocumentIdentifier, WorkDoneProgressParams,
};

/// Requests color decorations for colors in `uri`.
pub async fn request(client: &LspClient, uri: &lsp_types::Url) -> Result<Vec<ColorInformation>> {
    client.check_feature(LanguageServerFeature::Color).await?;
    let params = DocumentColorParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/documentColor", params)
        .await
}

/// Requests color presentation formatting for a color at `range`.
pub async fn presentation(
    client: &LspClient,
    uri: &lsp_types::Url,
    color: lsp_types::Color,
    range: lsp_types::Range,
) -> Result<Vec<ColorPresentation>> {
    client.check_feature(LanguageServerFeature::Color).await?;
    let params = ColorPresentationParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        color,
        range,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/colorPresentation", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::Color.as_str(),
            "textDocument/documentColor"
        );
    }
}
