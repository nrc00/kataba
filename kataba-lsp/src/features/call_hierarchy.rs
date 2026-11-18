//! Call hierarchy feature module.
//!
//! Provides `textDocument/prepareCallHierarchy`, `callHierarchy/incomingCalls`,
//! and `callHierarchy/outgoingCalls` requests.

use crate::client::LspClient;
use crate::error::Result;
use crate::state::LanguageServerFeature;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
};

/// Prepares a call hierarchy at `position`.
pub async fn prepare(
    client: &LspClient,
    uri: &lsp_types::Url,
    position: lsp_types::Position,
) -> Result<Option<Vec<CallHierarchyItem>>> {
    client
        .check_feature(LanguageServerFeature::CallHierarchy)
        .await?;
    let params = CallHierarchyPrepareParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    client
        .send_request("textDocument/prepareCallHierarchy", params)
        .await
}

/// Retrieves incoming calls for a call hierarchy item.
pub async fn incoming_calls(
    client: &LspClient,
    item: CallHierarchyItem,
) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
    let params = CallHierarchyIncomingCallsParams {
        item,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("callHierarchy/incomingCalls", params)
        .await
}

/// Retrieves outgoing calls for a call hierarchy item.
pub async fn outgoing_calls(
    client: &LspClient,
    item: CallHierarchyItem,
) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    let params = CallHierarchyOutgoingCallsParams {
        item,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };
    client
        .send_request("callHierarchy/outgoingCalls", params)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_matches_spec() {
        assert_eq!(
            LanguageServerFeature::CallHierarchy.as_str(),
            "textDocument/prepareCallHierarchy"
        );
    }
}
