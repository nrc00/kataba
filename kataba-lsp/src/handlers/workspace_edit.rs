use crate::conversion::text_edits_to_transaction;
use kataba_core::encoding::PositionEncoding;
use kataba_core::rope::Rope;
use kataba_core::transaction::Transaction;
use lsp_types::{DocumentChanges, OneOf, ResourceOp, TextEdit, Url, WorkspaceEdit};
use std::collections::HashMap;

/// Resolved multi-buffer workspace transactions and filesystem operations ready for atomic commit.
#[derive(Debug, Default, Clone)]
pub struct ResolvedWorkspaceEdit {
    /// Map of target document URLs to atomic, pure transactions.
    pub buffer_edits: HashMap<Url, Transaction>,
    /// File operations required by this edit (create, rename, delete).
    pub resource_operations: Vec<ResourceOp>,
}

/// Resolves a `WorkspaceEdit` against open document ropes into atomic `Transaction`s and resource ops.
pub fn resolve_workspace_edit(
    edit: &WorkspaceEdit,
    open_buffers: &HashMap<Url, Rope>,
    encoding: PositionEncoding,
) -> ResolvedWorkspaceEdit {
    let mut buffer_edits = HashMap::new();
    let mut resource_operations = Vec::new();

    // 1. Process document_changes if available (preferred in modern LSP)
    if let Some(ref doc_changes) = edit.document_changes {
        match doc_changes {
            DocumentChanges::Edits(edits_list) => {
                for text_doc_edit in edits_list {
                    if let Some(rope) = open_buffers.get(&text_doc_edit.text_document.uri) {
                        let text_edits = extract_edits(&text_doc_edit.edits);
                        let txn = text_edits_to_transaction(&text_edits, rope, encoding);
                        buffer_edits.insert(text_doc_edit.text_document.uri.clone(), txn);
                    }
                }
            }
            DocumentChanges::Operations(ops) => {
                for op in ops {
                    match op {
                        lsp_types::DocumentChangeOperation::Edit(text_doc_edit) => {
                            if let Some(rope) = open_buffers.get(&text_doc_edit.text_document.uri) {
                                let text_edits = extract_edits(&text_doc_edit.edits);
                                let txn = text_edits_to_transaction(&text_edits, rope, encoding);
                                buffer_edits.insert(text_doc_edit.text_document.uri.clone(), txn);
                            }
                        }
                        lsp_types::DocumentChangeOperation::Op(resource_op) => {
                            resource_operations.push(resource_op.clone());
                        }
                    }
                }
            }
        }
    } else if let Some(ref changes) = edit.changes {
        // 2. Fall back to standard changes map
        for (uri, text_edits) in changes {
            if let Some(rope) = open_buffers.get(uri) {
                let txn = text_edits_to_transaction(text_edits, rope, encoding);
                buffer_edits.insert(uri.clone(), txn);
            }
        }
    }

    ResolvedWorkspaceEdit {
        buffer_edits,
        resource_operations,
    }
}

fn extract_edits(edits: &[OneOf<TextEdit, lsp_types::AnnotatedTextEdit>]) -> Vec<TextEdit> {
    edits
        .iter()
        .map(|e| match e {
            OneOf::Left(edit) => edit.clone(),
            OneOf::Right(annotated) => annotated.text_edit.clone(),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lsp_types::{CreateFile, CreateFileOptions};

    #[test]
    fn test_resolve_resource_operation() {
        let uri: Url = "file:///test/new_file.rs".parse().unwrap();
        let edit = WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Operations(vec![
                lsp_types::DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                    uri: uri.clone(),
                    options: Some(CreateFileOptions {
                        overwrite: Some(false),
                        ignore_if_exists: Some(true),
                    }),
                    annotation_id: None,
                })),
            ])),
            change_annotations: None,
        };

        let open_buffers = HashMap::new();
        let resolved = resolve_workspace_edit(&edit, &open_buffers, PositionEncoding::Utf8);
        assert_eq!(resolved.resource_operations.len(), 1);
        match &resolved.resource_operations[0] {
            ResourceOp::Create(cf) => assert_eq!(cf.uri, uri),
            _ => panic!("Expected CreateFile resource op"),
        }
    }
}
