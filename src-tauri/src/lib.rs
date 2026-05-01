// ============================================================================
// Office Hub – lib.rs
// Root library crate: module declarations + Tauri application bootstrap
// ============================================================================

#![allow(clippy::empty_line_after_doc_comments)]
#![allow(clippy::empty_line_after_outer_attr)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::invisible_characters)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::manual_find)]
#![allow(clippy::manual_strip)]

// ─────────────────────────────────────────────
// Module declarations
// ─────────────────────────────────────────────

/// Central orchestrator: intent parsing, routing, session state, rule engine.
pub mod orchestrator;

/// Sub-agents: Analyst (Excel), Office Master (Word/PPT),
/// Web Researcher (UIA), Converter (MCP skill learning).
pub mod agents;

/// LLM provider abstraction: Gemini, OpenAI, Ollama, LM Studio,
/// token caching and hybrid-mode fallback.
pub mod llm_gateway;

/// MCP (Model Context Protocol) Host – registers, discovers and
/// calls MCP Server plugins.
pub mod mcp;

pub mod crdt;
/// Event-driven workflow engine: triggers, actions, YAML loader.
pub mod workflow;

/// WebSocket server for Office Web Add-in communication and
/// Human-in-the-Loop approval flow.
pub mod websocket;

/// MCP-Hybrid transport: simplified MCP structs for SSE+REST.
pub mod mcp_transport;

/// SSE+REST hybrid server for Mobile Client (MCP-Hybrid protocol).
pub mod sse_server;

/// Tauri IPC command handlers exposed to the React frontend.
pub mod commands;

/// HTTP Server for static file serving to mobile client.
pub mod http_server;

/// HTTPS server that serves the pre-built Office Web Add-in UI on port 3000.
/// Replaces the separate Node.js/Vite dev-server process.
pub mod addin_server;

/// System layer: tray icon, startup registration, OS integrations.
pub mod system;

/// Local knowledge base: `.md` file management
pub mod knowledge;

// ─────────────────────────────────────────────
// Re-exports used across the crate
// ─────────────────────────────────────────────

pub use llm_gateway::LlmGateway;
pub use orchestrator::{Orchestrator, OrchestratorHandle};
pub use workflow::WorkflowEngine;

// ─────────────────────────────────────────────
// Shared error type
// ─────────────────────────────────────────────

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Orchestrator error: {0}")]
    Orchestrator(String),

    #[error("Agent error [{agent}]: {message}")]
    Agent { agent: String, message: String },

    #[error("LLM Gateway error: {0}")]
    LlmGateway(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("Workflow error: {0}")]
    Workflow(String),

    #[error("COM Automation error (HRESULT {hresult:#010x}): {message}")]
    Com { hresult: u32, message: String },

    #[error("UI Automation error: {0}")]
    UiAutomation(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Tauri commands automatically serialize errors as strings.
/// This impl converts AppError into a string that Tauri can send to the frontend.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

// ─────────────────────────────────────────────
// Application state (shared across Tauri commands)
// ─────────────────────────────────────────────

use std::sync::Arc;
use tokio::sync::RwLock;

/// Global application state injected into every Tauri command via `tauri::State`.
pub struct AppState {
    pub orchestrator: OrchestratorHandle,
    pub llm_gateway: Arc<RwLock<LlmGateway>>,
    pub config: Arc<RwLock<AppConfig>>,
    pub workflow_engine: Arc<WorkflowEngine>,
    pub system_manager: system::SystemManager,
    /// WebSocket server — kept for Office Web Add-in (Word/Excel/Outlook)
    pub websocket_server: Arc<websocket::WebSocketServer>,
    pub hitl_manager: Arc<crate::orchestrator::HitlManager>,
    /// SSE hybrid server state — use broadcast_event() to route events to mobile clients
    pub hybrid_state: Arc<crate::sse_server::HybridServerState>,
    /// CRDT Manager for real-time document collaboration
    pub crdt_manager: Arc<crate::crdt::CrdtManager>,
    /// Chart render state for ChartServer to wait for frontend Base64 responses
    pub chart_render_state: Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>>,
    >,
}

// ─────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// Top-level application configuration (loaded from `config.yaml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// LLM provider settings
    pub llm: LlmConfig,
    /// WebSocket server settings
    pub websocket: websocket::WebSocketConfig,
    /// Agent-specific settings
    pub agents: AgentsConfig,
    /// Paths to rule and workflow definition files
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub fast_provider: String,
    pub fast_model: String,

    pub default_provider: String,
    pub default_model: String,

    pub reasoning_provider: String,
    pub reasoning_model: String,

    pub credentials: ProviderCredentials,
    /// Max tokens to keep in session context before summarisation
    pub context_window_limit: usize,
    /// Enable prompt/response token caching
    pub token_cache_enabled: bool,
    /// Automatically create handoff documents when session > 80% limit
    #[serde(default)]
    pub auto_handoff_enabled: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            fast_provider: "ollama".to_string(),
            fast_model: "qwen2.5-coder:7b".to_string(),

            default_provider: "gemini".to_string(),
            default_model: "gemini-2.0-flash".to_string(),

            reasoning_provider: "anthropic".to_string(),
            reasoning_model: "claude-3-5-sonnet-20241022".to_string(),

            credentials: ProviderCredentials::default(),
            context_window_limit: 32000,
            token_cache_enabled: true,
            auto_handoff_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderCredentials {
    pub gemini_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub zai_api_key: Option<String>,
    pub ollama_endpoint: Option<String>,
    pub lmstudio_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    pub analyst: AnalystAgentConfig,
    pub office_master: OfficeMasterAgentConfig,
    pub web_researcher: WebResearcherConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalystAgentConfig {
    /// Allow VBA macro generation and execution
    pub allow_vba_execution: bool,
    /// Max rows to process in a single Power Query operation
    pub max_rows_per_query: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeMasterAgentConfig {
    /// Default Word template path
    pub default_word_template: Option<String>,
    /// Default PPT template path
    pub default_ppt_template: Option<String>,
    /// Preserve document formatting when updating content
    pub preserve_format: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebResearcherConfig {
    /// Preferred browser: "edge" | "chrome"
    pub preferred_browser: String,
    /// Screenshot grounding: capture evidence images
    pub screenshot_grounding: bool,
    /// Require user approval before any form-fill or navigation action
    pub require_approval_for_navigation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Directory containing YAML rule files
    pub rules_dir: String,
    /// Directory containing YAML workflow definitions
    pub workflows_dir: String,
    /// Directory for local vector DB (RAG)
    pub vector_db_dir: String,
    /// Session state persistence directory
    pub sessions_dir: String,
    /// Audit log directory
    pub audit_log_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                credentials: ProviderCredentials {
                    gemini_api_key: None,
                    openai_api_key: None,
                    anthropic_api_key: None,
                    zai_api_key: None,
                    ollama_endpoint: Some("http://localhost:11434/v1".to_string()),
                    lmstudio_endpoint: Some("http://localhost:1234/v1".to_string()),
                },
                ..LlmConfig::default()
            },
            websocket: websocket::WebSocketConfig {
                host: "0.0.0.0".to_string(),
                port: 9001,
                max_clients: 5,
                require_approval_for_sensitive: true,
                auth_secret: None,
                idle_timeout_seconds: 300,
            },
            agents: AgentsConfig {
                analyst: AnalystAgentConfig {
                    allow_vba_execution: false,
                    max_rows_per_query: 100_000,
                },
                office_master: OfficeMasterAgentConfig {
                    default_word_template: None,
                    default_ppt_template: None,
                    preserve_format: true,
                },
                web_researcher: WebResearcherConfig {
                    preferred_browser: "edge".to_string(),
                    screenshot_grounding: true,
                    require_approval_for_navigation: true,
                },
            },
            paths: PathsConfig {
                rules_dir: "rules".to_string(),
                workflows_dir: "workflows".to_string(),
                vector_db_dir: "data/vectors".to_string(),
                sessions_dir: "data/sessions".to_string(),
                audit_log_dir: "data/audit".to_string(),
            },
        }
    }
}

// ─────────────────────────────────────────────
// Config loader
// ─────────────────────────────────────────────

impl AppConfig {
    /// Returns the canonical path for `config.yaml`.
    /// Stored in `%APPDATA%/office-hub/config.yaml` so it survives `cargo clean`.
    fn config_path() -> Option<std::path::PathBuf> {
        // Use %APPDATA% on Windows, $HOME/.config on other platforms
        let base = std::env::var("APPDATA")
            .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
            .ok()
            .map(std::path::PathBuf::from);

        if let Some(appdata) = base {
            let dir = appdata.join("office-hub");
            if std::fs::create_dir_all(&dir).is_ok() {
                return Some(dir.join("config.yaml"));
            }
        }
        // Fallback: beside the executable
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("config.yaml")))
    }

    /// Load configuration from `config.yaml`.
    /// Falls back to `AppConfig::default()` if the file doesn't exist.
    pub fn load() -> Self {
        if let Some(config_path) = Self::config_path() {
            if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(content) => match serde_yaml::from_str::<AppConfig>(&content) {
                        Ok(cfg) => {
                            tracing::info!("Configuration loaded from {:?}", config_path);
                            return cfg;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse config.yaml: {}. Using defaults.", e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to read config.yaml: {}. Using defaults.", e);
                    }
                }
            }
        }

        tracing::info!("No config.yaml found, using default configuration.");
        Self::default()
    }

    /// Save configuration to `config.yaml`.
    pub fn save(&self) -> AppResult<()> {
        if let Some(config_path) = Self::config_path() {
            let content = serde_yaml::to_string(self)
                .map_err(|e| AppError::Config(format!("Failed to serialize config: {}", e)))?;
            std::fs::write(&config_path, content)
                .map_err(|e| AppError::Config(format!("Failed to write config: {}", e)))?;
            tracing::info!("Configuration saved to {:?}", config_path);
        } else {
            tracing::warn!("Could not determine config directory.");
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────
// Tauri application entry point
// ─────────────────────────────────────────────

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialise structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "office_hub_lib=debug,warn".into()),
        )
        .with_target(true)
        .with_thread_ids(true)
        .compact()
        .init();

    tracing::info!("Office Hub starting…");

    // Load application configuration
    let mut config = AppConfig::load();

    // Ensure we have a secure WebSocket auth token
    if config.websocket.auth_secret.is_none() {
        let token = uuid::Uuid::new_v4().to_string().replace("-", "");
        config.websocket.auth_secret = Some(token);
        if let Err(e) = config.save() {
            tracing::warn!("Failed to save new auth_secret to config: {}", e);
        }
    }

    // Build shared application state
    let llm_gateway = Arc::new(RwLock::new(LlmGateway::new(config.llm.clone())));
    let hitl_manager = Arc::new(crate::orchestrator::HitlManager::new());
    let mut orchestrator = Orchestrator::new(Arc::clone(&llm_gateway), Arc::clone(&hitl_manager));

    // Register all core agents into the registry
    orchestrator
        .agent_registry
        .register(Box::new(crate::agents::analyst::AnalystAgent::new()));
    orchestrator.agent_registry.register(Box::new(
        crate::agents::office_master::OfficeMasterAgent::new(),
    ));
    orchestrator.agent_registry.register(Box::new(
        crate::agents::web_researcher::WebResearcherAgent::new(Default::default()),
    ));
    orchestrator
        .agent_registry
        .register(Box::new(crate::agents::converter::ConverterAgent::new()));
    orchestrator.agent_registry.register(Box::new(
        crate::agents::folder_scanner::FolderScannerAgent::new(),
    ));
    orchestrator
        .agent_registry
        .register(Box::new(crate::agents::outlook::OutlookAgent::new()));
    orchestrator
        .agent_registry
        .register(Box::new(crate::agents::system::SystemAgent::new()));
    orchestrator
        .agent_registry
        .register(Box::new(crate::agents::win32_admin::Win32AdminAgent::new()));

    let orchestrator_handle = OrchestratorHandle::new(orchestrator);

    if let Err(e) = tauri::async_runtime::block_on(
        orchestrator_handle.init_persistence(&config.paths.sessions_dir),
    ) {
        tracing::warn!("Failed to init session persistence: {}", e);
    }

    // Build workflow engine (load definitions from workflows dir).
    // WorkflowEngine::new handles non-existent directories gracefully (empty definitions).
    let workflow_engine =
        tauri::async_runtime::block_on(WorkflowEngine::new(&config.paths.workflows_dir))
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to initialise WorkflowEngine: {e}. Using empty engine.");
                Arc::new(WorkflowEngine::empty())
            });

    tauri::async_runtime::block_on(workflow_engine.set_orchestrator(orchestrator_handle.clone()));

    let system_config = system::SystemConfig {
        ws_auth_token: config.websocket.auth_secret.clone(),
        websocket_port: config.websocket.port,
        ..Default::default()
    };

    let system_manager = tauri::async_runtime::block_on(system::SystemManager::init(system_config))
        .expect("Failed to init SystemManager");

    let (ws_command_tx, mut ws_command_rx) = tokio::sync::mpsc::channel(32);
    let ws_server = Arc::new(websocket::WebSocketServer::new(
        config.websocket.clone(),
        ws_command_tx,
    ));

    // MCP-Hybrid: mobile command channel (REST uplink → orchestrator)
    let (mobile_cmd_tx, mut mobile_cmd_rx) =
        tokio::sync::mpsc::channel::<crate::mcp_transport::IncomingMobileCmd>(64);
    // SSE broadcast sender shared across AppState and sse_server
    let sse_port = config.websocket.port + 1; // e.g. 9001+1=9002
    let (hybrid_state, _sse_tx) = crate::sse_server::HybridServerState::new(
        mobile_cmd_tx,
        config.websocket.auth_secret.clone(),
        std::env::temp_dir().join("office_hub_exports"),
    );
    let hybrid_state = Arc::new(hybrid_state);

    let app_state = AppState {
        orchestrator: orchestrator_handle,
        llm_gateway,
        config: Arc::new(RwLock::new(config)),
        workflow_engine,
        system_manager: system_manager.clone(),
        websocket_server: Arc::clone(&ws_server),
        hitl_manager: Arc::clone(&hitl_manager),
        hybrid_state: Arc::clone(&hybrid_state),
        crdt_manager: Arc::new(crate::crdt::CrdtManager::new()),
        chart_render_state: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    };

    tauri::Builder::default()
        // ── Plugins ─────────────────────────────────────────
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_http::init())
        // ── Global state ────────────────────────────────────
        .manage(app_state)
        .manage(system_manager.clone())
        // ── IPC command handlers ─────────────────────────────
        .invoke_handler(tauri::generate_handler![
            // Chat / Orchestrator
            commands::send_chat_message,
            commands::raw_llm_request,
            commands::create_session,
            commands::delete_session,
            commands::list_sessions,
            commands::get_session_history,
            // LLM Gateway
            commands::update_llm_settings,
            commands::get_llm_settings,
            commands::ping_llm_provider,
            commands::detect_llm_limit,
            commands::get_available_models,
            commands::get_llm_metrics,
            // File operations
            commands::list_directory,
            commands::list_artifacts,
            commands::open_file,
            // Workflow
            commands::list_workflows,
            commands::trigger_workflow,
            commands::get_workflow_runs,
            commands::get_workflow_definition,
            commands::save_workflow_definition,
            // Agent statuses
            commands::get_agent_statuses,
            // MCP
            commands::list_mcp_servers,
            commands::install_mcp_server,
            commands::uninstall_mcp_server,
            commands::submit_chart_render,
            commands::start_skill_learning,
            commands::test_skill_sandbox,
            commands::save_skill_file,
            commands::evaluate_skill,
            commands::approve_new_skill,
            commands::call_mcp_tool,
            commands::list_installed_skills,
            commands::read_skill_file,
            commands::delete_skill_folder,
            // HITL
            commands::approve_action,
            commands::reject_action,
            commands::list_pending_approvals,
            // System
            commands::get_app_info,
            commands::check_system_requirements,
            commands::export_audit_logs,
            commands::get_telemetry_logs,
            system::commands::get_system_config,
            system::commands::save_system_config,
            system::commands::toggle_startup,
            system::commands::get_pairing_qr,
            system::commands::get_system_status,
            system::commands::get_startup_enabled,
            system::commands::get_network_info,
            system::commands::get_tailscale_status,
            system::commands::refresh_network,
            system::commands::suppress_sleep,
            system::commands::release_sleep,
            system::commands::install_office_addin,
            // Knowledge Base
            knowledge::list_knowledge,
            knowledge::list_skills_metadata,
            knowledge::read_knowledge_file,
            knowledge::save_knowledge_file,
            knowledge::delete_knowledge_file,
            // Workspace Management
            knowledge::list_workspaces,
            knowledge::create_workspace,
            knowledge::delete_workspace,
            knowledge::get_workspace_path,
            // Workspace Links
            knowledge::list_workspace_links,
            knowledge::add_workspace_link,
            knowledge::remove_workspace_link,
        ])
        // ── Window setup ─────────────────────────────────────
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let cfg = tauri::async_runtime::block_on(async {
                    state.system_manager.config.read().await.clone()
                });

                if cfg.minimise_to_tray {
                    window.hide().unwrap();
                    api.prevent_close();
                }
            }
        })
        .setup(move |app| {
            tracing::info!("Tauri setup running…");

            // Start the COM Anti-Deadlock watchdog
            crate::agents::com_utils::watchdog::spawn_com_watchdog();

            // Download and install Ollama on first launch if missing
            tauri::async_runtime::spawn(async {
                if let Err(e) = system::setup::ensure_ollama_installed().await {
                    tracing::error!("Failed to ensure Ollama installation: {}", e);
                }
            });

            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            let args: Vec<String> = std::env::args().collect();
            if args.iter().any(|arg| arg == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            if let Err(e) = system::tray::setup_tray(app.handle()) {
                tracing::warn!("Failed to setup system tray: {}", e);
            }

            let app_handle = app.handle().clone();
            let state = app_handle.state::<AppState>();

            let ws_server_clone = Arc::clone(&state.websocket_server);
            let _hybrid = Arc::clone(&state.hybrid_state);
            let orchestrator_clone = state.orchestrator.clone();
            let hitl_clone = state.hitl_manager.clone();

            if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
                let knowledge_dir = app_data_dir.join("knowledge");
                let policy_dir = app_data_dir.join("policies");
                let mut base_dir = std::env::current_dir().unwrap_or_default();
                if base_dir.ends_with("src-tauri") {
                    base_dir = base_dir.parent().unwrap().to_path_buf();
                }
                let skills_dir = base_dir.join(".agent").join("skills");
                let memory_db_path = app_data_dir.join("memory.sqlite");

                // Ensure directories exist
                let _ = std::fs::create_dir_all(&knowledge_dir);
                let _ = std::fs::create_dir_all(&policy_dir);
                let _ = std::fs::create_dir_all(&skills_dir);

                let memory_store = match crate::orchestrator::memory::MemoryStore::new(memory_db_path) {
                    Ok(m) => Some(Arc::new(m)),
                    Err(e) => {
                        tracing::error!("Failed to init MemoryStore: {}", e);
                        None
                    }
                };

                tauri::async_runtime::block_on(async {
                    orchestrator_clone.set_knowledge_dir(knowledge_dir).await;
                    orchestrator_clone.set_policy_dir(policy_dir).await;
                    orchestrator_clone.set_skills_dir(skills_dir).await;
                    orchestrator_clone.register_fs_server().await;
                    orchestrator_clone.register_win32_admin_server().await;
                    orchestrator_clone.register_scripting_server(app_handle.clone()).await;
                    orchestrator_clone.register_web_search_server().await;
                    orchestrator_clone.register_web_fetch_server().await;
                    orchestrator_clone.register_analytic_server().await;
                    orchestrator_clone.register_chart_server(app_handle.clone()).await;
                    orchestrator_clone.register_native_chart_server().await;
                    orchestrator_clone.register_agent_adapters().await;
                    if let Some(ms) = memory_store {
                        orchestrator_clone.set_memory_store(ms).await;
                    }
                });
            }

            let orchestrator_for_progress = orchestrator_clone.clone();
            let app_handle_for_progress = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                orchestrator_clone.set_ws_server(Arc::clone(&ws_server_clone)).await;
                hitl_clone.set_ws_server(Arc::clone(&ws_server_clone));
                orchestrator_clone.register_office_com_server().await;

                // Start WebSocket server (Office Web Add-in)
                if let Err(e) = ws_server_clone.start().await {
                    tracing::error!("WebSocket (Add-in) server failed to start: {}", e);
                }
            });

            // Start MCP-Hybrid SSE+REST server (Mobile client)
            let mut hybrid_state_owned = (*hybrid_state).clone();
            if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
                hybrid_state_owned.app_data_dir = Some(app_data_dir);
            }
            let orchestrator_for_hybrid = state.orchestrator.clone();
            tauri::async_runtime::spawn(async move {
                crate::sse_server::start_hybrid_server(sse_port, hybrid_state_owned, orchestrator_for_hybrid).await;
            });

            // Start built-in HTTPS server for Office Web Add-in (port 3000)
            // Resolves dist dir from Tauri resource_dir (release) or workspace path (dev).
            let resource_dir = app_handle.path().resource_dir().ok();
            if let Some(dist_dir) = crate::addin_server::resolve_dist_dir(resource_dir) {
                tauri::async_runtime::spawn(async move {
                    crate::addin_server::start_addin_server(dist_dir).await;
                });
            } else {
                tracing::warn!(
                    "Add-in HTTPS server: could not locate office-addin/dist/. \
                     Run `npm run build` in office-addin/ then restart Office Hub."
                );
            }

            let workflow_engine = Arc::clone(&state.workflow_engine);
            let app_handle_clone = app_handle.clone();
            let ws_server_for_workflow = Arc::clone(&state.websocket_server);
            let hybrid_for_workflow = Arc::clone(&state.hybrid_state);
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                let mut status_rx = workflow_engine.subscribe_status();
                while let Ok(update) = status_rx.recv().await {
                    if let Err(e) = app_handle_clone.emit("workflow_progress", &update) {
                        tracing::warn!("Failed to emit workflow_progress event: {}", e);
                    }

                    // Relay to Add-in via WebSocket + Mobile via SSE
                    match &update {
                        crate::workflow::WorkflowProgressUpdate::Run { run_id, workflow_id, workflow_name, status, message, .. } => {
                            let status_str = format!("{:?}", status).to_lowercase();
                            ws_server_for_workflow.relay_workflow_status(run_id, workflow_id, workflow_name, &status_str, message.clone()).await;
                            hybrid_for_workflow.broadcast_event(
                                crate::mcp_transport::SseEvent::status(None, run_id, workflow_name, &status_str, message.clone())
                            );
                        }
                        crate::workflow::WorkflowProgressUpdate::Step { run_id, workflow_id, step_name, status, message, .. } => {
                            let status_str = format!("{:?}", status).to_lowercase();
                            ws_server_for_workflow.relay_workflow_status(run_id, workflow_id, step_name, &status_str, message.clone()).await;
                            hybrid_for_workflow.broadcast_event(
                                crate::mcp_transport::SseEvent::status(None, run_id, step_name, &status_str, message.clone())
                            );
                        }
                        crate::workflow::WorkflowProgressUpdate::Thought { session_id, thought } => {
                            hybrid_for_workflow.broadcast_event(
                                crate::mcp_transport::SseEvent::progress(None, session_id, thought)
                            );
                        }
                    }
                }
            });

            let ws_server_for_progress = Arc::clone(&state.websocket_server);
            let hybrid_for_progress = Arc::clone(&state.hybrid_state);
            tauri::async_runtime::spawn(async move {
                if let Some(mut rx) = orchestrator_for_progress.subscribe_progress() {
                    while let Ok(update) = rx.recv().await {
                        use tauri::Emitter;
                        if let Err(e) = app_handle_for_progress.emit("workflow_progress", &update) {
                            tracing::warn!("Failed to emit orchestrator workflow_progress: {}", e);
                        }
                        match &update {
                            crate::workflow::WorkflowProgressUpdate::Step { run_id, workflow_id, step_name, status, message, .. } => {
                                let status_str = format!("{:?}", status).to_lowercase();
                                ws_server_for_progress.relay_workflow_status(run_id, workflow_id, step_name, &status_str, message.clone()).await;
                                hybrid_for_progress.broadcast_event(
                                    crate::mcp_transport::SseEvent::status(None, run_id, step_name, &status_str, message.clone())
                                );
                            }
                            crate::workflow::WorkflowProgressUpdate::Thought { session_id, thought } => {
                                if thought.starts_with("JSON:") {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&thought[5..]) {
                                        if val.get("type").and_then(|v| v.as_str()) == Some("task_status") {
                                            let agent = val.get("agent").and_then(|v| v.as_str()).unwrap_or("");
                                            let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("");
                                            let message = val.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                            hybrid_for_progress.broadcast_event(
                                                crate::mcp_transport::SseEvent::status(None, session_id, agent, status, Some(message))
                                            );
                                            continue;
                                        }
                                    }
                                }
                                hybrid_for_progress.broadcast_event(
                                    crate::mcp_transport::SseEvent::progress(None, session_id, thought)
                                );
                            }
                            _ => {}
                        }
                    }
                }
            });

            let app_handle_for_ws = app_handle.clone();
            let app_handle_for_mobile = app_handle.clone();
            // Extract hybrid_state Arc BEFORE app_handle is moved into ws spawn closure
            let hybrid_for_mobile = Arc::clone(&state.hybrid_state);
            tauri::async_runtime::spawn(async move {
                while let Some(incoming) = ws_command_rx.recv().await {
                    let state = app_handle_for_ws.state::<AppState>();
                    match incoming.message {
                        websocket::ClientMessage::Command { session_id, text, context } => {
                            let session_id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                            let orchestrator = state.orchestrator.clone();
                            // Process attached file from context
                            let mut context_file_path: Option<String> = None;
                            if let Some(ctx) = context {
                                if let Some(path) = ctx.get("file_path").and_then(|v| v.as_str()) {
                                    // Use the uploaded file path directly
                                    context_file_path = Some(path.to_string());
                                } else if let (Some(name), Some(base64_data)) = (
                                    ctx.get("file_name").and_then(|v| v.as_str()),
                                    ctx.get("file_base64").and_then(|v| v.as_str())
                                ) {
                                    use base64::Engine;
                                    let temp_dir = std::env::temp_dir().join("office_hub_mobile_uploads");
                                    let _ = std::fs::create_dir_all(&temp_dir);
                                    let file_path = temp_dir.join(name);

                                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(base64_data) {
                                        use std::io::Write;
                                        if let Ok(mut f) = std::fs::File::create(&file_path) {
                                            let _ = f.write_all(&decoded);
                                            context_file_path = Some(file_path.to_string_lossy().to_string());
                                        }
                                    }
                                }
                            }

                            let ws_server = state.websocket_server.clone();
                            let client_id = incoming.client_id.clone();

                            use tauri::Emitter;
                            let app_for_emit = app_handle.clone();
                            let user_msg_clone = text.clone();
                            let session_id_clone = session_id.clone();
                            let _ = app_for_emit.emit("chat_message_received", serde_json::json!({
                                "session_id": session_id_clone,
                                "message": {
                                    "id": uuid::Uuid::new_v4().to_string(),
                                    "role": "user",
                                    "content": user_msg_clone,
                                    "timestampMs": chrono::Utc::now().timestamp_millis(),
                                }
                            }));

                            let text_clone = text.clone();
                            let session_id_clone = session_id.clone();
                            let hybrid_clone_for_spawn = std::sync::Arc::clone(&state.hybrid_state);
                            tauri::async_runtime::spawn(async move {
                                // Broadcast "processing" status immediately so mobile ProgressScreen can display it
                                let run_id = uuid::Uuid::new_v4().to_string();
                                ws_server.relay_workflow_status(
                                    &run_id,
                                    &session_id_clone,
                                    "AI Processing",
                                    "running",
                                    Some(format!("Processing: {}…", text_clone.chars().take(60).collect::<String>()))
                                ).await;

                                let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                                let sse_for_progress = hybrid_clone_for_spawn;
                                let sid_for_progress = session_id_clone.clone();

                                tauri::async_runtime::spawn(async move {
                                    while let Some(thought) = progress_rx.recv().await {
                                        sse_for_progress.broadcast_event(
                                            crate::mcp_transport::SseEvent::progress(None, &sid_for_progress, &thought)
                                        );
                                    }
                                });

                                match orchestrator.process_message_native(&session_id_clone, &text_clone, context_file_path.as_deref(), None, Some(progress_tx)).await {
                                    Ok(resp) => {
                                        // Broadcast completion
                                        ws_server.relay_workflow_status(
                                            &run_id,
                                            &session_id_clone,
                                            "AI Processing",
                                            "completed",
                                            resp.intent.as_ref().map(|i| format!("Done: {}", i))
                                        ).await;

                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::ChatReply {
                                            session_id: session_id.clone(),
                                            content: resp.content.clone(),
                                            intent: resp.intent.clone(),
                                            agent_used: resp.agent_used.clone(),
                                            timestamp: chrono::Utc::now().to_rfc3339(),
                                            metadata: resp.metadata.clone(),
                                        }).await;

                                        let _ = app_for_emit.emit("chat_message_received", serde_json::json!({
                                            "session_id": session_id.clone(),
                                            "message": {
                                                "id": uuid::Uuid::new_v4().to_string(),
                                                "role": "assistant",
                                                "content": resp.content,
                                                "timestampMs": chrono::Utc::now().timestamp_millis(),
                                                "intent": resp.intent,
                                                "agentUsed": resp.agent_used,
                                            }
                                        }));
                                    }
                                    Err(e) => {
                                        // Broadcast failure
                                        ws_server.relay_workflow_status(
                                            &run_id,
                                            &session_id_clone,
                                            "AI Processing",
                                            "failed",
                                            Some(format!("Error: {}", e))
                                        ).await;

                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::Error {
                                            error_code: "PROCESS_ERROR".to_string(),
                                            message: e.to_string(),
                                            request_id: None,
                                        }).await;
                                    }
                                }
                            });
                        }
                        websocket::ClientMessage::VoiceCommand { session_id, audio_base64: _ } => {
                            let session_id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                            let orchestrator = state.orchestrator.clone();
                            let ws_server = state.websocket_server.clone();
                            let client_id = incoming.client_id.clone();

                            tauri::async_runtime::spawn(async move {
                                // STUB: Here we would call Whisper API or a local STT model to transcribe audio_base64
                                let transcribed_text = "Tạo báo cáo tuần"; // Placeholder
                                tracing::info!("Transcribed voice command: {}", transcribed_text);

                                match orchestrator.process_message_native(&session_id, transcribed_text, None, None, None).await {
                                    Ok(resp) => {
                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::ChatReply {
                                            session_id,
                                            content: resp.content,
                                            intent: resp.intent,
                                            agent_used: resp.agent_used,
                                            timestamp: chrono::Utc::now().to_rfc3339(),
                                            metadata: resp.metadata,
                                        }).await;
                                    }
                                    Err(e) => {
                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::Error {
                                            error_code: "PROCESS_ERROR".to_string(),
                                            message: e.to_string(),
                                            request_id: None,
                                        }).await;
                                    }
                                }
                            });
                        }
                        websocket::ClientMessage::ApprovalResponse { action_id, approved, .. } => {
                            let hitl_manager = state.hitl_manager.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = hitl_manager.resolve(&action_id, approved) {
                                    tracing::error!("Failed to resolve HITL from WS: {}", e);
                                }
                            });
                        }
                        websocket::ClientMessage::ChatRequest { content, file_context, email_context, app_type, document_content } => {
                            let session_id = uuid::Uuid::new_v4().to_string();
                            let orchestrator = state.orchestrator.clone();
                            let ws_server = state.websocket_server.clone();
                            let client_id = incoming.client_id.clone();

                            // Prepend email body as context when request comes from Outlook
                            let mut enriched_content = match (email_context, app_type.as_deref()) {
                                (Some(email), Some("Outlook")) if !email.is_empty() => {
                                    format!("[Ngữ cảnh email:\n{}]\n\nYêu cầu: {}", email, content)
                                }
                                _ => content.clone(),
                            };

                            if let Some(doc) = document_content {
                                if !doc.is_empty() {
                                    enriched_content = format!("[Nội dung tài liệu đang mở:\n{}]\n\nYêu cầu: {}", doc, enriched_content);
                                }
                            }

                            tauri::async_runtime::spawn(async move {
                                match orchestrator.process_message_native(&session_id, &enriched_content, file_context.as_deref(), None, None).await {
                                    Ok(resp) => {
                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::ChatResponse {
                                            content: resp.content,
                                        }).await;
                                    }
                                    Err(e) => {
                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::Error {
                                            error_code: "PROCESS_ERROR".to_string(),
                                            message: e.to_string(),
                                            request_id: None,
                                        }).await;
                                    }
                                }
                            });
                        }
                        websocket::ClientMessage::OfficeAddinEvent { event, file_path, app_type, subject: _, sender: _ } => {
                            if event == "DocumentOpened" {
                                let orchestrator = state.orchestrator.clone();
                                let ws_server = state.websocket_server.clone();
                                let client_id = incoming.client_id.clone();
                                let app = app_type.unwrap_or_else(|| "Unknown".to_string());
                                let path = file_path.unwrap_or_default();

                                // Skip auto-analysis for Outlook events (no file to analyze)
                                if app == "Outlook" {
                                    return;
                                }

                                tauri::async_runtime::spawn(async move {
                                    let prompt = format!("Tóm tắt ngắn gọn file '{}' (Loại: {}) và đưa ra 1-2 đề xuất nếu có thể. Trả về đúng nội dung tóm tắt.", path, app);
                                    let session_id = uuid::Uuid::new_v4().to_string();
                                    if let Ok(resp) = orchestrator.process_message_native(&session_id, &prompt, Some(&path), None, None).await {
                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::ContextAnalysis {
                                            summary: resp.content,
                                        }).await;
                                    }
                                });
                            }
                        }
                        websocket::ClientMessage::ListSessions => {
                            let orchestrator = state.orchestrator.clone();
                            let ws_server = state.websocket_server.clone();
                            let client_id = incoming.client_id.clone();

                            tauri::async_runtime::spawn(async move {
                                if let Ok(sessions) = orchestrator.list_sessions().await {
                                    let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::SessionList {
                                        sessions,
                                    }).await;
                                }
                            });
                        }
                        websocket::ClientMessage::GetSessionHistory { session_id } => {
                            let orchestrator = state.orchestrator.clone();
                            let ws_server = state.websocket_server.clone();
                            let client_id = incoming.client_id.clone();

                            tauri::async_runtime::spawn(async move {
                                let store = orchestrator.get_session_store().await;
                                if let Some(session) = store.get(&session_id) {
                                    let messages: Vec<serde_json::Value> = session.messages.iter().map(|msg| {
                                        serde_json::json!({
                                            "id": msg.id,
                                            "role": msg.role,
                                            "content": msg.content,
                                            "timestamp_ms": msg.created_at.timestamp_millis(),
                                            "intent": msg.intent,
                                            "agent_used": msg.agent_name
                                        })
                                    }).collect();

                                    let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::SessionHistory {
                                        session_id,
                                        messages,
                                    }).await;
                                }
                            });
                        }
                        websocket::ClientMessage::DeleteSession { session_id } => {
                            let orchestrator = state.orchestrator.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = orchestrator.delete_session(&session_id).await {
                                    tracing::error!("Failed to delete session via WS: {}", e);
                                }
                            });
                        }
                        websocket::ClientMessage::DeleteArtifact { filename } => {
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = crate::commands::delete_file(filename).await {
                                    tracing::error!("Failed to delete artifact via WS: {}", e);
                                }
                            });
                        }
                        websocket::ClientMessage::CrdtSync { doc_id, payload_base64 } => {
                            let crdt_manager = state.crdt_manager.clone();
                            let ws_server = state.websocket_server.clone();
                            let client_id = incoming.client_id.clone();
                            tauri::async_runtime::spawn(async move {
                                use base64::Engine;
                                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&payload_base64) {
                                    if let Ok(Some(response_msg)) = crdt_manager.process_sync_message(&doc_id, &client_id, &decoded).await {
                                        let response_base64 = base64::engine::general_purpose::STANDARD.encode(&response_msg);
                                        let _ = ws_server.send_to_client(&client_id, websocket::ServerMessage::CrdtSync {
                                            doc_id,
                                            payload_base64: response_base64,
                                        }).await;
                                    }
                                }
                            });
                        }
                        websocket::ClientMessage::DocumentExtracted { file_name, base64_data } => {
                            tauri::async_runtime::spawn(async move {
                                use base64::Engine;
                                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&base64_data) {
                                    use std::io::Write;
                                    let temp_dir = std::env::temp_dir().join("office_hub_exports");
                                    let _ = std::fs::create_dir_all(&temp_dir);
                                    let file_path = temp_dir.join(&file_name);
                                    if let Ok(mut f) = std::fs::File::create(&file_path) {
                                        let _ = f.write_all(&decoded);
                                        tracing::info!("Document extracted and saved to: {:?}", file_path);
                                    }
                                }
                            });
                        }
                        _ => {}
                    }
                }
            });

            // ── MCP-Hybrid: Mobile REST commands → Orchestrator → SSE ────────
            tauri::async_runtime::spawn(async move {
                while let Some(incoming) = mobile_cmd_rx.recv().await {
                    let state = app_handle_for_mobile.state::<AppState>();
                    let orchestrator = state.orchestrator.clone();
                    let sse = hybrid_for_mobile.clone();
                    let cmd_id = incoming.command_id.clone();
                    let session_id = incoming.session_id.clone();

                    // ── Intercept Mobile System Commands ────────────────────
                    if incoming.text.as_str() == "__DELETE_SESSION__" {
                        let sid = session_id.clone();
                        let sse_clone = sse.clone();
                        let cmd_id_clone = cmd_id.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = orchestrator.delete_session(&sid).await {
                                tracing::error!("delete_session via SSE failed: {}", e);
                            } else {
                                // Broadcast updated session list so mobile UI refreshes
                                match orchestrator.list_sessions().await {
                                    Ok(sessions) => sse_clone.broadcast_event(crate::mcp_transport::SseEvent {
                                        event_type: crate::mcp_transport::SseEventType::SessionList,
                                        call_id: Some(cmd_id_clone),
                                        payload: serde_json::json!({ "sessions": sessions }),
                                    }),
                                    Err(e) => tracing::warn!("list_sessions after delete failed: {}", e),
                                }
                            }
                        });
                        continue;
                    }

                    // ── Regular LLM command → Orchestrator → SSE ─────────────
                    let run_id = uuid::Uuid::new_v4().to_string();
                    // Emit "running" status immediately so mobile shows progress
                    sse.broadcast_event(crate::mcp_transport::SseEvent::status(
                        Some(cmd_id.clone()),
                        &run_id,
                        "AI Processing",
                        "running",
                        Some(format!("Processing: {}…", incoming.text.chars().take(60).collect::<String>())),
                    ));

                    tauri::async_runtime::spawn(async move {
                        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                        let sse_for_progress = sse.clone();
                        let sid_for_progress = session_id.clone();

                        tauri::async_runtime::spawn(async move {
                            while let Some(thought) = progress_rx.recv().await {
                                sse_for_progress.broadcast_event(
                                    crate::mcp_transport::SseEvent::progress(None, &sid_for_progress, &thought)
                                );
                            }
                        });

                        match orchestrator.process_message_native(
                            &session_id,
                            &incoming.text,
                            incoming.context_file_path.as_deref(),
                            None,
                            Some(progress_tx),
                        ).await {
                            Ok(resp) => {
                                sse.broadcast_event(crate::mcp_transport::SseEvent::status(
                                    Some(cmd_id.clone()),
                                    &run_id,
                                    "AI Processing",
                                    "completed",
                                    resp.intent.as_ref().map(|i| format!("Done: {}", i)),
                                ));
                                sse.broadcast_event(crate::mcp_transport::SseEvent::result(
                                    Some(cmd_id),
                                    &session_id,
                                    &resp.content,
                                    resp.intent.as_deref(),
                                    resp.agent_used.as_deref(),
                                    resp.metadata,
                                ));
                            }
                            Err(e) => {
                                tracing::error!("Orchestrator process_message failed: {:#}", e);
                                sse.broadcast_event(crate::mcp_transport::SseEvent::status(
                                    Some(cmd_id.clone()),
                                    &run_id,
                                    "AI Processing",
                                    "failed",
                                    Some(format!("Error: {}", e)),
                                ));
                                sse.broadcast_event(crate::mcp_transport::SseEvent::error(
                                    Some(cmd_id.clone()),
                                    "PROCESS_ERROR",
                                    &format!("{:#}", e),
                                ));
                                // Emit result so it appears as a chat bubble, ensuring the chat loop closes
                                sse.broadcast_event(crate::mcp_transport::SseEvent::result(
                                    Some(cmd_id),
                                    &session_id,
                                    &format!("⚠️ Hệ thống gặp lỗi: {:#}", e),
                                    Some("Error"),
                                    Some("Orchestrator"),
                                    None,
                                ));
                            }
                        }
                    });
                }
            });



            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Office Hub application");
}
