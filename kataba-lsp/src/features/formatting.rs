//! Formatting feature module.
//!
//! Provides `textDocument/formatting`, `textDocument/rangeFormatting`,
//! and `textDocument/onTypeFormatting` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    DocumentFormattingParams, DocumentOnTypeFormattingParams, DocumentRangeFormattingParams,
    FormattingOptions, TextDocumentIdentifier, TextEdit, WorkDoneProgressParams,
};

/// Requests whole document formatting.
pub async fn formatting(
    client: &LspClient,
    uri: &lsp_types::Url,
    options: FormattingOptions,
) -> Result<Option<Vec<TextEdit>>> {
    client
        .check_feature(LanguageServerFeature::DocumentFormatting)
        .await?;
    let params = DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        options,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client.send_request("textDocument/formatting", params).await
}

/// Requests document range formatting.
pub async fn range_formatting(
    client: &LspClient,
    uri: &lsp_types::Url,
    range: lsp_types::Range,
    options: FormattingOptions,
) -> Result<Option<Vec<TextEdit>>> {
    client
        .check_feature(LanguageServerFeature::DocumentRangeFormatting)
        .await?;
    let params = DocumentRangeFormattingParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        options,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client
        .send_request("textDocument/rangeFormatting", params)
        .await
}

/// Requests document on-type formatting.
pub async fn on_type_formatting(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
    ch: String,
    options: FormattingOptions,
) -> Result<Option<Vec<TextEdit>>> {
    client
        .check_feature(LanguageServerFeature::DocumentOnTypeFormatting)
        .await?;
    let params = DocumentOnTypeFormattingParams {
        text_document_position: lsp_types::TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        ch,
        options,
    };
    client
        .send_request("textDocument/onTypeFormatting", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_names_match_spec() {
        assert_eq!(
            LanguageServerFeature::DocumentFormatting.as_str(),
            "textDocument/formatting"
        );
        assert_eq!(
            LanguageServerFeature::DocumentRangeFormatting.as_str(),
            "textDocument/rangeFormatting"
        );
        assert_eq!(
            LanguageServerFeature::DocumentOnTypeFormatting.as_str(),
            "textDocument/onTypeFormatting"
        );
    }
}
