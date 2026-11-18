//! Definition feature module.
//!
//! Provides goto-definition, type-definition, implementation, and declaration requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

/// Builds a `TextDocumentPositionParams` for the given `uri` and `position`.
fn position_params(
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        position,
    }
}

/// Requests go-to-definition for the symbol at `position`.
pub async fn goto_definition(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<GotoDefinitionResponse>> {
    client
        .check_feature(LanguageServerFeature::Definition)
        .await?;
    let params = GotoDefinitionParams {
        text_document_position_params: position_params(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client.send_request("textDocument/definition", params).await
}

/// Requests go-to-type-definition for the symbol at `position`.
pub async fn goto_type_definition(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<GotoDefinitionResponse>> {
    client
        .check_feature(LanguageServerFeature::TypeDefinition)
        .await?;
    let params = GotoDefinitionParams {
        text_document_position_params: position_params(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/typeDefinition", params)
        .await
}

/// Requests go-to-implementation for the symbol at `position`.
pub async fn goto_implementation(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<GotoDefinitionResponse>> {
    client
        .check_feature(LanguageServerFeature::Implementation)
        .await?;
    let params = GotoDefinitionParams {
        text_document_position_params: position_params(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/implementation", params)
        .await
}

/// Requests go-to-declaration for the symbol at `position`.
pub async fn goto_declaration(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<GotoDefinitionResponse>> {
    client
        .check_feature(LanguageServerFeature::Declaration)
        .await?;
    let params = GotoDefinitionParams {
        text_document_position_params: position_params(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("textDocument/declaration", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_names_match_spec() {
        assert_eq!(
            LanguageServerFeature::Definition.as_str(),
            "textDocument/definition"
        );
        assert_eq!(
            LanguageServerFeature::TypeDefinition.as_str(),
            "textDocument/typeDefinition"
        );
        assert_eq!(
            LanguageServerFeature::Implementation.as_str(),
            "textDocument/implementation"
        );
        assert_eq!(
            LanguageServerFeature::Declaration.as_str(),
            "textDocument/declaration"
        );
    }
}
