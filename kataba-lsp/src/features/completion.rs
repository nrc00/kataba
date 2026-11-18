//! Completion feature module.
//!
//! Provides `textDocument/completion` and `completionItem/resolve` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionParams, CompletionResponse,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};

/// Requests code completions at `position` in the document identified by `uri`.
///
/// Returns `None` if the server does not support completion or returns no items.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<CompletionResponse>> {
    request_with_context(client, uri, position, None).await
}

/// Requests code completions at `position` with optional trigger context.
pub async fn request_with_context(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
    context: Option<CompletionContext>,
) -> Result<Option<CompletionResponse>> {
    client
        .check_feature(LanguageServerFeature::Completion)
        .await?;
    let params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
        context,
    };
    client.send_request("textDocument/completion", params).await
}

/// Resolves additional information for a `CompletionItem` via `completionItem/resolve`.
pub async fn resolve(client: &LspClient, item: CompletionItem) -> Result<CompletionItem> {
    client.send_request("completionItem/resolve", item).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::Completion.as_str(),
            "textDocument/completion"
        );
    }
}
