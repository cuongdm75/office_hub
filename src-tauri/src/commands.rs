//! commands.rs — Tauri IPC Command Bridge
//!
//! All `#[tauri::command]` functions exposed to the React frontend live here.
//! They are thin wrappers that delegate to the core domain modules (orchestrator,
//! agents, llm_gateway, workflow, etc.) and convert Rust errors into JSON-friendly
//! strings so that the frontend receives consistent `Result<T, string>` values.
//!
//! Registration: each command must also be listed in the `tauri::Builder::invoke_handler`
//! call inside `lib.rs`.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::AppState;

// ─────────────────────────────────────────────────────────────────────────────
// Shared result type for all IPC commands
// ─────────────────────────────────────────────────────────────────────────────

/// Every command returns `CommandResult<T>`.
/// Tauri serialises `Ok(T)` → `{ data: T }` and `Err(e)` → `{ error: "…" }`.
pub type CommandResult<T> = Result<T, String>;

/// Map any `anyhow::Error` (or `impl Display`) to a `String` for IPC transport, including the full error chain.
fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{:#}", e)
}

// ─────────────────────────────────────────────────────────────────────────────
// DTO types (shared with the frontend via TypeScript generation)
// ─────────────────────────────────────────────────────────────────────────────

/// A single chat message sent from / received by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub timestamp_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_used: Option<String>,
}

/// Request payload for `send_chat_message`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatRequest {
    pub session_id: Option<String>,
    pub message: String,
    pub context_file_path: Option<String>,
    pub workspace_id: Option<String>,
}

/// Response payload for `send_chat_message`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatResponse {
    pub session_id: String,
    pub reply: ChatMessage,
    pub intent: Option<String>,
    pub agent_used: Option<String>,
    pub tokens_used: Option<u32>,
}

/// Payload for listing directory contents (File Browser).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub extension: Option<String>,
}

// LlmProviderSettings removed; we use LlmConfig directly now.

/// Status of a workflow run.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunResult {
    pub run_id: String,
    pub workflow_id: String,
    pub status: String, // "pending" | "running" | "success" | "failed"
    pub message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// Info about a running or registered agent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String, // "idle" | "busy" | "error"
    pub last_used: Option<String>,
    pub capabilities: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// ── ORCHESTRATOR / CHAT COMMANDS ────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// Send a chat message to the Orchestrator.
///
/// The Orchestrator classifies the intent, selects the appropriate agent,
/// executes the task, and returns a structured response.
///
/// Frontend call:
/// ```ts
/// const res = await invoke<SendChatResponse>('send_chat_message', { request });
/// ```
#[tauri::command]
pub async fn send_chat_message(
    request: SendChatRequest,
    state: State<'_, AppState>,
    _app: AppHandle,
) -> CommandResult<SendChatResponse> {
    let session_id = request
        .session_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    tracing::info!(session_id = %session_id, "send_chat_message invoked");

    let response = state
        .orchestrator
        .process_message_native(
            &session_id,
            &request.message,
            request.context_file_path.as_deref(),
            request.workspace_id.as_deref(),
            None,
        )
        .await
        .map_err(err)?;

    Ok(SendChatResponse {
        session_id,
        reply: ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: response.content,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            intent: response.intent.clone(),
            agent_used: response.agent_used.clone(),
        },
        intent: response.intent,
        agent_used: response.agent_used,
        tokens_used: response.tokens_used,
    })
}

/// Create a new chat session and return its ID.
#[tauri::command]
pub async fn create_session(
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let id = state
        .orchestrator
        .create_session(workspace_id)
        .await
        .map_err(err)?;

    tracing::info!(session_id = %id, "new session created");
    Ok(id.to_string())
}

/// Delete (clear) a chat session by ID.
#[tauri::command]
pub async fn delete_session(session_id: String, state: State<'_, AppState>) -> CommandResult<()> {
    state
        .orchestrator
        .delete_session(&session_id)
        .await
        .map_err(err)?;
    Ok(())
}

/// Sends a raw prompt to the LLM Gateway without using the Orchestrator loop.
#[tauri::command]
pub async fn raw_llm_request(prompt: String, state: State<'_, AppState>) -> CommandResult<String> {
    tracing::info!("raw_llm_request invoked");

    let llm_req =
        crate::llm_gateway::LlmRequest::new(vec![crate::llm_gateway::LlmMessage::user(prompt)])
            .with_temperature(0.3);

    let gateway_arc = state.llm_gateway.clone();
    let gateway = gateway_arc.read().await;
    let resp = gateway.complete(llm_req).await.map_err(err)?;

    Ok(resp.content)
}

/// List all active sessions (id + summary).
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> CommandResult<Vec<serde_json::Value>> {
    let sessions = state.orchestrator.list_sessions().await.map_err(err)?;
    Ok(sessions)
}

/// Get the full message history for a specific session.
#[tauri::command]
pub async fn get_session_history(
    session_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<ChatMessage>> {
    let store = state.orchestrator.get_session_store().await;
    let session = store
        .get(&session_id)
        .ok_or_else(|| "Session not found".to_string())?;

    let messages = session
        .messages
        .iter()
        .map(|msg| ChatMessage {
            id: msg.id.clone(),
            role: msg.role.to_string(),
            content: msg.content.clone(),
            timestamp_ms: msg.created_at.timestamp_millis(),
            intent: msg.intent.clone(),
            agent_used: msg.agent_name.clone(),
        })
        .collect();

    Ok(messages)
}

// ─────────────────────────────────────────────────────────────────────────────
// ── LLM GATEWAY COMMANDS ────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// Update LLM provider settings (called from Settings panel).
/// The new config is persisted to `config.yaml` and applied immediately.
#[tauri::command]
pub async fn update_llm_settings(
    settings: crate::LlmConfig,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    tracing::info!("updating LLM settings");

    state
        .llm_gateway
        .write()
        .await
        .update_config(settings.clone())
        .await
        .map_err(err)?;

    {
        let mut config = state.config.write().await;
        config.llm = settings;

        if let Err(e) = config.save() {
            tracing::error!("Failed to save LLM settings to config.yaml: {}", e);
        }
    }

    Ok(())
}

/// Retrieve current LLM provider settings (for display in Settings panel).
#[tauri::command]
pub async fn get_llm_settings(state: State<'_, AppState>) -> CommandResult<crate::LlmConfig> {
    let cfg = state.config.read().await.llm.clone();
    Ok(cfg)
}

/// Retrieve current LLM gateway metrics (for Dashboard).
#[tauri::command]
pub async fn get_llm_metrics(
    state: State<'_, AppState>,
) -> CommandResult<crate::llm_gateway::GatewayMetrics> {
    let metrics = state.llm_gateway.read().await.metrics().await;
    Ok(metrics)
}

/// Detect and return the context window limit of the currently configured provider.
#[tauri::command]
pub async fn detect_llm_limit(
    state: State<'_, AppState>,
    _model: Option<String>,
) -> CommandResult<usize> {
    let limit = state
        .llm_gateway
        .read()
        .await
        .detect_context_limit()
        .await
        .map_err(err)?;
    Ok(limit)
}

#[derive(serde::Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(serde::Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(serde::Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(serde::Deserialize)]
struct OpenAiModel {
    id: String,
}

#[tauri::command]
pub async fn get_available_models(
    provider: String,
    endpoint: Option<String>,
    _api_key: Option<String>,
) -> CommandResult<Vec<String>> {
    match provider.as_str() {
        "gemini" => Ok(vec![
            "gemini-1.5-pro".to_string(),
            "gemini-1.5-flash".to_string(),
            "gemini-1.5-flash-8b".to_string(),
            "gemini-2.0-flash-exp".to_string(),
        ]),
        "openai" => Ok(vec![
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "o1-preview".to_string(),
            "o1-mini".to_string(),
        ]),
        "anthropic" => Ok(vec![
            "claude-opus-4-7".to_string(),
            "claude-opus-4-6".to_string(),
            "claude-sonnet-4-6".to_string(),
            "claude-haiku-4-6".to_string(),
        ]),
        "z.ai" => Ok(vec!["zai-model".to_string()]),
        "ollama" => {
            let ep = endpoint.unwrap_or_else(|| "http://localhost:11434/v1".to_string());
            let base_ep = ep.trim_end_matches("/v1").trim_end_matches('/');
            let url = format!("{}/api/tags", base_ep);

            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(data) = resp.json::<OllamaTagsResponse>().await {
                    let mut models: Vec<String> = data.models.into_iter().map(|m| m.name).collect();
                    models.sort();
                    return Ok(models);
                }
            }
            Ok(vec![
                "deepseek-r1:1.5b".to_string(),
                "deepseek-r1:7b".to_string(),
                "deepseek-r1:8b".to_string(),
                "deepseek-r1:14b".to_string(),
                "deepseek-r1:32b".to_string(),
                "llama3.3".to_string(),
                "llama3.2".to_string(),
                "llama3".to_string(),
                "phi4".to_string(),
                "qwen2.5:0.5b".to_string(),
                "qwen2.5:7b".to_string(),
                "mistral".to_string(),
                "gemma2".to_string(),
            ])
        }
        "lmstudio" => {
            let ep = endpoint.unwrap_or_else(|| "http://localhost:1234/v1".to_string());
            let mut ep = ep.trim_end_matches('/').to_string();
            if !ep.ends_with("/v1") {
                ep.push_str("/v1");
            }
            let url = format!("{}/models", ep);

            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(data) = resp.json::<OpenAiModelsResponse>().await {
                    let mut models: Vec<String> = data.data.into_iter().map(|m| m.id).collect();
                    models.sort();
                    return Ok(models);
                }
            }
            Ok(vec!["local-model".to_string()])
        }
        _ => Ok(vec![]),
    }
}

/// Ping the configured LLM provider to verify connectivity.
/// Returns `true` if reachable, `false` otherwise (never errors).
#[tauri::command]
pub async fn ping_llm_provider(state: State<'_, AppState>) -> CommandResult<bool> {
    let ok = state
        .llm_gateway
        .read()
        .await
        .health_check()
        .await
        .unwrap_or(false);
    Ok(ok)
}

// ─────────────────────────────────────────────────────────────────────────────
// ── FILE BROWSER COMMANDS ───────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// List directory contents for the File Browser.
#[tauri::command]
pub async fn list_directory(path: String) -> CommandResult<Vec<FileEntry>> {
    use std::path::Path;

    let dir = Path::new(&path);
    if !dir.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {path}"));
    }

    let mut entries: Vec<FileEntry> = Vec::new();

    let read_dir = std::fs::read_dir(dir).map_err(err)?;
    for entry in read_dir.flatten() {
        let meta = entry.metadata().ok();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_path = entry.path().to_string_lossy().to_string();
        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
        let size_bytes = meta
            .as_ref()
            .and_then(|m| if m.is_file() { Some(m.len()) } else { None });
        let modified_at = meta.as_ref().and_then(|m| m.modified().ok()).map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        });
        let extension = if is_dir {
            None
        } else {
            entry
                .path()
                .extension()
                .map(|e| e.to_string_lossy().to_string())
        };

        entries.push(FileEntry {
            name: file_name,
            path: file_path,
            is_dir,
            size_bytes,
            modified_at,
            extension,
        });
    }

    // Directories first, then files; both sorted alphabetically
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// List generated artifacts from the system's temporary directory.
#[tauri::command]
pub async fn list_artifacts() -> CommandResult<Vec<FileEntry>> {
    let dir = std::env::temp_dir().join("office_hub_exports");
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<FileEntry> = Vec::new();

    let read_dir = std::fs::read_dir(&dir).map_err(err)?;
    for entry in read_dir.flatten() {
        let meta = entry.metadata().ok();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_path = entry.path().to_string_lossy().to_string();
        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
        let size_bytes = meta
            .as_ref()
            .and_then(|m| if m.is_file() { Some(m.len()) } else { None });
        let modified_at = meta.as_ref().and_then(|m| m.modified().ok()).map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        });
        let extension = if is_dir {
            None
        } else {
            entry
                .path()
                .extension()
                .map(|e| e.to_string_lossy().to_string())
        };

        entries.push(FileEntry {
            name: file_name,
            path: file_path,
            is_dir,
            size_bytes,
            modified_at,
            extension,
        });
    }

    // Sort by modified_at descending (newest first)
    entries.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

    Ok(entries)
}

/// Delete an artifact by its filename.
#[tauri::command]
pub async fn delete_file(filename: String) -> CommandResult<()> {
    let dir = std::env::temp_dir().join("office_hub_exports");
    let file_path = dir.join(&filename);

    // Security check
    if !file_path.starts_with(&dir)
        || filename.contains("..")
        || filename.contains("/")
        || filename.contains("\\")
    {
        return Err("Invalid filename".to_string());
    }

    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(err)?;
        tracing::info!(filename = %filename, "Artifact deleted via IPC");
    }

    // Also try deleting from the HTTP server public folder just in case they are different
    let public_dir = std::env::temp_dir().join("office_hub_exports");
    let pub_file_path = public_dir.join(&filename);
    if pub_file_path.exists() && pub_file_path.starts_with(&public_dir) {
        let _ = std::fs::remove_file(&pub_file_path);
    }

    Ok(())
}

/// Open a file with the default system application (Office, PDF viewer, etc.).
#[tauri::command]
#[allow(deprecated)]
pub async fn open_file(path: String, app: AppHandle) -> CommandResult<()> {
    tauri_plugin_shell::ShellExt::shell(&app)
        .open(&path, None::<tauri_plugin_shell::open::Program>)
        .map_err(err)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// ── WORKFLOW ENGINE COMMANDS ─────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// List all registered workflow definitions.
#[tauri::command]
pub async fn list_workflows(state: State<'_, AppState>) -> CommandResult<Vec<serde_json::Value>> {
    Ok(state.workflow_engine.list_workflows_json())
}

/// Manually trigger a workflow by its ID.
#[tauri::command]
pub async fn trigger_workflow(
    workflow_id: String,
    payload: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> CommandResult<WorkflowRunResult> {
    tracing::info!(workflow_id = %workflow_id, "manual workflow trigger");

    let run = state
        .workflow_engine
        .trigger(&workflow_id, payload)
        .await
        .map_err(err)?;

    Ok(WorkflowRunResult {
        run_id: run.run_id.to_string(),
        workflow_id: run.workflow_id,
        status: format!("{:?}", run.status).to_lowercase(),
        message: run.message,
        started_at: run.started_at.to_rfc3339(),
        finished_at: run
            .finished_at
            .map(|t: chrono::DateTime<chrono::Utc>| t.to_rfc3339()),
    })
}

/// Get the run history for a given workflow.
#[tauri::command]
pub async fn get_workflow_runs(
    workflow_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<WorkflowRunResult>> {
    let runs = state.workflow_engine.get_runs(&workflow_id);

    Ok(runs
        .into_iter()
        .map(|run| WorkflowRunResult {
            run_id: run.run_id.to_string(),
            workflow_id: run.workflow_id,
            status: format!("{:?}", run.status).to_lowercase(),
            message: run.message,
            started_at: run.started_at.to_rfc3339(),
            finished_at: run
                .finished_at
                .map(|t: chrono::DateTime<chrono::Utc>| t.to_rfc3339()),
        })
        .collect())
}

/// Get the full workflow definition by its ID.
#[tauri::command]
pub async fn get_workflow_definition(
    workflow_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<serde_json::Value>> {
    let def = state.workflow_engine.get_definition(&workflow_id);
    match def {
        Some(d) => Ok(Some(serde_json::to_value(&d).map_err(err)?)),
        None => Ok(None),
    }
}

/// Save a workflow definition (called from the React Flow UI).
#[tauri::command]
pub async fn save_workflow_definition(
    workflow: serde_json::Value,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    tracing::info!("save_workflow_definition invoked");
    let def: crate::workflow::WorkflowDefinition = serde_json::from_value(workflow).map_err(err)?;
    state
        .workflow_engine
        .save_definition(def)
        .await
        .map_err(err)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// ── AGENT STATUS COMMANDS ───────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// Return the current status of all sub-agents (for the dashboard).
#[tauri::command]
pub async fn get_agent_statuses(state: State<'_, AppState>) -> CommandResult<Vec<AgentInfo>> {
    let statuses = state.orchestrator.get_agent_statuses().await.map_err(err)?;
    Ok(statuses
        .into_iter()
        .map(|s| AgentInfo {
            id: s.id,
            name: s.name,
            status: s.status,
            last_used: s.last_used.map(|t| t.to_rfc3339()),
            capabilities: s.capabilities,
        })
        .collect())
}

// ─────────────────────────────────────────────────────────────────────────────
// ── MCP COMMANDS ────────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// List all registered MCP servers.
#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> CommandResult<Vec<serde_json::Value>> {
    state.orchestrator.list_mcp_servers().await.map_err(err)
}

/// Install a new MCP server by specifying its binary path or npm package.
#[tauri::command]
pub async fn install_mcp_server(
    source: String, // filesystem path or "npm:package-name"
    state: State<'_, AppState>,
) -> CommandResult<String> {
    tracing::info!(source = %source, "installing MCP server");
    let id = state
        .orchestrator
        .install_mcp_server(&source)
        .await
        .map_err(err)?;
    Ok(id)
}

/// Uninstall an MCP server by its ID.
#[tauri::command]
pub async fn uninstall_mcp_server(
    server_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state
        .orchestrator
        .uninstall_mcp_server(&server_id)
        .await
        .map_err(err)
}

/// Accept chart rendering response from Frontend (Base64 PNG string)
#[tauri::command]
pub async fn submit_chart_render(
    request_id: String,
    base64_image: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let mut map = state.chart_render_state.lock().await;
    if let Some(sender) = map.remove(&request_id) {
        let _ = sender.send(base64_image);
    } else {
        tracing::warn!(
            "Received chart render for unknown request_id: {}",
            request_id
        );
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// ── HUMAN-IN-THE-LOOP APPROVAL COMMANDS ─────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// Approve a pending Human-in-the-Loop action.
/// Called from both the Desktop UI and the Mobile app (via WebSocket relay).
#[tauri::command]
pub async fn approve_action(action_id: String, state: State<'_, AppState>) -> CommandResult<()> {
    tracing::info!(action_id = %action_id, "HITL action APPROVED");
    state
        .orchestrator
        .resolve_hitl(&action_id, true)
        .await
        .map_err(err)
}

/// Reject a pending Human-in-the-Loop action.
#[tauri::command]
pub async fn reject_action(
    action_id: String,
    reason: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    tracing::info!(action_id = %action_id, ?reason, "HITL action REJECTED");
    state
        .orchestrator
        .resolve_hitl(&action_id, false)
        .await
        .map_err(err)
}

/// List all currently pending HITL approval requests.
#[tauri::command]
pub async fn list_pending_approvals(
    state: State<'_, AppState>,
) -> CommandResult<Vec<serde_json::Value>> {
    state.orchestrator.list_pending_hitl().await.map_err(err)
}

// ─────────────────────────────────────────────────────────────────────────────
// ── SYSTEM / UTILITY COMMANDS ───────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// ── MCP SKILL BUILDER COMMANDS (PHASE 7) ────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// Starts learning a new skill from a documentation URL and generates MCP code.
#[tauri::command]
pub async fn start_skill_learning(
    source_url: String,
    state: State<'_, AppState>,
) -> CommandResult<serde_json::Value> {
    tracing::info!(source_url = %source_url, "Starting skill learning");

    let task = crate::orchestrator::AgentTask {
        task_id: uuid::Uuid::new_v4().to_string(),
        action: "learn_skill_from_docs".to_string(),
        intent: crate::orchestrator::intent::Intent::Ambiguous(Default::default()),
        message: format!("Học skill từ {}", source_url),
        context_file: None,
        session_id: "system".to_string(),
        parameters: {
            let mut map = std::collections::HashMap::new();
            map.insert("url".to_string(), serde_json::Value::String(source_url));
            map
        },
        llm_gateway: None,
        global_policy: None,
        knowledge_context: None,
        parent_task_id: None,
        dependencies: vec![],
    };

    let out = state
        .orchestrator
        .execute_agent_action("converter", task)
        .await
        .map_err(err)?;
    Ok(out.metadata.unwrap_or_else(|| serde_json::json!({})))
}

/// Tests a generated MCP server by installing it into the sandbox.
#[tauri::command]
pub async fn test_skill_sandbox(
    script_path: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    tracing::info!(script_path = %script_path, "Testing skill in sandbox");
    let source = if script_path.ends_with(".md") {
        format!("md:{}", script_path)
    } else if script_path.ends_with(".py") {
        format!("python:{}", script_path)
    } else if script_path.ends_with(".ps1") {
        format!("powershell:{}", script_path)
    } else {
        format!("python:{}", script_path)
    };
    let server_id = state
        .orchestrator
        .install_mcp_server(&source)
        .await
        .map_err(err)?;
    Ok(server_id)
}

/// Saves edits to a skill file before sandbox testing
#[tauri::command]
pub async fn save_skill_file(script_path: String, content: String) -> CommandResult<()> {
    tokio::fs::write(&script_path, content).await.map_err(err)?;
    Ok(())
}

fn extract_yaml_frontmatter(content: &str) -> Option<String> {
    if !content.starts_with("---") {
        return None;
    }
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return None;
    }
    Some(parts[1].trim().to_string())
}

fn get_skills_dir() -> std::path::PathBuf {
    let mut base_dir = std::env::current_dir().unwrap_or_default();
    if base_dir.ends_with("src-tauri") {
        if let Some(parent) = base_dir.parent() {
            base_dir = parent.to_path_buf();
        }
    }
    base_dir.join(".agent").join("skills")
}

/// List all installed skills by reading their SKILL.md frontmatter.
#[tauri::command]
pub async fn list_installed_skills() -> CommandResult<Vec<serde_json::Value>> {
    let mut results = Vec::new();
    let agent_dir = get_skills_dir();

    if !agent_dir.exists() {
        return Ok(results);
    }

    if let Ok(entries) = std::fs::read_dir(agent_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let md_path = path.join("SKILL.md");
                if md_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&md_path) {
                        if let Some(yaml) = extract_yaml_frontmatter(&content) {
                            if let Ok(meta) = serde_yaml::from_str::<serde_json::Value>(&yaml) {
                                let mut obj = meta.clone();
                                if let Some(m) = obj.as_object_mut() {
                                    m.insert(
                                        "id".to_string(),
                                        serde_json::Value::String(
                                            path.file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string(),
                                        ),
                                    );
                                    m.insert(
                                        "path".to_string(),
                                        serde_json::Value::String(
                                            md_path.to_string_lossy().to_string(),
                                        ),
                                    );
                                }
                                results.push(obj);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(results)
}

/// Read raw Markdown content of a skill.
#[tauri::command]
pub async fn read_skill_file(skill_id: String) -> CommandResult<String> {
    let md_path = get_skills_dir().join(&skill_id).join("SKILL.md");

    if md_path.exists() {
        std::fs::read_to_string(md_path).map_err(err)
    } else {
        Err("Skill file not found".to_string())
    }
}

/// Delete a skill folder entirely.
#[tauri::command]
pub async fn delete_skill_folder(skill_id: String) -> CommandResult<()> {
    if skill_id.contains("..") || skill_id.contains("/") || skill_id.contains("\\") {
        return Err("Invalid skill ID".to_string());
    }
    let folder_path = get_skills_dir().join(&skill_id);

    if folder_path.exists() {
        std::fs::remove_dir_all(folder_path).map_err(err)
    } else {
        Err("Skill folder not found".to_string())
    }
}

/// Evaluates a generated MCP server script using the LLM.
#[tauri::command]
pub async fn evaluate_skill(
    script_path: String,
    state: State<'_, AppState>,
) -> CommandResult<serde_json::Value> {
    tracing::info!(script_path = %script_path, "Evaluating skill");

    // Read the script file
    let script_content = std::fs::read_to_string(&script_path).map_err(err)?;

    // Prepare prompt for LLM
    let prompt = format!(
        "Bạn là một chuyên gia bảo mật và kiến trúc phần mềm. Hãy đánh giá đoạn mã MCP Server sau đây và đưa ra báo cáo dưới định dạng JSON:\n\n\
        {{\n  \"strengths\": [\"điểm mạnh 1\", \"điểm mạnh 2\"],\n  \"weaknesses\": [\"điểm yếu 1\", \"điểm yếu 2\"],\n  \"security\": [\"lưu ý bảo mật 1\", \"lưu ý bảo mật 2\"]\n}}\n\n\
        Mã nguồn:\n\n{}",
        script_content
    );

    let llm_req = crate::llm_gateway::LlmRequest::new(vec![
        crate::llm_gateway::LlmMessage::system("Trả về duy nhất dữ liệu JSON.".to_string()),
        crate::llm_gateway::LlmMessage::user(prompt),
    ])
    .with_json_schema(serde_json::json!({
        "type": "object",
        "properties": {
            "strengths": { "type": "array", "items": { "type": "string" } },
            "weaknesses": { "type": "array", "items": { "type": "string" } },
            "security": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["strengths", "weaknesses", "security"]
    }));

    // We need to access the LlmGateway directly
    let gateway_arc = state.llm_gateway.clone();
    let gateway = gateway_arc.read().await;
    let resp = gateway.complete(llm_req).await.map_err(err)?;

    let parsed: serde_json::Value = serde_json::from_str(&resp.content).unwrap_or_else(|_| {
        serde_json::json!({
            "strengths": ["Failed to parse AI strengths"],
            "weaknesses": ["Failed to parse AI weaknesses"],
            "security": ["Failed to parse AI security notes"]
        })
    });

    Ok(parsed)
}

/// Approves a tested skill, persisting it (currently just confirms it).
#[tauri::command]
pub async fn approve_new_skill(
    server_id: String,
    _state: State<'_, AppState>,
) -> CommandResult<()> {
    tracing::info!(server_id = %server_id, "Skill approved");
    Ok(())
}

/// Calls a specific tool on a specific MCP server directly.
#[tauri::command]
pub async fn call_mcp_tool(
    server_id: String,
    tool_name: String,
    arguments: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> CommandResult<serde_json::Value> {
    tracing::info!(server_id = %server_id, tool_name = %tool_name, "Directly calling MCP tool");

    let result = state
        .orchestrator
        .call_mcp_tool(&tool_name, arguments)
        .await
        .map_err(err)?;
    serde_json::to_value(result).map_err(err)
}

/// Return app version and build metadata.
#[tauri::command]
pub fn get_app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name":    env!("CARGO_PKG_NAME"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "buildDate": chrono::Utc::now().to_rfc3339(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

/// Check system requirements (Office installed, COM accessible, UIA accessible).
#[tauri::command]
pub async fn check_system_requirements(
    state: State<'_, AppState>,
) -> CommandResult<serde_json::Value> {
    let checks = state
        .orchestrator
        .check_system_requirements()
        .await
        .map_err(err)?;
    Ok(checks)
}

/// Export all audit logs for a given date range (YYYY-MM-DD).
#[tauri::command]
pub async fn export_audit_logs(
    from_date: String,
    to_date: String,
    output_path: String,
) -> CommandResult<String> {
    tracing::info!(from = %from_date, to = %to_date, out = %output_path, "exporting audit logs");
    // TODO(phase-4): implement log export from the structured log store
    Ok(format!("Audit log export to {} is scheduled.", output_path))
}

/// Fetch telemetry logs for the dashboard
#[tauri::command]
pub async fn get_telemetry_logs(
    limit: usize,
    state: State<'_, AppState>,
) -> CommandResult<Vec<serde_json::Value>> {
    let orchestrator = state.orchestrator.clone();

    // We need to add a get_telemetry function to MemoryStore
    let inner = orchestrator.0.read().await;
    let mut logs = Vec::new();

    if let Some(mem) = &inner.memory_store {
        // Fallback to direct SQLite access since we haven't added get_telemetry to MemoryStore yet
        let conn = mem
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))
            .map_err(err)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, agent_name, action, latency_ms, tokens_used, status, timestamp FROM telemetry_logs ORDER BY timestamp DESC LIMIT ?1"
        ).map_err(err)?;

        let results = stmt
            .query_map(rusqlite::params![limit], |row: &rusqlite::Row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "sessionId": row.get::<_, String>(1)?,
                    "agentName": row.get::<_, String>(2)?,
                    "action": row.get::<_, String>(3)?,
                    "latencyMs": row.get::<_, i64>(4)?,
                    "tokensUsed": row.get::<_, i64>(5)?,
                    "status": row.get::<_, String>(6)?,
                    "timestamp": row.get::<_, String>(7)?,
                }))
            })
            .map_err(err)?;

        for val in results.flatten() {
            logs.push(val);
        }
    }

    Ok(logs)
}

// ─────────────────────────────────────────────────────────────────────────────
// ── COMMAND REGISTRATION HELPER ─────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the `generate_handler![]` macro invocation for use in `lib.rs`.
///
/// Usage in `lib.rs`:
/// ```rust,ignore
/// tauri::Builder::default()
///     .invoke_handler(all_commands())
///     …
/// ```
#[tauri::command]
pub async fn setup_local_ai() -> Result<String, String> {
    let current_dir = std::env::current_dir().unwrap_or_default();
    let script_path = current_dir.join("scripts").join("setup_ollama.ps1");

    // Check if script exists
    if !script_path.exists() {
        return Err("Setup script not found".to_string());
    }

    let output = tokio::process::Command::new("powershell")
        .args([
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path.to_str().unwrap(),
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to execute script: {}", e))?;

    if output.status.success() {
        Ok("Local AI setup completed successfully.".to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Setup failed: {}", err))
    }
}

#[macro_export]
macro_rules! all_commands {
    () => {
        tauri::generate_handler![
            // Orchestrator / Chat
            $crate::commands::send_chat_message,
            $crate::commands::create_session,
            $crate::commands::delete_session,
            $crate::commands::list_sessions,
            $crate::commands::get_session_history,
            // LLM Gateway
            $crate::commands::update_llm_settings,
            $crate::commands::get_llm_settings,
            $crate::commands::ping_llm_provider,
            $crate::commands::detect_llm_limit,
            $crate::commands::get_available_models,
            $crate::commands::get_llm_metrics,
            // File Browser
            $crate::commands::list_directory,
            $crate::commands::open_file,
            $crate::commands::list_artifacts,
            $crate::commands::delete_file,
            // Workflow Engine
            $crate::commands::list_workflows,
            $crate::commands::trigger_workflow,
            $crate::commands::get_workflow_runs,
            $crate::commands::get_workflow_definition,
            $crate::commands::save_workflow_definition,
            // Agent status
            $crate::commands::get_agent_statuses,
            // MCP
            $crate::commands::list_mcp_servers,
            $crate::commands::install_mcp_server,
            $crate::commands::uninstall_mcp_server,
            // HITL approvals
            $crate::commands::approve_action,
            $crate::commands::reject_action,
            $crate::commands::list_pending_approvals,
            // System
            $crate::commands::get_app_info,
            $crate::commands::check_system_requirements,
            $crate::commands::export_audit_logs,
            // MCP Skill Builder
            $crate::commands::start_skill_learning,
            $crate::commands::test_skill_sandbox,
            $crate::commands::evaluate_skill,
            $crate::commands::approve_new_skill,
            $crate::commands::call_mcp_tool,
            $crate::commands::setup_local_ai,
            $crate::commands::save_skill_file,
            $crate::commands::list_installed_skills,
            $crate::commands::read_skill_file,
            $crate::commands::delete_skill_folder,
        ]
    };
}
