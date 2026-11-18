//! Document synchronization protocol handlers and delta translation.

use crate::conversion::point_to_position;
use crate::error::{LspError, Result};
use crate::rpc::JsonRpcNotification;
use crate::state::DocumentVersionTracker;
use kataba_core::change::{ChangeSet, Operation};
use kataba_core::encoding::PositionEncoding;
use kataba_core::rope::Rope;
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentSyncKind, Url, VersionedTextDocumentIdentifier,
};

/// Builds a `textDocument/didOpen` notification.
pub fn build_did_open(
    uri: &Url,
    language_id: impl Into<String>,
    text: String,
    version_tracker: &mut DocumentVersionTracker,
) -> JsonRpcNotification {
    let version = version_tracker.register(uri.as_str());
    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: language_id.into(),
            version,
            text,
        },
    };
    let value = serde_json::to_value(params).unwrap_or(serde_json::Value::Null);
    JsonRpcNotification::new("textDocument/didOpen", Some(value))
}

/// Builds a `textDocument/didChange` notification from a `ChangeSet` or full document text.
pub fn build_did_change(
    uri: &Url,
    doc_before: &Rope,
    changes: &ChangeSet,
    sync_kind: TextDocumentSyncKind,
    encoding: PositionEncoding,
    version_tracker: &mut DocumentVersionTracker,
) -> Result<JsonRpcNotification> {
    let version = version_tracker.increment(uri.as_str())?;

    let content_changes = match sync_kind {
        TextDocumentSyncKind::INCREMENTAL => {
            let mut lsp_changes = Vec::new();
            let mut cursor = 0usize;

            let mut iter = changes.changes().iter().peekable();
            while let Some(op) = iter.next() {
                match op {
                    Operation::Retain(n) => {
                        cursor += *n;
                    }
                    Operation::Delete(n) => {
                        let del_len = *n;
                        let insert_text = if let Some(Operation::Insert(s)) = iter.peek() {
                            let text = s.to_string();
                            let _ = iter.next();
                            text
                        } else {
                            String::new()
                        };

                        let start_pt = doc_before.char_to_point(cursor, encoding);
                        let end_pt = doc_before.char_to_point(cursor + del_len, encoding);

                        lsp_changes.push(TextDocumentContentChangeEvent {
                            range: Some(lsp_types::Range {
                                start: point_to_position(start_pt),
                                end: point_to_position(end_pt),
                            }),
                            range_length: None,
                            text: insert_text,
                        });

                        cursor += del_len;
                    }
                    Operation::Insert(s) => {
                        let insert_text = s.to_string();
                        let pt = doc_before.char_to_point(cursor, encoding);
                        let pos = point_to_position(pt);

                        lsp_changes.push(TextDocumentContentChangeEvent {
                            range: Some(lsp_types::Range {
                                start: pos,
                                end: pos,
                            }),
                            range_length: None,
                            text: insert_text,
                        });
                    }
                }
            }

            // In LSP, multiple incremental edits are applied sequentially.
            // If empty, return no changes
            lsp_changes
        }
        _ => {
            // Apply changes to derive the new full text
            let mut doc_after = doc_before.clone();
            changes.apply(&mut doc_after);
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: doc_after.slice_text(0, doc_after.len()),
            }]
        }
    };

    let params = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version,
        },
        content_changes,
    };

    let value = serde_json::to_value(params).map_err(LspError::Json)?;
    Ok(JsonRpcNotification::new(
        "textDocument/didChange",
        Some(value),
    ))
}

/// Builds a `textDocument/didSave` notification.
pub fn build_did_save(uri: &Url, text: Option<String>) -> JsonRpcNotification {
    let params = DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        text,
    };
    let value = serde_json::to_value(params).unwrap_or(serde_json::Value::Null);
    JsonRpcNotification::new("textDocument/didSave", Some(value))
}

/// Builds a `textDocument/didClose` notification.
pub fn build_did_close(
    uri: &Url,
    version_tracker: &mut DocumentVersionTracker,
) -> JsonRpcNotification {
    version_tracker.unregister(uri.as_str());
    let params = DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
    };
    let value = serde_json::to_value(params).unwrap_or(serde_json::Value::Null);
    JsonRpcNotification::new("textDocument/didClose", Some(value))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use kataba_core::change::Operation;
    use lsp_types::TextDocumentSyncKind;

    fn incremental_change_at(offset: usize, text: &str) -> ChangeSet {
        ChangeSet::new(vec![
            Operation::Retain(offset),
            Operation::Insert(text.chars().collect()),
        ])
    }

    fn start_character(notif: &JsonRpcNotification) -> u32 {
        notif.params.as_ref().unwrap()["contentChanges"][0]["range"]["start"]["character"]
            .as_u64()
            .unwrap() as u32
    }

    #[test]
    fn did_change_offsets_follow_negotiated_utf16() {
        let uri: Url = "file:///test.rs".parse().unwrap();
        // The emoji is 4 UTF-8 bytes but 2 UTF-16 code units, so character
        // offset 2 maps to column 3 under UTF-16 and column 5 under UTF-8.
        let rope = Rope::from_str("a😀b");
        let changes = incremental_change_at(2, "Z");
        let mut tracker = DocumentVersionTracker::default();
        tracker.register(uri.as_str());

        let notif = build_did_change(
            &uri,
            &rope,
            &changes,
            TextDocumentSyncKind::INCREMENTAL,
            PositionEncoding::Utf16,
            &mut tracker,
        )
        .unwrap();
        assert_eq!(start_character(&notif), 3);

        let notif = build_did_change(
            &uri,
            &rope,
            &changes,
            TextDocumentSyncKind::INCREMENTAL,
            PositionEncoding::Utf8,
            &mut tracker,
        )
        .unwrap();
        assert_eq!(start_character(&notif), 5);
    }

    #[test]
    fn did_change_falls_back_to_full_sync_for_unknown_kind() {
        let uri: Url = "file:///test.rs".parse().unwrap();
        let rope = Rope::from_str("hello");
        let changes = incremental_change_at(5, "!");
        let mut tracker = DocumentVersionTracker::default();
        tracker.register(uri.as_str());

        let notif = build_did_change(
            &uri,
            &rope,
            &changes,
            TextDocumentSyncKind::FULL,
            PositionEncoding::Utf16,
            &mut tracker,
        )
        .unwrap();
        assert!(notif.params.unwrap()["contentChanges"][0]["range"].is_null());
    }
}
