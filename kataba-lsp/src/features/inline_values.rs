//! Inline values feature module.
//!
//! Provides `textDocument/inlineValue` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    InlineValue, InlineValueContext, InlineValueParams, TextDocumentIdentifier,
    WorkDoneProgressParams,
};

/// Requests inline values for debugging within `range` under `context`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    range: lsp_types::Range,
    context: InlineValueContext,
) -> Result<Option<Vec<InlineValue>>> {
    client
        .check_feature(LanguageServerFeature::InlineValue)
        .await?;
    let params = InlineValueParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range,
        context,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client
        .send_request("textDocument/inlineValue", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::InlineValue.as_str(),
            "textDocument/inlineValue"
        );
    }
}
