//! Client capability matrix and configuration for LSP 3.17 handshake.

use lsp_types::{
    ClientCapabilities, CodeActionCapabilityResolveSupport, CodeActionClientCapabilities,
    CodeActionKind, CodeActionKindLiteralSupport, CodeActionLiteralSupport,
    CodeLensClientCapabilities, CodeLensWorkspaceClientCapabilities, CompletionClientCapabilities,
    CompletionItemCapability, CompletionItemCapabilityResolveSupport, CompletionItemKind,
    CompletionItemKindCapability, CompletionItemTag, DiagnosticClientCapabilities, DiagnosticTag,
    DiagnosticWorkspaceClientCapabilities, DidChangeConfigurationClientCapabilities,
    DidChangeWatchedFilesClientCapabilities, DocumentLinkClientCapabilities,
    DocumentSymbolClientCapabilities, DynamicRegistrationClientCapabilities, FailureHandlingKind,
    FoldingRangeClientCapabilities, GeneralClientCapabilities, GotoCapability,
    HoverClientCapabilities, InlayHintClientCapabilities, InlayHintResolveClientCapabilities,
    InlayHintWorkspaceClientCapabilities, InlineValueClientCapabilities,
    InlineValueWorkspaceClientCapabilities, InsertTextMode, InsertTextModeSupport,
    LinkedEditingRangeClientCapabilities, MarkupKind, MessageActionItemCapabilities,
    MonikerClientCapabilities, ParameterInformationSettings, PositionEncodingKind,
    PublishDiagnosticsClientCapabilities, RenameClientCapabilities, ResourceOperationKind,
    SelectionRangeClientCapabilities, SemanticTokensClientCapabilities,
    SemanticTokensClientCapabilitiesRequests, SemanticTokensFullOptions,
    SemanticTokensWorkspaceClientCapabilities, ShowDocumentClientCapabilities,
    ShowMessageRequestClientCapabilities, SignatureHelpClientCapabilities,
    SignatureInformationSettings, SymbolKind, SymbolKindCapability, SymbolTag, TagSupport,
    TextDocumentClientCapabilities, TextDocumentSyncClientCapabilities, TokenFormat,
    WindowClientCapabilities, WorkspaceClientCapabilities, WorkspaceEditClientCapabilities,
    WorkspaceFileOperationsClientCapabilities, WorkspaceSymbolClientCapabilities,
};

/// Returns all standard `SymbolKind` variants for symbol capability negotiation.
#[must_use]
pub fn all_symbol_kinds() -> Vec<SymbolKind> {
    vec![
        SymbolKind::FILE,
        SymbolKind::MODULE,
        SymbolKind::NAMESPACE,
        SymbolKind::PACKAGE,
        SymbolKind::CLASS,
        SymbolKind::METHOD,
        SymbolKind::PROPERTY,
        SymbolKind::FIELD,
        SymbolKind::CONSTRUCTOR,
        SymbolKind::ENUM,
        SymbolKind::INTERFACE,
        SymbolKind::FUNCTION,
        SymbolKind::VARIABLE,
        SymbolKind::CONSTANT,
        SymbolKind::STRING,
        SymbolKind::NUMBER,
        SymbolKind::BOOLEAN,
        SymbolKind::ARRAY,
        SymbolKind::OBJECT,
        SymbolKind::KEY,
        SymbolKind::NULL,
        SymbolKind::ENUM_MEMBER,
        SymbolKind::STRUCT,
        SymbolKind::EVENT,
        SymbolKind::OPERATOR,
        SymbolKind::TYPE_PARAMETER,
    ]
}

/// Returns all standard `CompletionItemKind` variants.
#[must_use]
pub fn all_completion_item_kinds() -> Vec<CompletionItemKind> {
    vec![
        CompletionItemKind::TEXT,
        CompletionItemKind::METHOD,
        CompletionItemKind::FUNCTION,
        CompletionItemKind::CONSTRUCTOR,
        CompletionItemKind::FIELD,
        CompletionItemKind::VARIABLE,
        CompletionItemKind::CLASS,
        CompletionItemKind::INTERFACE,
        CompletionItemKind::MODULE,
        CompletionItemKind::PROPERTY,
        CompletionItemKind::UNIT,
        CompletionItemKind::VALUE,
        CompletionItemKind::ENUM,
        CompletionItemKind::KEYWORD,
        CompletionItemKind::SNIPPET,
        CompletionItemKind::COLOR,
        CompletionItemKind::FILE,
        CompletionItemKind::REFERENCE,
        CompletionItemKind::FOLDER,
        CompletionItemKind::ENUM_MEMBER,
        CompletionItemKind::CONSTANT,
        CompletionItemKind::STRUCT,
        CompletionItemKind::EVENT,
        CompletionItemKind::OPERATOR,
        CompletionItemKind::TYPE_PARAMETER,
    ]
}

/// Returns the standard semantic token types supported by Kataba.
#[must_use]
pub fn all_semantic_token_types() -> Vec<lsp_types::SemanticTokenType> {
    vec![
        lsp_types::SemanticTokenType::NAMESPACE,
        lsp_types::SemanticTokenType::TYPE,
        lsp_types::SemanticTokenType::CLASS,
        lsp_types::SemanticTokenType::ENUM,
        lsp_types::SemanticTokenType::INTERFACE,
        lsp_types::SemanticTokenType::STRUCT,
        lsp_types::SemanticTokenType::TYPE_PARAMETER,
        lsp_types::SemanticTokenType::PARAMETER,
        lsp_types::SemanticTokenType::VARIABLE,
        lsp_types::SemanticTokenType::PROPERTY,
        lsp_types::SemanticTokenType::ENUM_MEMBER,
        lsp_types::SemanticTokenType::EVENT,
        lsp_types::SemanticTokenType::FUNCTION,
        lsp_types::SemanticTokenType::METHOD,
        lsp_types::SemanticTokenType::MACRO,
        lsp_types::SemanticTokenType::KEYWORD,
        lsp_types::SemanticTokenType::MODIFIER,
        lsp_types::SemanticTokenType::COMMENT,
        lsp_types::SemanticTokenType::STRING,
        lsp_types::SemanticTokenType::NUMBER,
        lsp_types::SemanticTokenType::REGEXP,
        lsp_types::SemanticTokenType::OPERATOR,
        lsp_types::SemanticTokenType::DECORATOR,
    ]
}

/// Returns the standard semantic token modifiers supported by Kataba.
#[must_use]
pub fn all_semantic_token_modifiers() -> Vec<lsp_types::SemanticTokenModifier> {
    vec![
        lsp_types::SemanticTokenModifier::DECLARATION,
        lsp_types::SemanticTokenModifier::DEFINITION,
        lsp_types::SemanticTokenModifier::READONLY,
        lsp_types::SemanticTokenModifier::STATIC,
        lsp_types::SemanticTokenModifier::DEPRECATED,
        lsp_types::SemanticTokenModifier::ABSTRACT,
        lsp_types::SemanticTokenModifier::ASYNC,
        lsp_types::SemanticTokenModifier::MODIFICATION,
        lsp_types::SemanticTokenModifier::DOCUMENTATION,
        lsp_types::SemanticTokenModifier::DEFAULT_LIBRARY,
    ]
}

/// Builds the comprehensive, best-in-class LSP 3.17 `ClientCapabilities`.
#[must_use]
pub fn default_client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        workspace: Some(WorkspaceClientCapabilities {
            apply_edit: Some(true),
            workspace_edit: Some(WorkspaceEditClientCapabilities {
                document_changes: Some(true),
                resource_operations: Some(vec![
                    ResourceOperationKind::Create,
                    ResourceOperationKind::Rename,
                    ResourceOperationKind::Delete,
                ]),
                failure_handling: Some(FailureHandlingKind::Abort),
                normalizes_line_endings: Some(false),
                change_annotation_support: None,
            }),
            did_change_configuration: Some(DidChangeConfigurationClientCapabilities {
                dynamic_registration: Some(true),
            }),
            did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                dynamic_registration: Some(true),
                relative_pattern_support: Some(true),
            }),
            symbol: Some(WorkspaceSymbolClientCapabilities {
                dynamic_registration: Some(false),
                symbol_kind: Some(SymbolKindCapability {
                    value_set: Some(all_symbol_kinds()),
                }),
                tag_support: Some(TagSupport {
                    value_set: vec![SymbolTag::DEPRECATED],
                }),
                resolve_support: None,
            }),
            execute_command: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
            workspace_folders: Some(true),
            configuration: Some(true),
            semantic_tokens: Some(SemanticTokensWorkspaceClientCapabilities {
                refresh_support: Some(true),
            }),
            code_lens: Some(CodeLensWorkspaceClientCapabilities {
                refresh_support: Some(true),
            }),
            file_operations: Some(WorkspaceFileOperationsClientCapabilities {
                dynamic_registration: Some(false),
                did_create: Some(true),
                will_create: Some(true),
                did_rename: Some(true),
                will_rename: Some(true),
                did_delete: Some(true),
                will_delete: Some(true),
            }),
            inline_value: Some(InlineValueWorkspaceClientCapabilities {
                refresh_support: Some(true),
            }),
            inlay_hint: Some(InlayHintWorkspaceClientCapabilities {
                refresh_support: Some(true),
            }),
            diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
                refresh_support: Some(true),
            }),
        }),
        text_document: Some(TextDocumentClientCapabilities {
            synchronization: Some(TextDocumentSyncClientCapabilities {
                dynamic_registration: Some(false),
                will_save: Some(true),
                will_save_wait_until: Some(true),
                did_save: Some(true),
            }),
            completion: Some(CompletionClientCapabilities {
                dynamic_registration: Some(false),
                completion_item: Some(CompletionItemCapability {
                    snippet_support: Some(true),
                    commit_characters_support: Some(true),
                    documentation_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    deprecated_support: Some(true),
                    preselect_support: Some(true),
                    tag_support: Some(TagSupport {
                        value_set: vec![CompletionItemTag::DEPRECATED],
                    }),
                    insert_replace_support: Some(true),
                    resolve_support: Some(CompletionItemCapabilityResolveSupport {
                        properties: vec![
                            "documentation".to_string(),
                            "detail".to_string(),
                            "additionalTextEdits".to_string(),
                        ],
                    }),
                    insert_text_mode_support: Some(InsertTextModeSupport {
                        value_set: vec![InsertTextMode::AS_IS, InsertTextMode::ADJUST_INDENTATION],
                    }),
                    label_details_support: Some(true),
                }),
                completion_item_kind: Some(CompletionItemKindCapability {
                    value_set: Some(all_completion_item_kinds()),
                }),
                context_support: Some(true),
                insert_text_mode: Some(InsertTextMode::AS_IS),
                completion_list: None,
            }),
            hover: Some(HoverClientCapabilities {
                dynamic_registration: Some(false),
                content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
            }),
            signature_help: Some(SignatureHelpClientCapabilities {
                dynamic_registration: Some(false),
                signature_information: Some(SignatureInformationSettings {
                    documentation_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    parameter_information: Some(ParameterInformationSettings {
                        label_offset_support: Some(true),
                    }),
                    active_parameter_support: Some(true),
                }),
                context_support: Some(true),
            }),
            references: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
            document_highlight: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
            document_symbol: Some(DocumentSymbolClientCapabilities {
                dynamic_registration: Some(false),
                symbol_kind: Some(SymbolKindCapability {
                    value_set: Some(all_symbol_kinds()),
                }),
                hierarchical_document_symbol_support: Some(true),
                tag_support: Some(TagSupport {
                    value_set: vec![SymbolTag::DEPRECATED],
                }),
            }),
            formatting: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(true),
            }),
            range_formatting: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(true),
            }),
            on_type_formatting: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
            declaration: Some(GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(true),
            }),
            definition: Some(GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(true),
            }),
            type_definition: Some(GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(true),
            }),
            implementation: Some(GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(true),
            }),
            code_action: Some(CodeActionClientCapabilities {
                dynamic_registration: Some(false),
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: vec![
                            CodeActionKind::EMPTY.as_str().to_string(),
                            CodeActionKind::QUICKFIX.as_str().to_string(),
                            CodeActionKind::REFACTOR.as_str().to_string(),
                            CodeActionKind::REFACTOR_EXTRACT.as_str().to_string(),
                            CodeActionKind::REFACTOR_INLINE.as_str().to_string(),
                            CodeActionKind::REFACTOR_REWRITE.as_str().to_string(),
                            CodeActionKind::SOURCE.as_str().to_string(),
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS.as_str().to_string(),
                        ],
                    },
                }),
                is_preferred_support: Some(true),
                disabled_support: Some(true),
                data_support: Some(true),
                resolve_support: Some(CodeActionCapabilityResolveSupport {
                    properties: vec!["edit".to_string()],
                }),
                honors_change_annotations: Some(false),
            }),
            code_lens: Some(CodeLensClientCapabilities {
                dynamic_registration: Some(false),
            }),
            document_link: Some(DocumentLinkClientCapabilities {
                dynamic_registration: Some(false),
                tooltip_support: Some(true),
            }),
            color_provider: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
            rename: Some(RenameClientCapabilities {
                dynamic_registration: Some(false),
                prepare_support: Some(true),
                prepare_support_default_behavior: None,
                honors_change_annotations: Some(false),
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                related_information: Some(true),
                tag_support: Some(TagSupport {
                    value_set: vec![DiagnosticTag::UNNECESSARY, DiagnosticTag::DEPRECATED],
                }),
                version_support: Some(true),
                code_description_support: Some(true),
                data_support: Some(true),
            }),
            folding_range: Some(FoldingRangeClientCapabilities {
                dynamic_registration: Some(false),
                range_limit: None,
                line_folding_only: Some(true),
                folding_range_kind: None,
                folding_range: None,
            }),
            selection_range: Some(SelectionRangeClientCapabilities {
                dynamic_registration: Some(false),
            }),
            linked_editing_range: Some(LinkedEditingRangeClientCapabilities {
                dynamic_registration: Some(false),
            }),
            call_hierarchy: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
            semantic_tokens: Some(SemanticTokensClientCapabilities {
                dynamic_registration: Some(false),
                requests: SemanticTokensClientCapabilitiesRequests {
                    range: Some(true),
                    full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                },
                token_types: all_semantic_token_types(),
                token_modifiers: all_semantic_token_modifiers(),
                formats: vec![TokenFormat::RELATIVE],
                overlapping_token_support: Some(true),
                multiline_token_support: Some(false),
                server_cancel_support: Some(true),
                augments_syntax_tokens: Some(true),
            }),
            inlay_hint: Some(InlayHintClientCapabilities {
                dynamic_registration: Some(false),
                resolve_support: Some(InlayHintResolveClientCapabilities {
                    properties: vec![
                        "tooltip".to_string(),
                        "textEdits".to_string(),
                        "label.tooltip".to_string(),
                        "label.location".to_string(),
                    ],
                }),
            }),
            diagnostic: Some(DiagnosticClientCapabilities {
                dynamic_registration: Some(false),
                related_document_support: Some(true),
            }),
            inline_value: Some(InlineValueClientCapabilities {
                dynamic_registration: Some(false),
            }),
            moniker: Some(MonikerClientCapabilities {
                dynamic_registration: Some(false),
            }),
            type_hierarchy: Some(DynamicRegistrationClientCapabilities {
                dynamic_registration: Some(false),
            }),
        }),
        window: Some(WindowClientCapabilities {
            show_message: Some(ShowMessageRequestClientCapabilities {
                message_action_item: Some(MessageActionItemCapabilities {
                    additional_properties_support: Some(true),
                }),
            }),
            show_document: Some(ShowDocumentClientCapabilities { support: true }),
            work_done_progress: Some(true),
        }),
        general: Some(GeneralClientCapabilities {
            position_encodings: Some(vec![
                PositionEncodingKind::UTF8,
                PositionEncodingKind::UTF32,
                PositionEncodingKind::UTF16,
            ]),
            stale_request_support: None,
            regular_expressions: None,
            markdown: None,
        }),
        experimental: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_default_client_capabilities_valid() {
        let caps = default_client_capabilities();
        assert!(caps.workspace.is_some());
        assert!(caps.text_document.is_some());
        assert!(caps.window.is_some());
        assert!(caps.general.is_some());

        let text_doc = caps.text_document.unwrap();
        assert!(text_doc.completion.is_some());
        assert!(text_doc.hover.is_some());
        assert!(text_doc.semantic_tokens.is_some());
        assert!(text_doc.inlay_hint.is_some());
        assert!(text_doc.code_action.is_some());
    }
}
