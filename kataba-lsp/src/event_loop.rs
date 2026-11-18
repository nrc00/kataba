//! LSP event loop actor: processes inbound server messages and outbound client commands.
//!
//! The loop is split into two tasks:
//!
//! * the **read/command task** (this module) owns `stdout` and the command queue,
//!   and never blocks on writing;
//! * the **writer task** owns `stdin` and drains a bounded frame queue.
//!
//! Writing from the read task directly (as a single `select!` loop would) is a
//! deadlock hazard: language servers routinely stop reading stdin while they
//! index a workspace, and a blocked `write_all` would also stop the client from
//! reading responses. Queueing frames decouples the two directions.

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;
use tokio_util::codec::{Encoder, FramedRead};
use tracing::warn;

use crate::error::{LspError, Result};
use crate::framing::LspCodec;
use crate::handlers::{DiagnosticStore, HandlerError, LspHandlers, ProgressStore};
use crate::rpc::{
    parse_inbound_message, InboundMessage, JsonRpcNotification, JsonRpcRequest, PendingRequests,
    RequestId,
};
use crate::state::{CapabilityStore, ClientState};
use crate::transport::ProcessTransport;
use lsp_types::{
    ApplyWorkspaceEditParams, ConfigurationParams, DidChangeConfigurationParams,
    DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams, ExecuteCommandParams,
    LogMessageParams, ProgressParams, PublishDiagnosticsParams, RegistrationParams,
    ShowDocumentParams, ShowMessageParams, ShowMessageRequestParams, UnregistrationParams,
    WorkDoneProgressCreateParams,
};
use std::sync::Arc;
use std::time::Duration;

/// Capacity of the outbound frame queue.
///
/// Large enough that normal bursts (completion, `didChange`, server-request
/// replies) never block the read loop, small enough to apply backpressure
/// instead of buffering unbounded memory.
const OUTBOUND_QUEUE_CAPACITY: usize = 256;

/// Grace period for flushing queued frames during shutdown.
const OUTBOUND_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// Commands sent from client handles to the background event loop actor.
pub enum LspCommand {
    /// Outgoing request expecting a response.
    Request {
        /// Correlation ID.
        id: RequestId,
        /// Method name.
        method: String,
        /// Parameter payload.
        params: Option<serde_json::Value>,
        /// Response sender.
        responder: oneshot::Sender<Result<serde_json::Value>>,
    },
    /// Outgoing fire-and-forget notification.
    Notification {
        /// Notification payload.
        notification: JsonRpcNotification,
    },
    /// Request cancellation.
    Cancel {
        /// ID of request to cancel.
        id: RequestId,
    },
    /// Clean shutdown command.
    Shutdown {
        /// Correlation ID for shutdown request.
        id: RequestId,
        /// Responder notified when shutdown request response arrives and exit is sent.
        responder: oneshot::Sender<Result<()>>,
    },
}

/// Dedicated writer task owning the server's stdin.
struct OutboundWriter {
    tx: mpsc::Sender<Bytes>,
    handle: JoinHandle<()>,
}

impl OutboundWriter {
    fn spawn(mut stdin: ChildStdin) -> Self {
        let (tx, mut rx) = mpsc::channel::<Bytes>(OUTBOUND_QUEUE_CAPACITY);
        let handle = tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                if let Err(err) = stdin.write_all(&frame).await {
                    warn!(target: "lsp::write", error = %err, "failed to write LSP frame");
                    return;
                }
                if let Err(err) = stdin.flush().await {
                    warn!(target: "lsp::write", error = %err, "failed to flush LSP frame");
                    return;
                }
            }
        });
        Self { tx, handle }
    }

    async fn send(&self, frame: Bytes) -> Result<()> {
        self.tx
            .send(frame)
            .await
            .map_err(|_| LspError::ChannelClosed)
    }

    /// Closes the queue and waits for the writer to flush what is left.
    async fn close(self) {
        let Self { tx, handle } = self;
        drop(tx);
        if tokio::time::timeout(OUTBOUND_FLUSH_TIMEOUT, handle)
            .await
            .is_err()
        {
            warn!(target: "lsp::write", "timed out flushing queued LSP frames");
        }
    }
}

/// The background event loop driver task.
pub struct LspEventLoop {
    reader: FramedRead<ChildStdout, LspCodec>,
    /// Holds the child process alive for as long as the loop runs.
    _child: Child,
    outbound: OutboundWriter,
    state: ClientState,
    capabilities: Arc<RwLock<CapabilityStore>>,
    pending: PendingRequests,
    cmd_rx: mpsc::Receiver<LspCommand>,
    diagnostics: DiagnosticStore,
    progress: ProgressStore,
    handlers: Arc<dyn LspHandlers>,
    shutdown_id: Option<RequestId>,
    shutdown_responder: Option<oneshot::Sender<Result<()>>>,
}

impl LspEventLoop {
    /// Spawns the background event loop task.
    pub fn spawn(
        transport: ProcessTransport,
        cmd_rx: mpsc::Receiver<LspCommand>,
        diagnostics: DiagnosticStore,
        progress: ProgressStore,
        capabilities: Arc<RwLock<CapabilityStore>>,
        handlers: Arc<dyn LspHandlers>,
    ) {
        let (reader, stdin, child) = transport.into_parts();
        let event_loop = Self {
            reader,
            _child: child,
            outbound: OutboundWriter::spawn(stdin),
            state: ClientState::Uninitialized,
            capabilities,
            pending: PendingRequests::default(),
            cmd_rx,
            diagnostics,
            progress,
            handlers,
            shutdown_id: None,
            shutdown_responder: None,
        };

        tokio::spawn(async move {
            if let Err(err) = event_loop.run().await {
                warn!(target: "lsp::event_loop", error = %err, "LSP event loop terminated with error");
            }
        });
    }

    /// Runs the main select loop until the connection terminates.
    pub async fn run(mut self) -> Result<()> {
        let result = self.run_inner().await;

        // Flush anything queued (notably the `exit` notification) *before* the
        // child handle is dropped, since `kill_on_drop` would otherwise cut the
        // server off mid-write.
        let Self {
            outbound, _child, ..
        } = self;
        outbound.close().await;
        drop(_child);

        result
    }

    async fn run_inner(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                // 1. Inbound messages from server stdout. The read task never
                //    awaits the writer, so a busy server cannot stall us here.
                frame_res = self.reader.next() => {
                    match frame_res {
                        Some(Ok(bytes)) => {
                            if self.handle_inbound_frame(&bytes).await? {
                                break;
                            }
                        }
                        Some(Err(err)) => return Err(err),
                        None => {
                            // Server stdout closed (EOF)
                            self.state = ClientState::Terminated;
                            break;
                        }
                    }
                }

                // 2. Outbound commands from LspClient handles
                cmd_opt = self.cmd_rx.recv() => {
                    match cmd_opt {
                        Some(cmd) => {
                            if self.handle_client_command(cmd).await? {
                                break;
                            }
                        }
                        None => {
                            // All client handles dropped; send exit if ready then exit
                            if self.state == ClientState::Ready {
                                let exit_notif = JsonRpcNotification::new("exit", None);
                                if let Ok(json_str) = serde_json::to_string(&exit_notif) {
                                    let _ = self.send_json(&json_str).await;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Encodes one JSON message and hands it to the writer task.
    async fn send_json(&self, json: &str) -> Result<()> {
        let mut buf = BytesMut::with_capacity(json.len() + 64);
        LspCodec::new().encode(json, &mut buf)?;
        self.outbound.send(buf.freeze()).await
    }

    async fn handle_inbound_frame(&mut self, bytes: &[u8]) -> Result<bool> {
        let inbound = parse_inbound_message(bytes)?;
        match inbound {
            InboundMessage::Response(resp) => {
                if let Some(id) = resp.id {
                    // Check if this response correlates to the active shutdown request
                    if self.shutdown_id.as_ref() == Some(&id) {
                        // 1. Send exit notification to server
                        let exit_notif = JsonRpcNotification::new("exit", None);
                        let exit_str = serde_json::to_string(&exit_notif)?;
                        let _ = self.send_json(&exit_str).await;

                        self.state = ClientState::Terminated;

                        // 2. Notify shutdown responder
                        if let Some(responder) = self.shutdown_responder.take() {
                            let _ = responder.send(Ok(()));
                        }
                        return Ok(true);
                    }

                    let result = match resp.error {
                        Some(err) => Err(LspError::Protocol {
                            code: err.code,
                            message: err.message,
                            data: err.data,
                        }),
                        None => {
                            let val = resp.result.unwrap_or(serde_json::Value::Null);
                            if let Ok(init_res) =
                                serde_json::from_value::<lsp_types::InitializeResult>(val.clone())
                            {
                                self.capabilities.write().await.set(init_res.capabilities);
                                self.state = ClientState::Ready;
                            }
                            Ok(val)
                        }
                    };
                    self.pending.complete(&id, result);
                }
            }
            InboundMessage::Notification(notif) => match notif.method.as_str() {
                "textDocument/publishDiagnostics" => {
                    if let Some(params) = notif.params {
                        if let Ok(diag_params) =
                            serde_json::from_value::<PublishDiagnosticsParams>(params)
                        {
                            self.diagnostics.update(diag_params.clone()).await;
                            self.handlers.on_diagnostics(diag_params);
                        }
                    }
                }
                "$/progress" => {
                    if let Some(params) = notif.params {
                        if let Ok(prog_params) = serde_json::from_value::<ProgressParams>(params) {
                            self.progress.handle(prog_params.clone()).await;
                            self.handlers.on_progress(prog_params);
                        }
                    }
                }
                "window/logMessage" => {
                    if let Some(params) = notif.params {
                        if let Ok(log_params) = serde_json::from_value::<LogMessageParams>(params) {
                            self.handlers.on_log_message(log_params);
                        }
                    }
                }
                "window/showMessage" => {
                    if let Some(params) = notif.params {
                        if let Ok(show_params) = serde_json::from_value::<ShowMessageParams>(params)
                        {
                            self.handlers.on_show_message(show_params);
                        }
                    }
                }
                "workspace/didChangeWorkspaceFolders" => {
                    if let Some(params) = notif.params {
                        if let Ok(folder_params) =
                            serde_json::from_value::<DidChangeWorkspaceFoldersParams>(params)
                        {
                            self.handlers.on_workspace_folders_change(folder_params);
                        }
                    }
                }
                "workspace/didChangeConfiguration" => {
                    if let Some(params) = notif.params {
                        if let Ok(config_params) =
                            serde_json::from_value::<DidChangeConfigurationParams>(params)
                        {
                            self.handlers.on_config_change(config_params);
                        }
                    }
                }
                "workspace/didChangeWatchedFiles" => {
                    if let Some(params) = notif.params {
                        if let Ok(watch_params) =
                            serde_json::from_value::<DidChangeWatchedFilesParams>(params)
                        {
                            self.handlers.on_watched_files_change(watch_params);
                        }
                    }
                }
                "workspace/diagnostic/refresh" => {
                    self.handlers.on_refresh_diagnostics();
                }
                "workspace/inlayHint/refresh" => {
                    self.handlers.on_refresh_inlay_hints();
                }
                "workspace/codeLens/refresh" => {
                    self.handlers.on_refresh_code_lenses();
                }
                "workspace/semanticTokens/refresh" => {
                    self.handlers.on_refresh_semantic_tokens();
                }
                "workspace/inlineValue/refresh" => {
                    self.handlers.on_refresh_inline_values();
                }
                _ => {}
            },
            InboundMessage::Request(req) => {
                // Server-initiated request handling compliant with JSON-RPC 2.0
                let resp_result: std::result::Result<serde_json::Value, HandlerError> = match req
                    .method
                    .as_str()
                {
                    "workspace/configuration" => {
                        if let Some(params) = req.params {
                            match serde_json::from_value::<ConfigurationParams>(params) {
                                Ok(config_params) => {
                                    let configs = self.handlers.on_configuration(config_params);
                                    Ok(serde_json::to_value(configs)
                                        .unwrap_or(serde_json::Value::Null))
                                }
                                Err(e) => Err(HandlerError {
                                    code: -32602,
                                    message: format!("Invalid ConfigurationParams: {}", e),
                                    data: None,
                                }),
                            }
                        } else {
                            Ok(serde_json::json!([]))
                        }
                    }
                    "workspace/applyEdit" => {
                        if let Some(params) = req.params {
                            match serde_json::from_value::<ApplyWorkspaceEditParams>(params) {
                                Ok(apply_params) => {
                                    let result = self.handlers.on_apply_edit(apply_params);
                                    Ok(serde_json::to_value(result)
                                        .unwrap_or(serde_json::json!({"applied": false})))
                                }
                                Err(e) => Err(HandlerError {
                                    code: -32602,
                                    message: format!("Invalid ApplyWorkspaceEditParams: {}", e),
                                    data: None,
                                }),
                            }
                        } else {
                            Ok(serde_json::json!({"applied": false}))
                        }
                    }
                    "workspace/executeCommand" => {
                        if let Some(params) = req.params {
                            match serde_json::from_value::<ExecuteCommandParams>(params) {
                                Ok(exec_params) => self
                                    .handlers
                                    .on_execute_command(exec_params)
                                    .map(|opt| opt.unwrap_or(serde_json::Value::Null)),
                                Err(e) => Err(HandlerError {
                                    code: -32602,
                                    message: format!("Invalid ExecuteCommandParams: {}", e),
                                    data: None,
                                }),
                            }
                        } else {
                            Err(HandlerError {
                                code: -32602,
                                message: "Missing params for workspace/executeCommand".to_string(),
                                data: None,
                            })
                        }
                    }
                    "workspace/workspaceFolders" => {
                        let folders = self.handlers.on_workspace_folders();
                        Ok(serde_json::to_value(folders).unwrap_or(serde_json::json!([])))
                    }
                    "window/workDoneProgress/create" => {
                        if let Some(params) = req.params {
                            if let Ok(create_params) =
                                serde_json::from_value::<WorkDoneProgressCreateParams>(params)
                            {
                                self.handlers.on_work_done_progress_create(create_params);
                            }
                        }
                        Ok(serde_json::Value::Null)
                    }
                    "window/showDocument" => {
                        if let Some(params) = req.params {
                            if let Ok(show_params) =
                                serde_json::from_value::<ShowDocumentParams>(params)
                            {
                                let res = self.handlers.on_show_document(show_params);
                                Ok(serde_json::to_value(res)
                                    .unwrap_or(serde_json::json!({"success": false})))
                            } else {
                                Ok(serde_json::json!({"success": false}))
                            }
                        } else {
                            Ok(serde_json::json!({"success": false}))
                        }
                    }
                    "window/showMessageRequest" => {
                        if let Some(params) = req.params {
                            if let Ok(msg_params) =
                                serde_json::from_value::<ShowMessageRequestParams>(params)
                            {
                                let item = self.handlers.on_show_message_request(msg_params);
                                Ok(serde_json::to_value(item).unwrap_or(serde_json::Value::Null))
                            } else {
                                Ok(serde_json::Value::Null)
                            }
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    "client/registerCapability" => {
                        if let Some(params) = req.params {
                            if let Ok(reg_params) =
                                serde_json::from_value::<RegistrationParams>(params)
                            {
                                let mut caps = self.capabilities.write().await;
                                for reg in &reg_params.registrations {
                                    caps.register(reg.clone());
                                }
                                drop(caps);
                                self.handle_register_capability(reg_params).await;
                            }
                        }
                        Ok(serde_json::Value::Null)
                    }
                    "client/unregisterCapability" => {
                        if let Some(params) = req.params {
                            if let Ok(unreg_params) =
                                serde_json::from_value::<UnregistrationParams>(params)
                            {
                                let mut caps = self.capabilities.write().await;
                                for unreg in &unreg_params.unregisterations {
                                    caps.unregister(&unreg.id);
                                }
                                drop(caps);
                                self.handle_unregister_capability(unreg_params).await;
                            }
                        }
                        Ok(serde_json::Value::Null)
                    }
                    "workspace/diagnostic/refresh" => {
                        self.handlers.on_refresh_diagnostics();
                        Ok(serde_json::Value::Null)
                    }
                    "workspace/inlayHint/refresh" => {
                        self.handlers.on_refresh_inlay_hints();
                        Ok(serde_json::Value::Null)
                    }
                    "workspace/codeLens/refresh" => {
                        self.handlers.on_refresh_code_lenses();
                        Ok(serde_json::Value::Null)
                    }
                    "workspace/semanticTokens/refresh" => {
                        self.handlers.on_refresh_semantic_tokens();
                        Ok(serde_json::Value::Null)
                    }
                    "workspace/inlineValue/refresh" => {
                        self.handlers.on_refresh_inline_values();
                        Ok(serde_json::Value::Null)
                    }
                    _ => Err(HandlerError {
                        code: -32601,
                        message: format!("Method '{}' not implemented", req.method),
                        data: None,
                    }),
                };

                let response_json = match resp_result {
                    Ok(val) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req.id,
                        "result": val
                    }),
                    Err(err) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req.id,
                        "error": {
                            "code": err.code,
                            "message": err.message,
                            "data": err.data
                        }
                    }),
                };

                self.send_json(&response_json.to_string()).await?;
            }
        }
        Ok(false)
    }

    async fn handle_register_capability(&mut self, params: RegistrationParams) {
        for reg in &params.registrations {
            self.handlers.on_register_capability(RegistrationParams {
                registrations: vec![reg.clone()],
            });
        }
    }

    async fn handle_unregister_capability(&mut self, params: UnregistrationParams) {
        self.handlers.on_unregister_capability(params);
    }

    async fn handle_client_command(&mut self, cmd: LspCommand) -> Result<bool> {
        match cmd {
            LspCommand::Request {
                id,
                method,
                params,
                responder,
            } => {
                // Register the correlation slot before enqueueing the frame so a
                // response can never arrive without a waiting receiver.
                self.pending.insert(id.clone(), responder);

                let req = JsonRpcRequest::new(id.clone(), method, params);
                let json_str = serde_json::to_string(&req)?;

                if let Err(err) = self.send_json(&json_str).await {
                    self.pending.cancel(&id);
                    return Err(err);
                }
            }
            LspCommand::Notification { notification } => {
                let json_str = serde_json::to_string(&notification)?;
                self.send_json(&json_str).await?;
            }
            LspCommand::Cancel { id } => {
                self.pending.cancel(&id);

                let cancel_params = serde_json::json!({ "id": id });
                let notif = JsonRpcNotification::new("$/cancelRequest", Some(cancel_params));
                let json_str = serde_json::to_string(&notif)?;
                let _ = self.send_json(&json_str).await;
            }
            LspCommand::Shutdown { id, responder } => {
                // Send shutdown request and save correlation ID to wait for server response
                self.shutdown_id = Some(id.clone());
                self.shutdown_responder = Some(responder);
                self.state = ClientState::ShuttingDown;

                let req = JsonRpcRequest::new(id, "shutdown", None);
                let json_str = serde_json::to_string(&req)?;

                if let Err(err) = self.send_json(&json_str).await {
                    if let Some(responder) = self.shutdown_responder.take() {
                        let _ = responder.send(Err(err));
                    }
                }
            }
        }
        Ok(false)
    }
}
