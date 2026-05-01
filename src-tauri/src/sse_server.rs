// ============================================================================
// Office Hub – sse_server.rs
//
// MCP-Hybrid Transport Server (Axum)
//
// Control Plane (Downlink):  GET  /api/v1/stream          → SSE
// Control Plane (Uplink):    POST /api/v1/command         → REST
//                            POST /api/v1/tool_call       → REST
// Auth:                      POST /api/v1/auth            → REST
// Data Plane (Files):        POST /api/v1/files/upload    → REST (multipart)
//                            GET  /api/v1/files/download/{id} → REST streaming
// Artifacts:                 GET  /api/v1/artifacts       → REST (list)
//                            DELETE /api/v1/artifacts/{f} → REST
// Static:                    GET  /files/*                → ServeDir
//
// Memory-safety design:
//   - Each SSE client gets its own mpsc::channel(1024) — no shared ring buffer.
//   - Non-critical events (Log, Progress, Status) are silently dropped when the
//     per-client queue is above SOFT_DROP_THRESHOLD (75% full).
//   - Critical events (Result, Error, ApprovalRequest, SessionList,
//     SessionHistory) are ALWAYS delivered; they bypass the drop logic.
//   - Progress events are throttled to 1 per PROGRESS_THROTTLE_MS per session
//     via a DashMap<session_id, Instant> to prevent broadcast spam.
// ============================================================================

use std::{
    convert::Infallible,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Multipart, Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, get_service, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use futures::stream::Stream;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};
use tokio_util::io::ReaderStream;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::mcp_transport::{
    AuthRequest, AuthResponse, IncomingMobileCmd, McpResource, McpToolCall, MobileCommand,
    SseEvent, SseEventType,
};

// ── Tuning constants ──────────────────────────────────────────────────────────

/// Per-client mpsc queue depth. 1024 events × ~1KB avg → max ~1MB per client.
const CLIENT_CHANNEL_CAPACITY: usize = 1024;

/// When the per-client queue is above this fraction full, non-critical events
/// are silently dropped to protect memory.
/// 0.75 = drop Log/Progress/Status when > 768 events pending.
const SOFT_DROP_THRESHOLD: usize = (CLIENT_CHANNEL_CAPACITY as f64 * 0.75) as usize;

/// Minimum interval between Progress events for the same session.
/// Prevents LLM streaming from spamming thousands of small events.
const PROGRESS_THROTTLE_MS: u64 = 500;

// ─────────────────────────────────────────────────────────────────────────────
// Shared server state
// ─────────────────────────────────────────────────────────────────────────────

/// State shared across all SSE+REST handlers.
#[derive(Clone)]
pub struct HybridServerState {
    /// Fan-out channel — used ONLY to notify subscriber tasks of new events.
    /// Actual event data is routed via per-client mpsc channels.
    pub sse_tx: broadcast::Sender<SseEvent>,
    /// Per-client sender map: client_id → mpsc::Sender<SseEvent>
    pub client_senders: Arc<DashMap<String, mpsc::Sender<SseEvent>>>,
    /// Progress throttle: session_id → last Progress event timestamp
    pub progress_throttle: Arc<DashMap<String, Instant>>,
    /// Commands routed to the orchestrator worker loop in lib.rs.
    pub command_tx: mpsc::Sender<IncomingMobileCmd>,
    /// Shared auth secret (same as WebSocket auth for legacy compat).
    pub auth_secret: Option<String>,
    /// Directory where exported/output files are stored.
    pub public_dir: PathBuf,
    /// App data directory for accessing workspaces
    pub app_data_dir: Option<PathBuf>,
}

impl HybridServerState {
    pub fn new(
        command_tx: mpsc::Sender<IncomingMobileCmd>,
        auth_secret: Option<String>,
        public_dir: PathBuf,
    ) -> (Self, broadcast::Sender<SseEvent>) {
        // broadcast channel is only for legacy/fallback; main path is per-client mpsc
        let (sse_tx, _) = broadcast::channel(256);
        let state = Self {
            sse_tx: sse_tx.clone(),
            client_senders: Arc::new(DashMap::new()),
            progress_throttle: Arc::new(DashMap::new()),
            command_tx,
            auth_secret,
            public_dir,
            app_data_dir: None,
        };
        (state, sse_tx)
    }

    /// Route an event to all connected SSE clients with priority-aware drop logic.
    pub fn broadcast_event(&self, evt: SseEvent) {
        let is_critical = evt.event_type.is_critical();

        // For Progress events, apply per-session throttle
        if evt.event_type == SseEventType::Progress {
            if let Some(session_id) = evt.payload.get("session_id").and_then(|v| v.as_str()) {
                let now = Instant::now();
                let throttle_duration = Duration::from_millis(PROGRESS_THROTTLE_MS);
                let should_drop = self
                    .progress_throttle
                    .get(session_id)
                    .map(|last| now.duration_since(*last) < throttle_duration)
                    .unwrap_or(false);

                if should_drop {
                    return; // Drop this Progress event — too soon
                }
                self.progress_throttle.insert(session_id.to_string(), now);
            }
        }

        // Deliver to each connected client
        let mut dead_clients: Vec<String> = Vec::new();
        for entry in self.client_senders.iter() {
            let client_id = entry.key().clone();
            let sender = entry.value();

            // Smart drop: if queue is above threshold AND event is non-critical, skip
            if !is_critical {
                let queue_len = CLIENT_CHANNEL_CAPACITY - sender.capacity();
                if queue_len > SOFT_DROP_THRESHOLD {
                    debug!(
                        client_id = %client_id,
                        queue_len,
                        "Dropping non-critical {:?} event (queue > {}%)",
                        evt.event_type,
                        (SOFT_DROP_THRESHOLD * 100) / CLIENT_CHANNEL_CAPACITY
                    );
                    continue;
                }
            }

            match sender.try_send(evt.clone()) {
                Ok(_) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    if is_critical {
                        // Force-deliver critical events even on full queue by blocking briefly
                        let sender_clone = sender.clone();
                        let evt_clone = evt.clone();
                        tokio::spawn(async move {
                            let _ = sender_clone.send(evt_clone).await;
                        });
                    } else {
                        debug!(client_id = %client_id, "Dropping non-critical event — queue full");
                    }
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    dead_clients.push(client_id);
                }
            }
        }

        // Cleanup disconnected clients
        for id in dead_clients {
            self.client_senders.remove(&id);
            self.progress_throttle.remove(&id);
        }
    }

    /// Verify a Bearer token from Authorization header OR ?token= query param.
    fn check_auth_full(&self, headers: &HeaderMap, query_token: Option<&str>) -> bool {
        let Some(secret) = &self.auth_secret else {
            return true;
        };
        let header_ok = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|tok| tok == secret.as_str())
            .unwrap_or(false);
        let query_ok = query_token.map(|t| t == secret.as_str()).unwrap_or(false);
        header_ok || query_ok
    }

    fn check_auth(&self, headers: &HeaderMap) -> bool {
        self.check_auth_full(headers, None)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Server bootstrap
// ─────────────────────────────────────────────────────────────────────────────

pub async fn start_hybrid_server(
    port: u16,
    state: HybridServerState,
    orchestrator: crate::OrchestratorHandle,
) {
    let public_dir = state.public_dir.clone();

    if let Err(e) = std::fs::create_dir_all(&public_dir) {
        error!("Failed to create public_dir for hybrid server: {}", e);
        return;
    }

    // Spawn a background garbage collection task to clean up files older than 24 hours
    let gc_dir = public_dir.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour
        loop {
            interval.tick().await;
            if let Ok(mut entries) = tokio::fs::read_dir(&gc_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(metadata) = entry.metadata().await {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(age) = modified.elapsed() {
                                if age > Duration::from_secs(24 * 3600) {
                                    let _ = tokio::fs::remove_file(entry.path()).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // ── SSE control plane ────────────────────────────────────────────────
        .route("/api/v1/stream", get(sse_handler))
        // ── REST uplink ──────────────────────────────────────────────────────
        .route("/api/v1/auth", post(auth_handler))
        .route("/api/v1/command", post(post_command))
        .route("/api/v1/tool_call", post(post_tool_call))
        // ── File data plane ───────────────────────────────────────────────────
        .route("/api/v1/files/upload", post(upload_file))
        .route("/api/v1/files/download/{id}", get(download_file))
        // ── Artifact management ───────────────────────────────────────────────
        .route("/api/v1/artifacts", get(list_artifacts))
        .route("/api/v1/artifacts/{filename}", delete(delete_artifact))
        // ── Session management (REST) ─────────────────────────────────────────
        .route("/api/v1/sessions", get(list_sessions_rest))
        .route(
            "/api/v1/sessions/{id}/history",
            get(get_session_history_rest),
        )
        // ── Workspace management ──────────────────────────────────────────────
        .route("/api/v1/workspaces", get(list_workspaces_rest))
        .route(
            "/api/v1/workspaces/{id}/links",
            get(get_workspace_links_rest),
        )
        .route(
            "/api/v1/workspaces/{id}/files",
            get(get_workspace_files_rest),
        )
        // ── Static file serving ───────────────────────────────────────────────
        .nest_service("/files", get_service(ServeDir::new(public_dir.clone())))
        // ── Legacy upload (kept for backwards compat) ─────────────────────────
        .route("/upload", post(legacy_upload))
        .layer(axum::Extension(orchestrator))
        .with_state(Arc::new(state))
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("SSE+REST hybrid server starting on {}", addr);

    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Hybrid server error: {}", e);
            }
        }
        Err(e) => error!("Hybrid server failed to bind {}: {}", addr, e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AUTH
// ─────────────────────────────────────────────────────────────────────────────

async fn auth_handler(
    State(state): State<Arc<HybridServerState>>,
    Json(req): Json<AuthRequest>,
) -> Json<AuthResponse> {
    let ok = match &state.auth_secret {
        Some(secret) => req.token == *secret,
        None => true,
    };
    Json(AuthResponse {
        ok,
        message: if ok {
            "Authenticated".to_string()
        } else {
            "Invalid token".to_string()
        },
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// SSE DOWNLINK  –  GET /api/v1/stream
// ─────────────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Default)]
struct SseQuery {
    token: Option<String>,
    session_id: Option<String>,
}

async fn sse_handler(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    Query(query): Query<SseQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if !state.check_auth_full(&headers, query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let client_id = query
        .session_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    info!(client_id = %client_id, "New SSE client connected");

    // Create per-client bounded channel — this is the key memory fix
    let (tx, mut rx) = mpsc::channel::<SseEvent>(CLIENT_CHANNEL_CAPACITY);
    state.client_senders.insert(client_id.clone(), tx);

    // Clone for cleanup on disconnect
    let senders = Arc::clone(&state.client_senders);
    let throttle_map = Arc::clone(&state.progress_throttle);

    let client_id_clone = client_id.clone();
    let stream = async_stream::stream! {
        // Immediately send a 'connected' handshake event so the mobile XHR
        // transitions to LOADING state and fires the 'open' callback.
        // Without this, the client waits up to 15s for the first KeepAlive
        // comment, causing spurious timeout disconnects.
        let handshake = serde_json::json!({
            "event_type": "connected",
            "call_id": null,
            "payload": { "client_id": &client_id_clone, "session_id": query.session_id, "ts": chrono::Utc::now().to_rfc3339() }
        });
        if let Ok(j) = serde_json::to_string(&handshake) {
            yield Ok(Event::default().event("connected").data(j));
        }

        loop {
            match rx.recv().await {
                Some(evt) => {
                    let json = match serde_json::to_string(&evt) {
                        Ok(j) => j,
                        Err(e) => {
                            warn!("Failed to serialize SseEvent: {}", e);
                            continue;
                        }
                    };
                    let event_name = match serde_json::to_value(&evt.event_type) {
                        Ok(serde_json::Value::String(s)) => s,
                        _ => format!("{:?}", evt.event_type).to_lowercase(),
                    };
                    yield Ok(Event::default().event(event_name).data(json));
                }
                None => {
                    // Channel closed — client disconnected
                    info!(client_id = %client_id_clone, "SSE client disconnected — cleaning up");
                    senders.remove(&client_id_clone);
                    throttle_map.retain(|k, _| !k.starts_with(&client_id_clone));
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("heartbeat"),
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// REST UPLINK  –  POST /api/v1/command
// ─────────────────────────────────────────────────────────────────────────────

async fn post_command(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    Json(cmd): Json<MobileCommand>,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    let session_id = cmd.session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let context_file_path = cmd.context.as_ref().and_then(|ctx| {
        ctx.get("file_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    let incoming = IncomingMobileCmd {
        command_id: cmd.command_id.clone(),
        session_id: session_id.clone(),
        text: cmd.text,
        context_file_path,
        received_at: Utc::now(),
    };

    match state.command_tx.send(incoming).await {
        Ok(_) => {
            debug!(command_id = %cmd.command_id, "Command queued");
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({
                    "ok": true,
                    "session_id": session_id,
                    "command_id": cmd.command_id,
                    "message": "Command accepted — watch SSE stream for results",
                })),
            )
        }
        Err(e) => {
            error!("Failed to queue command: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error":"queue full"})),
            )
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// REST UPLINK  –  POST /api/v1/tool_call
// ─────────────────────────────────────────────────────────────────────────────

async fn post_tool_call(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    Json(call): Json<McpToolCall>,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    let text = format!(
        "[MCP Tool Call] tool={} args={}",
        call.tool_name,
        serde_json::to_string(&call.arguments).unwrap_or_default()
    );

    let session_id = call
        .arguments
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let incoming = IncomingMobileCmd {
        command_id: call.call_id.clone(),
        session_id: if session_id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            session_id
        },
        text,
        context_file_path: None,
        received_at: Utc::now(),
    };

    match state.command_tx.send(incoming).await {
        Ok(_) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "ok": true,
                "call_id": call.call_id,
                "message": "Tool call accepted — watch SSE for result",
            })),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"queue full"})),
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FILE DATA PLANE  –  POST /api/v1/files/upload
// ─────────────────────────────────────────────────────────────────────────────

async fn upload_file(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    let upload_dir = state.public_dir.clone();

    if let Ok(Some(mut field)) = multipart.next_field().await {
        let original_name = field.file_name().unwrap_or("upload.tmp").to_string();

        let safe_name = original_name
            .replace("/", "_")
            .replace("\\", "_")
            .replace("..", "_");
        let unique_id = safe_name.clone();
        let file_path = upload_dir.join(&unique_id);

        let mime = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let mut file = match tokio::fs::File::create(&file_path).await {
            Ok(f) => f,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to create file: {}", e)
                    })),
                );
            }
        };

        use tokio::io::AsyncWriteExt;
        let mut size = 0u64;

        while let Ok(Some(chunk)) = field.chunk().await {
            if let Err(e) = file.write_all(&chunk).await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to write chunk: {}", e)
                    })),
                );
            }
            size += chunk.len() as u64;
        }

        let resource = McpResource::for_file(&unique_id, &original_name, &mime, size);

        return (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "ok": true,
                "resource": resource,
                "file_path": file_path.to_string_lossy(),
            })),
        );
    }

    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error":"No file received"})),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// FILE DATA PLANE  –  GET /api/v1/files/download/{id}
// ─────────────────────────────────────────────────────────────────────────────

async fn download_file(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if !state.check_auth(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if id.contains("..") || id.contains('/') || id.contains('\\') {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let mut file_path = state.public_dir.join(&id);
    if !file_path.exists() {
        let artifacts_dir = std::env::temp_dir().join("office_hub_artifacts");
        file_path = artifacts_dir.join(&id);
        if !file_path.starts_with(&artifacts_dir) {
            return StatusCode::BAD_REQUEST.into_response();
        }
    } else if !file_path.starts_with(&state.public_dir) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match tokio::fs::File::open(&file_path).await {
        Ok(file) => {
            let metadata = file.metadata().await;
            let stream = ReaderStream::new(file);
            let body = Body::from_stream(stream);
            let mime = mime_from_path(&file_path);

            let mut response = Response::new(body);
            response.headers_mut().insert(
                "content-type",
                mime.parse()
                    .unwrap_or("application/octet-stream".parse().unwrap()),
            );
            response.headers_mut().insert(
                "content-disposition",
                format!("attachment; filename=\"{}\"", id)
                    .parse()
                    .unwrap_or("attachment".parse().unwrap()),
            );
            if let Ok(meta) = metadata {
                response
                    .headers_mut()
                    .insert("content-length", meta.len().to_string().parse().unwrap());
            }
            response
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ARTIFACT MANAGEMENT
// ─────────────────────────────────────────────────────────────────────────────

async fn list_artifacts(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    let artifacts_dir = state.public_dir.clone();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&artifacts_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let ts = meta
                        .modified()
                        .ok()
                        .map(|t| chrono::DateTime::<Utc>::from(t).to_rfc3339())
                        .unwrap_or_default();
                    files.push(serde_json::json!({
                        "id": name,
                        "name": name,
                        "url": format!("/api/v1/files/download/{}", name),
                        "timestamp": ts,
                        "size": meta.len(),
                    }));
                }
            }
        }
    }
    files.sort_by(|a, b| {
        b["timestamp"]
            .as_str()
            .unwrap_or("")
            .cmp(a["timestamp"].as_str().unwrap_or(""))
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "artifacts": files })),
    )
}

async fn delete_artifact(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    AxumPath(filename): AxumPath<String>,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid filename"})),
        );
    }

    let artifacts_dir = state.public_dir.clone();
    let path = artifacts_dir.join(&filename);

    match std::fs::remove_file(&path) {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LEGACY UPLOAD  –  POST /upload  (backward compat)
// ─────────────────────────────────────────────────────────────────────────────

async fn legacy_upload(
    State(state): State<Arc<HybridServerState>>,
    multipart: Multipart,
) -> Json<Value> {
    let (_status, Json(body)) = upload_file(State(state), HeaderMap::new(), multipart).await;
    Json(body)
}

// ─────────────────────────────────────────────────────────────────────────────
// SESSION MANAGEMENT (REST)
// ─────────────────────────────────────────────────────────────────────────────

async fn list_sessions_rest(
    State(state): State<Arc<HybridServerState>>,
    axum::Extension(orchestrator): axum::Extension<crate::OrchestratorHandle>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    match orchestrator.list_sessions().await {
        Ok(sessions) => (
            StatusCode::OK,
            Json(serde_json::json!({ "sessions": sessions })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn get_session_history_rest(
    State(state): State<Arc<HybridServerState>>,
    axum::Extension(orchestrator): axum::Extension<crate::OrchestratorHandle>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }

    let store = orchestrator.get_session_store().await;
    if let Some(session) = store.get(&session_id) {
        let messages: Vec<serde_json::Value> = session
            .messages
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "id": msg.id,
                    "role": msg.role,
                    "content": msg.content,
                    "timestamp_ms": msg.created_at.timestamp_millis(),
                    "agent_used": msg.agent_name,
                })
            })
            .collect();
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": session_id,
                "messages": messages,
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session not found"})),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WORKSPACE MANAGEMENT (REST)
// ─────────────────────────────────────────────────────────────────────────────

async fn list_workspaces_rest(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }
    let app_data_dir = match &state.app_data_dir {
        Some(dir) => dir,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"app_data_dir missing"})),
            )
        }
    };

    let base_dir = app_data_dir.join("workspaces");
    let mut workspaces = Vec::new();
    workspaces.push(crate::knowledge::Workspace {
        id: "default".to_string(),
        name: "Default Workspace".to_string(),
        created_at: 0,
    });

    if base_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let id = entry.file_name().to_string_lossy().to_string();
                    if id == "default" {
                        continue;
                    }
                    let meta_path = path.join("meta.json");
                    let mut name = id.clone();
                    let mut created_at = 0;
                    if meta_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&meta_path) {
                            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                                if let Some(n) = meta.get("name").and_then(|v| v.as_str()) {
                                    name = n.to_string();
                                }
                                if let Some(c) = meta.get("created_at").and_then(|v| v.as_u64()) {
                                    created_at = c;
                                }
                            }
                        }
                    }
                    workspaces.push(crate::knowledge::Workspace {
                        id,
                        name,
                        created_at,
                    });
                }
            }
        }
    }
    (StatusCode::OK, Json(serde_json::json!(workspaces)))
}

async fn get_workspace_links_rest(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    AxumPath(workspace_id): AxumPath<String>,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }
    let app_data_dir = match &state.app_data_dir {
        Some(dir) => dir,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"app_data_dir missing"})),
            )
        }
    };
    let path = if workspace_id == "default" {
        app_data_dir.join("links.json")
    } else {
        app_data_dir
            .join("workspaces")
            .join(&workspace_id)
            .join("links.json")
    };
    if !path.exists() {
        return (StatusCode::OK, Json(serde_json::json!([])));
    }
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| "[]".to_string());
    let links: Vec<crate::knowledge::WorkspaceLink> =
        serde_json::from_str(&content).unwrap_or_default();
    (StatusCode::OK, Json(serde_json::json!(links)))
}

async fn get_workspace_files_rest(
    State(state): State<Arc<HybridServerState>>,
    headers: HeaderMap,
    AxumPath(workspace_id): AxumPath<String>,
) -> (StatusCode, Json<Value>) {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        );
    }
    let app_data_dir = match &state.app_data_dir {
        Some(dir) => dir,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":"app_data_dir missing"})),
            )
        }
    };

    let base_dir = app_data_dir.join("workspaces").join(&workspace_id);
    let mut files = Vec::new();

    let categories = [
        "knowledge",
        "policies",
        "data",
        "docs/inbox",
        "docs/outbox",
        "links",
        "memory",
    ];
    for category in categories {
        let cat_dir = base_dir.join(category);
        if cat_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&cat_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        files.push(serde_json::json!({
                            "name": name,
                            "category": category,
                            "size": size,
                            "path": path.to_string_lossy().to_string()
                        }));
                    }
                }
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!(files)))
}
