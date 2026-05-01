// ============================================================================

// Office Hub – orchestrator/mod.rs

#![allow(clippy::empty_line_after_doc_comments)]
#![allow(clippy::empty_line_after_outer_attr)]
#![allow(unreachable_code)]

// The Orchestrator is the central "brain" of Office Hub.

//

// It is responsible for:

//   1. Receiving raw user messages from the Tauri IPC layer.

//   2. Classifying the user's intent via the LLM Gateway.

//   3. Routing tasks to the appropriate sub-agent(s).

//   4. Managing session state (conversation history, context, summaries).

//   5. Running output through the Rule Engine before committing results.

//   6. Coordinating Human-in-the-Loop (HITL) approval flows.

//   7. Exposing agent status and system health information.

// ============================================================================

pub mod intent;

pub mod router;

pub mod rule_engine;

pub mod session;

pub mod memory;

pub mod plan;
pub mod plan_monitor;
pub mod plan_runner;
pub mod planned_method;

use std::collections::HashMap;

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use chrono::{DateTime, Utc};

use dashmap::DashMap;

use serde::{Deserialize, Serialize};

use tokio::sync::{oneshot, RwLock};

use tracing::{debug, error, info, instrument, warn};

use uuid::Uuid;

use crate::agents::{AgentId, AgentRegistry, AgentStatusInfo};

use crate::llm_gateway::LlmGateway;

use crate::mcp::{broker::McpBroker, McpRegistry};

use self::intent::{Intent, IntentClassifier};

use self::router::Router;

use self::rule_engine::{RuleEngine, ValidationRequest, ValidationTarget};

use self::session::{SessionId, SessionStore, SessionSummary};

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// Re-exports for convenience

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// Re-exports

// Note: Intent and SessionId are already imported via use self::... above

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// OrchestratorHandle â€“ cheap, cloneable handle used in commands.rs

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// A cheaply cloneable, thread-safe handle to the `Orchestrator`.

/// Wraps the inner struct in `Arc<RwLock<â€¦>>`.

/// All public methods take `&self` and acquire locks internally.

#[derive(Clone)]

pub struct OrchestratorHandle(pub Arc<RwLock<Orchestrator>>);

impl OrchestratorHandle {
    pub fn new(orchestrator: Orchestrator) -> Self {
        Self(Arc::new(RwLock::new(orchestrator)))
    }

    /// Set the WebSocket Server instance for HITL manager

    pub async fn set_ws_server(&self, ws: Arc<crate::websocket::WebSocketServer>) {
        self.0.read().await.hitl_manager.set_ws_server(ws);
    }

    /// Set the Knowledge Base directory path

    pub async fn set_knowledge_dir(&self, dir: std::path::PathBuf) {
        let mut inner = self.0.write().await;

        inner.knowledge_dir = Some(dir.clone());

        let server = Arc::new(crate::mcp::internal_servers::KnowledgeServer::new(Some(
            dir,
        )));

        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Set the Policies directory path
    pub async fn set_policy_dir(&self, dir: std::path::PathBuf) {
        let mut inner = self.0.write().await;
        inner.policy_dir = Some(dir.clone());
        let server = Arc::new(crate::mcp::internal_servers::PolicyServer::new(Some(dir)));
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Set the Skills directory path
    pub async fn set_skills_dir(&self, dir: std::path::PathBuf) {
        let mut inner = self.0.write().await;
        inner.skills_dir = Some(dir.clone());
        let server = Arc::new(crate::mcp::internal_servers::SkillServer::new(Some(dir)));
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the local FileSystem server
    pub async fn register_fs_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::FileSystemServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Polars Analytic Server
    pub async fn register_analytic_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::AnalyticServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Office COM Server
    pub async fn register_office_com_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::OfficeComServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Win32 Admin server
    pub async fn register_win32_admin_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::Win32AdminServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Rhai Scripting Server
    pub async fn register_scripting_server(&self, app_handle: tauri::AppHandle) {
        let inner = self.0.write().await;
        let skills_dir = inner.skills_dir.clone();
        let server = Arc::new(crate::mcp::internal_servers::ScriptingServer::new(
            Some(app_handle),
            skills_dir,
        ));
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Chart Render Server
    pub async fn register_chart_server(&self, app_handle: tauri::AppHandle) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::ChartServer::new(Some(
            app_handle,
        )));
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Native Chart Server
    pub async fn register_native_chart_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::native_chart::NativeChartServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Web Search Server
    pub async fn register_web_search_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::WebSearchServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Register the Web Fetch Server (Obscura headless engine)
    pub async fn register_web_fetch_server(&self) {
        let inner = self.0.write().await;
        let server = Arc::new(crate::mcp::internal_servers::WebFetchServer::new());
        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Set the Memory Store

    pub async fn set_memory_store(&self, store: Arc<memory::MemoryStore>) {
        let mut inner = self.0.write().await;

        inner.memory_store = Some(Arc::clone(&store));

        let server = Arc::new(crate::mcp::internal_servers::MemoryServer::new(Some(store)));

        inner.mcp_broker.register_internal_server(server).await;
    }

    /// Process a user chat message end-to-end (classify â†’ route â†’ execute â†’ validate).

    pub async fn process_message(
        &self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
        workspace_id: Option<&str>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<OrchestratorResponse> {
        let mut inner = self.0.write().await;
        inner
            .process_message(session_id, message, context_file, workspace_id, progress_tx)
            .await
    }

    /// [Phase 1] Process message using Native Tool Calling via genai crate.
    ///
    /// ÄÃ¢y lÃ  Hybrid ReAct Loop má»›i:
    /// - MCP Tools â†’ Native `genai::Tool` (khÃ´ng cáº§n JSON Schema thá»§ cÃ´ng)
    /// - Legacy Agents â†’ `call_legacy_agent` bridge tool
    /// - Fallback: náº¿u genai bridge tháº¥t báº¡i, tá»± Ä‘á»™ng dÃ¹ng `process_message` cÅ©
    pub async fn process_message_native(
        &self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
        workspace_id: Option<&str>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<OrchestratorResponse> {
        let mut inner = self.0.write().await;
        inner
            .process_message_native(session_id, message, context_file, workspace_id, progress_tx)
            .await
    }

    /// [Phase 1] Smart Planning Execution: Generates a DAG plan and executes it via Agent-to-Agent MCP.
    pub async fn process_message_planned(
        &self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
        workspace_id: Option<&str>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<OrchestratorResponse> {
        let mut inner = self.0.write().await;
        inner
            .process_message_planned(session_id, message, context_file, workspace_id, progress_tx)
            .await
    }

    /// Wraps all existing Agents into AgentMcpAdapter and registers them into the McpBroker.
    pub async fn register_agent_adapters(&self) {
        let inner = self.0.write().await;
        let agent_registry = inner.agent_registry.clone();
        let llm_gateway = Some(inner.llm_gateway.clone());
        let mcp_broker = inner.mcp_broker.clone();

        let statuses = agent_registry.all_statuses();
        for status in statuses {
            let agent_id = status.id.clone();
            if let Some(agent_arc) =
                agent_registry.get_mut(&crate::agents::AgentId::custom(&agent_id))
            {
                let adapter = crate::mcp::agent_mcp_adapter::AgentMcpAdapter::new(
                    agent_id.clone(),
                    agent_arc,
                    llm_gateway.clone(),
                );
                mcp_broker.register_internal_server(Arc::new(adapter)).await;
            }
        }
    }

    pub async fn create_session(&self, workspace_id: Option<String>) -> Result<SessionId> {
        let inner = self.0.write().await;
        Ok(inner.session_store.create(workspace_id))
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let inner = self.0.write().await;

        // Clean up handoff file if it exists
        let agent_dir = inner
            .skills_dir
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".agent")
            });
        let handoff_path = agent_dir
            .join("handoffs")
            .join(format!("session_{}.md", session_id));
        if handoff_path.exists() {
            let _ = tokio::fs::remove_file(handoff_path).await;
        }

        inner.session_store.delete(session_id);
        Ok(())
    }

    pub async fn list_sessions(&self) -> Result<Vec<serde_json::Value>> {
        let inner = self.0.read().await;

        let summaries = inner.session_store.list_summaries();

        Ok(summaries
            .into_iter()
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
            .collect())
    }

    pub async fn get_agent_statuses(&self) -> Result<Vec<AgentStatusInfo>> {
        let inner = self.0.read().await;

        Ok(inner.agent_registry.all_statuses())
    }

    pub async fn get_session_store(&self) -> SessionStore {
        let inner = self.0.read().await;

        inner.session_store.clone()
    }

    pub async fn init_persistence(&self, dir: impl Into<std::path::PathBuf>) -> Result<()> {
        let store = self.get_session_store().await;

        store
            .init_persistence(dir)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn list_mcp_servers(&self) -> Result<Vec<serde_json::Value>> {
        let inner = self.0.read().await;
        let mut external = inner.mcp_broker.external_registry.list_json();
        let mut internal = inner.mcp_broker.list_internal_json().await;
        internal.append(&mut external);
        Ok(internal)
    }

    pub async fn install_mcp_server(&self, source: &str) -> Result<String> {
        let inner = self.0.write().await;

        inner.mcp_broker.external_registry.install(source).await
    }

    pub async fn uninstall_mcp_server(&self, server_id: &str) -> Result<()> {
        let inner = self.0.write().await;

        inner
            .mcp_broker
            .external_registry
            .uninstall(server_id)
            .await
    }

    pub async fn resolve_hitl(&self, _action_id: &str, _approved: bool) -> Result<()> {
        // Obsolete: HitlManager is now accessed via AppState directly.

        Err(anyhow::anyhow!(
            "resolve_hitl should be called directly on AppState::hitl_manager"
        ))
    }

    pub async fn list_pending_hitl(&self) -> Result<Vec<serde_json::Value>> {
        let inner = self.0.read().await;

        Ok(inner.hitl_manager.list_pending_json())
    }

    pub async fn check_system_requirements(&self) -> Result<serde_json::Value> {
        let inner = self.0.read().await;

        inner.check_system_requirements().await
    }

    pub async fn execute_agent_action(
        &self,
        agent_id: &str,
        mut task: crate::orchestrator::AgentTask,
    ) -> Result<crate::orchestrator::AgentOutput> {
        let (memory_store, agent, task_session_id, task_action, workspace_id) = {
            let inner = self.0.write().await;
            task.llm_gateway = Some(Arc::clone(&inner.llm_gateway));
            let agent_arc = inner
                .agent_registry
                .get_mut(&crate::agents::AgentId::custom(agent_id))
                .ok_or_else(|| anyhow::anyhow!("Agent {} not found", agent_id))?;
            let ws_id = inner
                .session_store
                .get(&task.session_id)
                .and_then(|s| s.workspace_id.clone());
            (
                inner.memory_store.clone(),
                agent_arc.clone(),
                task.session_id.clone(),
                task.action.clone(),
                ws_id,
            )
        };

        let start_time = std::time::Instant::now();
        let mut agent_guard = agent.write().await;
        let result = agent_guard
            .execute(task)
            .await
            .map_err(|e| anyhow::anyhow!(e));
        let latency_ms = start_time.elapsed().as_millis() as i64;

        if let Some(mem) = memory_store {
            let (tokens, status) = match &result {
                Ok(output) => (output.tokens_used.unwrap_or(0) as usize, "success"),
                Err(_) => (0usize, "error"),
            };
            if let Err(e) = mem.log_telemetry(
                &task_session_id,
                workspace_id.as_deref(),
                agent_id,
                &task_action,
                latency_ms,
                tokens,
                status,
            ) {
                tracing::warn!("Failed to log telemetry: {}", e);
            }
        }

        result
    }

    pub async fn call_mcp_tool(
        &self,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<crate::mcp::ToolCallResult> {
        let inner = self.0.read().await;

        inner.mcp_broker.call_tool(tool_name, arguments).await
    }

    pub fn subscribe_progress(
        &self,
    ) -> Option<tokio::sync::broadcast::Receiver<crate::workflow::WorkflowProgressUpdate>> {
        let inner = futures::executor::block_on(self.0.read());
        inner.progress_tx.as_ref().map(|tx| tx.subscribe())
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// Orchestrator â€“ inner struct

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Core orchestrator.  Holds all sub-systems and coordinates their interactions.

pub struct Orchestrator {
    /// Classifies free-text messages into structured `Intent` values.
    pub intent_classifier: IntentClassifier,

    /// Routes classified intents to the correct agent(s).
    pub router: Router,

    /// Validates agent output against YAML rule files before committing.
    pub rule_engine: Arc<RuleEngine>,

    /// Persistent conversation/session state.
    pub session_store: SessionStore,

    /// Registry of all available sub-agents and their current status.
    pub agent_registry: AgentRegistry,

    /// MCP Broker (plugin ecosystem + internal servers).
    pub mcp_broker: Arc<McpBroker>,

    /// Human-in-the-Loop approval manager.
    pub hitl_manager: Arc<HitlManager>,

    /// Reference to the LLM Gateway for intent classification and task execution.
    pub llm_gateway: Arc<RwLock<LlmGateway>>,

    /// Directory for the Local Knowledge Base (`.md` files)
    pub knowledge_dir: Option<std::path::PathBuf>,

    /// Directory for the Global Policies (`.md` files)
    pub policy_dir: Option<std::path::PathBuf>,

    /// Long-term memory store (SQLite FTS5)
    pub memory_store: Option<Arc<memory::MemoryStore>>,

    /// Directory for Declarative Prompt Tools (Skills)
    pub skills_dir: Option<std::path::PathBuf>,

    /// Channel to broadcast realtime progress.
    pub progress_tx:
        Option<tokio::sync::broadcast::Sender<crate::workflow::WorkflowProgressUpdate>>,

    /// Runtime metrics collected during operation.
    metrics: OrchestratorMetrics,
}

impl Orchestrator {
    /// Construct a new `Orchestrator` with the given LLM Gateway reference.

    /// Note: This uses a placeholder RuleEngine. Call `initialize()` for full setup.

    pub fn new(llm_gateway: Arc<RwLock<LlmGateway>>, hitl_manager: Arc<HitlManager>) -> Self {
        info!("Initialising Orchestrator");

        // Create RuleEngine with default/empty state (full initialization requires async)

        let rule_engine = Arc::new(RuleEngine::default());

        Self {
            intent_classifier: IntentClassifier,

            router: Router::new(Arc::clone(&rule_engine)),

            rule_engine,

            session_store: SessionStore::default(),

            agent_registry: AgentRegistry::new(),

            mcp_broker: Arc::new(McpBroker::new(McpRegistry::new())),

            hitl_manager,

            llm_gateway,
            knowledge_dir: None,
            policy_dir: None,
            memory_store: None,
            skills_dir: None,
            progress_tx: Some(tokio::sync::broadcast::channel(256).0),
            metrics: OrchestratorMetrics::default(),
        }
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    // Core processing pipeline

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ï¿½        // â”€â”€ 2. Build Agent Tool Prompt â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[instrument(skip(self, message), fields(session = session_id))]
    pub async fn process_message(
        &mut self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
        workspace_id: Option<&str>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<OrchestratorResponse> {
        let started_at = Utc::now();
        self.metrics.total_requests += 1;

        // â”€â”€ 1. Retrieve Session â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let session_clone = {
            let mut session = self
                .session_store
                .get_or_create(session_id)
                .context("Failed to retrieve/create session")?;

            if session.workspace_id.is_none() && workspace_id.is_some() {
                session.workspace_id = workspace_id.map(|s| s.to_string());
            }

            debug!(turns = session.messages.len(), "Session retrieved");
            session.clone()
        };

        // â”€â”€ 2. Build Agent Tool Prompt â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        // Inject TOÃ€N Bá»˜ catalog: MCP tools + Agent tool schemas Ä‘á»ƒ LLM khÃ´ng cáº§n search mÃ¹.
        // == 2. Build Agent Tool Prompt (Legacy Fallback Mode) ====================
        // NOTE: Legacy pipeline chi con la fallback khi native pipeline that bai.
        // Inject compact list (ten + mo ta ngan) thay vi full JSON schema de giam token.
        // Native pipeline (process_message_native) dung native Tool Calling API.
        let mut mcp_tools_desc = String::new();

        // 2a. Compact MCP tool list (name + short description, ~800 tokens total)
        let all_mcp = self.mcp_broker.list_all_tools().await.unwrap_or_default();
        for tool in &all_mcp {
            let desc = if tool.description.len() > 120 {
                format!("{}...", &tool.description[..120])
            } else {
                tool.description.clone()
            };
            mcp_tools_desc.push_str(&format!("- {} (mcp): {}\n", tool.name, desc));
        }

        // 2b. Compact Agent tool list
        let all_agent_tools = self.agent_registry.all_tool_schemas_complete();
        for tool in &all_agent_tools {
            let desc = if tool.description.len() > 120 {
                format!("{}...", &tool.description[..120])
            } else {
                tool.description.clone()
            };
            mcp_tools_desc.push_str(&format!("- {} (agent): {}\n", tool.name, desc));
        }

        // 2c. search_available_tools: dung khi can tim tool runtime-added
        mcp_tools_desc
            .push_str("- search_available_tools (mcp): Tim kiem them cong cu theo tu khoa.\n");

        let _tools_desc = format!(
            "(Danh sach {} MCP tools va {} Agent tools. Goi theo Tool ID chinh xac.)\n",
            all_mcp.len(),
            all_agent_tools.len()
        );

        let mut project_policy_content = String::new();
        let workspace_instruction = if let Some(wid) = workspace_id {
            if let Some(kd) = &self.knowledge_dir {
                let workspace_root = if wid == "default" {
                    kd.parent().unwrap_or(kd.as_path()).to_path_buf()
                } else {
                    kd.parent()
                        .unwrap_or(kd.as_path())
                        .join("workspaces")
                        .join(wid)
                };

                let policy_dir = workspace_root.join("policies");
                if policy_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&policy_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file()
                                && path.extension().and_then(|e| e.to_str()) == Some("md")
                            {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let filename =
                                        path.file_name().unwrap_or_default().to_string_lossy();
                                    project_policy_content.push_str(&format!(
                                        "\n--- Má»Ÿ Ä‘áº§u Policy: {} ---\n",
                                        filename
                                    ));
                                    project_policy_content.push_str(&content);
                                    project_policy_content.push_str(&format!(
                                        "\n--- Káº¿t thÃºc Policy: {} ---\n",
                                        filename
                                    ));
                                }
                            }
                        }
                    }
                }

                let root_str = workspace_root
                    .to_string_lossy()
                    .to_string()
                    .replace("\\", "/");
                let policies_prompt = if project_policy_content.is_empty() {
                    String::new()
                } else {
                    format!("\n[PROJECT POLICIES]\nDÆ°á»›i Ä‘Ã¢y lÃ  cÃ¡c Policy riÃªng cá»§a dá»± Ã¡n nÃ y, Báº®T BUá»˜C pháº£i tuÃ¢n thá»§ (Æ°u tiÃªn cao hÆ¡n Global Policy náº¿u cÃ³ xung Ä‘á»™t, trá»« khi Global Policy Ä‘Ã¡nh dáº¥u lÃ  'Báº¯t buá»™c'):\n{}", project_policy_content)
                };

                format!("\n[QUAN TRá»ŒNG: WORKSPACE CONTEXT]\nBáº¡n Ä‘ang hoáº¡t Ä‘á»™ng trong workspace cÃ³ ID lÃ : '{}'. ThÆ° má»¥c gá»‘c cá»§a workspace nÃ y trÃªn á»• Ä‘Ä©a lÃ : `{}`.\n- Dá»¯ liá»‡u Ä‘áº§u vÃ o (file táº£i lÃªn, ghi Ã¢m...) náº±m trong thÆ° má»¥c `{}/docs/inbox/`.\n- Káº¿t quáº£ xá»­ lÃ½ (file bÃ¡o cÃ¡o, xuáº¥t ra) Báº®T BUá»˜C lÆ°u vÃ o thÆ° má»¥c `{}/docs/outbox/`.\n- Náº¿u ngÆ°á»i dÃ¹ng nháº¯c Ä‘áº¿n dá»± Ã¡n khÃ¡c (VD: 'Vá»›i dá»± Ã¡n Beta...'), hÃ£y gá»i agent_id = 'orchestrator', action = 'set_active_project' vá»›i tham sá»‘ `project_name` Ä‘á»ƒ chuyá»ƒn ngá»¯ cáº£nh sang dá»± Ã¡n Ä‘Ã³.\n- Khi dÃ¹ng tool `search_memory`, Báº®T BUá»˜C thÃªm tiá»n tá»‘ `[{}]` vÃ o tá»« khÃ³a tÃ¬m kiáº¿m (VD: `[{}] quy trÃ¬nh mua hÃ ng`).\nKhi gá»i cÃ¡c tool táº¡o file (nhÆ° `office_master`), báº¡n pháº£i truyá»n Ä‘Æ°á»ng dáº«n lÆ°u file tuyá»‡t Ä‘á»‘i vÃ o thÆ° má»¥c outbox nÃ y.\nKhi gá»i báº¥t ká»³ MCP tool nÃ o liÃªn quan Ä‘áº¿n dá»¯ liá»‡u (nhÆ° knowledge, policy...), Báº N Báº®T BUá»˜C pháº£i truyá»n thÃªm tham sá»‘ `\"workspace_id\": \"{}\"` vÃ o arguments cá»§a tool.\n{}", wid, root_str, root_str, root_str, wid, wid, wid, policies_prompt)
            } else {
                format!("\n[QUAN TRá»ŒNG: WORKSPACE CONTEXT]\nBáº¡n Ä‘ang hoáº¡t Ä‘á»™ng trong workspace cÃ³ ID lÃ : '{}'.\n- Náº¿u ngÆ°á»i dÃ¹ng nháº¯c Ä‘áº¿n dá»± Ã¡n khÃ¡c (VD: 'Vá»›i dá»± Ã¡n Beta...'), hÃ£y gá»i agent_id = 'orchestrator', action = 'set_active_project' vá»›i tham sá»‘ `project_name` Ä‘á»ƒ chuyá»ƒn ngá»¯ cáº£nh sang dá»± Ã¡n Ä‘Ã³.\n- Khi dÃ¹ng tool `search_memory`, Báº®T BUá»˜C thÃªm tiá»n tá»‘ `[{}]` vÃ o tá»« khÃ³a tÃ¬m kiáº¿m.\nKhi gá»i báº¥t ká»³ MCP tool nÃ o liÃªn quan Ä‘áº¿n dá»¯ liá»‡u (nhÆ° knowledge, policy...), Báº N Báº®T BUá»˜C pháº£i truyá»n thÃªm tham sá»‘ `\"workspace_id\": \"{}\"` vÃ o arguments cá»§a tool.\n", wid, wid, wid)
            }
        } else {
            String::new()
        };

        let schema = serde_json::json!({

            "type": "object",

            "properties": {

                "thought": { "type": "string" },

                "direct_response": { "type": "string" },

                "agent_calls": {

                    "type": "array",

                    "items": {

                        "type": "object",

                        "properties": {

                            "agent_id": { "type": "string" },

                            "action": { "type": "string" },

                            "parameters": { "type": "object" },

                            "task_id": { "type": "string" },

                            "dependencies": { "type": "array", "items": { "type": "string" } }

                        },

                        "required": ["agent_id", "action"]

                    }

                }

            },

            "required": ["thought"]

        });

        let system_prompt = format!(
            "Báº¡n lÃ  Office Hub Orchestrator, má»™t trá»£ lÃ½ Ä‘iá»u phá»‘i Agent.\n\
             \n[AVAILABLE MCP TOOLS]\n\
             Báº¡n cÃ³ thá»ƒ dÃ¹ng cÃ¡c MCP Tools sau Ä‘Ã¢y Ä‘á»ƒ tra cá»©u Policy, Memory, Knowledge hoáº·c gá»i plugin:\n\
             {mcp_tools_desc}\n\
             \n[AVAILABLE SKILLS/AGENTS]\n\
             Báº¡n cÃ³ thá»ƒ gá»i cÃ¡c Agents sau Ä‘Ã¢y Ä‘á»ƒ thá»±c hiá»‡n nhiá»‡m vá»¥:\n\
             {tools_desc}\n\
             [QUAN TRá»ŒNG: TRÃ NHá»š DÃ€I Háº N]\n\
             Há»‡ thá»‘ng KHÃ”NG tá»± Ä‘á»™ng nhá»“i ngá»¯ cáº£nh cÅ© vÃ o cuá»™c há»™i thoáº¡i Ä‘á»ƒ tiáº¿t kiá»‡m token. Náº¿u cÃ¢u há»i cá»§a ngÆ°á»i dÃ¹ng Ã¡m chá»‰ Ä‘áº¿n sá»± kiá»‡n, quyáº¿t Ä‘á»‹nh, hoáº·c thÃ´ng tin trong quÃ¡ khá»© (VD: 'láº§n trÆ°á»›c', 'hÃ´m qua', 'phÆ°Æ¡ng Ã¡n Ä‘Ã£ chá»‘t'), Báº N Báº®T BUá»˜C PHáº¢I DÃ™NG MCP Tool `search_memory` (agent_id = 'mcp_broker') Ä‘á»ƒ láº¥y láº¡i trÃ­ nhá»› trÆ°á»›c khi tráº£ lá»i.\n\
             \n\
             [QUAN TRá»ŒNG: TUÃ‚N THá»¦ POLICY VÃ€ RULE]\n\
             TrÆ°á»›c khi báº¯t Ä‘áº§u báº¥t ká»³ tÃ¡c vá»¥ nÃ o (vÃ­ dá»¥: táº¡o file Word, viáº¿t bÃ¡o cÃ¡o, láº­p trÃ¬nh, hay thay Ä‘á»•i há»‡ thá»‘ng), Báº N Báº®T BUá»˜C pháº£i gá»i MCP Tool `list_policies` hoáº·c `query_policy` (agent_id = 'mcp_broker') Ä‘á»ƒ kiá»ƒm tra xem há»‡ thá»‘ng cÃ³ quy Ä‘á»‹nh (rule/policy) nÃ o cáº§n tuÃ¢n thá»§ khÃ´ng.\n\
             \n\
             [QUAN TRá»ŒNG: Gá»˜P THÃ”NG TIN Tá»ª NHIá»€U FILE]\n\
             Náº¿u ngÆ°á»i dÃ¹ng yÃªu cáº§u tá»•ng há»£p bÃ¡o cÃ¡o tá»« má»™t thÆ° má»¥c, hÃ£y sá»­ dá»¥ng MCP Tool `read_folder_files` (agent_id = 'mcp_broker') Ä‘á»ƒ láº¥y ná»™i dung táº¥t cáº£ file, sau Ä‘Ã³ gá»i agent (VD: `office_master`) Ä‘á»ƒ táº¡o bÃ¡o cÃ¡o. Náº¿u cÃ¡c tool khÃ´ng phá»¥ thuá»™c nhau, báº¡n cÃ³ thá»ƒ gá»i Ä‘á»“ng thá»i nhiá»u tools/agents trong má»™t máº£ng `agent_calls` Ä‘á»ƒ chÃºng cháº¡y song song.\n\
             \n\
             [QUAN TRá»ŒNG: Cáº¬P NHáº¬T FILE ÄÃƒ Táº O]\n\
             Náº¿u ngÆ°á»i dÃ¹ng yÃªu cáº§u cáº­p nháº­t/chá»‰nh sá»­a má»™t file Ä‘Ã£ táº¡o trÆ°á»›c Ä‘Ã³, Báº N Báº®T BUá»˜C pháº£i xem láº¡i lá»‹ch sá»­ há»™i thoáº¡i, tÃ¬m ÄÃšNG tÃªn file cÅ© vÃ  truyá»n chÃ­nh xÃ¡c tÃªn Ä‘Ã³ vÃ o tham sá»‘ cá»§a Agent/Tool. KHÃ”NG ÄÆ¯á»¢C táº¡o file má»›i.\n\
             \n\
             [QUAN TRá»ŒNG: CÃ€I Äáº¶T SKILL Tá»ª FILE ZIP]\n\
             Náº¿u ngÆ°á»i dÃ¹ng yÃªu cáº§u cÃ i Ä‘áº·t skill tá»« file Ä‘Ã­nh kÃ¨m (zip), Báº®T BUá»˜C gá»i Agent `converter` vá»›i action `analyze_and_convert_zip_skill` vÃ  truyá»n Ä‘Æ°á»ng dáº«n file vÃ o `zip_path`. CÃ´ng cá»¥ nÃ y sáº½ tá»± Ä‘á»™ng phÃ¢n tÃ­ch mÃ£ nguá»“n, map dependencies vÃ  convert skill Ä‘Ã³ cho báº¡n. KHÃ”NG sá»­ dá»¥ng tool `write_skill` vÃ¬ ná»™i dung file dÃ i sáº½ lÃ m há»ng cáº¥u trÃºc JSON.\n\
             \n\
             [QUAN TRá»ŒNG: Tá»° TIáº¾N HÃ“A & Táº O TOOL (SELF-EVOLUTION)]\n\
             Náº¿u báº¡n nháº­n Ä‘Æ°á»£c má»™t yÃªu cáº§u khÃ´ng cÃ³ sáºµn cÃ´ng cá»¥ xá»­ lÃ½ (ká»ƒ cáº£ sau khi Ä‘Ã£ dÃ¹ng `search_available_tools`), hÃ£y Ä‘Ã¡nh giÃ¡ xem tÃ¡c vá»¥ Ä‘Ã³ cÃ³ mang tÃ­nh quy trÃ¬nh láº·p Ä‘i láº·p láº¡i (reusable workflow/automation) hay khÃ´ng. Náº¿u CÃ“, Báº N CÃ“ QUYá»€N Tá»° Táº O RA CÃ”NG Cá»¤ Má»šI báº±ng cÃ¡ch gá»i MCP Tool `write_skill` (agent_id = 'mcp_broker'). \n\
             Tham sá»‘ cá»§a `write_skill` (Báº N ÄÆ¯á»¢C DÃ™NG MÃ€ KHÃ”NG Cáº¦N SEARCH): \n\
             - `skill_name` (chuá»—i kebab-case, Báº®T BUá»˜C cÃ³ tiá»n tá»‘ `auto-`, VD: `auto-csv-formatter`). \n\
             - `description` (mÃ´ táº£ chá»©c nÄƒng cá»§a tool). \n\
             - `parameters` (object chá»©a Ä‘á»‹nh nghÄ©a properties theo chuáº©n JSON Schema). \n\
             - `instructions` (ná»™i dung hÆ°á»›ng dáº«n chi tiáº¿t báº±ng Markdown Ä‘á»ƒ há»‡ thá»‘ng biáº¿t cÃ¡ch thá»±c thi). \n\
             Ngay sau khi tool `write_skill` tráº£ vá» thÃ nh cÃ´ng, tool sáº½ tá»± Ä‘á»™ng cÃ³ máº·t trÃªn há»‡ thá»‘ng. HÃ£y Gá»ŒI Láº I CHÃNH TOOL ÄÃ“ á»Ÿ turn tiáº¿p theo Ä‘á»ƒ giáº£i quyáº¿t yÃªu cáº§u ban Ä‘áº§u cá»§a user!\n\
             LÆ°u Ã½: Äá»‘i vá»›i cÃ¡c tÃ¡c vá»¥ má»™t láº§n (one-off), hÃ£y tá»± xá»­ lÃ½ báº±ng code python/powershell (qua win32) hoáº·c tá»« chá»‘i, KHÃ”NG Ä‘Æ°á»£c táº¡o tool rÃ¡c.\n\
             \n\
             [NGHIEN CUU WEB - OBSCURA ENGINE]\n\
             He thong tich hop Obscura headless browser (V8, stealth mode). KHONG can Chrome/Edge mo. Dung khi can doc web:\n\
             - web_fetch (agent_id='mcp_broker'): Doc 1 URL, render JS day du. Params: url(bat buoc), mode(text/html/links), eval(JS tuy chon).\n\
             - web_scrape_parallel (agent_id='mcp_broker'): Scrape nhieu URL song song. Params: urls(array), concurrency(default 5, max 25).\n\
             - search_web (agent_id='mcp_broker'): Tim kiem va tra ve danh sach URLs lien quan.\n\
             WORKFLOW CHUAN: search_web -> lay URLs -> web_scrape_parallel -> doc song song -> tong hop ket qua.\n\
             {workspace_instruction}\n\
             Náº¿u báº¡n cáº§n dÃ¹ng Agent hoáº·c Tool, hÃ£y tráº£ vá» danh sÃ¡ch agent_calls vá»›i agent_id (vá»›i Agent) hoáº·c tÃªn tool (vá»›i MCP Tool) vÃ  action, kÃ¨m parameters dÆ°á»›i dáº¡ng JSON.\n\
             LÆ°u Ã½: Äá»‘i vá»›i MCP Tools, báº¯t buá»™c Ä‘áº·t agent_id = 'mcp_broker' vÃ  action = tÃªn cá»§a tool.\n\
             Náº¿u cÃ¢u há»i chá»‰ lÃ  trÃ² chuyá»‡n thÃ´ng thÆ°á»ng hoáº·c báº¡n Ä‘Ã£ cÃ³ Ä‘á»§ thÃ´ng tin, hÃ£y Ä‘iá»n vÃ o direct_response.\n\
             LuÃ´n Ä‘Æ°a ra 'thought' giáº£i thÃ­ch quÃ¡ trÃ¬nh suy luáº­n cá»§a báº¡n.\n\
             \n\
             IMPORTANT: Your ENTIRE output MUST be a valid JSON object matching the requested schema. Do NOT wrap the JSON in Markdown formatting (no `json). Do NOT output bullet points. Output ONLY the raw JSON object.\n\
             \n\
             === EXPECTED JSON RESPONSE SCHEMA ===\n\
             {schema_json}\n\
             =====================================",
            tools_desc = mcp_tools_desc,
            workspace_instruction = workspace_instruction,
            schema_json = serde_json::to_string_pretty(&schema).unwrap_or_default()
        );

        let mut messages = vec![crate::llm_gateway::LlmMessage::system(system_prompt)];

        for msg in &session_clone.messages {
            if msg.content.trim().is_empty() {
                continue;
            }

            let role_str = msg.role.to_string();

            let role = match role_str.as_str() {
                "user" => crate::llm_gateway::MessageRole::User,

                _ => crate::llm_gateway::MessageRole::Assistant,
            };

            messages.push(crate::llm_gateway::LlmMessage {
                role,

                content: msg.content.clone(),

                image_base64s: vec![],
            });
        }

        let context_str = if let Some(path) = context_file {
            format!("\n[Ngá»¯ cáº£nh file Ä‘ang má»Ÿ: {}]\n", path)
        } else {
            String::new()
        };

        messages.push(crate::llm_gateway::LlmMessage::user(format!(
            "{}{}",
            message, context_str
        )));

        let mut final_content = String::new();

        let mut total_tokens = 0;

        let mut metadata = serde_json::Map::new();

        let main_agent = String::from("orchestrator");

        let max_turns = 5;

        for turn_idx in 0..max_turns {
            let llm = self.llm_gateway.read().await;

            // Define JSON schema

            let llm_req = crate::llm_gateway::LlmRequest::new(messages.clone())
                .with_agent_id("orchestrator")
                .with_temperature(0.1)
                .with_json_schema(schema.clone())
                .with_complexity(crate::llm_gateway::request::TaskComplexity::Reasoning);

            let mut stream = llm
                .complete_stream(llm_req)
                .await
                .context("LLM Orchestrator stream failed")?;
            drop(llm);

            use futures::StreamExt;
            let mut accumulated_json = String::new();
            let mut last_thought = String::new();

            while let Some(chunk_res) = stream.next().await {
                if let Ok(chunk) = chunk_res {
                    accumulated_json.push_str(&chunk);

                    // Partial JSON extraction for "thought"
                    if let Some(start_idx) = accumulated_json.find("\"thought\": \"") {
                        let content_start = start_idx + 13;
                        let mut end_idx = content_start;
                        let bytes = accumulated_json.as_bytes();
                        let mut escaped = false;

                        while end_idx < bytes.len() {
                            if bytes[end_idx] == b'\\' && !escaped {
                                escaped = true;
                            } else if bytes[end_idx] == b'\"' && !escaped {
                                break;
                                break;
                            } else {
                                escaped = false;
                            }
                            end_idx += 1;
                        }

                        let raw_thought = &accumulated_json[content_start..end_idx];
                        let parsed_thought = raw_thought.replace("\\n", "\n").replace("\\\"", "\"");

                        if parsed_thought != last_thought {
                            last_thought = parsed_thought.clone();
                            if let Some(ref tx) = progress_tx {
                                let _ = tx.send(parsed_thought);
                            }
                        }
                    }
                }
            }

            // After stream finishes, the accumulated string is the full JSON response.
            // Create a fake LlmResponse so the rest of the parsing logic remains unchanged.
            let resp = crate::llm_gateway::response::LlmResponse {
                request_id: uuid::Uuid::new_v4(),
                content: accumulated_json,
                stop_reason: crate::llm_gateway::response::StopReason::Stop,
                usage: crate::llm_gateway::response::LlmUsage::default(),
                provider: "gemini".to_string(),
                model: "streamed".to_string(),
                from_cache: false,
                latency_ms: 0,
                received_at: chrono::Utc::now(),
            };

            total_tokens += resp.usage.total_tokens;

            // Parse the OrchestrationDecision

            #[derive(Deserialize, Clone)]

            struct AgentCall {
                agent_id: String,

                action: String,

                parameters: Option<serde_json::Value>,

                task_id: Option<String>,

                dependencies: Option<Vec<String>>,
            }

            #[derive(Deserialize)]

            struct OrchestrationDecision {
                #[serde(default)]
                thought: String,

                direct_response: Option<String>,

                agent_calls: Option<Vec<AgentCall>>,
            }

            info!("Raw LLM response: {}", resp.content);

            let mut decision: OrchestrationDecision = {
                let parse_value = |json_str: &str| -> Option<serde_json::Value> {
                    serde_json::from_str(json_str).ok()
                };

                let mut parsed_val = parse_value(&resp.content);
                if parsed_val.is_none() {
                    let content = resp.content.trim();
                    let extracted = if let Some(start) = content.find("```json") {
                        if let Some(end) = content[start + 7..].find("```") {
                            content[start + 7..start + 7 + end].trim()
                        } else {
                            content
                        }
                    } else if let Some(start) = content.find("```") {
                        if let Some(end) = content[start + 3..].find("```") {
                            content[start + 3..start + 3 + end].trim()
                        } else {
                            content
                        }
                    } else {
                        let start = content.find('{');
                        let end = content.rfind('}');
                        if let (Some(s), Some(e)) = (start, end) {
                            if s < e {
                                &content[s..e + 1]
                            } else {
                                content
                            }
                        } else {
                            content
                        }
                    };
                    parsed_val = parse_value(extracted);
                }

                if let Some(mut val) = parsed_val {
                    if let Some(output) = val.get("output").cloned() {
                        if output.is_object() {
                            val = output;
                        }
                    } else if let Some(response) = val.get("response").cloned() {
                        if response.is_object() {
                            val = response;
                        }
                    }
                    serde_json::from_value(val).unwrap_or_else(|e| {
                        warn!("Failed to deserialize LLM decision JSON into struct. Error: {}", e);
                        OrchestrationDecision {
                            thought: "The LLM returned JSON that does not match the OrchestrationDecision schema.".into(),
                            direct_response: Some(resp.content.clone()),
                            agent_calls: None,
                        }
                    })
                } else {
                    warn!("Failed to parse LLM decision as JSON. Using fallback.");
                    OrchestrationDecision {
                        thought: "The LLM responded with unstructured text. Switching to direct response fallback.".into(),
                        direct_response: Some(resp.content.clone()),
                        agent_calls: None,
                    }
                }
            };

            // Extract <thought> tags if LLM outputted them outside JSON (Anthropic style)
            if decision.thought.is_empty() {
                if let Some(start) = resp.content.find("<thought>") {
                    if let Some(end) = resp.content.find("</thought>") {
                        if start + 9 < end {
                            decision.thought = resp.content[start + 9..end].trim().to_string();
                        }
                    }
                }
            }

            info!(turn = turn_idx, thought = %decision.thought, "Orchestrator reasoning");

            if let Some(ws) = self.hitl_manager.get_ws_server() {
                let session_id_clone = session_id.to_string();
                let thought_clone = decision.thought.clone();
                tauri::async_runtime::spawn(async move {
                    let msg = crate::websocket::ServerMessage::ChatProgress {
                        session_id: session_id_clone,
                        thought: thought_clone,
                    };
                    let _ = ws.broadcast(msg).await;
                });
            }

            if let Some(tx) = &self.progress_tx {
                let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Thought {
                    session_id: session_id.to_string(),
                    thought: decision.thought.clone(),
                });
            }

            if let Some(calls) = decision.agent_calls.filter(|c| !c.is_empty()) {
                let mut remaining_calls = calls.clone();
                let mut completed_task_ids: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut all_committed = true;
                let mut turn_content = String::new();
                let mut turn_metadata = serde_json::Map::new();

                while !remaining_calls.is_empty() {
                    let mut ready_calls = Vec::new();
                    let mut pending_calls = Vec::new();
                    for c in remaining_calls {
                        if let Some(deps) = &c.dependencies {
                            if deps.iter().all(|d| completed_task_ids.contains(d)) {
                                ready_calls.push(c);
                            } else {
                                pending_calls.push(c);
                            }
                        } else {
                            ready_calls.push(c);
                        }
                    }

                    if ready_calls.is_empty() {
                        turn_content.push_str("âš ï¸ Lá»—i láº­p lá»‹ch DAG: PhÃ¡t hiá»‡n vÃ²ng láº·p hoáº·c dependency khÃ´ng tá»“n táº¡i.\n\n");
                        all_committed = false;
                        break;
                    }

                    let mut futures = vec![];

                    for call in ready_calls {
                        let agent_id = call.agent_id.clone();
                        let action = call.action.clone();
                        let params = call
                            .parameters
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({}));
                        let task_id_opt = call.task_id.clone();
                        let dependencies = call.dependencies.clone().unwrap_or_default();

                        let hitl_manager = Arc::clone(&self.hitl_manager);
                        let llm_gateway = Arc::clone(&self.llm_gateway);
                        let mcp_broker = Arc::clone(&self.mcp_broker);
                        let progress_tx = self.progress_tx.clone();
                        let message_str = message.to_string();
                        let context_file_str = context_file.map(String::from);
                        let session_id_str = session_id.to_string();

                        let agent_arc_opt = self
                            .agent_registry
                            .get_mut(&crate::agents::AgentId::custom(&agent_id));

                        let mut is_set_project = false;
                        let mut proj_clone = String::new();
                        if agent_id == "orchestrator" && action == "set_active_project" {
                            let new_proj = params
                                .get("project_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Global");
                            if let Some(mut session) = self.session_store.get_mut(&session_id_str) {
                                session.workspace_id = Some(new_proj.to_string());
                            }
                            is_set_project = true;
                            proj_clone = new_proj.to_string();
                        }

                        futures.push(async move {
                            if is_set_project {
                                return (agent_id.clone(), task_id_opt, true, format!("ÄÃ£ chuyá»ƒn ngá»¯ cáº£nh sang dá»± Ã¡n '{}'. CÃ¡c cÃ¢u tráº£ lá»i tiáº¿p theo sáº½ dÃ¹ng trÃ­ nhá»› vÃ  file cá»§a dá»± Ã¡n nÃ y.\n\n", proj_clone), 0, None);
                            }

                            info!(agent = %agent_id, action = %action, "Dispatching to agent");

                            if let Some(tx) = &progress_tx {
                                let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Step {
                                    run_id: session_id_str.clone(),
                                    workflow_id: "orchestrator".to_string(),
                                    step_id: agent_id.clone(),
                                    step_name: agent_id.clone(),
                                    status: crate::workflow::RunStatus::Running,
                                    message: Some(format!("{} Ä‘ang {}", agent_id, action)),
                                    updated_at: chrono::Utc::now(),
                                });
                            }

                            if agent_id == "mcp_broker" {
                                if action == "search_available_tools" {
                                    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
                                    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
                                    let result = match mcp_broker.search_tools(query, std::cmp::min(limit, 3)).await {
                                        Ok(tools) => {
                                            let mut desc = String::new();
                                            for tool in tools {
                                                desc.push_str(&format!("- Tool ID: {}\n", tool.name));
                                                desc.push_str(&format!("  Description: {}\n", tool.description));
                                                desc.push_str(&format!("  Schema: {}\n\n", serde_json::to_string(&tool.input_schema).unwrap_or_default()));
                                            }
                                            if desc.is_empty() {
                                                "KhÃ´ng tÃ¬m tháº¥y cÃ´ng cá»¥ nÃ o phÃ¹ há»£p vá»›i yÃªu cáº§u.".to_string()
                                            } else {
                                                format!("Danh sÃ¡ch cÃ´ng cá»¥ phÃ¹ há»£p:\n{}", desc)
                                            }
                                        }
                                        Err(e) => format!("Lá»—i khi tÃ¬m kiáº¿m cÃ´ng cá»¥: {}", e),
                                    };
                                    return (agent_id.clone(), task_id_opt, false, format!("MCP Tool '{}' result:\n{}\n\n", action, result), 0, None);
                                }

                                let requires_hitl = matches!(
                                    action.as_str(),
                                    "win32_registry_write" | "win32_winget_install" | "win32_winget_uninstall" | "win32_process_kill" | "win32_shell_execute" | "win32_file_delete"
                                );

                                if requires_hitl {
                                    let hitl_req = crate::orchestrator::HitlRequestBuilder {
                                        description: format!("YÃªu cáº§u phÃª duyá»‡t gá»i há»‡ thá»‘ng Win32: '{}'", action),
                                        risk_level: crate::orchestrator::HitlRiskLevel::High,
                                        payload: Some(params.clone()),
                                    };
                                    let (action_id, rx) = hitl_manager.register(hitl_req);
                                    info!(action_id, "Waiting for MCP HITL approval...");
                                    let approved = rx.await.unwrap_or(false);
                                    if !approved {
                                        return (agent_id.clone(), task_id_opt, false, format!("User rejected the MCP tool '{}'\n\n", action), 0, None);
                                    }
                                }

                                match mcp_broker.call_tool(&action, Some(params)).await {
                                    Ok(result) => {
                                        if let Some(tx) = &progress_tx {
                                            let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Step {
                                                run_id: session_id_str.clone(),
                                                workflow_id: "orchestrator".to_string(),
                                                step_id: agent_id.clone(),
                                                step_name: agent_id.clone(),
                                                status: if result.is_error { crate::workflow::RunStatus::Failed } else { crate::workflow::RunStatus::Success },
                                                message: Some(format!("MCP {} {}", action, if result.is_error { "lá»—i" } else { "hoÃ n táº¥t" })),
                                                updated_at: chrono::Utc::now(),
                                            });
                                        }
                                        let mut result_content = String::new();
                                        for res in result.content {
                                            if res.content_type == "text" {
                                                if let Some(text) = res.text {
                                                    result_content.push_str(&text);
                                                    result_content.push('\n');
                                                }
                                            }
                                        }
                                        let msg = if result.is_error {
                                            format!("MCP Tool '{}' error:\n{}\n\n", action, result_content)
                                        } else {
                                            format!("MCP Tool '{}' result:\n{}\n\n", action, result_content)
                                        };
                                        return (agent_id, task_id_opt, false, msg, 0, None);
                                    }
                                    Err(e) => {
                                        if let Some(tx) = &progress_tx {
                                            let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Step {
                                                run_id: session_id_str.clone(),
                                                workflow_id: "orchestrator".to_string(),
                                                step_id: agent_id.clone(),
                                                step_name: agent_id.clone(),
                                                status: crate::workflow::RunStatus::Failed,
                                                message: Some(format!("Lá»—i gá»i MCP {}: {}", action, e)),
                                                updated_at: chrono::Utc::now(),
                                            });
                                        }
                                        return (agent_id, task_id_opt, false, format!("Failed to call MCP Tool '{}': {}\n\n", action, e), 0, None);
                                    }
                                }
                            }

                            let agent_arc = match agent_arc_opt {
                                Some(arc) => arc,
                                None => {
                                    if let Some(tx) = &progress_tx {
                                        let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Step {
                                            run_id: session_id_str.clone(),
                                            workflow_id: "orchestrator".to_string(),
                                            step_id: agent_id.clone(),
                                            step_name: agent_id.clone(),
                                            status: crate::workflow::RunStatus::Failed,
                                            message: Some(format!("Agent '{}' khÃ´ng tá»“n táº¡i", agent_id)),
                                            updated_at: chrono::Utc::now(),
                                        });
                                    }
                                    return (agent_id.clone(), task_id_opt, false, format!("âš ï¸ Lá»—i: Agent '{}' not found in registry\n\n", agent_id), 0, None);
                                }
                            };

                            let requires_hitl = matches!(
                                action.as_str(),
                                "send_email" | "run_power_query" | "generate_vba" | "web_navigate"
                            );

                            if requires_hitl {
                                let hitl_req = crate::orchestrator::HitlRequestBuilder {
                                    description: format!("YÃªu cáº§u phÃª duyá»‡t hÃ nh Ä‘á»™ng '{}' cho agent '{}'.", action, agent_id),
                                    risk_level: crate::orchestrator::HitlRiskLevel::High,
                                    payload: Some(params.clone()),
                                };
                                let (action_id, rx) = hitl_manager.register(hitl_req);
                                info!(action_id, "Waiting for HITL approval...");
                                let approved = rx.await.unwrap_or(false);
                                if !approved {
                                    return (agent_id.clone(), task_id_opt, false, format!("User rejected the action '{}'\n\n", action), 0, None);
                                }
                            }

                            let internal_task_id = task_id_opt.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                            let task = crate::orchestrator::AgentTask {
                                task_id: internal_task_id,
                                action: action.clone(),
                                intent: crate::orchestrator::intent::Intent::Ambiguous(Default::default()),
                                message: message_str,
                                context_file: context_file_str,
                                session_id: session_id_str.clone(),
                                parameters: params.as_object().cloned().unwrap_or_default().into_iter().collect(),
                                llm_gateway: Some(llm_gateway),
                                global_policy: None,
                                knowledge_context: None,
                                parent_task_id: None,
                                dependencies,
                            };

                            let agent_result = {
                                let mut agent_guard = agent_arc.write().await;
                                agent_guard.execute(task).await
                            };

                            if let Some(tx) = &progress_tx {
                                let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Step {
                                    run_id: session_id_str.clone(),
                                    workflow_id: "orchestrator".to_string(),
                                    step_id: agent_id.clone(),
                                    step_name: agent_id.clone(),
                                    status: if agent_result.is_ok() { crate::workflow::RunStatus::Success } else { crate::workflow::RunStatus::Failed },
                                    message: Some(format!("{} {}", agent_id, if agent_result.is_ok() { "hoÃ n táº¥t" } else { "lá»—i" })),
                                    updated_at: chrono::Utc::now(),
                                });
                            }

                            match agent_result {
                                Ok(out) => (agent_id.clone(), task_id_opt, out.committed, format!("{}\n\n", out.content), out.tokens_used.unwrap_or(0), out.metadata),
                                Err(e) => {
                                    error!(error = %e, agent = %agent_id, "Agent execution failed");
                                    (agent_id.clone(), task_id_opt, false, format!("âš ï¸ Lá»—i khi gá»i agent {}: {}\n\n", agent_id, e), 0, None)
                                }
                            }
                        });
                    }

                    let results = futures::future::join_all(futures).await;

                    for (a_id, t_id_opt, committed, content, tokens, meta) in results {
                        if let Some(t_id) = t_id_opt {
                            completed_task_ids.insert(t_id);
                        }
                        if !committed {
                            all_committed = false;
                        }
                        turn_content.push_str(&content);
                        total_tokens += tokens;
                        if let Some(m) = meta {
                            turn_metadata.insert(a_id, m);
                        }
                    }

                    remaining_calls = pending_calls;
                }

                if !all_committed && turn_idx < max_turns - 1 {
                    // Append the LLM's raw JSON output as Assistant so it knows what it did
                    messages.push(crate::llm_gateway::LlmMessage::assistant(
                        resp.content.clone(),
                    ));

                    let mut combined_content = format!("Káº¿t quáº£ thá»±c thi tá»« Agent (ChÆ°a hoÃ n táº¥t, hÃ£y phÃ¢n tÃ­ch vÃ  Ä‘Æ°a ra quyáº¿t Ä‘á»‹nh tiáº¿p theo hoáº·c tráº£ lá»i trá»±c tiáº¿p):\n{}", turn_content);
                    if !turn_metadata.is_empty() {
                        let meta_str =
                            serde_json::to_string_pretty(&turn_metadata).unwrap_or_default();
                        combined_content.push_str(&format!("\nMetadata tá»« Agent:\n{}", meta_str));
                    }

                    // Append the tool results (and metadata) as User (from the environment)
                    messages.push(crate::llm_gateway::LlmMessage::user(combined_content));
                    continue;
                } else {
                    if let Some(direct) = decision.direct_response.filter(|d| !d.trim().is_empty())
                    {
                        final_content = direct;
                    } else {
                        final_content = turn_content;
                    }
                    metadata.extend(turn_metadata);
                    break;
                }
            } else if let Some(direct) = decision.direct_response.filter(|d| !d.trim().is_empty()) {
                final_content = direct;
                metadata.insert(
                    "handled_by".to_string(),
                    serde_json::json!("orchestrator_direct"),
                );
                break;
            } else {
                final_content = "TÃ´i khÃ´ng cháº¯c cháº¯n pháº£i lÃ m gÃ¬.".to_string();

                break;
            }
        }

        // â”€â”€ 5. Rule Engine Validation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

        let validation_request = ValidationRequest::new(
            main_agent.clone(),
            ValidationTarget::LlmResponse,
            final_content.clone(),
        );

        let validated = self.rule_engine.validate(validation_request).await;

        if !validated.passed {
            warn!(

                violations = ?validated.violations,

                "Rule Engine blocked agent output"

            );

            self.metrics.rule_violations += validated.violations.len() as u64;

            if !validated.blocking_violations().is_empty() {
                return Err(anyhow!(
                    "Output blocked by Rule Engine: {}",
                    validated
                        .blocking_violations()
                        .first()
                        .map(|v| v.message.as_str())
                        .unwrap_or("unknown violation")
                ));
            }
        }

        // â”€â”€ 6. Session update â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

        let auto_handoff_enabled = self
            .llm_gateway
            .read()
            .await
            .config()
            .await
            .auto_handoff_enabled;

        let (needs_summarisation, is_first_turn) = {
            if let Some(mut session) = self.session_store.get_mut(session_id) {
                let is_first = session.turn_count() == 0;

                session.add_turn(
                    message.to_string(),
                    final_content.clone(),
                    "LLM_Orchestration".to_string(),
                    AgentId::custom(&main_agent),
                );

                (session.needs_summarisation(auto_handoff_enabled), is_first)
            } else {
                tracing::warn!(
                    session_id,
                    "Session was deleted during processing; skipping update"
                );
                (false, false)
            }
        };

        if needs_summarisation {
            if auto_handoff_enabled {
                self.perform_auto_handoff(session_id).await?;
            } else {
                self.summarise_session(session_id).await?;
            }
        }

        // â”€â”€ 6.5 Generate Topic ID for New Sessions â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

        if is_first_turn {
            let prompt = format!(

                "Táº¡o má»™t cá»¥m tá»« ngáº¯n (2-4 tá»«) Ä‘áº¡i diá»‡n cho chá»§ Ä‘á» cá»§a yÃªu cáº§u sau: '{}'. Chá»‰ tráº£ vá» chá»§ Ä‘á», khÃ´ng giáº£i thÃ­ch.",

                message

            );

            if let Ok(resp) = self
                .llm_gateway
                .read()
                .await
                .complete(crate::llm_gateway::LlmRequest::new(vec![
                    crate::llm_gateway::LlmMessage::user(prompt),
                ]))
                .await
            {
                let topic = resp.content.trim().trim_matches('"').to_string();

                if let Some(mut s) = self.session_store.get_mut(session_id) {
                    s.topic_id = Some(topic);
                }
            }
        }

        // â”€â”€ 7. Build response â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

        let duration_ms = (Utc::now() - started_at).num_milliseconds() as u64;

        self.metrics.total_duration_ms += duration_ms;

        self.metrics.successful_requests += 1;

        info!(

            duration_ms,

            agent = %main_agent,

            "Message processed successfully"

        );

        if let Err(e) = self.session_store.save_session(session_id).await {
            tracing::warn!("Failed to persist session {}: {}", session_id, e);
        }

        // -- 8. Persist to Long-Term Memory -----------------------------------
        // Write a concise summary of this turn so the LLM can recall it later
        // via the search_memory MCP tool.
        if let Some(mem) = &self.memory_store {
            let topic = format!("session_{}", &session_id[..session_id.len().min(8)]);
            let snippet = format!(
                "User: {}\nAgent ({}): {}",
                message.chars().take(200).collect::<String>(),
                main_agent,
                final_content.chars().take(400).collect::<String>(),
            );

            let workspace_id = self
                .session_store
                .get(session_id)
                .and_then(|s| s.workspace_id.clone());

            if let Err(e) = mem.insert_memory(session_id, workspace_id.as_deref(), &topic, &snippet)
            {
                tracing::warn!("Failed to write long-term memory: {}", e);
            }
        }

        Ok(OrchestratorResponse {
            content: final_content.trim().to_string(),

            intent: Some("LLM_Orchestration".to_string()),

            agent_used: Some(main_agent),

            tokens_used: Some(total_tokens),

            duration_ms,

            metadata: Some(serde_json::Value::Object(metadata)),
        })
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    // Session summarisation (anti-drift / context-window management)

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Perform an automatic handoff when the context window reaches 80% capacity.
    /// It prompts the LLM to write a handoff markdown file, saves it, clears the
    /// current session context, and injects a pointer to the file.
    #[instrument(skip(self))]
    async fn perform_auto_handoff(&mut self, session_id: &str) -> Result<()> {
        info!(session_id, "Performing auto-handoff for session context");

        let messages = {
            let session = self
                .session_store
                .get(session_id)
                .ok_or_else(|| anyhow!("Session not found during handoff"))?;
            session.messages.iter().cloned().collect::<Vec<_>>()
        };

        let system_prompt = "Báº¡n lÃ  AI tá»± Ä‘á»™ng táº¡o tÃ i liá»‡u Handoff (chuyá»ƒn giao). \
                             Context Ä‘Ã£ Ä‘áº¡t ngÆ°á»¡ng giá»›i háº¡n bá»™ nhá»›. HÃ£y tÃ³m táº¯t láº¡i tÃ¡c vá»¥ Ä‘ang thá»±c hiá»‡n má»™t cÃ¡ch cá»±c ká»³ chi tiáº¿t, \
                             bao gá»“m:\n\
                             1. Má»¥c tiÃªu cÃ´ng viá»‡c cá»‘t lÃµi\n\
                             2. Nhá»¯ng viá»‡c Ä‘Ã£ lÃ m xong (bao gá»“m file nÃ o Ä‘Ã£ sá»­a, logic gÃ¬ Ä‘Ã£ thÃªm)\n\
                             3. Nhá»¯ng viá»‡c cÃ²n dang dá»Ÿ\n\
                             4. Bá»‘i cáº£nh quan trá»ng cáº§n ghi nhá»›\n\
                             5. BÆ°á»›c tiáº¿p theo Cáº¦N LÃ€M NGAY (Next Steps).\n\n\
                             Viáº¿t dÆ°á»›i Ä‘á»‹nh dáº¡ng Markdown chuyÃªn nghiá»‡p. Output duy nháº¥t cá»§a báº¡n lÃ  file Handoff nÃ y.";

        let mut conversation = String::new();
        for msg in messages {
            conversation.push_str(&format!("[{}]: {}\n", msg.role, msg.content));
        }

        let llm = self.llm_gateway.read().await;
        let req = crate::llm_gateway::LlmRequest::new(vec![
            crate::llm_gateway::LlmMessage::system(system_prompt),
            crate::llm_gateway::LlmMessage::user(format!(
                "Lá»‹ch sá»­ há»™i thoáº¡i trÆ°á»›c khi bá»‹ reset:\n\n{}\n\nHÃ£y táº¡o file Handoff.",
                conversation
            )),
        ])
        .with_temperature(0.1);

        let resp = llm
            .complete(req)
            .await
            .context("LLM failed to generate handoff")?;

        // Save to .agent/handoffs
        let agent_dir = self
            .skills_dir
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".agent")
            });
        let handoff_dir = agent_dir.join("handoffs");
        if !handoff_dir.exists() {
            tokio::fs::create_dir_all(&handoff_dir).await?;
        }

        let handoff_path = handoff_dir.join(format!("session_{}.md", session_id));
        tokio::fs::write(&handoff_path, &resp.content).await?;

        // Clear history and inject handoff notice
        {
            let mut session = self
                .session_store
                .get_mut(session_id)
                .ok_or_else(|| anyhow!("Session not found when applying handoff"))?;

            session.clear_history();
            session.add_turn(
                "[Há»‡ thá»‘ng tá»± Ä‘á»™ng reset context]".to_string(),
                format!("ÄÃ£ Ä‘áº¡t giá»›i háº¡n context. Há»‡ thá»‘ng Ä‘Ã£ xoÃ¡ lá»‹ch sá»­ chat vÃ  lÆ°u tráº¡ng thÃ¡i cÃ´ng viá»‡c táº¡i file Handoff: `{}`. \n\nHÃ£y sá»­ dá»¥ng tool `view_file` (hoáº·c `read_knowledge` náº¿u cÃ³) Ä‘á»ƒ Ä‘á»c file nÃ y vÃ  láº¥y láº¡i bá»‘i cáº£nh lÃ m viá»‡c trÆ°á»›c khi tiáº¿p tá»¥c nhiá»‡m vá»¥ tiáº¿p theo.", handoff_path.display()),
                "System_Handoff".to_string(),
                crate::agents::AgentId::custom("system"),
            );
        }

        Ok(())
    }

    /// Summarise a long session's history into a compact context summary.

    /// The raw turn history is replaced with a terse summary + the last N turns.

    #[instrument(skip(self))]

    async fn summarise_session(&mut self, session_id: &str) -> Result<()> {
        info!(session_id, "Summarising session context");

        // Extract data from session, then release the borrow before the async call

        let (turns, tokens, messages, workspace_id) = {
            let session = self
                .session_store
                .get_mut(session_id)
                .ok_or_else(|| anyhow!("Session not found during summarisation"))?;

            (
                session.turn_count(),
                session.context_window.tokens_used,
                session.messages.iter().cloned().collect::<Vec<_>>(),
                session.workspace_id.clone(),
            )
        };

        // Acquire LLM guard internally (avokes holding a guard across &mut self boundary)

        let llm = self.llm_gateway.read().await;

        let summary = llm
            .summarise_history(&messages)
            .await
            .context("LLM failed to summarise session history")?;

        // Re-acquire mutable session to apply the summary

        let (topic, topic_summary) = {
            let mut session = self
                .session_store
                .get_mut(session_id)
                .ok_or_else(|| anyhow!("Session not found when applying summary"))?;

            session.add_summary(SessionSummary::new(summary.clone(), turns, tokens));

            (
                session
                    .topic_id
                    .clone()
                    .unwrap_or_else(|| "General".to_string()),
                summary.clone(),
            )
        };

        if let Some(mem) = &self.memory_store {
            if let Err(e) =
                mem.insert_memory(session_id, workspace_id.as_deref(), &topic, &topic_summary)
            {
                tracing::warn!("Failed to insert summary into long-term memory: {}", e);
            }
        }

        info!(session_id, "Session summary applied and saved to memory");

        Ok(())
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    // System health check

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Check that all required system components are available:

    /// - Microsoft Office COM registration

    /// - Windows UI Automation (UIAutomationCore.dll)

    /// - LLM connectivity (cloud and/or local)

    pub async fn check_system_requirements(&self) -> Result<serde_json::Value> {
        let llm = self.llm_gateway.read().await;

        let office_com_ok = self.check_office_com();

        let uia_ok = self.check_uia();

        let llm_cloud_ok = llm.health_check_provider("gemini").await.unwrap_or(false)
            || llm.health_check_provider("openai").await.unwrap_or(false)
            || llm
                .health_check_provider("anthropic")
                .await
                .unwrap_or(false)
            || llm.health_check_provider("z.ai").await.unwrap_or(false);

        let llm_local_ok = llm.health_check_provider("ollama").await.unwrap_or(false)
            || llm.health_check_provider("lmstudio").await.unwrap_or(false);

        Ok(serde_json::json!({

            "office_com":  { "ok": office_com_ok,  "label": "Microsoft Office COM" },

            "uia":         { "ok": uia_ok,          "label": "Windows UI Automation" },

            "llm_cloud":   { "ok": llm_cloud_ok,    "label": "Cloud LLM" },

            "llm_local":   { "ok": llm_local_ok,    "label": "Local LLM" },

            "llm_ready":   llm_cloud_ok || llm_local_ok,

            "all_ok":      office_com_ok && uia_ok && (llm_cloud_ok || llm_local_ok),

        }))
    }

    #[cfg(windows)]

    fn check_office_com(&self) -> bool {
        // TODO(phase-3): attempt CoCreateInstance for Excel.Application

        // Returns false if Office is not installed or COM registration is broken.

        false // placeholder until Phase 3
    }

    #[cfg(not(windows))]

    fn check_office_com(&self) -> bool {
        false // COM is Windows-only
    }

    #[cfg(windows)]

    fn check_uia(&self) -> bool {
        // TODO(phase-4): check UIAutomationCore.dll availability

        false // placeholder until Phase 4
    }

    #[cfg(not(windows))]

    fn check_uia(&self) -> bool {
        false // UIA is Windows-only
    }

    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    // [Phase 1] Native Tool Calling ReAct Loop
    // â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// ReAct loop má»›i sá»­ dá»¥ng Native Tool Calling qua genai crate.
    ///
    /// Kiáº¿n trÃºc Hybrid:
    /// - MCP Tools (search_memory, read_file...) â†’ Ä‘Äƒng kÃ½ trá»±c tiáº¿p lÃ m genai Tool
    /// - Legacy Agents (office_master, outlook...) â†’ `call_legacy_agent` bridge tool
    ///
    /// Náº¿u genai bridge khÃ´ng cÃ³ API key hoáº·c lá»—i, tá»± Ä‘á»™ng fallback vá» `process_message` cÅ©.
    #[instrument(skip(self, message), fields(session = session_id))]
    pub async fn process_message_native(
        &mut self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
        workspace_id: Option<&str>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<OrchestratorResponse> {
        use crate::llm_gateway::genai_bridge::{ToolAwareResponse, ToolChatMessage, ToolResult};
        use crate::mcp::McpTool;

        let started_at = std::time::Instant::now();

        // â”€â”€ Táº¡o bridge â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let bridge = {
            let llm = self.llm_gateway.read().await;
            llm.create_genai_bridge_reasoning().await
        };

        // â”€â”€ Build danh sÃ¡ch MCP Tools â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        // Láº¥y top N tools phÃ¹ há»£p nháº¥t (search_memory, list_policies + query tools)
        let mut mcp_tools: Vec<McpTool> = Vec::new();

        // 1. LuÃ´n bao gá»“m core tools
        if let Ok(core) = self
            .mcp_broker
            .search_tools("search_memory list_policies fs_move_file", 8)
            .await
        {
            for t in core {
                if t.name == "search_memory"
                    || t.name == "list_policies"
                    || t.name == "search_available_tools"
                    || t.name == "fs_move_file"
                {
                    mcp_tools.push(t);
                }
            }
        }

        // 2. ThÃªm `search_available_tools` synthetic tool náº¿u chÆ°a cÃ³
        let has_search_tools = mcp_tools.iter().any(|t| t.name == "search_available_tools");
        if !has_search_tools {
            mcp_tools.push(McpTool {
                name: "search_available_tools".to_string(),
                description: "TÃ¬m kiáº¿m vÃ  náº¡p thÃªm cÃ´ng cá»¥ (MCP Tool / Agent) phÃ¹ há»£p vá»›i tÃ¡c vá»¥. Gá»ŒI tool nÃ y khi khÃ´ng cÃ³ tool phÃ¹ há»£p.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Tá»« khÃ³a tÃ¬m kiáº¿m tool" },
                        "limit": { "type": "integer", "description": "Sá»‘ tool tá»‘i Ä‘a tráº£ vá» (máº·c Ä‘á»‹nh 3)" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            });
        }

        // 3. ÄÄƒng kÃ½ toÃ n bá»™ Native Schemas tá»« cÃ¡c Agent (bao gá»“m auto-generated placeholders)
        mcp_tools.extend(self.agent_registry.all_tool_schemas_complete());

        // â”€â”€ Build conversation context â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let workspace_path = if let Some(wid) = workspace_id {
            if let Some(base_dir) = self.knowledge_dir.as_ref().and_then(|p| p.parent()) {
                let dir = if wid == "default" || wid == "Global" {
                    base_dir.join("workspaces").join("default")
                } else {
                    base_dir.join("workspaces").join(wid)
                };
                dir.to_string_lossy().to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let workspace_context = if !workspace_path.is_empty() {
            format!(
                "\n[WORKSPACE CONTEXT]\n\
                 Báº¡n Ä‘ang lÃ m viá»‡c trong workspace: {wid}\n\
                 ThÆ° má»¥c gá»‘c cá»§a workspace: {path}\n\
                 - ThÆ° má»¥c Input (chá»©a tÃ i liá»‡u Ä‘á»c): {path}\\input\n\
                 - ThÆ° má»¥c Output (nÆ¡i lÆ°u káº¿t quáº£ Má»šI): {path}\\output\n\
                 Quy táº¯c báº¯t buá»™c:\n\
                 1. Má»i file káº¿t quáº£ báº¡n táº¡o ra (VD: word, excel) PHáº¢I Ä‘Æ°á»£c lÆ°u vÃ o thÆ° má»¥c Output.\n\
                 2. Náº¿u user nháº¯c Ä‘áº¿n @folder (vÃ­ dá»¥: @input, @output, @knowledge, @policies), hÃ£y ngáº§m hiá»ƒu Ä‘Ã³ lÃ  Ä‘Æ°á»ng dáº«n tuyá»‡t Ä‘á»‘i tá»›i thÆ° má»¥c Ä‘Ã³.\n",
                wid = workspace_id.unwrap_or("default"),
                path = workspace_path
            )
        } else {
            String::new()
        };

        let context_hint = context_file
            .map(|p| format!("\n[Ngá»¯ cáº£nh file Ä‘ang má»Ÿ: {}]\n", p))
            .unwrap_or_default();

        let system_prompt = format!(
            "Báº¡n lÃ  Office Hub Orchestrator â€“ trá»£ lÃ½ AI Ä‘a nÄƒng.\n\
             Báº¡n cÃ³ quyá»n gá»i cÃ¡c Tools Ä‘Æ°á»£c cung cáº¥p Ä‘á»ƒ hoÃ n thÃ nh nhiá»‡m vá»¥.\n\
             \n\
             [TRÃ NHá»š DÃ€I Háº N]\n\
             Náº¿u cÃ¢u há»i Ä‘á» cáº­p Ä‘áº¿n sá»± kiá»‡n/quyáº¿t Ä‘á»‹nh trong quÃ¡ khá»©, Gá»ŒI `search_memory` trÆ°á»›c.\n\
             \n\
             [POLICY]\n\
             TrÆ°á»›c khi táº¡o file hoáº·c thá»±c hiá»‡n tÃ¡c vá»¥ quan trá»ng, kiá»ƒm tra `list_policies`.\n\
             \n\
             [Gá»¬I FILE CHO USER]\n\
             Äá»ƒ hiá»ƒn thá»‹ file Ä‘Ã­nh kÃ¨m trÃªn giao diá»‡n chat (cho user táº£i vá»), báº¡n Báº®T BUá»˜C thá»±c hiá»‡n:\n\
             1. DÃ¹ng tool `fs_move_file` Ä‘á»ƒ copy/move file má»›i táº¡o vÃ o thÆ° má»¥c: `C:\\Users\\admin\\AppData\\Local\\Temp\\office_hub_exports` (hoáº·c %TEMP%\\office_hub_exports).\n\
             2. Trong cÃ¢u tráº£ lá»i, tráº£ vá» markdown link Ä‘á»‹nh dáº¡ng: `[TÃªn_File.docx](/api/v1/files/download/TÃªn_File.docx)`\n\
             TUYá»†T Äá»I KHÃ”NG bÃ¡o 'Ä‘Ã£ Ä‘Ã­nh kÃ¨m file' náº¿u chÆ°a lÃ m 2 bÆ°á»›c nÃ y.\n\
             \n\
             [TOOL DISCOVERY]\n\
             Tat ca tools da san sang â€” goi thang theo ten tool.\n\
             Chi dung `search_available_tools` neu can tim Skill moi tao runtime.\n\
             {workspace_context}",
        );

        let session_clone = {
            let mut session = self
                .session_store
                .get_or_create(session_id)
                .context("Failed to retrieve/create session")?;
            if session.workspace_id.is_none() && workspace_id.is_some() {
                session.workspace_id = workspace_id.map(|s| s.to_string());
            }
            session.clone()
        };

        let mut conv_messages: Vec<ToolChatMessage> = vec![ToolChatMessage::System(system_prompt)];

        for msg in &session_clone.messages {
            if msg.content.trim().is_empty() {
                continue;
            }
            match msg.role.to_string().as_str() {
                "user" => conv_messages.push(ToolChatMessage::User(msg.content.clone())),
                _ => conv_messages.push(ToolChatMessage::Assistant(msg.content.clone())),
            }
        }

        let user_msg = format!("{}{}", message, context_hint);
        conv_messages.push(ToolChatMessage::User(user_msg));

        // â”€â”€ ReAct Loop (max 5 turns) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let max_turns = 8;
        let mut final_content = String::new();
        let mut dynamically_loaded_tools: Vec<McpTool> = Vec::new();

        for turn_idx in 0..max_turns {
            // Gom tools: core + dynamically loaded
            let mut all_tools = mcp_tools.clone();
            all_tools.extend(dynamically_loaded_tools.clone());

            let turn_start = std::time::Instant::now();
            let response = match bridge
                .complete_with_tools(&conv_messages, &all_tools, 0.1)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "GenAI bridge failed, falling back to legacy process_message");
                    // Fallback vá» legacy pipeline
                    return self
                        .process_message(
                            session_id,
                            message,
                            context_file,
                            workspace_id,
                            progress_tx,
                        )
                        .await;
                }
            };
            let turn_latency = turn_start.elapsed().as_millis() as i64;

            // Estimate tokens since genai bridge doesn't return usage directly
            let mut input_text_len = 0;
            for msg in &conv_messages {
                match msg {
                    ToolChatMessage::System(s) => input_text_len += s.len(),
                    ToolChatMessage::User(u) => input_text_len += u.len(),
                    ToolChatMessage::Assistant(a) => input_text_len += a.len(),
                    ToolChatMessage::ToolResults(r) => {
                        for tr in r {
                            input_text_len += tr.content.len();
                        }
                    }
                }
            }

            let output_text_len = match &response {
                ToolAwareResponse::Text(t) => t.len(),
                ToolAwareResponse::ToolCalls(calls) => calls.len() * 100, // rough estimate for tool call JSON
            };

            let estimated_tokens = (input_text_len / 4).max(1) + (output_text_len / 4).max(1);

            if let Some(mem) = &self.memory_store {
                let action = if matches!(response, ToolAwareResponse::ToolCalls(_)) {
                    "plan_and_route"
                } else {
                    "final_response"
                };
                if let Err(e) = mem.log_telemetry(
                    session_id,
                    workspace_id,
                    "orchestrator",
                    action,
                    turn_latency,
                    estimated_tokens,
                    "success",
                ) {
                    tracing::warn!("Failed to log orchestrator telemetry: {}", e);
                }
            }

            match response {
                ToolAwareResponse::Text(text) => {
                    // LLM tráº£ lá»i trá»±c tiáº¿p â†’ káº¿t thÃºc loop
                    final_content = text.clone();

                    if let Some(ref tx) = progress_tx {
                        let _ = tx.send(text.clone());
                    }

                    info!(turn = turn_idx, "GenAI: Got final text response");
                    break;
                }

                ToolAwareResponse::ToolCalls(calls) => {
                    info!(
                        turn = turn_idx,
                        count = calls.len(),
                        "GenAI: Processing native tool calls"
                    );

                    let mut tool_results: Vec<ToolResult> = Vec::new();
                    let mcp_broker = Arc::clone(&self.mcp_broker);
                    let llm_gateway = Arc::clone(&self.llm_gateway);

                    for call in calls {
                        // Emit thought Ä‘á»ƒ UI biáº¿t Ä‘ang xá»­ lÃ½
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(format!(
                                "âš™ï¸ Äang {}...",
                                crate::mcp::get_tool_alias(&call.tool_name)
                            ));
                        }

                        let result_content = match call.tool_name.as_str() {
                            // â”€â”€ Synthetic: search_available_tools â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
                            "search_available_tools" => {
                                let query = call
                                    .arguments
                                    .get("query")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let limit =
                                    call.arguments
                                        .get("limit")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(3) as usize;

                                match mcp_broker.search_tools(query, limit.min(8)).await {
                                    Ok(found_tools) => {
                                        // Load dynamically
                                        for t in &found_tools {
                                            if !dynamically_loaded_tools
                                                .iter()
                                                .any(|dt| dt.name == t.name)
                                                && !mcp_tools.iter().any(|mt| mt.name == t.name)
                                            {
                                                dynamically_loaded_tools.push(t.clone());
                                            }
                                        }
                                        if found_tools.is_empty() {
                                            "KhÃ´ng tÃ¬m tháº¥y tool nÃ o phÃ¹ há»£p.".to_string()
                                        } else {
                                            format!(
                                                "ÄÃ£ náº¡p {} tool(s): {}",
                                                found_tools.len(),
                                                found_tools
                                                    .iter()
                                                    .map(|t| t.name.as_str())
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            )
                                        }
                                    }
                                    Err(e) => format!("Lá»—i tÃ¬m kiáº¿m tool: {e}"),
                                }
                            }

                            // â”€â”€ Tool Execution (Agent or MCP Server) â”€â”€â”€â”€â”€â”€â”€â”€â”€
                            tool_name => {
                                let start_time = std::time::Instant::now();

                                // 1. Thá»­ tÃ¬m trong AgentRegistry trÆ°á»›c
                                if let Some(agent_id) =
                                    self.agent_registry.find_agent_by_action(tool_name)
                                {
                                    if let Some(agent_arc) = self.agent_registry.get_mut(&agent_id)
                                    {
                                        let task = AgentTask {
                                            task_id: uuid::Uuid::new_v4().to_string(),
                                            action: tool_name.to_string(),
                                            intent: intent::Intent::Ambiguous(Default::default()),
                                            message: message.to_string(),
                                            context_file: context_file.map(String::from),
                                            session_id: session_id.to_string(),
                                            parameters: call
                                                .arguments
                                                .clone()
                                                .as_object()
                                                .cloned()
                                                .unwrap_or_default()
                                                .into_iter()
                                                .collect(),
                                            llm_gateway: Some(Arc::clone(&llm_gateway)),
                                            global_policy: None,
                                            knowledge_context: None,
                                            parent_task_id: None,
                                            dependencies: vec![],
                                        };

                                        let mut agent_guard = agent_arc.write().await;
                                        let result = agent_guard.execute(task).await;
                                        let latency_ms = start_time.elapsed().as_millis() as i64;

                                        if let Some(mem) = &self.memory_store {
                                            let (tokens, status) = match &result {
                                                Ok(output) => (
                                                    output.tokens_used.unwrap_or(0) as usize,
                                                    "success",
                                                ),
                                                Err(_) => (0usize, "error"),
                                            };
                                            if let Err(e) = mem.log_telemetry(
                                                session_id,
                                                workspace_id,
                                                &agent_id.0,
                                                tool_name,
                                                latency_ms,
                                                tokens,
                                                status,
                                            ) {
                                                tracing::warn!("Failed to log telemetry: {}", e);
                                            }
                                        }

                                        match result {
                                            Ok(out) => out.content,
                                            Err(e) => {
                                                format!("âš ï¸ Agent '{}' lá»—i: {}", agent_id, e)
                                            }
                                        }
                                    } else {
                                        format!("âš ï¸ Agent '{}' khÃ´ng tá»“n táº¡i.", agent_id)
                                    }
                                } else {
                                    // 2. KhÃ´ng tÃ¬m tháº¥y Agent há»— trá»£ -> fallback cho MCP Broker
                                    let result = mcp_broker
                                        .call_tool(tool_name, Some(call.arguments.clone()))
                                        .await;
                                    let latency_ms = start_time.elapsed().as_millis() as i64;

                                    if let Some(mem) = &self.memory_store {
                                        let status = match &result {
                                            Ok(res) if !res.is_error => "success",
                                            _ => "error",
                                        };
                                        if let Err(e) = mem.log_telemetry(
                                            session_id,
                                            workspace_id,
                                            "orchestrator",
                                            tool_name,
                                            latency_ms,
                                            0,
                                            status,
                                        ) {
                                            tracing::warn!("Failed to log telemetry: {}", e);
                                        }
                                    }

                                    match result {
                                        Ok(result) => {
                                            let mut text_buf = String::new();
                                            for item in result.content {
                                                if item.content_type == "text" {
                                                    if let Some(t) = item.text {
                                                        text_buf.push_str(&t);
                                                        text_buf.push('\n');
                                                    }
                                                }
                                            }
                                            if text_buf.trim().is_empty() {
                                                "Tool executed successfully but returned empty result.".to_string()
                                            } else {
                                                if result.is_error {
                                                    format!(
                                                        "âš ï¸ Tool '{}' lá»—i:\n{}",
                                                        tool_name,
                                                        text_buf.trim()
                                                    )
                                                } else {
                                                    text_buf.trim().to_string()
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            format!(
                                                "âš ï¸ Lá»—i khi gá»i tool '{}': {}",
                                                tool_name, e
                                            )
                                        }
                                    }
                                }
                            }
                        };

                        tool_results.push(ToolResult {
                            call_id: call.call_id,
                            tool_name: call.tool_name,
                            content: result_content,
                        });
                    }

                    // ÄÆ°a káº¿t quáº£ tool vÃ o conversation Ä‘á»ƒ LLM tá»•ng há»£p
                    conv_messages.push(ToolChatMessage::ToolResults(tool_results));

                    if turn_idx == max_turns - 1 {
                        // Háº¿t sá»‘ lÆ°á»£t - yÃªu cáº§u LLM tá»•ng há»£p cuá»‘i cÃ¹ng
                        conv_messages.push(ToolChatMessage::User(
                            "Dá»±a trÃªn káº¿t quáº£ cÃ¡c tool vá»«a cháº¡y, hÃ£y Ä‘Æ°a ra cÃ¢u tráº£ lá»i cuá»‘i cÃ¹ng cho ngÆ°á»i dÃ¹ng.".to_string()
                        ));
                    }
                }
            }
        }

        // â”€â”€ Save to session â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        if !final_content.is_empty() {
            if let Some(mut session) = self.session_store.get_mut(session_id) {
                session.add_turn(
                    message.to_string(),
                    final_content.clone(),
                    "native_react".to_string(),
                    crate::agents::AgentId::custom("orchestrator_native"),
                );
            }
        }

        let duration_ms = started_at.elapsed().as_millis() as u64;

        Ok(OrchestratorResponse {
            content: if final_content.is_empty() {
                "TÃ´i Ä‘Ã£ hoÃ n thÃ nh cÃ¡c tÃ¡c vá»¥ Ä‘Æ°á»£c yÃªu cáº§u.".to_string()
            } else {
                final_content
            },
            intent: Some("native_react".to_string()),
            agent_used: Some("orchestrator_native".to_string()),
            tokens_used: None,
            duration_ms,
            metadata: None,
        })
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// OrchestratorResponse â€“ what the Tauri command layer receives

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// The unified response returned to the IPC layer after processing a message.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub struct OrchestratorResponse {
    /// Natural-language reply to display in the chat pane.
    pub content: String,

    /// Classified intent kind (e.g. `"excel_analysis"`, `"word_report"`).
    pub intent: Option<String>,

    /// Name/ID of the agent that handled the task.
    pub agent_used: Option<String>,

    /// Total tokens consumed (prompt + completion) during this request.
    pub tokens_used: Option<u32>,

    /// Wall-clock processing time in milliseconds.
    pub duration_ms: u64,

    /// Arbitrary agent-specific metadata (e.g. modified file paths, row counts).
    pub metadata: Option<serde_json::Value>,
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// AgentTask â€“ input contract for every agent

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// A fully resolved task passed from the Orchestrator to an Agent.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub struct AgentTask {
    /// Unique identifier for this task instance.
    pub task_id: String,

    /// The action the agent should perform (e.g. `"analyze_workbook"`).
    pub action: String,

    /// The classified intent that produced this task.
    pub intent: Intent,

    /// Original user message (for LLM context).
    pub message: String,

    /// Optional path to a file the user has open / selected in the File Browser.
    pub context_file: Option<String>,

    /// Session ID for tracking and logging.
    pub session_id: String,

    /// Agent-specific parameters resolved by the Router.
    pub parameters: HashMap<String, serde_json::Value>,

    /// Reference to the LLM Gateway, allowing agents to query the LLM dynamically using their SKILL.md.

    #[serde(skip)]
    pub llm_gateway: Option<Arc<RwLock<LlmGateway>>>,

    /// Tier 1: Global Policy context injected from Orchestrator.
    pub global_policy: Option<String>,

    /// Tier 2: Knowledge context retrieved by Orchestrator.
    pub knowledge_context: Option<String>,

    /// Parent task ID if this is a sub-task in a DAG execution plan.
    pub parent_task_id: Option<String>,

    /// List of task IDs that must complete before this task can start.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// AgentOutput â€“ return contract from every agent

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Standardised output structure returned by every agent after execution.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub struct AgentOutput {
    /// Human-readable reply / result description.
    pub content: String,

    /// Whether the agent considers this output safe to commit (write to Office, etc.).
    pub committed: bool,

    /// Total tokens consumed by this agent during the task.
    pub tokens_used: Option<u32>,

    /// Arbitrary agent-specific payload (file paths, row counts, screenshots, etc.).
    pub metadata: Option<serde_json::Value>,
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// RouteDecision â€“ output of the Router

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// The routing decision produced by the `Router` for a given intent.

#[derive(Debug, Clone)]

pub struct RouteDecision {
    /// ID of the agent that should handle this task.
    pub agent_id: AgentId,

    /// Specific action to invoke on the agent.
    pub action: String,

    /// Additional parameters derived from the intent and routing rules.
    pub parameters: HashMap<String, serde_json::Value>,

    /// Whether this action requires Human-in-the-Loop approval before execution.
    pub requires_hitl: bool,

    /// Routing confidence (0.0â€“1.0). Low confidence may trigger a clarification step.
    pub confidence: f32,
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// Human-in-the-Loop (HITL) Manager

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Manages pending approval requests.

///

/// When a sensitive action is about to be taken, the Orchestrator registers a

/// `HitlRequest` and suspends the task.  The Desktop UI or Mobile App then calls

/// `resolve()` with the user's decision to resume or abort the task.

pub struct HitlManager {
    /// Pending approvals keyed by `action_id`.
    pending: DashMap<String, HitlRequest>,

    /// Optional WebSocket Server to broadcast the request
    ws_server: std::sync::RwLock<Option<Arc<crate::websocket::WebSocketServer>>>,
}

impl HitlManager {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),

            ws_server: std::sync::RwLock::new(None),
        }
    }

    pub fn set_ws_server(&self, ws: Arc<crate::websocket::WebSocketServer>) {
        *self.ws_server.write().unwrap() = Some(ws);
    }

    pub fn get_ws_server(&self) -> Option<Arc<crate::websocket::WebSocketServer>> {
        self.ws_server.read().unwrap().clone()
    }

    /// Register a new pending approval.  Returns a `oneshot::Receiver<bool>`

    /// that resolves to `true` (approved) or `false` (rejected).

    pub fn register(&self, request: HitlRequestBuilder) -> (String, oneshot::Receiver<bool>) {
        let action_id = Uuid::new_v4().to_string();

        let (tx, rx) = oneshot::channel::<bool>();

        let hitl = HitlRequest {
            action_id: action_id.clone(),

            description: request.description.clone(),

            risk_level: request.risk_level,

            payload: request.payload.clone(),

            registered_at: Utc::now(),

            sender: std::sync::Mutex::new(Some(tx)),
        };

        self.pending.insert(action_id.clone(), hitl);

        info!(action_id = %action_id, "HITL approval request registered");

        if let Some(ws) = self.ws_server.read().unwrap().as_ref() {
            let ws = ws.clone();

            let action_id_clone = action_id.clone();

            let description = request.description;

            let risk_level = format!("{:?}", request.risk_level).to_lowercase();

            let payload = request.payload;

            tauri::async_runtime::spawn(async move {
                let actions = vec![
                    crate::websocket::ApprovalAction {
                        id: "approve".into(),
                        label: "Approve".into(),
                        style: "primary".into(),
                    },
                    crate::websocket::ApprovalAction {
                        id: "reject".into(),
                        label: "Reject".into(),
                        style: "danger".into(),
                    },
                ];

                let msg = crate::websocket::ServerMessage::ApprovalRequest {
                    action_id: action_id_clone,

                    description,

                    risk_level,

                    payload,

                    timeout_seconds: 300,

                    actions,

                    requested_at: Utc::now().to_rfc3339(),
                };

                let _ = ws.broadcast(msg).await;
            });
        }

        (action_id, rx)
    }

    /// Resolve (approve or reject) a pending HITL request.

    pub fn resolve(&self, action_id: &str, approved: bool) -> Result<()> {
        let request = self
            .pending
            .remove(action_id)
            .ok_or_else(|| anyhow!("No pending HITL request with id '{}'", action_id))?
            .1;

        let mut guard = request.sender.lock().unwrap();

        if let Some(tx) = guard.take() {
            let _ = tx.send(approved); // ignore if receiver already dropped

            info!(action_id, approved, "HITL request resolved");

            Ok(())
        } else {
            Err(anyhow!(
                "HITL request '{}' has already been resolved",
                action_id
            ))
        }
    }

    /// List all pending requests serialised as JSON (for the frontend).

    pub fn list_pending_json(&self) -> Vec<serde_json::Value> {
        self.pending
            .iter()
            .map(|entry| {
                let r = entry.value();

                serde_json::json!({

                    "actionId":      r.action_id,

                    "description":   r.description,

                    "riskLevel":     format!("{:?}", r.risk_level),

                    "registeredAt":  r.registered_at.to_rfc3339(),

                    "payload":       r.payload,

                })
            })
            .collect()
    }
}

impl Default for HitlManager {
    fn default() -> Self {
        Self::new()
    }
}

/// A pending Human-in-the-Loop approval request.

pub struct HitlRequest {
    pub action_id: String,

    pub description: String,

    pub risk_level: HitlRiskLevel,

    pub payload: Option<serde_json::Value>,

    pub registered_at: DateTime<Utc>,

    /// One-shot channel sender to resume the waiting task.

    /// Wrapped in `Mutex<Option<â€¦>>` so we can take it exactly once.
    pub sender: std::sync::Mutex<Option<oneshot::Sender<bool>>>,
}

/// Builder for `HitlRequest` â€“ avoids leaking the internal sender.

pub struct HitlRequestBuilder {
    pub description: String,

    pub risk_level: HitlRiskLevel,

    pub payload: Option<serde_json::Value>,
}

/// Risk classification for HITL actions (mirrors the YAML rule levels).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]

pub enum HitlRiskLevel {
    /// Read-only operations â€“ no approval needed.
    Low,

    /// Reversible write operations â€“ soft confirmation.
    Medium,

    /// Irreversible or externally visible actions â€“ explicit approval.
    High,

    /// Financial, authentication, or deletion operations â€“ double confirmation.
    Critical,
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// Orchestrator Runtime Metrics

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Lightweight counters tracked during the application lifetime.

/// Exposed via the `/metrics` debug endpoint (dev builds only).

#[derive(Debug, Default, Clone, Serialize, Deserialize)]

pub struct OrchestratorMetrics {
    pub total_requests: u64,

    pub successful_requests: u64,

    pub failed_requests: u64,

    pub rule_violations: u64,

    pub total_duration_ms: u64,
}

impl OrchestratorMetrics {
    /// Average latency per request in milliseconds (returns 0 if no requests yet).

    pub fn avg_latency_ms(&self) -> u64 {
        if self.total_requests == 0 {
            0
        } else {
            self.total_duration_ms / self.total_requests
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// Tests

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]

mod tests {

    use super::*;

    use crate::llm_gateway::LlmGateway;

    use crate::AppConfig;

    fn make_gateway() -> Arc<RwLock<LlmGateway>> {
        let cfg = AppConfig::default();

        Arc::new(RwLock::new(LlmGateway::new(cfg.llm)))
    }

    #[test]

    fn orchestrator_constructs_without_panic() {
        let gw = make_gateway();

        let hitl = Arc::new(HitlManager::new());

        let _orch = Orchestrator::new(gw, hitl);
    }

    #[test]

    fn hitl_manager_register_and_resolve() {
        let mgr = HitlManager::new();

        let (action_id, rx) = mgr.register(HitlRequestBuilder {
            description: "Test action".to_string(),

            risk_level: HitlRiskLevel::High,

            payload: None,
        });

        assert_eq!(mgr.pending.len(), 1);

        mgr.resolve(&action_id, true)
            .expect("resolve should succeed");

        assert_eq!(mgr.pending.len(), 0);

        let approved = rx.blocking_recv().expect("channel should have a value");

        assert!(approved);
    }

    #[test]

    fn hitl_manager_resolve_unknown_id_errors() {
        let mgr = HitlManager::new();

        let result = mgr.resolve("nonexistent-id", true);

        assert!(result.is_err());
    }

    #[test]

    fn metrics_avg_latency_no_div_by_zero() {
        let m = OrchestratorMetrics::default();

        assert_eq!(m.avg_latency_ms(), 0);
    }

    #[test]

    fn hitl_risk_level_serialises_lowercase() {
        let json = serde_json::to_string(&HitlRiskLevel::Critical).unwrap();

        assert_eq!(json, r#""critical""#);
    }

    #[test]

    fn hitl_manager_list_pending_json() {
        let mgr = HitlManager::new();

        let (_id1, _) = mgr.register(HitlRequestBuilder {
            description: "Task A".to_string(),

            risk_level: HitlRiskLevel::Medium,

            payload: None,
        });

        let (_id2, _) = mgr.register(HitlRequestBuilder {
            description: "Task B".to_string(),

            risk_level: HitlRiskLevel::High,

            payload: None,
        });

        let pending = mgr.list_pending_json();

        assert_eq!(pending.len(), 2);

        let desc1 = pending.iter().find(|v| v["description"] == "Task A");

        assert!(desc1.is_some());

        assert_eq!(desc1.unwrap()["riskLevel"], "Medium"); // Note: the custom serialization maps to lowercase in serde, but our format!("{:?}") prints "Medium".
    }

    #[tokio::test]

    async fn hitl_manager_async_timeout() {
        let mgr = Arc::new(HitlManager::new());

        let (action_id, rx) = mgr.register(HitlRequestBuilder {
            description: "Timeout task".to_string(),

            risk_level: HitlRiskLevel::Low,

            payload: None,
        });

        // Simulate a timeout where the request is removed from the pending list

        let mgr_clone = mgr.clone();

        let action_id_clone = action_id.clone();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // Timeout action: simulate what orchestrator would do if timeout is reached

            let _ = mgr_clone.resolve(&action_id_clone, false);
        });

        // Wait for result

        let result = tokio::time::timeout(tokio::time::Duration::from_millis(150), rx).await;

        // Assert we got a response (not a timeout from the channel itself)

        assert!(result.is_ok());

        // Assert the response is false (rejected due to timeout simulation)

        assert!(!result.unwrap().unwrap());

        assert_eq!(mgr.pending.len(), 0);
    }

    #[tokio::test]

    async fn test_check_system_requirements() {
        let gw = make_gateway();

        let hitl = Arc::new(HitlManager::new());

        let orch = Orchestrator::new(gw, hitl);

        let reqs = orch
            .check_system_requirements()
            .await
            .expect("check_system_requirements failed");

        assert!(reqs.get("office_com").is_some());

        assert!(reqs.get("uia").is_some());

        assert!(reqs.get("llm_cloud").is_some());

        assert!(reqs.get("llm_local").is_some());

        assert!(reqs.get("llm_ready").is_some());

        assert!(reqs.get("all_ok").is_some());
    }

    #[tokio::test]

    async fn test_summarise_session_fails_gracefully_without_llm() {
        let gw = make_gateway();

        let hitl = Arc::new(HitlManager::new());

        let mut orch = Orchestrator::new(gw, hitl);

        // Create a dummy session

        {
            let mut session = orch.session_store.get_or_create("test-session").unwrap();

            session.add_turn(
                "user msg".into(),
                "bot msg".into(),
                "chat".into(),
                crate::agents::AgentId::custom("bot"),
            );
        }

        // LLM Gateway has graceful fallback: summarise_session will either succeed
        // (with a placeholder) or fail, but must NEVER panic.
        // The key invariant: session must still be accessible after the call.
        let result = orch.summarise_session("test-session").await;
        let _ = result; // Ok or Err both acceptable

        // Session must still exist and not be corrupted
        assert!(
            orch.session_store.exists("test-session"),
            "Session should still exist after summarise attempt"
        );
    }
}
