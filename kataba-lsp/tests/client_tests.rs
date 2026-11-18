#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kataba_core::encoding::PositionEncoding;
use kataba_core::rope::Rope;
use kataba_lsp::capabilities::default_client_capabilities;
use kataba_lsp::conversion::{point_to_position, position_to_point};
use kataba_lsp::handlers::{resolve_workspace_edit, HandlerError, LspHandlers};
use kataba_lsp::rpc::{parse_inbound_message, InboundMessage, RequestId};
use kataba_lsp::state::{CapabilityStore, LanguageServerFeature};
use lsp_types::*;
use std::collections::HashMap;

#[allow(dead_code)]
struct MockHandlers;
impl LspHandlers for MockHandlers {
    fn on_diagnostics(&self, _: PublishDiagnosticsParams) {}
    fn on_progress(&self, _: ProgressParams) {}
    fn on_apply_edit(&self, _: ApplyWorkspaceEditParams) -> ApplyWorkspaceEditResponse {
        ApplyWorkspaceEditResponse {
            applied: true,
            failure_reason: None,
            failed_change: None,
        }
    }
    fn on_register_capability(&self, _: RegistrationParams) {}
    fn on_unregister_capability(&self, _: UnregistrationParams) {}
    fn on_workspace_folders_change(&self, _: DidChangeWorkspaceFoldersParams) {}
    fn on_config_change(&self, _: DidChangeConfigurationParams) {}
    fn on_watched_files_change(&self, _: DidChangeWatchedFilesParams) {}
    fn on_log_message(&self, _: LogMessageParams) {}
    fn on_show_message(&self, _: ShowMessageParams) {}
    fn on_configuration(&self, _: ConfigurationParams) -> Vec<serde_json::Value> {
        vec![serde_json::json!({"tabSize": 4})]
    }
    fn on_execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>, HandlerError> {
        if params.command == "fail" {
            Err(HandlerError {
                code: -32603,
                message: "Command execution failed".to_string(),
                data: None,
            })
        } else {
            Ok(Some(serde_json::json!({"success": true})))
        }
    }
    fn on_workspace_folders(&self) -> Vec<WorkspaceFolder> {
        vec![WorkspaceFolder {
            uri: "file:///workspace".parse().unwrap(),
            name: "workspace".to_string(),
        }]
    }
}

#[test]
fn test_capabilities_completeness() {
    let caps = default_client_capabilities();
    let ws = caps.workspace.expect("Workspace capabilities missing");
    assert_eq!(ws.apply_edit, Some(true));
    assert_eq!(ws.workspace_folders, Some(true));
    assert_eq!(ws.configuration, Some(true));

    let td = caps
        .text_document
        .expect("TextDocument capabilities missing");
    let comp = td.completion.expect("Completion capabilities missing");
    let comp_item = comp.completion_item.expect("Completion item missing");
    assert_eq!(comp_item.snippet_support, Some(true));

    let sem = td.semantic_tokens.expect("Semantic tokens missing");
    assert!(sem.requests.full.is_some());
    assert_eq!(sem.formats, vec![TokenFormat::RELATIVE]);
}

#[test]
fn test_coordinate_conversion_preservation() {
    let pt = kataba_core::point::Point::new(15, 30);
    let pos = point_to_position(pt);
    assert_eq!(pos.line, 15);
    assert_eq!(pos.character, 30);

    let pt2 = position_to_point(pos);
    assert_eq!(pt, pt2);
}

#[test]
fn test_workspace_edit_with_resource_operations() {
    let file_uri: Url = "file:///project/src/lib.rs".parse().unwrap();
    let new_uri: Url = "file:///project/src/new_module.rs".parse().unwrap();

    let mut rope = Rope::from_str("fn foo() {}");
    let mut open_buffers = HashMap::new();
    open_buffers.insert(file_uri.clone(), rope.clone());

    let edit = WorkspaceEdit {
        changes: None,
        document_changes: Some(DocumentChanges::Operations(vec![
            DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                uri: new_uri.clone(),
                options: None,
                annotation_id: None,
            })),
            DocumentChangeOperation::Edit(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: file_uri.clone(),
                    version: None,
                },
                edits: vec![OneOf::Left(TextEdit {
                    range: Range::new(Position::new(0, 3), Position::new(0, 6)),
                    new_text: "bar".to_string(),
                })],
            }),
        ])),
        change_annotations: None,
    };

    let resolved = resolve_workspace_edit(&edit, &open_buffers, PositionEncoding::Utf8);
    assert_eq!(resolved.resource_operations.len(), 1);
    assert!(resolved.buffer_edits.contains_key(&file_uri));

    let txn = &resolved.buffer_edits[&file_uri];
    assert!(txn.apply_doc(&mut rope));
    assert_eq!(rope.to_string(), "fn bar() {}");
}

#[test]
fn test_inbound_message_parsing() {
    let req_json = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "workspace/configuration",
        "params": {
            "items": []
        }
    });
    let bytes = serde_json::to_vec(&req_json).unwrap();
    let parsed = parse_inbound_message(&bytes).expect("Failed to parse inbound request");
    match parsed {
        InboundMessage::Request(req) => {
            assert_eq!(req.id, RequestId::Number(42));
            assert_eq!(req.method, "workspace/configuration");
        }
        _ => panic!("Expected InboundMessage::Request"),
    }
}

#[test]
fn test_dynamic_capability_store_integration() {
    let mut store = CapabilityStore::default();
    assert!(!store.supports_feature(LanguageServerFeature::InlayHint));

    store.register(Registration {
        id: "inlay-hints-dynamic".to_string(),
        method: "textDocument/inlayHint".to_string(),
        register_options: None,
    });

    assert!(store.supports_feature(LanguageServerFeature::InlayHint));
    assert!(store.is_dynamically_registered("textDocument/inlayHint"));

    store.unregister("inlay-hints-dynamic");
    assert!(!store.supports_feature(LanguageServerFeature::InlayHint));
}
