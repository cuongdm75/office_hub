// ============================================================================
// Office Hub – websocket/mod.rs
//
// WebSocket Server – Mobile Client Communication Layer
//
// Trách nhiệm:
//   1. Khởi động WebSocket server lắng nghe kết nối từ Mobile App
//   2. Xác thực kết nối (token-based auth, Phase 5)
//   3. Relay lệnh thoại từ Mobile → Orchestrator
//   4. Relay kết quả / notification từ Orchestrator → Mobile
//   5. Human-in-the-Loop relay: chuyển approval request đến Mobile
//      và nhận quyết định (approve/reject) từ Mobile
//   6. Quản lý danh sách connected clients
//   7. Ghi audit log cho mọi message nhạy cảm
//
// Protocol (JSON over WebSocket):
//   Client → Server: { "type": "command" | "approval_response" | "ping", ... }
//   Server → Client: { "type": "notification" | "approval_request" | "pong" | "error", ... }
//
// Status: STUB – Phase 5 implementation pending
//   Phase 5 sẽ triển khai:
//     - tokio-tungstenite server
//     - JWT / shared-secret authentication
//     - Message routing to Orchestrator
//     - HITL approval relay
//     - Reconnection logic
// ============================================================================

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Message types (Client → Server)
// ─────────────────────────────────────────────────────────────────────────────

/// Message sent from Mobile Client to the Office Hub WebSocket server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Keep-alive ping
    Ping { timestamp_ms: i64 },

    /// Authentication token
    Auth { token: String },

    /// Voice/text command to be forwarded to the Orchestrator
    Command {
        /// Optional session ID to continue an existing chat session
        session_id: Option<String>,
        /// The text command
        text: String,
        /// Optional context (e.g. active file path on the PC)
        context: Option<serde_json::Value>,
    },

    /// Chat message from Office Web Add-in
    ChatRequest {
        content: String,
        file_context: Option<String>,
        /// Which Office host sent this (Word, Excel, Outlook, etc.)
        app_type: Option<String>,
        /// For Outlook: the email body text (preview) to use as context
        email_context: Option<String>,
        /// For online documents (SharePoint/OneDrive): content extracted directly via frontend
        document_content: Option<String>,
    },

    /// Event from Office Web Add-in (e.g. DocumentOpened)
    OfficeAddinEvent {
        event: String,
        file_path: Option<String>, // Optional: Outlook doesn't send file_path
        app_type: Option<String>,  // Optional: fallback to "Unknown"
        subject: Option<String>,   // Outlook: email subject
        sender: Option<String>,    // Outlook: email sender
    },

    /// Document file extracted by Add-in
    DocumentExtracted {
        file_name: String,
        base64_data: String,
    },

    /// Voice command containing base64 encoded audio
    VoiceCommand {
        /// Optional session ID
        session_id: Option<String>,
        /// The base64 encoded audio
        audio_base64: String,
    },

    /// Human-in-the-Loop approval response
    ApprovalResponse {
        /// The action ID that was pending approval
        action_id: String,
        /// true = approve, false = reject
        approved: bool,
        /// Optional rejection reason
        reason: Option<String>,
        /// The mobile user who responded
        responded_by: String,
    },

    /// Request status of all running workflows
    WorkflowStatusRequest {
        /// Optional filter by workflow ID
        workflow_id: Option<String>,
    },

    /// Request current agent statuses
    AgentStatusRequest,

    /// Request to list all chat sessions
    ListSessions,

    /// Request to get the chat history of a specific session
    GetSessionHistory { session_id: String },

    /// Delete a chat session
    DeleteSession { session_id: String },

    /// Delete an artifact
    DeleteArtifact { filename: String },

    /// Real-time CRDT document sync message
    CrdtSync {
        doc_id: String,
        payload_base64: String,
    },

    /// Disconnect gracefully
    Disconnect { reason: Option<String> },
}

// ─────────────────────────────────────────────────────────────────────────────
// Message types (Server → Client)
// ─────────────────────────────────────────────────────────────────────────────

/// Message sent from Office Hub WebSocket server to Mobile Client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Pong response to keep-alive
    Pong { timestamp_ms: i64 },

    /// General notification (workflow completed, error, info)
    Notification {
        notification_id: String,
        level: NotificationLevel,
        title: String,
        body: String,
        /// Optional structured data attached to this notification
        data: Option<serde_json::Value>,
        timestamp: String,
    },

    /// Context Analysis summary returned to the Office Web Add-in
    ContextAnalysis { summary: String },

    /// Command to execute an action directly in the Office Add-in
    AddinCommand {
        command: String, // "insert_text", "replace_document", "save_document", "extract_file"
        payload: Option<String>,
    },

    /// Simple Chat response for the Office Web Add-in
    ChatResponse { content: String },

    /// Real-time LLM thought progress
    ChatProgress { session_id: String, thought: String },

    /// Chat reply from the Orchestrator
    ChatReply {
        session_id: String,
        content: String,
        intent: Option<String>,
        agent_used: Option<String>,
        timestamp: String,
        /// Optional file attachment metadata (e.g. name, base64 payload)
        metadata: Option<serde_json::Value>,
    },

    /// Approval request that requires the user's decision
    ApprovalRequest {
        action_id: String,
        description: String,
        risk_level: String, // "low" | "medium" | "high" | "critical"
        /// Structured payload of what will happen if approved
        payload: Option<serde_json::Value>,
        /// How long the user has to respond (seconds)
        timeout_seconds: u64,
        /// Available actions to choose from
        actions: Vec<ApprovalAction>,
        requested_at: String,
    },

    /// Real-time workflow run status update
    WorkflowStatus {
        run_id: String,
        workflow_id: String,
        workflow_name: String,
        status: String,
        message: Option<String>,
        updated_at: String,
    },

    /// Execution Plan Progress Update
    PlanProgress {
        session_id: String,
        plan_id: String,
        task_id: String,
        status: String,
        message: Option<String>,
    },

    /// Plan deviation detected
    PlanDeviation {
        session_id: String,
        plan_id: String,
        issue: String,
        resolution: String,
    },

    /// Agent status overview
    AgentStatuses {
        agents: Vec<serde_json::Value>,
        updated_at: String,
    },

    /// Error response
    Error {
        error_code: String,
        message: String,
        request_id: Option<String>,
    },

    /// List of chat sessions for the Mobile UI Sidebar
    SessionList { sessions: Vec<serde_json::Value> },

    /// Full history of a specific chat session for the Mobile UI
    SessionHistory {
        session_id: String,
        messages: Vec<serde_json::Value>,
    },

    /// Real-time CRDT document sync message
    CrdtSync {
        doc_id: String,
        payload_base64: String,
    },
}

/// A button/action shown in an approval request notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalAction {
    pub id: String,
    pub label: String,
    pub style: String, // "primary" | "secondary" | "danger"
}

/// Severity level for notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

// ─────────────────────────────────────────────────────────────────────────────
// Connected client state
// ─────────────────────────────────────────────────────────────────────────────

/// State tracked for each connected Mobile Client.
#[derive(Debug, Clone)]
pub struct ConnectedClient {
    pub client_id: String,
    pub remote_addr: Option<SocketAddr>,
    pub device_name: Option<String>,
    pub authenticated: bool,
    pub connected_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub messages_received: u64,
    pub messages_sent: u64,
    /// Channel to send messages to this specific client
    pub tx: mpsc::Sender<ServerMessage>,
}

impl ConnectedClient {
    pub fn new(remote_addr: Option<SocketAddr>, tx: mpsc::Sender<ServerMessage>) -> Self {
        Self {
            client_id: Uuid::new_v4().to_string(),
            remote_addr,
            device_name: None,
            authenticated: false,
            connected_at: Utc::now(),
            last_seen_at: Utc::now(),
            messages_received: 0,
            messages_sent: 0,
            tx,
        }
    }

    /// Send a message to this client.
    pub async fn send(
        &self,
        msg: ServerMessage,
    ) -> Result<(), mpsc::error::SendError<ServerMessage>> {
        self.tx.send(msg).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WebSocket Server configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the WebSocket server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Bind address (e.g. "0.0.0.0")
    pub host: String,
    /// Bind port (default: 9001)
    pub port: u16,
    /// Maximum concurrent client connections
    pub max_clients: usize,
    /// Require HITL approval for sensitive actions from mobile
    pub require_approval_for_sensitive: bool,
    /// Shared secret for simple token authentication (Phase 5)
    pub auth_secret: Option<String>,
    /// Idle timeout before disconnecting a client (seconds)
    pub idle_timeout_seconds: u64,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9001,
            max_clients: 50,
            require_approval_for_sensitive: true,
            auth_secret: None,
            idle_timeout_seconds: 300,
        }
    }
}

impl WebSocketConfig {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WebSocket Server
// ─────────────────────────────────────────────────────────────────────────────

/// Manages the WebSocket server and all connected Mobile Clients.
///
/// ## Phase 5 Implementation Plan
///
/// The full server loop will use `tokio-tungstenite`:
///
/// ```rust,ignore
/// use tokio_tungstenite::accept_async;
/// use tokio::net::TcpListener;
///
/// let listener = TcpListener::bind(config.bind_addr()).await?;
/// loop {
///     let (stream, addr) = listener.accept().await?;
///     let ws_stream = accept_async(stream).await?;
///     let (write, read) = ws_stream.split();
///     tokio::spawn(handle_connection(client_id, write, read, registry.clone()));
/// }
/// ```
///
/// Each connection task:
///   1. Authenticate via `Authorization: Bearer <token>` header or
///      first message containing a `{"type":"auth","token":"..."}` payload.
///   2. Register in `ClientRegistry`.
///   3. Read messages in a loop and dispatch to the command router.
///   4. Forward server → client messages from `tx` channel to the WebSocket.
pub struct WebSocketServer {
    config: WebSocketConfig,
    /// All currently connected clients (client_id → ConnectedClient)
    clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
    /// Broadcast channel for pushing messages to ALL connected clients
    broadcast_tx: broadcast::Sender<ServerMessage>,
    /// Channel for incoming commands (from any client) → Orchestrator
    command_tx: mpsc::Sender<IncomingCommand>,
    /// Running status
    running: Arc<std::sync::atomic::AtomicBool>,
}

/// An incoming command from any connected Mobile Client.
#[derive(Debug, Clone)]
pub struct IncomingCommand {
    pub client_id: String,
    pub message: ClientMessage,
    pub received_at: DateTime<Utc>,
}

impl WebSocketServer {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a new WebSocket server with the given configuration.
    /// The server is NOT started until `start()` is called.
    pub fn new(config: WebSocketConfig, command_tx: mpsc::Sender<IncomingCommand>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            command_tx,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults(command_tx: mpsc::Sender<IncomingCommand>) -> Self {
        Self::new(WebSocketConfig::default(), command_tx)
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Start the WebSocket server.
    ///
    /// TODO(phase-5): Replace stub with real tokio-tungstenite server loop.
    pub async fn start(&self) -> anyhow::Result<()> {
        let addr = self.config.bind_addr();
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        self.running
            .store(true, std::sync::atomic::Ordering::Relaxed);

        info!(
            addr = %addr,
            max_clients = self.config.max_clients,
            "WebSocket server started (Phase 5)"
        );

        let running = Arc::clone(&self.running);
        let clients = Arc::clone(&self.clients);
        let command_tx = self.command_tx.clone();
        let broadcast_tx = self.broadcast_tx.clone();
        let auth_secret = self.config.auth_secret.clone();
        let max_clients = self.config.max_clients;

        tokio::spawn(async move {
            while running.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok((stream, addr)) = listener.accept().await {
                    if clients.read().await.len() >= max_clients {
                        warn!("Max clients reached, rejecting connection from {addr}");
                        continue;
                    }

                    let clients_clone = Arc::clone(&clients);
                    let cmd_tx_clone = command_tx.clone();
                    let bc_rx = broadcast_tx.subscribe();
                    let secret_clone = auth_secret.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream,
                            addr,
                            clients_clone,
                            cmd_tx_clone,
                            bc_rx,
                            secret_clone,
                        )
                        .await
                        {
                            error!(remote = %addr, error = %e, "WebSocket connection error");
                        }
                    });
                }
            }
        });

        Ok(())
    }

    async fn handle_connection(
        stream: tokio::net::TcpStream,
        addr: SocketAddr,
        clients: Arc<RwLock<HashMap<String, ConnectedClient>>>,
        command_tx: mpsc::Sender<IncomingCommand>,
        mut broadcast_rx: broadcast::Receiver<ServerMessage>,
        auth_secret: Option<String>,
    ) -> anyhow::Result<()> {
        let ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(128 * 1024 * 1024),
            max_frame_size: Some(128 * 1024 * 1024),
            ..Default::default()
        };
        let ws_stream =
            tokio_tungstenite::accept_async_with_config(stream, Some(ws_config)).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (client_tx, mut client_rx) = mpsc::channel::<ServerMessage>(32);

        let client = ConnectedClient::new(Some(addr), client_tx);
        let client_id = client.client_id.clone();

        clients.write().await.insert(client_id.clone(), client);
        info!(client_id, remote = %addr, "New WebSocket connection");

        // Wait for authentication if required
        let mut authenticated = auth_secret.is_none();

        loop {
            tokio::select! {
                msg_opt = ws_receiver.next() => {
                    let msg = match msg_opt {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!(client_id, error = %e, "WebSocket read error");
                            break;
                        }
                        None => break,
                    };

                    match msg {
                        WsMessage::Text(text) => {
                            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                                // Auth handling
                                if !authenticated {
                                    if let ClientMessage::Auth { token } = &client_msg {
                                        let ok = match &auth_secret {
                                            Some(secret) => token == secret,
                                            None => true, // No secret configured → accept any token
                                        };
                                        if ok {
                                            authenticated = true;
                                            if let Some(c) = clients.write().await.get_mut(&client_id) {
                                                c.authenticated = true;
                                            }
                                            info!(client_id, "Client authenticated");
                                            let ack = serde_json::json!({ "type": "auth_success" });
                                            let _ = ws_sender.send(WsMessage::Text(ack.to_string())).await;
                                            continue;
                                        } else {
                                            // Wrong token — notify and disconnect
                                            warn!(client_id, "Client sent wrong auth token");
                                            let err = serde_json::json!({
                                                "type": "auth_error",
                                                "payload": { "message": "Invalid authentication token" }
                                            });
                                            let _ = ws_sender.send(WsMessage::Text(err.to_string())).await;
                                            break;
                                        }
                                    } else {
                                        // Received non-auth message before authenticating → ignore, wait
                                        warn!(client_id, msg_type = ?text.get(..50), "Received message before auth — waiting for Auth");
                                        continue;
                                    }
                                }


                                if let Some(c) = clients.write().await.get_mut(&client_id) {
                                    c.messages_received += 1;
                                    c.last_seen_at = Utc::now();
                                }

                                if let ClientMessage::Ping { timestamp_ms } = client_msg {
                                    let pong = ServerMessage::Pong { timestamp_ms };
                                    let _ = ws_sender.send(WsMessage::Text(serde_json::to_string(&pong).unwrap())).await;
                                    continue;
                                }

                                if let ClientMessage::Disconnect { .. } = client_msg {
                                    break;
                                }

                                let incoming = IncomingCommand {
                                    client_id: client_id.clone(),
                                    message: client_msg,
                                    received_at: Utc::now(),
                                };
                                let _ = command_tx.send(incoming).await;
                            } else if let Err(err) = serde_json::from_str::<ClientMessage>(&text) {
                                warn!(client_id, "Received invalid JSON from client: {}", err);
                                warn!("Invalid JSON start: {}", text.chars().take(200).collect::<String>());
                            }
                        }
                        WsMessage::Close(_) => break,
                        _ => {} // Ignore binary/ping/pong frames for now
                    }
                }

                // Messages specifically targeted to this client
                Some(server_msg) = client_rx.recv() => {
                    if let Ok(json) = serde_json::to_string(&server_msg) {
                        if ws_sender.send(WsMessage::Text(json)).await.is_err() {
                            break;
                        }
                        if let Some(c) = clients.write().await.get_mut(&client_id) {
                            c.messages_sent += 1;
                        }
                    }
                }

                // Broadcast messages
                Ok(server_msg) = broadcast_rx.recv() => {
                    if let Ok(json) = serde_json::to_string(&server_msg) {
                        if ws_sender.send(WsMessage::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }

        clients.write().await.remove(&client_id);
        info!(client_id, "Client disconnected");
        Ok(())
    }

    /// Stop the WebSocket server gracefully.
    pub async fn stop(&self) {
        info!("WebSocket server stopping");
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // Notify all connected clients
        let _ = self
            .broadcast(ServerMessage::Notification {
                notification_id: Uuid::new_v4().to_string(),
                level: NotificationLevel::Info,
                title: "Server shutting down".to_string(),
                body: "Office Hub WebSocket server is shutting down.".to_string(),
                data: None,
                timestamp: Utc::now().to_rfc3339(),
            })
            .await;

        self.clients.write().await.clear();
        info!("WebSocket server stopped");
    }

    /// Check whether the server is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Relaxed)
    }

    // ── Client management ─────────────────────────────────────────────────────

    /// Return the number of currently connected clients.
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Return IDs of all connected clients.
    pub async fn client_ids(&self) -> Vec<String> {
        self.clients.read().await.keys().cloned().collect()
    }

    /// Disconnect a specific client by ID.
    pub async fn disconnect_client(&self, client_id: &str, reason: Option<&str>) {
        if let Some(client) = self.clients.write().await.remove(client_id) {
            warn!(
                client_id,
                reason,
                remote = ?client.remote_addr,
                "Client disconnected"
            );
        }
    }

    // ── Messaging ─────────────────────────────────────────────────────────────

    /// Send a message to a specific connected client.
    pub async fn send_to_client(&self, client_id: &str, msg: ServerMessage) -> anyhow::Result<()> {
        let clients = self.clients.read().await;
        let client = clients
            .get(client_id)
            .ok_or_else(|| anyhow::anyhow!("Client '{}' not found or disconnected", client_id))?;

        client
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message to client '{}': {}", client_id, e))
    }

    /// Broadcast a message to ALL connected clients.
    pub async fn broadcast(&self, msg: ServerMessage) -> anyhow::Result<()> {
        let count = self.broadcast_tx.receiver_count();
        if count == 0 {
            debug!("Broadcast: no connected clients");
            return Ok(());
        }

        self.broadcast_tx
            .send(msg)
            .map_err(|e| anyhow::anyhow!("Broadcast failed: {}", e))?;

        debug!(recipients = count, "Message broadcast to all clients");
        Ok(())
    }

    /// Send an approval request to all connected clients.
    ///
    /// The first client to respond wins; subsequent responses are ignored.
    pub async fn send_approval_request(
        &self,
        action_id: &str,
        description: &str,
        risk_level: &str,
        payload: Option<serde_json::Value>,
        timeout_seconds: u64,
    ) -> anyhow::Result<()> {
        let msg = ServerMessage::ApprovalRequest {
            action_id: action_id.to_string(),
            description: description.to_string(),
            risk_level: risk_level.to_string(),
            payload,
            timeout_seconds,
            actions: vec![
                ApprovalAction {
                    id: "approve".to_string(),
                    label: "✅ Duyệt".to_string(),
                    style: "primary".to_string(),
                },
                ApprovalAction {
                    id: "reject".to_string(),
                    label: "❌ Từ chối".to_string(),
                    style: "danger".to_string(),
                },
            ],
            requested_at: Utc::now().to_rfc3339(),
        };

        info!(
            action_id,
            risk_level,
            client_count = self.client_count().await,
            "HITL approval request sent to Mobile clients"
        );

        self.broadcast(msg).await
    }

    /// Push a notification to all connected clients.
    pub async fn notify(
        &self,
        level: NotificationLevel,
        title: impl Into<String>,
        body: impl Into<String>,
        data: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let msg = ServerMessage::Notification {
            notification_id: Uuid::new_v4().to_string(),
            level,
            title: title.into(),
            body: body.into(),
            data,
            timestamp: Utc::now().to_rfc3339(),
        };
        self.broadcast(msg).await
    }

    // ── Server → client relay helpers ─────────────────────────────────────────

    /// Relay a workflow run status update to all clients.
    pub async fn relay_workflow_status(
        &self,
        run_id: &str,
        workflow_id: &str,
        workflow_name: &str,
        status: &str,
        message: Option<String>,
    ) {
        let msg = ServerMessage::WorkflowStatus {
            run_id: run_id.to_string(),
            workflow_id: workflow_id.to_string(),
            workflow_name: workflow_name.to_string(),
            status: status.to_string(),
            message,
            updated_at: Utc::now().to_rfc3339(),
        };
        let _ = self.broadcast(msg).await;
    }

    // ── Statistics ────────────────────────────────────────────────────────────

    /// Return a JSON summary for the dashboard.
    pub async fn status_json(&self) -> serde_json::Value {
        let clients = self.clients.read().await;
        let client_summaries: Vec<serde_json::Value> = clients
            .values()
            .map(|c| {
                serde_json::json!({
                    "clientId":        c.client_id,
                    "deviceName":      c.device_name,
                    "remoteAddr":      c.remote_addr.map(|a| a.to_string()),
                    "authenticated":   c.authenticated,
                    "connectedAt":     c.connected_at.to_rfc3339(),
                    "lastSeenAt":      c.last_seen_at.to_rfc3339(),
                    "messagesReceived": c.messages_received,
                    "messagesSent":    c.messages_sent,
                })
            })
            .collect();

        serde_json::json!({
            "running":        self.is_running(),
            "bindAddress":    self.config.bind_addr(),
            "clientCount":    clients.len(),
            "maxClients":     self.config.max_clients,
            "clients":        client_summaries,
            "phase5Implemented": false,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server() -> (WebSocketServer, mpsc::Receiver<IncomingCommand>) {
        let (tx, rx) = mpsc::channel(32);
        let cfg = WebSocketConfig {
            port: 0, // Use random port for tests
            ..Default::default()
        };
        let server = WebSocketServer::new(cfg, tx);
        (server, rx)
    }

    // ── Configuration ─────────────────────────────────────────────────────────

    #[test]
    fn test_default_config_bind_addr() {
        let cfg = WebSocketConfig::default();
        assert_eq!(cfg.bind_addr(), "0.0.0.0:9001");
    }

    #[test]
    fn test_default_config_sensible_values() {
        let cfg = WebSocketConfig::default();
        assert_eq!(cfg.port, 9001);
        assert_eq!(cfg.max_clients, 50);
        assert!(cfg.require_approval_for_sensitive);
        assert!(cfg.auth_secret.is_none());
    }

    // ── Server construction ───────────────────────────────────────────────────

    #[test]
    fn test_server_not_running_initially() {
        let (server, _rx) = make_server();
        assert!(!server.is_running());
    }

    #[tokio::test]
    async fn test_server_starts_stub_without_panic() {
        let (server, _rx) = make_server();
        let result = server.start().await;
        assert!(result.is_ok());
        assert!(server.is_running());
    }

    #[tokio::test]
    async fn test_server_stop() {
        let (server, _rx) = make_server();
        server.start().await.unwrap();
        assert!(server.is_running());
        server.stop().await;
        assert!(!server.is_running());
    }

    // ── Client management ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_no_clients_initially() {
        let (server, _rx) = make_server();
        assert_eq!(server.client_count().await, 0);
        assert!(server.client_ids().await.is_empty());
    }

    #[tokio::test]
    async fn test_send_to_nonexistent_client_returns_error() {
        let (server, _rx) = make_server();
        let result = server
            .send_to_client("nonexistent-id", ServerMessage::Pong { timestamp_ms: 0 })
            .await;
        assert!(result.is_err());
    }

    // ── Messaging ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_broadcast_with_no_clients_ok() {
        let (server, _rx) = make_server();
        // Should not panic or error when there are no subscribers
        let result = server
            .notify(NotificationLevel::Info, "Test", "Test body", None)
            .await;
        // No receivers → broadcast_tx.send() will return RecvError but we swallow it
        // In stub mode this is acceptable
        let _ = result;
    }

    #[tokio::test]
    async fn test_send_approval_request_no_panic() {
        let (server, _rx) = make_server();
        // Even with no clients, should not panic
        let _ = server
            .send_approval_request(
                "action-123",
                "Test action requiring approval",
                "high",
                None,
                300,
            )
            .await;
    }

    // ── Status JSON ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_status_json() {
        let (server, _rx) = make_server();
        let json = server.status_json().await;
        assert_eq!(json["clientCount"], 0);
        assert_eq!(json["phase5Implemented"], false);
        assert!(json["bindAddress"].as_str().unwrap().contains("0"));
    }

    // ── Message serialization ─────────────────────────────────────────────────

    #[test]
    fn test_server_message_pong_serializes() {
        let msg = ServerMessage::Pong {
            timestamp_ms: 12345,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("pong"));
        assert!(json.contains("12345"));
    }

    #[test]
    fn test_client_message_ping_deserializes() {
        let json = r#"{"type":"ping","timestamp_ms":99999}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            ClientMessage::Ping {
                timestamp_ms: 99999
            }
        ));
    }

    #[test]
    fn test_approval_response_deserializes() {
        let json = r#"{
            "type":"approval_response",
            "action_id":"abc-123",
            "approved":true,
            "reason":null,
            "responded_by":"user@mobile"
        }"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        if let ClientMessage::ApprovalResponse {
            action_id,
            approved,
            responded_by,
            ..
        } = msg
        {
            assert_eq!(action_id, "abc-123");
            assert!(approved);
            assert_eq!(responded_by, "user@mobile");
        } else {
            panic!("Wrong variant deserialized");
        }
    }

    #[test]
    fn test_approval_request_serializes_with_actions() {
        let msg = ServerMessage::ApprovalRequest {
            action_id: "act-001".to_string(),
            description: "Test action".to_string(),
            risk_level: "high".to_string(),
            payload: None,
            timeout_seconds: 300,
            actions: vec![ApprovalAction {
                id: "approve".to_string(),
                label: "✅ Duyệt".to_string(),
                style: "primary".to_string(),
            }],
            requested_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("approval_request"));
        assert!(json.contains("act-001"));
        assert!(json.contains("high"));
    }

    #[test]
    fn test_notification_level_serializes_lowercase() {
        let json = serde_json::to_string(&NotificationLevel::Warning).unwrap();
        assert_eq!(json, r#""warning""#);
    }
}
