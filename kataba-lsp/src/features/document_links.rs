//! Document links feature module.
//!
//! Provides `textDocument/documentLink` and `documentLink/resolve` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{DocumentLink, DocumentLinkParams, TextDocumentIdentifier, WorkDoneProgressParams};

/// Requests document links (hyperlinks) within `uri`.
pub async fn request(
    client: &LspClient,
    uri: &lsp_types::Url,
) -> Result<Option<Vec<DocumentLink>>> {
    client
        .check_feature(LanguageServerFeature::DocumentLink)
        .await?;
    let params = DocumentLinkParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/documentLink", params)
        .await
}

/// Resolves a document link's target via `documentLink/resolve`.
pub async fn resolve(client: &LspClient, link: DocumentLink) -> Result<DocumentLink> {
    client
        .check_feature(LanguageServerFeature::DocumentLink)
        .await?;
    client.send_request("documentLink/resolve", link).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::DocumentLink.as_str(),
            "textDocument/documentLink"
        );
    }
}
