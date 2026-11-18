//! High-level LSP client handle and complete LSP 3.17 feature request APIs.

use kataba_core::change::ChangeSet;
use kataba_core::encoding::PositionEncoding;
use kataba_core::rope::Rope;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, ClientCapabilities,
    CodeAction, CodeActionContext, CodeActionKind, CodeActionResponse, CodeActionTriggerKind,
    CodeLens, CodeLensParams, ColorInformation, ColorPresentation, CompletionContext,
    CompletionItem, CompletionResponse, DidChangeWatchedFilesParams, DocumentHighlight,
    DocumentHighlightParams, DocumentLink, DocumentSymbolParams, DocumentSymbolResponse,
    FileCreate, FileDelete, FileEvent, FileRename, FoldingRange, FormattingOptions,
    GotoDefinitionResponse, Hover, InitializeParams, InitializeResult, InlayHint, InlineValue,
    Location, Moniker, Position, PrepareRenameResponse, PreviousResultId, Range as LspRange,
    SelectionRange, SemanticTokens, SemanticTokensFullDeltaResult, SignatureHelp,
    TextDocumentIdentifier, TextDocumentPositionParams, TextDocumentSyncKind, TextEdit, Url,
    WorkDoneProgressParams, WorkspaceDiagnosticReportResult, WorkspaceEdit, WorkspaceFolder,
    WorkspaceSymbolParams,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::capabilities::default_client_capabilities;
use crate::error::{LspError, Result};
use crate::event_loop::{LspCommand, LspEventLoop};
use crate::features;
use crate::handlers::{DiagnosticStore, LspHandlers, ProgressStore, ResolvedWorkspaceEdit};
use crate::rpc::{JsonRpcNotification, RequestId, RequestIdGenerator};
use crate::state::{CapabilityStore, DocumentVersionTracker, LanguageServerFeature};
use crate::sync::{build_did_change, build_did_close, build_did_open, build_did_save};
use crate::transport::{ProcessTransport, ServerConfig};

/// Inner shared state of the `LspClient`.
struct ClientInner {
    cmd_tx: mpsc::Sender<LspCommand>,
    diagnostics: DiagnosticStore,
    progress: ProgressStore,
    capabilities: Arc<RwLock<CapabilityStore>>,
    server_pid: Option<u32>,
    id_gen: RequestIdGenerator,
    default_timeout: std::time::Duration,
}

/// A cloneable, thread-safe handle to an active language server connection.
#[derive(Clone)]
pub struct LspClient {
    inner: Arc<ClientInner>,
}

impl LspClient {
    /// Spawns a language server subprocess and starts the background actor event loop.
    pub fn start(config: &ServerConfig, handlers: Arc<dyn LspHandlers>) -> Result<Self> {
        let transport = ProcessTransport::spawn(config)?;
        let server_pid = transport.id();

        let (cmd_tx, cmd_rx) = mpsc::channel(128);
        let diagnostics = DiagnosticStore::new();
        let progress = ProgressStore::new();
        let capabilities = Arc::new(RwLock::new(CapabilityStore::default()));

        LspEventLoop::spawn(
            transport,
            cmd_rx,
            diagnostics.clone(),
            progress.clone(),
            capabilities.clone(),
            handlers,
        );

        Ok(Self {
            inner: Arc::new(ClientInner {
                cmd_tx,
                diagnostics,
                progress,
                capabilities,
                server_pid,
                id_gen: RequestIdGenerator::new(),
                default_timeout: std::time::Duration::from_secs(30),
            }),
        })
    }

    /// Returns the process ID of the child language server, if running.
    #[must_use]
    pub fn server_pid(&self) -> Option<u32> {
        self.inner.server_pid
    }

    /// Returns a reference to the in-memory diagnostic cache.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticStore {
        &self.inner.diagnostics
    }

    /// Returns a reference to the in-memory progress store.
    #[must_use]
    pub fn progress(&self) -> &ProgressStore {
        &self.inner.progress
    }

    /// Returns the default request timeout.
    #[must_use]
    pub fn default_timeout(&self) -> std::time::Duration {
        self.inner.default_timeout
    }

    /// Returns the position encoding negotiated during `initialize`.
    ///
    /// Servers are free to select UTF-8, UTF-16, or UTF-32; a server that omits
    /// `positionEncoding` is treated as UTF-16 per the LSP specification. Every
    /// `Position`/`Range` conversion and `didChange` delta must use this value.
    pub async fn position_encoding(&self) -> PositionEncoding {
        self.inner.capabilities.read().await.position_encoding()
    }

    /// Returns the document sync kind negotiated during `initialize`.
    pub async fn sync_kind(&self) -> TextDocumentSyncKind {
        self.inner.capabilities.read().await.sync_kind()
    }

    /// Checks if a feature is supported by the connected server.
    pub async fn supports_feature(&self, feature: LanguageServerFeature) -> bool {
        let caps = self.inner.capabilities.read().await;
        caps.supports_feature(feature)
    }

    /// Ensures a feature is supported by the server, returning `Err(CapabilityNotSupported)` otherwise.
    pub async fn check_feature(&self, feature: LanguageServerFeature) -> Result<()> {
        let caps = self.inner.capabilities.read().await;
        caps.check_feature(feature)
    }

    /// Cancels an active in-flight request by ID (`$/cancelRequest`).
    pub async fn cancel_request(&self, id: RequestId) -> Result<()> {
        self.inner
            .cmd_tx
            .send(LspCommand::Cancel { id })
            .await
            .map_err(|_| LspError::ChannelClosed)
    }

    /// Sends a JSON-RPC request with a custom timeout.
    ///
    /// If the request times out, a `$/cancelRequest` notification is automatically
    /// dispatched to the server to prevent wasting CPU cycles on orphaned calculations.
    pub async fn send_request_timeout<P, R>(
        &self,
        method: impl Into<String>,
        params: P,
        timeout: std::time::Duration,
    ) -> Result<R>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let params_val = serde_json::to_value(params).map_err(LspError::Json)?;
        let (responder, rx) = oneshot::channel();
        let id = self.inner.id_gen.next();

        self.inner
            .cmd_tx
            .send(LspCommand::Request {
                id: id.clone(),
                method: method.into(),
                params: Some(params_val),
                responder,
            })
            .await
            .map_err(|_| LspError::ChannelClosed)?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result_val)) => serde_json::from_value(result_val?).map_err(LspError::Json),
            Ok(Err(_)) => Err(LspError::ChannelClosed),
            Err(_) => {
                // Inform server that this request was cancelled due to timeout
                let _ = self.cancel_request(id).await;
                Err(LspError::Timeout)
            }
        }
    }

    /// Sends a generic JSON-RPC request and awaits the parsed result with default timeout.
    pub async fn send_request<P, R>(&self, method: impl Into<String>, params: P) -> Result<R>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        self.send_request_timeout(method, params, self.inner.default_timeout)
            .await
    }

    /// Sends a fire-and-forget JSON-RPC notification.
    pub async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        self.inner
            .cmd_tx
            .send(LspCommand::Notification { notification })
            .await
            .map_err(|_| LspError::ChannelClosed)
    }

    /// Sends a fire-and-forget JSON-RPC notification with a method name and optional params.
    pub async fn send_notification_params(
        &self,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        self.send_notification(notification).await
    }

    /// Executes the standard LSP `initialize` request using Kataba's best-in-class default capabilities.
    pub async fn initialize(
        &self,
        root_uri: Option<Url>,
        workspace_folders: Option<Vec<WorkspaceFolder>>,
        initialization_options: Option<serde_json::Value>,
        settings: Option<serde_json::Value>,
    ) -> Result<InitializeResult> {
        self.initialize_with_capabilities(
            root_uri,
            workspace_folders,
            initialization_options,
            settings,
            default_client_capabilities(),
        )
        .await
    }

    /// Executes `initialize` with caller-provided `ClientCapabilities`.
    pub async fn initialize_with_capabilities(
        &self,
        root_uri: Option<Url>,
        workspace_folders: Option<Vec<WorkspaceFolder>>,
        initialization_options: Option<serde_json::Value>,
        settings: Option<serde_json::Value>,
        capabilities: ClientCapabilities,
    ) -> Result<InitializeResult> {
        #[allow(deprecated)]
        let params = InitializeParams {
            process_id: std::process::id().into(),
            root_path: None,
            root_uri,
            initialization_options,
            capabilities,
            trace: None,
            workspace_folders,
            client_info: Some(lsp_types::ClientInfo {
                name: "Kataba".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            locale: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let result: InitializeResult = self.send_request("initialize", params).await?;

        // Send required `initialized` notification to complete the protocol handshake
        let notif = JsonRpcNotification::new(
            "initialized",
            Some(serde_json::to_value(lsp_types::InitializedParams {}).map_err(LspError::Json)?),
        );
        self.send_notification(notif).await?;

        // If settings were provided, send didChangeConfiguration
        if let Some(settings) = settings {
            self.did_change_configuration(settings).await?;
        }

        Ok(result)
    }

    // --- Navigation & Definitions ---

    /// Requests go-to-definition for the symbol at `position`.
    pub async fn goto_definition(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        features::definition::goto_definition(self, uri, position).await
    }

    /// Requests go-to-type-definition for the symbol at `position`.
    pub async fn goto_type_definition(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        features::definition::goto_type_definition(self, uri, position).await
    }

    /// Requests go-to-implementation for the symbol at `position`.
    pub async fn goto_implementation(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        features::definition::goto_implementation(self, uri, position).await
    }

    /// Requests go-to-declaration for the symbol at `position`.
    pub async fn goto_declaration(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        features::definition::goto_declaration(self, uri, position).await
    }

    /// Requests all references to the symbol at `position`.
    pub async fn references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<Location>>> {
        features::references::request(self, uri, position, include_declaration).await
    }

    // --- Code Editing & Refactoring ---

    /// Requests code completions at `position`.
    pub async fn completion(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<CompletionResponse>> {
        features::completion::request(self, uri, position).await
    }

    /// Requests code completions at `position` with trigger context.
    pub async fn completion_with_context(
        &self,
        uri: &Url,
        position: Position,
        context: Option<CompletionContext>,
    ) -> Result<Option<CompletionResponse>> {
        features::completion::request_with_context(self, uri, position, context).await
    }

    /// Resolves additional information for a `CompletionItem`.
    pub async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        features::completion::resolve(self, item).await
    }

    /// Requests hover documentation at `position`.
    pub async fn hover(&self, uri: &Url, position: Position) -> Result<Option<Hover>> {
        features::hover::request(self, uri, position).await
    }

    /// Requests signature help for the parameter list at `position`.
    pub async fn signature_help(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<SignatureHelp>> {
        features::signature::request(self, uri, position).await
    }

    /// Requests available code actions for the given range.
    pub async fn code_action(
        &self,
        uri: &Url,
        range: LspRange,
        context: CodeActionContext,
    ) -> Result<Option<CodeActionResponse>> {
        features::code_action::request(self, uri, range, context).await
    }

    /// Requests code actions including diagnostics from the diagnostic store.
    pub async fn code_action_with_diagnostics(
        &self,
        uri: &Url,
        range: LspRange,
        only: Option<Vec<CodeActionKind>>,
        trigger_kind: Option<CodeActionTriggerKind>,
    ) -> Result<Option<CodeActionResponse>> {
        self.check_feature(LanguageServerFeature::CodeAction)
            .await?;
        let diagnostics = self.inner.diagnostics.get(uri).await;
        let context = CodeActionContext {
            diagnostics,
            only,
            trigger_kind,
        };
        features::code_action::request(self, uri, range, context).await
    }

    /// Resolves additional information for a single `CodeAction`.
    pub async fn code_action_resolve(&self, action: CodeAction) -> Result<CodeAction> {
        features::code_action::resolve(self, action).await
    }

    /// Requests code lenses for `uri`.
    pub async fn code_lens(&self, uri: &Url) -> Result<Option<Vec<CodeLens>>> {
        self.check_feature(LanguageServerFeature::CodeLens).await?;
        let params = CodeLensParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };
        self.send_request("textDocument/codeLens", params).await
    }

    /// Resolves additional information for a `CodeLens`.
    pub async fn code_lens_resolve(&self, lens: CodeLens) -> Result<CodeLens> {
        self.send_request("codeLens/resolve", lens).await
    }

    /// Requests whole document formatting.
    pub async fn formatting(
        &self,
        uri: &Url,
        options: FormattingOptions,
    ) -> Result<Option<Vec<TextEdit>>> {
        features::formatting::formatting(self, uri, options).await
    }

    /// Requests document range formatting.
    pub async fn range_formatting(
        &self,
        uri: &Url,
        range: LspRange,
        options: FormattingOptions,
    ) -> Result<Option<Vec<TextEdit>>> {
        features::formatting::range_formatting(self, uri, range, options).await
    }

    /// Requests document on-type formatting.
    pub async fn on_type_formatting(
        &self,
        uri: &Url,
        position: Position,
        ch: String,
        options: FormattingOptions,
    ) -> Result<Option<Vec<TextEdit>>> {
        features::formatting::on_type_formatting(self, uri, position, ch, options).await
    }

    /// Prepares rename validation at `position`.
    pub async fn prepare_rename(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>> {
        features::rename::prepare(self, uri, position).await
    }

    /// Requests a symbol rename across the workspace.
    pub async fn rename(
        &self,
        uri: &Url,
        position: Position,
        new_name: String,
    ) -> Result<Option<WorkspaceEdit>> {
        features::rename::rename(self, uri, position, new_name).await
    }

    // --- Advanced Code Intelligence ---

    /// Requests document symbols (hierarchical or flat).
    pub async fn document_symbols(&self, uri: &Url) -> Result<Option<DocumentSymbolResponse>> {
        self.check_feature(LanguageServerFeature::DocumentSymbol)
            .await?;
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };
        self.send_request("textDocument/documentSymbol", params)
            .await
    }

    /// Queries symbols across the entire workspace.
    pub async fn workspace_symbols(
        &self,
        query: String,
    ) -> Result<Option<Vec<lsp_types::SymbolInformation>>> {
        self.check_feature(LanguageServerFeature::WorkspaceSymbol)
            .await?;
        let params = WorkspaceSymbolParams {
            query,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };
        self.send_request("workspace/symbol", params).await
    }

    /// Requests document highlights for the symbol under `position`.
    pub async fn document_highlight(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        self.check_feature(LanguageServerFeature::DocumentHighlight)
            .await?;
        let params = DocumentHighlightParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };
        self.send_request("textDocument/documentHighlight", params)
            .await
    }

    /// Requests document links (hyperlinks) within `uri`.
    pub async fn document_links(&self, uri: &Url) -> Result<Option<Vec<DocumentLink>>> {
        features::document_links::request(self, uri).await
    }

    /// Resolves a document link's target via `documentLink/resolve`.
    pub async fn document_link_resolve(&self, link: DocumentLink) -> Result<DocumentLink> {
        features::document_links::resolve(self, link).await
    }

    /// Requests color decorations for colors in `uri`.
    pub async fn document_color(&self, uri: &Url) -> Result<Vec<ColorInformation>> {
        features::document_color::request(self, uri).await
    }

    /// Requests color presentation formatting.
    pub async fn color_presentation(
        &self,
        uri: &Url,
        color: lsp_types::Color,
        range: LspRange,
    ) -> Result<Vec<ColorPresentation>> {
        features::document_color::presentation(self, uri, color, range).await
    }

    /// Requests folding ranges for code structure.
    pub async fn folding_ranges(&self, uri: &Url) -> Result<Option<Vec<FoldingRange>>> {
        features::folding::request(self, uri).await
    }

    /// Requests selection ranges (smart expand/shrink selection).
    pub async fn selection_ranges(
        &self,
        uri: &Url,
        positions: Vec<Position>,
    ) -> Result<Option<Vec<SelectionRange>>> {
        features::selection::request(self, uri, positions).await
    }

    /// Requests full semantic tokens for syntax highlighting.
    pub async fn semantic_tokens_full(&self, uri: &Url) -> Result<Option<SemanticTokens>> {
        features::semantic_tokens::full(self, uri).await
    }

    /// Requests semantic tokens for a sub-range.
    pub async fn semantic_tokens_range(
        &self,
        uri: &Url,
        range: LspRange,
    ) -> Result<Option<lsp_types::SemanticTokensRangeResult>> {
        features::semantic_tokens::range(self, uri, range).await
    }

    /// Requests semantic tokens delta from `previous_result_id`.
    pub async fn semantic_tokens_delta(
        &self,
        uri: &Url,
        previous_result_id: String,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        features::semantic_tokens::delta(self, uri, previous_result_id).await
    }

    /// Requests inlay hints (inline type and parameter annotations).
    pub async fn inlay_hints(&self, uri: &Url, range: LspRange) -> Result<Option<Vec<InlayHint>>> {
        features::inlay_hints::request(self, uri, range).await
    }

    /// Resolves an inlay hint via `inlayHint/resolve`.
    pub async fn inlay_hint_resolve(&self, hint: InlayHint) -> Result<InlayHint> {
        features::inlay_hints::resolve(self, hint).await
    }

    /// Requests inline values for debugging.
    pub async fn inline_values(
        &self,
        uri: &Url,
        range: LspRange,
        context: lsp_types::InlineValueContext,
    ) -> Result<Option<Vec<InlineValue>>> {
        features::inline_values::request(self, uri, range, context).await
    }

    /// Requests monikers (cross-repository symbol identifiers).
    pub async fn moniker(&self, uri: &Url, position: Position) -> Result<Option<Vec<Moniker>>> {
        features::moniker::request(self, uri, position).await
    }

    /// Requests linked editing ranges (simultaneous multiple tag editing).
    pub async fn linked_editing_range(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<lsp_types::LinkedEditingRanges>> {
        features::linked_editing::request(self, uri, position).await
    }

    /// Prepares a call hierarchy at `position`.
    pub async fn call_hierarchy_prepare(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        features::call_hierarchy::prepare(self, uri, position).await
    }

    /// Retrieves incoming calls for a call hierarchy item.
    pub async fn call_hierarchy_incoming_calls(
        &self,
        item: CallHierarchyItem,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        features::call_hierarchy::incoming_calls(self, item).await
    }

    /// Retrieves outgoing calls for a call hierarchy item.
    pub async fn call_hierarchy_outgoing_calls(
        &self,
        item: CallHierarchyItem,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        features::call_hierarchy::outgoing_calls(self, item).await
    }

    /// Requests pull diagnostics for `uri`.
    pub async fn pull_diagnostics(
        &self,
        uri: &Url,
        previous_result_id: Option<String>,
    ) -> Result<lsp_types::DocumentDiagnosticReportResult> {
        features::pull_diagnostics::request(self, uri, previous_result_id).await
    }

    /// Requests workspace-wide pull diagnostics.
    pub async fn workspace_diagnostic(
        &self,
        previous_result_ids: Vec<PreviousResultId>,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        features::pull_diagnostics::workspace_diagnostic(self, previous_result_ids).await
    }

    /// Executes a custom workspace command on the language server.
    pub async fn execute_command(
        &self,
        command: String,
        arguments: Vec<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>> {
        features::workspace::execute_command(self, command, arguments).await
    }

    /// Resolves a `WorkspaceEdit` into atomic transactions using the negotiated encoding.
    ///
    /// Server-initiated `workspace/applyEdit` payloads and rename/code-action
    /// edits must all be converted with the same encoding that the server used
    /// to produce them; this helper centralizes that rule.
    pub async fn resolve_workspace_edit(
        &self,
        edit: &WorkspaceEdit,
        open_buffers: &HashMap<Url, Rope>,
    ) -> ResolvedWorkspaceEdit {
        let encoding = self.position_encoding().await;
        crate::handlers::resolve_workspace_edit(edit, open_buffers, encoding)
    }

    // --- Document Sync & Lifecycle ---

    /// Sends a `didOpen` notification.
    pub async fn did_open(
        &self,
        uri: &Url,
        language_id: impl Into<String>,
        text: String,
        version_tracker: &mut DocumentVersionTracker,
    ) -> Result<()> {
        let notif = build_did_open(uri, language_id, text, version_tracker);
        self.send_notification(notif).await
    }

    /// Sends a `didChange` notification.
    ///
    /// Uses the sync kind and position encoding negotiated during `initialize`,
    /// so callers never have to thread encoding through by hand.
    pub async fn did_change(
        &self,
        uri: &Url,
        doc_before: &Rope,
        changes: &ChangeSet,
        version_tracker: &mut DocumentVersionTracker,
    ) -> Result<()> {
        let (sync_kind, encoding) = {
            let caps = self.inner.capabilities.read().await;
            (caps.sync_kind(), caps.position_encoding())
        };
        let notif = build_did_change(
            uri,
            doc_before,
            changes,
            sync_kind,
            encoding,
            version_tracker,
        )?;
        self.send_notification(notif).await
    }

    /// Sends a `didChange` notification with explicit sync kind and encoding.
    ///
    /// Prefer [`LspClient::did_change`] unless you are intentionally overriding
    /// the values negotiated with the server (e.g. in tests).
    pub async fn did_change_with(
        &self,
        uri: &Url,
        doc_before: &Rope,
        changes: &ChangeSet,
        sync_kind: TextDocumentSyncKind,
        encoding: PositionEncoding,
        version_tracker: &mut DocumentVersionTracker,
    ) -> Result<()> {
        let notif = build_did_change(
            uri,
            doc_before,
            changes,
            sync_kind,
            encoding,
            version_tracker,
        )?;
        self.send_notification(notif).await
    }

    /// Sends a `didSave` notification.
    pub async fn did_save(&self, uri: &Url, text: Option<String>) -> Result<()> {
        let notif = build_did_save(uri, text);
        self.send_notification(notif).await
    }

    /// Sends a `didClose` notification.
    pub async fn did_close(
        &self,
        uri: &Url,
        version_tracker: &mut DocumentVersionTracker,
    ) -> Result<()> {
        let notif = build_did_close(uri, version_tracker);
        self.send_notification(notif).await
    }

    // --- Workspace Management & File Operations ---

    /// Notifies the server of added or removed workspace root folders.
    pub async fn did_change_workspace_folders(
        &self,
        added: Vec<WorkspaceFolder>,
        removed: Vec<WorkspaceFolder>,
    ) -> Result<()> {
        features::workspace::did_change_workspace_folders(self, added, removed).await
    }

    /// Notifies the server of editor configuration changes.
    pub async fn did_change_configuration(&self, settings: serde_json::Value) -> Result<()> {
        features::workspace::did_change_configuration(self, settings).await
    }

    /// Notifies the server of file system changes matching active watchers.
    pub async fn did_change_watched_files(&self, changes: Vec<FileEvent>) -> Result<()> {
        let params = DidChangeWatchedFilesParams { changes };
        let notif = JsonRpcNotification::new(
            "workspace/didChangeWatchedFiles",
            Some(serde_json::to_value(params).map_err(LspError::Json)?),
        );
        self.send_notification(notif).await
    }

    /// Asks the server for workspace edits prior to creating files (`workspace/willCreateFiles`).
    pub async fn will_create_files(&self, files: Vec<FileCreate>) -> Result<Option<WorkspaceEdit>> {
        features::file_operations::will_create_files(self, files).await
    }

    /// Notifies the server that files were created (`workspace/didCreateFiles`).
    pub async fn did_create_files(&self, files: Vec<FileCreate>) -> Result<()> {
        features::file_operations::did_create_files(self, files).await
    }

    /// Asks the server for workspace edits prior to renaming files (`workspace/willRenameFiles`).
    pub async fn will_rename_files(&self, files: Vec<FileRename>) -> Result<Option<WorkspaceEdit>> {
        features::file_operations::will_rename_files(self, files).await
    }

    /// Notifies the server that files were renamed (`workspace/didRenameFiles`).
    pub async fn did_rename_files(&self, files: Vec<FileRename>) -> Result<()> {
        features::file_operations::did_rename_files(self, files).await
    }

    /// Asks the server for workspace edits prior to deleting files (`workspace/willDeleteFiles`).
    pub async fn will_delete_files(&self, files: Vec<FileDelete>) -> Result<Option<WorkspaceEdit>> {
        features::file_operations::will_delete_files(self, files).await
    }

    /// Notifies the server that files were deleted (`workspace/didDeleteFiles`).
    pub async fn did_delete_files(&self, files: Vec<FileDelete>) -> Result<()> {
        features::file_operations::did_delete_files(self, files).await
    }

    /// Initiates a graceful shutdown of the server subprocess (`shutdown` request -> wait -> `exit` notification).
    pub async fn shutdown(&self) -> Result<()> {
        let (responder, rx) = oneshot::channel();
        let id = self.inner.id_gen.next();
        self.inner
            .cmd_tx
            .send(LspCommand::Shutdown { id, responder })
            .await
            .map_err(|_| LspError::ChannelClosed)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(LspError::ChannelClosed),
            Err(_) => {
                // Hard timeout expired waiting for shutdown response; force exit notification
                let notif = JsonRpcNotification::new("exit", None);
                let _ = self.send_notification(notif).await;
                Ok(())
            }
        }
    }

    /// Sends the fire-and-forget `exit` notification to the server.
    pub async fn exit(&self) -> Result<()> {
        let notif = JsonRpcNotification::new("exit", None);
        self.send_notification(notif).await
    }

    /// Shuts down the language server cleanly and sends the exit notification.
    pub async fn shutdown_and_exit(&self) -> Result<()> {
        self.shutdown().await
    }
}
