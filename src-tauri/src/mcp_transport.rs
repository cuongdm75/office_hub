// ============================================================================
// Office Hub – mcp_transport.rs
//
// MCP-Hybrid Transport Layer: simplified Model Context Protocol structs
// used by both the SSE downlink and REST uplink.
//
// Downlink (Server → Mobile): SseEvent streamed via GET /api/v1/stream
// Uplink   (Mobile → Server): MobileCommand / McpToolCall via POST endpoints
// Data Plane                : McpResource URIs carried via SSE, actual bytes
//                             streamed via /api/v1/files/*
// ============================================================================

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Uplink: Mobile → Server
// ─────────────────────────────────────────────────────────────────────────────

/// A chat/voice command sent from the Mobile client via POST /api/v1/command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileCommand {
    /// Unique ID for correlation (echoed back in SseEvent.call_id).
    pub command_id: String,
    /// Continue an existing session, or None to start a new one.
    pub session_id: Option<String>,
    /// The user text.
    pub text: String,
    /// Optional structured context (e.g. attached file metadata).
    pub context: Option<Value>,
}

/// A structured tool invocation following simplified MCP conventions.
/// Sent via POST /api/v1/tool_call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    /// Correlation ID — returned in the SseEvent result.
    pub call_id: String,
    /// Name of the tool to invoke (e.g. "format_excel_cell", "create_word_doc").
    pub tool_name: String,
    /// Named arguments for the tool.
    pub arguments: serde_json::Map<String, Value>,
}

/// Authentication request (POST /api/v1/auth).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub token: String,
}

/// Authentication response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub ok: bool,
    pub message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Downlink: Server → Mobile (SSE payload)
// ─────────────────────────────────────────────────────────────────────────────

/// Discriminator for SSE event types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SseEventType {
    /// Agent/workflow status update (running, completed, failed …).
    Status,
    /// Informational log line (droppable under backpressure).
    Log,
    /// Final task result — never dropped under backpressure.
    Result,
    /// Real-time LLM thought / streaming token (droppable).
    Progress,
    /// Human-in-the-Loop approval request — never dropped.
    ApprovalRequest,
    /// Error from server — never dropped.
    Error,
    /// Heartbeat (sent automatically every 15 s, never stored).
    Heartbeat,
    /// Session list response.
    SessionList,
    /// Single session history response.
    SessionHistory,
}

impl SseEventType {
    /// Returns true for high-priority events that must NOT be dropped under
    /// backpressure (slow-consumer protection).
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            SseEventType::Result
                | SseEventType::ApprovalRequest
                | SseEventType::Error
                | SseEventType::SessionList
                | SseEventType::SessionHistory
        )
    }
}

/// A single event pushed from the server to the mobile client via SSE.
///
/// Wire format (JSON):
/// ```text
/// event: result
/// data: {"event_type":"result","call_id":"abc","payload":{...}}
///
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    pub event_type: SseEventType,
    /// Echoed `command_id` / `call_id` for client-side correlation.
    pub call_id: Option<String>,
    /// The main payload — structure depends on `event_type`.
    pub payload: Value,
}

impl SseEvent {
    // ── Constructors ─────────────────────────────────────────────────────────

    pub fn status(call_id: Option<String>, run_id: &str, name: &str, status: &str, message: Option<String>) -> Self {
        Self {
            event_type: SseEventType::Status,
            call_id,
            payload: serde_json::json!({
                "run_id": run_id,
                "name": name,
                "status": status,
                "message": message,
                "updated_at": chrono::Utc::now().to_rfc3339(),
            }),
        }
    }

    pub fn log(text: impl Into<String>) -> Self {
        Self {
            event_type: SseEventType::Log,
            call_id: None,
            payload: serde_json::json!({ "text": text.into(), "ts": chrono::Utc::now().to_rfc3339() }),
        }
    }

    pub fn progress(call_id: Option<String>, session_id: &str, thought: &str) -> Self {
        Self {
            event_type: SseEventType::Progress,
            call_id,
            payload: serde_json::json!({ "session_id": session_id, "thought": thought }),
        }
    }

    pub fn result(call_id: Option<String>, session_id: &str, content: &str, intent: Option<&str>, agent_used: Option<&str>, metadata: Option<Value>) -> Self {
        Self {
            event_type: SseEventType::Result,
            call_id,
            payload: serde_json::json!({
                "session_id": session_id,
                "content": content,
                "intent": intent,
                "agent_used": agent_used,
                "metadata": metadata,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        }
    }

    pub fn error(call_id: Option<String>, code: &str, message: &str) -> Self {
        Self {
            event_type: SseEventType::Error,
            call_id,
            payload: serde_json::json!({ "error_code": code, "message": message }),
        }
    }

    pub fn approval_request(action_id: &str, description: &str, risk_level: &str, payload: Option<Value>, timeout_seconds: u64) -> Self {
        Self {
            event_type: SseEventType::ApprovalRequest,
            call_id: Some(action_id.to_string()),
            payload: serde_json::json!({
                "action_id": action_id,
                "description": description,
                "risk_level": risk_level,
                "payload": payload,
                "timeout_seconds": timeout_seconds,
                "requested_at": chrono::Utc::now().to_rfc3339(),
            }),
        }
    }

    pub fn session_list(sessions: Vec<Value>) -> Self {
        Self {
            event_type: SseEventType::SessionList,
            call_id: None,
            payload: serde_json::json!({ "sessions": sessions }),
        }
    }

    pub fn session_history(session_id: &str, messages: Vec<Value>) -> Self {
        Self {
            event_type: SseEventType::SessionHistory,
            call_id: Some(session_id.to_string()),
            payload: serde_json::json!({ "session_id": session_id, "messages": messages }),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Plane: Files as MCP Resources
// ─────────────────────────────────────────────────────────────────────────────

/// A file treated as an MCP Resource.
/// The URI is pushed via SSE; the client downloads bytes via REST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// Addressable URI, e.g. `office-hub://files/report_abc123.docx`
    pub uri: String,
    /// MIME type of the resource.
    pub mime_type: String,
    /// Arbitrary key-value metadata (filename, size, created_at …).
    pub metadata: HashMap<String, String>,
}

impl McpResource {
    pub fn for_file(id: &str, filename: &str, mime_type: &str, size_bytes: u64) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), filename.to_string());
        metadata.insert("size_bytes".to_string(), size_bytes.to_string());
        metadata.insert("created_at".to_string(), chrono::Utc::now().to_rfc3339());
        Self {
            uri: format!("office-hub://files/{}", id),
            mime_type: mime_type.to_string(),
            metadata,
        }
    }

    /// HTTP download path derived from URI.
    pub fn download_path(&self) -> Option<String> {
        self.uri
            .strip_prefix("office-hub://files/")
            .map(|id| format!("/api/v1/files/download/{}", id))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal command routed from REST handler → Orchestrator worker
// ─────────────────────────────────────────────────────────────────────────────

/// Decoded and authenticated mobile command, ready for the orchestrator.
#[derive(Debug, Clone)]
pub struct IncomingMobileCmd {
    /// Echoed to SSE responses for client-side correlation.
    pub command_id: String,
    pub session_id: String,
    pub text: String,
    pub context_file_path: Option<String>,
    pub received_at: chrono::DateTime<chrono::Utc>,
}
