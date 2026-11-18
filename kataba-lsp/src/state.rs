//! Client lifecycle state machine, capability store, and document version registry.

use crate::error::{LspError, Result};
use kataba_core::encoding::PositionEncoding;
use lsp_types::{OneOf, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind};
use std::collections::HashMap;

/// The lifecycle state of the LSP client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientState {
    /// Client has been spawned but `initialize` request has not completed.
    #[default]
    Uninitialized,
    /// `initialize` request sent; waiting for `InitializeResult`.
    Initializing,
    /// Protocol handshake complete (`initialized` sent); normal operation.
    Ready,
    /// `shutdown` request sent; waiting for response before exiting.
    ShuttingDown,
    /// Server process terminated or cleanly exited.
    Terminated,
}

impl ClientState {
    /// Returns `true` if the client is fully initialized and ready for requests.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Enumeration of distinct LSP features for capability gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageServerFeature {
    Completion,
    Hover,
    SignatureHelp,
    Definition,
    TypeDefinition,
    Implementation,
    References,
    DocumentHighlight,
    DocumentSymbol,
    WorkspaceSymbol,
    CodeAction,
    CodeLens,
    DocumentFormatting,
    DocumentRangeFormatting,
    DocumentOnTypeFormatting,
    Rename,
    DocumentLink,
    Color,
    FoldingRange,
    Declaration,
    ExecuteCommand,
    CallHierarchy,
    SemanticTokens,
    Moniker,
    LinkedEditingRange,
    InlineValue,
    InlayHint,
    SelectionRange,
    WorkspaceFolders,
}

impl LanguageServerFeature {
    /// Returns the static name of the feature.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completion => "textDocument/completion",
            Self::Hover => "textDocument/hover",
            Self::SignatureHelp => "textDocument/signatureHelp",
            Self::Definition => "textDocument/definition",
            Self::TypeDefinition => "textDocument/typeDefinition",
            Self::Implementation => "textDocument/implementation",
            Self::References => "textDocument/references",
            Self::DocumentHighlight => "textDocument/documentHighlight",
            Self::DocumentSymbol => "textDocument/documentSymbol",
            Self::WorkspaceSymbol => "workspace/symbol",
            Self::CodeAction => "textDocument/codeAction",
            Self::CodeLens => "textDocument/codeLens",
            Self::DocumentFormatting => "textDocument/formatting",
            Self::DocumentRangeFormatting => "textDocument/rangeFormatting",
            Self::DocumentOnTypeFormatting => "textDocument/onTypeFormatting",
            Self::Rename => "textDocument/rename",
            Self::DocumentLink => "textDocument/documentLink",
            Self::Color => "textDocument/documentColor",
            Self::FoldingRange => "textDocument/foldingRange",
            Self::Declaration => "textDocument/declaration",
            Self::ExecuteCommand => "workspace/executeCommand",
            Self::CallHierarchy => "textDocument/prepareCallHierarchy",
            Self::SemanticTokens => "textDocument/semanticTokens",
            Self::Moniker => "textDocument/moniker",
            Self::LinkedEditingRange => "textDocument/linkedEditingRange",
            Self::InlineValue => "textDocument/inlineValue",
            Self::InlayHint => "textDocument/inlayHint",
            Self::SelectionRange => "textDocument/selectionRange",
            Self::WorkspaceFolders => "workspace/workspaceFolders",
        }
    }
}

/// Stores negotiated server capabilities and gates feature requests.
#[derive(Debug, Clone, Default)]
pub struct CapabilityStore {
    capabilities: Option<ServerCapabilities>,
    dynamic_registrations: HashMap<String, lsp_types::Registration>,
    /// Column encoding agreed during `initialize` (UTF-16 if not negotiated).
    position_encoding: PositionEncoding,
}

impl CapabilityStore {
    /// Stores the negotiated server capabilities from `InitializeResult`.
    ///
    /// Also latches the `positionEncoding` the server selected. Per the LSP
    /// specification, a server that omits the field is using UTF-16, so the
    /// store falls back to UTF-16 rather than leaking the previous value.
    pub fn set(&mut self, caps: ServerCapabilities) {
        self.position_encoding = caps
            .position_encoding
            .as_ref()
            .and_then(|kind| kind.as_str().parse().ok())
            .unwrap_or(PositionEncoding::Utf16);
        self.capabilities = Some(caps);
    }

    /// Returns the position encoding negotiated with the server.
    ///
    /// This must be used for every `Position`/`Range` conversion and every
    /// `didChange` delta; servers are free to pick UTF-8, UTF-16, or UTF-32.
    #[must_use]
    pub const fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    /// Overrides the negotiated position encoding (e.g. for servers that do not
    /// advertise `positionEncoding` but implement a non-standard default).
    pub fn set_position_encoding(&mut self, encoding: PositionEncoding) {
        self.position_encoding = encoding;
    }

    /// Dynamically registers a capability advertised via `client/registerCapability`.
    pub fn register(&mut self, reg: lsp_types::Registration) {
        self.dynamic_registrations.insert(reg.id.clone(), reg);
    }

    /// Dynamically unregisters a capability by ID via `client/unregisterCapability`.
    pub fn unregister(&mut self, id: &str) {
        self.dynamic_registrations.remove(id);
    }

    /// Returns `true` if a capability is registered dynamically for `method`.
    #[must_use]
    pub fn is_dynamically_registered(&self, method: &str) -> bool {
        self.dynamic_registrations
            .values()
            .any(|reg| reg.method == method)
    }

    /// Borrows the raw server capabilities, if initialized.
    #[must_use]
    pub fn get(&self) -> Option<&ServerCapabilities> {
        self.capabilities.as_ref()
    }

    /// Returns a copy of all current dynamic registrations.
    #[must_use]
    pub fn dynamic_registrations(&self) -> Vec<lsp_types::Registration> {
        self.dynamic_registrations.values().cloned().collect()
    }

    /// Checks if the server supports a given feature.
    #[must_use]
    pub fn supports_feature(&self, feature: LanguageServerFeature) -> bool {
        if self.is_dynamically_registered(feature.as_str()) {
            return true;
        }

        let caps = match self.capabilities.as_ref() {
            Some(c) => c,
            None => return false,
        };

        match feature {
            LanguageServerFeature::Completion => caps.completion_provider.is_some(),
            LanguageServerFeature::Hover => caps.hover_provider.is_some(),
            LanguageServerFeature::SignatureHelp => caps.signature_help_provider.is_some(),
            LanguageServerFeature::Definition => matches!(
                caps.definition_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::TypeDefinition => caps.type_definition_provider.is_some(),
            LanguageServerFeature::Implementation => caps.implementation_provider.is_some(),
            LanguageServerFeature::References => matches!(
                caps.references_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::DocumentHighlight => matches!(
                caps.document_highlight_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::DocumentSymbol => matches!(
                caps.document_symbol_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::WorkspaceSymbol => matches!(
                caps.workspace_symbol_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::CodeAction => caps.code_action_provider.is_some(),
            LanguageServerFeature::CodeLens => caps.code_lens_provider.is_some(),
            LanguageServerFeature::DocumentFormatting => matches!(
                caps.document_formatting_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::DocumentRangeFormatting => matches!(
                caps.document_range_formatting_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::DocumentOnTypeFormatting => {
                caps.document_on_type_formatting_provider.is_some()
            }
            LanguageServerFeature::Rename => matches!(
                caps.rename_provider,
                Some(OneOf::Left(true) | OneOf::Right(_))
            ),
            LanguageServerFeature::DocumentLink => caps.document_link_provider.is_some(),
            LanguageServerFeature::Color => caps.color_provider.is_some(),
            LanguageServerFeature::FoldingRange => caps.folding_range_provider.is_some(),
            LanguageServerFeature::Declaration => caps.declaration_provider.is_some(),
            LanguageServerFeature::ExecuteCommand => caps.execute_command_provider.is_some(),
            LanguageServerFeature::CallHierarchy => caps.call_hierarchy_provider.is_some(),
            LanguageServerFeature::SemanticTokens => caps.semantic_tokens_provider.is_some(),
            LanguageServerFeature::Moniker => caps.moniker_provider.is_some(),
            LanguageServerFeature::LinkedEditingRange => {
                caps.linked_editing_range_provider.is_some()
            }
            LanguageServerFeature::InlineValue => caps.inline_value_provider.is_some(),
            LanguageServerFeature::InlayHint => caps.inlay_hint_provider.is_some(),
            LanguageServerFeature::SelectionRange => caps.selection_range_provider.is_some(),
            LanguageServerFeature::WorkspaceFolders => caps
                .workspace
                .as_ref()
                .and_then(|w| w.workspace_folders.as_ref())
                .is_some_and(|wf| wf.supported.unwrap_or(false)),
        }
    }

    /// Validates that a feature is supported, returning `Err(CapabilityNotSupported)` otherwise.
    pub fn check_feature(&self, feature: LanguageServerFeature) -> Result<()> {
        if self.supports_feature(feature) {
            Ok(())
        } else {
            Err(LspError::CapabilityNotSupported(feature.as_str()))
        }
    }

    /// Returns the negotiated text document sync kind.
    #[must_use]
    pub fn sync_kind(&self) -> TextDocumentSyncKind {
        match self
            .capabilities
            .as_ref()
            .and_then(|c| c.text_document_sync.as_ref())
        {
            Some(TextDocumentSyncCapability::Kind(kind)) => *kind,
            Some(TextDocumentSyncCapability::Options(opts)) => {
                opts.change.unwrap_or(TextDocumentSyncKind::NONE)
            }
            None => TextDocumentSyncKind::NONE,
        }
    }
}

/// Tracks monotonic document versions keyed by URI ($v \ge 1$).
#[derive(Debug, Default)]
pub struct DocumentVersionTracker {
    versions: HashMap<String, i32>,
}

impl DocumentVersionTracker {
    /// Registers a newly opened document URI with initial version 1.
    pub fn register(&mut self, uri: impl Into<String>) -> i32 {
        let uri = uri.into();
        self.versions.insert(uri, 1);
        1
    }

    /// Increments and returns the next monotonic version number for `uri`.
    pub fn increment(&mut self, uri: &str) -> Result<i32> {
        match self.versions.get_mut(uri) {
            Some(v) => {
                *v = v.saturating_add(1);
                Ok(*v)
            }
            None => Err(LspError::DocumentNotTracked(uri.to_string())),
        }
    }

    /// Returns the current version of the document, if tracked.
    #[must_use]
    pub fn get(&self, uri: &str) -> Option<i32> {
        self.versions.get(uri).copied()
    }

    /// Removes a closed document from tracking.
    pub fn unregister(&mut self, uri: &str) -> Option<i32> {
        self.versions.remove(uri)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_version_tracker_lifecycle() {
        let mut tracker = DocumentVersionTracker::default();
        let uri = "file:///project/src/main.rs";

        assert_eq!(tracker.register(uri), 1);
        assert_eq!(tracker.get(uri), Some(1));

        let v2 = tracker.increment(uri).unwrap();
        assert_eq!(v2, 2);
        assert_eq!(tracker.get(uri), Some(2));

        let v3 = tracker.increment(uri).unwrap();
        assert_eq!(v3, 3);

        assert_eq!(tracker.unregister(uri), Some(3));
        assert_eq!(tracker.get(uri), None);
        assert!(tracker.increment(uri).is_err());
    }

    #[test]
    fn test_capability_gating() {
        let mut store = CapabilityStore::default();
        assert!(!store.supports_feature(LanguageServerFeature::Hover));
        assert!(store.check_feature(LanguageServerFeature::Hover).is_err());

        let caps = ServerCapabilities {
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            completion_provider: Some(lsp_types::CompletionOptions::default()),
            ..Default::default()
        };
        store.set(caps);

        assert!(store.supports_feature(LanguageServerFeature::Hover));
        assert!(store.check_feature(LanguageServerFeature::Hover).is_ok());

        assert!(store.supports_feature(LanguageServerFeature::Completion));
        assert!(store
            .check_feature(LanguageServerFeature::Completion)
            .is_ok());

        assert!(!store.supports_feature(LanguageServerFeature::SignatureHelp));
        assert!(store
            .check_feature(LanguageServerFeature::SignatureHelp)
            .is_err());
    }

    #[test]
    fn test_position_encoding_defaults_to_utf16() {
        let mut store = CapabilityStore::default();
        assert_eq!(store.position_encoding(), PositionEncoding::Utf16);

        // A server that omits `positionEncoding` must be treated as UTF-16.
        store.set(ServerCapabilities::default());
        assert_eq!(store.position_encoding(), PositionEncoding::Utf16);
    }

    #[test]
    fn test_position_encoding_negotiated_from_server() {
        let mut store = CapabilityStore::default();

        store.set(ServerCapabilities {
            position_encoding: Some(lsp_types::PositionEncodingKind::UTF8),
            ..Default::default()
        });
        assert_eq!(store.position_encoding(), PositionEncoding::Utf8);

        // Re-initializing with a server that omitted the field must not keep
        // the previously negotiated encoding.
        store.set(ServerCapabilities::default());
        assert_eq!(store.position_encoding(), PositionEncoding::Utf16);
    }

    #[test]
    fn test_dynamic_capability_registration() {
        let mut store = CapabilityStore::default();
        assert!(!store.supports_feature(LanguageServerFeature::DocumentFormatting));

        let reg = lsp_types::Registration {
            id: "formatting-dynamic-id".to_string(),
            method: "textDocument/formatting".to_string(),
            register_options: None,
        };
        store.register(reg);

        assert!(store.supports_feature(LanguageServerFeature::DocumentFormatting));
        assert!(store
            .check_feature(LanguageServerFeature::DocumentFormatting)
            .is_ok());

        store.unregister("formatting-dynamic-id");
        assert!(!store.supports_feature(LanguageServerFeature::DocumentFormatting));
        assert!(store
            .check_feature(LanguageServerFeature::DocumentFormatting)
            .is_err());
    }
}
