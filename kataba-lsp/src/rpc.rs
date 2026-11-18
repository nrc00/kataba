//! JSON-RPC 2.0 message protocol types and correlation tracking.

use crate::error::{LspError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::sync::oneshot;

/// A JSON-RPC 2.0 request identifier (integer or string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// Integer identifier.
    Number(i64),
    /// String identifier.
    String(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::String(s) => write!(f, "\"{s}\""),
        }
    }
}

/// A client-to-server or server-to-client JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version string, must be "2.0".
    pub jsonrpc: String,
    /// Request correlation ID.
    pub id: RequestId,
    /// LSP method name (e.g. "textDocument/completion").
    pub method: String,
    /// Optional parameter payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Creates a new JSON-RPC request.
    pub fn new(
        id: RequestId,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 Notification (fire-and-forget, no ID, no response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// Protocol version string, must be "2.0".
    pub jsonrpc: String,
    /// Notification method name.
    pub method: String,
    /// Optional parameter payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    /// Creates a new JSON-RPC notification.
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 Response Error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i64,
    /// Human-readable message.
    pub message: String,
    /// Optional structured error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 Response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Protocol version string, must be "2.0".
    pub jsonrpc: String,
    /// Request correlation ID matching the original request.
    pub id: Option<RequestId>,
    /// Result payload (on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error payload (on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Inbound wire message demultiplexed from server stdout.
#[derive(Debug)]
pub enum InboundMessage {
    /// Response to a client request.
    Response(JsonRpcResponse),
    /// Server-pushed notification (diagnostics, progress, etc.).
    Notification(JsonRpcNotification),
    /// Server-initiated request (workspace/applyEdit, etc.).
    Request(JsonRpcRequest),
}

/// Deserializes raw frame bytes into an [`InboundMessage`].
pub fn parse_inbound_message(bytes: &[u8]) -> Result<InboundMessage> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(LspError::Json)?;

    if !value.is_object() {
        return Err(LspError::Framing(
            "JSON-RPC message must be a JSON object".to_string(),
        ));
    }

    let obj = match value.as_object() {
        Some(o) => o,
        None => return Err(LspError::Framing("Expected JSON object".to_string())),
    };

    let has_id = obj.contains_key("id");
    let has_method = obj.contains_key("method");

    if has_method {
        if has_id {
            let req: JsonRpcRequest = serde_json::from_value(value).map_err(LspError::Json)?;
            Ok(InboundMessage::Request(req))
        } else {
            let notif: JsonRpcNotification =
                serde_json::from_value(value).map_err(LspError::Json)?;
            Ok(InboundMessage::Notification(notif))
        }
    } else {
        let resp: JsonRpcResponse = serde_json::from_value(value).map_err(LspError::Json)?;
        Ok(InboundMessage::Response(resp))
    }
}

/// Thread-safe monotonic generator for integer request IDs.
#[derive(Debug, Default)]
pub struct RequestIdGenerator {
    counter: AtomicI64,
}

impl RequestIdGenerator {
    /// Creates a new generator starting from ID 1.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counter: AtomicI64::new(1),
        }
    }

    /// Generates the next sequential `RequestId`.
    pub fn next(&self) -> RequestId {
        let id = self.counter.fetch_add(1, Ordering::SeqCst);
        RequestId::Number(id)
    }
}

/// Registry of pending in-flight requests waiting for server responses.
#[derive(Default)]
pub struct PendingRequests {
    pending: HashMap<RequestId, oneshot::Sender<Result<serde_json::Value>>>,
}

impl PendingRequests {
    /// Registers a pending request with its response channel.
    pub fn insert(&mut self, id: RequestId, sender: oneshot::Sender<Result<serde_json::Value>>) {
        self.pending.insert(id, sender);
    }

    /// Resolves a response by matching its ID and dispatching to the waiting channel.
    pub fn complete(&mut self, id: &RequestId, response: Result<serde_json::Value>) -> bool {
        if let Some(sender) = self.pending.remove(id) {
            let _ = sender.send(response);
            true
        } else {
            false
        }
    }

    /// Cancels an in-flight request by ID.
    pub fn cancel(&mut self, id: &RequestId) -> bool {
        if let Some(sender) = self.pending.remove(id) {
            let _ = sender.send(Err(LspError::Cancelled));
            true
        } else {
            false
        }
    }
}
