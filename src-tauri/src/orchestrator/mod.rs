// ============================================================================

// Office Hub – orchestrator/mod.rs

//

// The Orchestrator is the central "brain" of Office Hub.

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

// ─────────────────────────────────────────────────────────────────────────────

// Re-exports for convenience

// ─────────────────────────────────────────────────────────────────────────────

// Re-exports

// Note: Intent and SessionId are already imported via use self::... above

// ─────────────────────────────────────────────────────────────────────────────

// OrchestratorHandle – cheap, cloneable handle used in commands.rs

// ─────────────────────────────────────────────────────────────────────────────

/// A cheaply cloneable, thread-safe handle to the `Orchestrator`.

/// Wraps the inner struct in `Arc<RwLock<…>>`.

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

    /// Process a user chat message end-to-end (classify → route → execute → validate).

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
    /// Đây là Hybrid ReAct Loop mới:
    /// - MCP Tools → Native `genai::Tool` (không cần JSON Schema thủ công)
    /// - Legacy Agents → `call_legacy_agent` bridge tool
    /// - Fallback: nếu genai bridge thất bại, tự động dùng `process_message` cũ
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

// ─────────────────────────────────────────────────────────────────────────────

// Orchestrator – inner struct

// ─────────────────────────────────────────────────────────────────────────────

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

    // ─────────────────────────────────────────────────────────────────────────

    // Core processing pipeline

    // ────────────────────────────────────────────────────────────────────────�        // ── 2. Build Agent Tool Prompt ──────────────────────────────────────────

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

        // ── 1. Retrieve Session ───────────────────────────────────────────────
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

        // ── 2. Build Agent Tool Prompt ──────────────────────────────────────────
        // Inject TOÀN BỘ catalog: MCP tools + Agent tool schemas để LLM không cần search mù.
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
                                        "\n--- Mở đầu Policy: {} ---\n",
                                        filename
                                    ));
                                    project_policy_content.push_str(&content);
                                    project_policy_content.push_str(&format!(
                                        "\n--- Kết thúc Policy: {} ---\n",
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
                    format!("\n[PROJECT POLICIES]\nDưới đây là các Policy riêng của dự án này, BẮT BUỘC phải tuân thủ (ưu tiên cao hơn Global Policy nếu có xung đột, trừ khi Global Policy đánh dấu là 'Bắt buộc'):\n{}", project_policy_content)
                };

                format!("\n[QUAN TRỌNG: WORKSPACE CONTEXT]\nBạn đang hoạt động trong workspace có ID là: '{}'. Thư mục gốc của workspace này trên ổ đĩa là: `{}`.\n- Dữ liệu đầu vào (file tải lên, ghi âm...) nằm trong thư mục `{}/docs/inbox/`.\n- Kết quả xử lý (file báo cáo, xuất ra) BẮT BUỘC lưu vào thư mục `{}/docs/outbox/`.\n- Nếu người dùng nhắc đến dự án khác (VD: 'Với dự án Beta...'), hãy gọi agent_id = 'orchestrator', action = 'set_active_project' với tham số `project_name` để chuyển ngữ cảnh sang dự án đó.\n- Khi dùng tool `search_memory`, BẮT BUỘC thêm tiền tố `[{}]` vào từ khóa tìm kiếm (VD: `[{}] quy trình mua hàng`).\nKhi gọi các tool tạo file (như `office_master`), bạn phải truyền đường dẫn lưu file tuyệt đối vào thư mục outbox này.\nKhi gọi bất kỳ MCP tool nào liên quan đến dữ liệu (như knowledge, policy...), BẠN BẮT BUỘC phải truyền thêm tham số `\"workspace_id\": \"{}\"` vào arguments của tool.\n{}", wid, root_str, root_str, root_str, wid, wid, wid, policies_prompt)
            } else {
                format!("\n[QUAN TRỌNG: WORKSPACE CONTEXT]\nBạn đang hoạt động trong workspace có ID là: '{}'.\n- Nếu người dùng nhắc đến dự án khác (VD: 'Với dự án Beta...'), hãy gọi agent_id = 'orchestrator', action = 'set_active_project' với tham số `project_name` để chuyển ngữ cảnh sang dự án đó.\n- Khi dùng tool `search_memory`, BẮT BUỘC thêm tiền tố `[{}]` vào từ khóa tìm kiếm.\nKhi gọi bất kỳ MCP tool nào liên quan đến dữ liệu (như knowledge, policy...), BẠN BẮT BUỘC phải truyền thêm tham số `\"workspace_id\": \"{}\"` vào arguments của tool.\n", wid, wid, wid)
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
            "Bạn là Office Hub Orchestrator, một trợ lý điều phối Agent.\n\
             \n[AVAILABLE MCP TOOLS]\n\
             Bạn có thể dùng các MCP Tools sau đây để tra cứu Policy, Memory, Knowledge hoặc gọi plugin:\n\
             {mcp_tools_desc}\n\
             \n[AVAILABLE SKILLS/AGENTS]\n\
             Bạn có thể gọi các Agents sau đây để thực hiện nhiệm vụ:\n\
             {tools_desc}\n\
             [QUAN TRỌNG: TRÍ NHỚ DÀI HẠN]\n\
             Hệ thống KHÔNG tự động nhồi ngữ cảnh cũ vào cuộc hội thoại để tiết kiệm token. Nếu câu hỏi của người dùng ám chỉ đến sự kiện, quyết định, hoặc thông tin trong quá khứ (VD: 'lần trước', 'hôm qua', 'phương án đã chốt'), BẠN BẮT BUỘC PHẢI DÙNG MCP Tool `search_memory` (agent_id = 'mcp_broker') để lấy lại trí nhớ trước khi trả lời.\n\
             \n\
             [QUAN TRỌNG: TUÂN THỦ POLICY VÀ RULE]\n\
             Trước khi bắt đầu bất kỳ tác vụ nào (ví dụ: tạo file Word, viết báo cáo, lập trình, hay thay đổi hệ thống), BẠN BẮT BUỘC phải gọi MCP Tool `list_policies` hoặc `query_policy` (agent_id = 'mcp_broker') để kiểm tra xem hệ thống có quy định (rule/policy) nào cần tuân thủ không.\n\
             \n\
             [QUAN TRỌNG: GỘP THÔNG TIN TỪ NHIỀU FILE]\n\
             Nếu người dùng yêu cầu tổng hợp báo cáo từ một thư mục, hãy sử dụng MCP Tool `read_folder_files` (agent_id = 'mcp_broker') để lấy nội dung tất cả file, sau đó gọi agent (VD: `office_master`) để tạo báo cáo. Nếu các tool không phụ thuộc nhau, bạn có thể gọi đồng thời nhiều tools/agents trong một mảng `agent_calls` để chúng chạy song song.\n\
             \n\
             [QUAN TRỌNG: CẬP NHẬT FILE ĐÃ TẠO]\n\
             Nếu người dùng yêu cầu cập nhật/chỉnh sửa một file đã tạo trước đó, BẠN BẮT BUỘC phải xem lại lịch sử hội thoại, tìm ĐÚNG tên file cũ và truyền chính xác tên đó vào tham số của Agent/Tool. KHÔNG ĐƯỢC tạo file mới.\n\
             \n\
             [QUAN TRỌNG: CÀI ĐẶT SKILL TỪ FILE ZIP]\n\
             Nếu người dùng yêu cầu cài đặt skill từ file đính kèm (zip), BẮT BUỘC gọi Agent `converter` với action `analyze_and_convert_zip_skill` và truyền đường dẫn file vào `zip_path`. Công cụ này sẽ tự động phân tích mã nguồn, map dependencies và convert skill đó cho bạn. KHÔNG sử dụng tool `write_skill` vì nội dung file dài sẽ làm hỏng cấu trúc JSON.\n\
             \n\
             [QUAN TRỌNG: TỰ TIẾN HÓA & TẠO TOOL (SELF-EVOLUTION)]\n\
             Nếu bạn nhận được một yêu cầu không có sẵn công cụ xử lý (kể cả sau khi đã dùng `search_available_tools`), hãy đánh giá xem tác vụ đó có mang tính quy trình lặp đi lặp lại (reusable workflow/automation) hay không. Nếu CÓ, BẠN CÓ QUYỀN TỰ TẠO RA CÔNG CỤ MỚI bằng cách gọi MCP Tool `write_skill` (agent_id = 'mcp_broker'). \n\
             Tham số của `write_skill` (BẠN ĐƯỢC DÙNG MÀ KHÔNG CẦN SEARCH): \n\
             - `skill_name` (chuỗi kebab-case, BẮT BUỘC có tiền tố `auto-`, VD: `auto-csv-formatter`). \n\
             - `description` (mô tả chức năng của tool). \n\
             - `parameters` (object chứa định nghĩa properties theo chuẩn JSON Schema). \n\
             - `instructions` (nội dung hướng dẫn chi tiết bằng Markdown để hệ thống biết cách thực thi). \n\
             Ngay sau khi tool `write_skill` trả về thành công, tool sẽ tự động có mặt trên hệ thống. Hãy GỌI LẠI CHÍNH TOOL ĐÓ ở turn tiếp theo để giải quyết yêu cầu ban đầu của user!\n\
             Lưu ý: Đối với các tác vụ một lần (one-off), hãy tự xử lý bằng code python/powershell (qua win32) hoặc từ chối, KHÔNG được tạo tool rác.\n\
             \n\
             [NGHIEN CUU WEB - OBSCURA ENGINE]\n\
             He thong tich hop Obscura headless browser (V8, stealth mode). KHONG can Chrome/Edge mo. Dung khi can doc web:\n\
             - web_fetch (agent_id='mcp_broker'): Doc 1 URL, render JS day du. Params: url(bat buoc), mode(text/html/links), eval(JS tuy chon).\n\
             - web_scrape_parallel (agent_id='mcp_broker'): Scrape nhieu URL song song. Params: urls(array), concurrency(default 5, max 25).\n\
             - search_web (agent_id='mcp_broker'): Tim kiem va tra ve danh sach URLs lien quan.\n\
             WORKFLOW CHUAN: search_web -> lay URLs -> web_scrape_parallel -> doc song song -> tong hop ket qua.\n\
             {workspace_instruction}\n\
             Nếu bạn cần dùng Agent hoặc Tool, hãy trả về danh sách agent_calls với agent_id (với Agent) hoặc tên tool (với MCP Tool) và action, kèm parameters dưới dạng JSON.\n\
             Lưu ý: Đối với MCP Tools, bắt buộc đặt agent_id = 'mcp_broker' và action = tên của tool.\n\
             Nếu câu hỏi chỉ là trò chuyện thông thường hoặc bạn đã có đủ thông tin, hãy điền vào direct_response.\n\
             Luôn đưa ra 'thought' giải thích quá trình suy luận của bạn.\n\
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
            format!("\n[Ngữ cảnh file đang mở: {}]\n", path)
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
                        let mut found_end = false;

                        while end_idx < bytes.len() {
                            if bytes[end_idx] == b'\\' && !escaped {
                                escaped = true;
                            } else if bytes[end_idx] == b'\"' && !escaped {
                                found_end = true;
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
                        turn_content.push_str("⚠️ Lỗi lập lịch DAG: Phát hiện vòng lặp hoặc dependency không tồn tại.\n\n");
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
                                return (agent_id.clone(), task_id_opt, true, format!("Đã chuyển ngữ cảnh sang dự án '{}'. Các câu trả lời tiếp theo sẽ dùng trí nhớ và file của dự án này.\n\n", proj_clone), 0, None);
                            }

                            info!(agent = %agent_id, action = %action, "Dispatching to agent");

                            if let Some(tx) = &progress_tx {
                                let _ = tx.send(crate::workflow::WorkflowProgressUpdate::Step {
                                    run_id: session_id_str.clone(),
                                    workflow_id: "orchestrator".to_string(),
                                    step_id: agent_id.clone(),
                                    step_name: agent_id.clone(),
                                    status: crate::workflow::RunStatus::Running,
                                    message: Some(format!("{} đang {}", agent_id, action)),
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
                                                "Không tìm thấy công cụ nào phù hợp với yêu cầu.".to_string()
                                            } else {
                                                format!("Danh sách công cụ phù hợp:\n{}", desc)
                                            }
                                        }
                                        Err(e) => format!("Lỗi khi tìm kiếm công cụ: {}", e),
                                    };
                                    return (agent_id.clone(), task_id_opt, false, format!("MCP Tool '{}' result:\n{}\n\n", action, result), 0, None);
                                }

                                let requires_hitl = matches!(
                                    action.as_str(),
                                    "win32_registry_write" | "win32_winget_install" | "win32_winget_uninstall" | "win32_process_kill" | "win32_shell_execute" | "win32_file_delete"
                                );

                                if requires_hitl {
                                    let hitl_req = crate::orchestrator::HitlRequestBuilder {
                                        description: format!("Yêu cầu phê duyệt gọi hệ thống Win32: '{}'", action),
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
                                                message: Some(format!("MCP {} {}", action, if result.is_error { "lỗi" } else { "hoàn tất" })),
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
                                                message: Some(format!("Lỗi gọi MCP {}: {}", action, e)),
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
                                            message: Some(format!("Agent '{}' không tồn tại", agent_id)),
                                            updated_at: chrono::Utc::now(),
                                        });
                                    }
                                    return (agent_id.clone(), task_id_opt, false, format!("⚠️ Lỗi: Agent '{}' not found in registry\n\n", agent_id), 0, None);
                                }
                            };

                            let requires_hitl = matches!(
                                action.as_str(),
                                "send_email" | "run_power_query" | "generate_vba" | "web_navigate"
                            );

                            if requires_hitl {
                                let hitl_req = crate::orchestrator::HitlRequestBuilder {
                                    description: format!("Yêu cầu phê duyệt hành động '{}' cho agent '{}'.", action, agent_id),
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
                                    message: Some(format!("{} {}", agent_id, if agent_result.is_ok() { "hoàn tất" } else { "lỗi" })),
                                    updated_at: chrono::Utc::now(),
                                });
                            }

                            match agent_result {
                                Ok(out) => (agent_id.clone(), task_id_opt, out.committed, format!("{}\n\n", out.content), out.tokens_used.unwrap_or(0), out.metadata),
                                Err(e) => {
                                    error!(error = %e, agent = %agent_id, "Agent execution failed");
                                    (agent_id.clone(), task_id_opt, false, format!("⚠️ Lỗi khi gọi agent {}: {}\n\n", agent_id, e), 0, None)
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

                    let mut combined_content = format!("Kết quả thực thi từ Agent (Chưa hoàn tất, hãy phân tích và đưa ra quyết định tiếp theo hoặc trả lời trực tiếp):\n{}", turn_content);
                    if !turn_metadata.is_empty() {
                        let meta_str =
                            serde_json::to_string_pretty(&turn_metadata).unwrap_or_default();
                        combined_content.push_str(&format!("\nMetadata từ Agent:\n{}", meta_str));
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
                final_content = "Tôi không chắc chắn phải làm gì.".to_string();

                break;
            }
        }

        // ── 5. Rule Engine Validation ─────────────────────────────────────────

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

        // ── 6. Session update ─────────────────────────────────────────────────

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

        // ── 6.5 Generate Topic ID for New Sessions ────────────────────────────

        if is_first_turn {
            let prompt = format!(

                "Tạo một cụm từ ngắn (2-4 từ) đại diện cho chủ đề của yêu cầu sau: '{}'. Chỉ trả về chủ đề, không giải thích.",

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

        // ── 7. Build response ─────────────────────────────────────────────────

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

    // ─────────────────────────────────────────────────────────────────────────

    // Session summarisation (anti-drift / context-window management)

    // ─────────────────────────────────────────────────────────────────────────

    // ─────────────────────────────────────────────────────────────────────────

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

        let system_prompt = "Bạn là AI tự động tạo tài liệu Handoff (chuyển giao). \
                             Context đã đạt ngưỡng giới hạn bộ nhớ. Hãy tóm tắt lại tác vụ đang thực hiện một cách cực kỳ chi tiết, \
                             bao gồm:\n\
                             1. Mục tiêu công việc cốt lõi\n\
                             2. Những việc đã làm xong (bao gồm file nào đã sửa, logic gì đã thêm)\n\
                             3. Những việc còn dang dở\n\
                             4. Bối cảnh quan trọng cần ghi nhớ\n\
                             5. Bước tiếp theo CẦN LÀM NGAY (Next Steps).\n\n\
                             Viết dưới định dạng Markdown chuyên nghiệp. Output duy nhất của bạn là file Handoff này.";

        let mut conversation = String::new();
        for msg in messages {
            conversation.push_str(&format!("[{}]: {}\n", msg.role, msg.content));
        }

        let llm = self.llm_gateway.read().await;
        let req = crate::llm_gateway::LlmRequest::new(vec![
            crate::llm_gateway::LlmMessage::system(system_prompt),
            crate::llm_gateway::LlmMessage::user(format!(
                "Lịch sử hội thoại trước khi bị reset:\n\n{}\n\nHãy tạo file Handoff.",
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
                "[Hệ thống tự động reset context]".to_string(),
                format!("Đã đạt giới hạn context. Hệ thống đã xoá lịch sử chat và lưu trạng thái công việc tại file Handoff: `{}`. \n\nHãy sử dụng tool `view_file` (hoặc `read_knowledge` nếu có) để đọc file này và lấy lại bối cảnh làm việc trước khi tiếp tục nhiệm vụ tiếp theo.", handoff_path.display()),
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

    // ─────────────────────────────────────────────────────────────────────────

    // System health check

    // ─────────────────────────────────────────────────────────────────────────

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

    // ─────────────────────────────────────────────────────────────────────────
    // [Phase 1] Native Tool Calling ReAct Loop
    // ─────────────────────────────────────────────────────────────────────────

    /// ReAct loop mới sử dụng Native Tool Calling qua genai crate.
    ///
    /// Kiến trúc Hybrid:
    /// - MCP Tools (search_memory, read_file...) → đăng ký trực tiếp làm genai Tool
    /// - Legacy Agents (office_master, outlook...) → `call_legacy_agent` bridge tool
    ///
    /// Nếu genai bridge không có API key hoặc lỗi, tự động fallback về `process_message` cũ.
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

        // ── Tạo bridge ────────────────────────────────────────────────────────
        let bridge = {
            let llm = self.llm_gateway.read().await;
            llm.create_genai_bridge_reasoning().await
        };

        // ── Build danh sách MCP Tools ─────────────────────────────────────────
        // Lấy top N tools phù hợp nhất (search_memory, list_policies + query tools)
        let mut mcp_tools: Vec<McpTool> = Vec::new();

        // 1. Luôn bao gồm core tools
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

        // 2. Thêm `search_available_tools` synthetic tool nếu chưa có
        let has_search_tools = mcp_tools.iter().any(|t| t.name == "search_available_tools");
        if !has_search_tools {
            mcp_tools.push(McpTool {
                name: "search_available_tools".to_string(),
                description: "Tìm kiếm và nạp thêm công cụ (MCP Tool / Agent) phù hợp với tác vụ. GỌI tool này khi không có tool phù hợp.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Từ khóa tìm kiếm tool" },
                        "limit": { "type": "integer", "description": "Số tool tối đa trả về (mặc định 3)" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            });
        }

        // 3. Đăng ký toàn bộ Native Schemas từ các Agent (bao gồm auto-generated placeholders)
        mcp_tools.extend(self.agent_registry.all_tool_schemas_complete());

        // ── Build conversation context ─────────────────────────────────────────
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
                 Bạn đang làm việc trong workspace: {wid}\n\
                 Thư mục gốc của workspace: {path}\n\
                 - Thư mục Input (chứa tài liệu đọc): {path}\\input\n\
                 - Thư mục Output (nơi lưu kết quả MỚI): {path}\\output\n\
                 Quy tắc bắt buộc:\n\
                 1. Mọi file kết quả bạn tạo ra (VD: word, excel) PHẢI được lưu vào thư mục Output.\n\
                 2. Nếu user nhắc đến @folder (ví dụ: @input, @output, @knowledge, @policies), hãy ngầm hiểu đó là đường dẫn tuyệt đối tới thư mục đó.\n",
                wid = workspace_id.unwrap_or("default"),
                path = workspace_path
            )
        } else {
            String::new()
        };

        let context_hint = context_file
            .map(|p| format!("\n[Ngữ cảnh file đang mở: {}]\n", p))
            .unwrap_or_default();

        let system_prompt = format!(
            "Bạn là Office Hub Orchestrator – trợ lý AI đa năng.\n\
             Bạn có quyền gọi các Tools được cung cấp để hoàn thành nhiệm vụ.\n\
             \n\
             [TRÍ NHỚ DÀI HẠN]\n\
             Nếu câu hỏi đề cập đến sự kiện/quyết định trong quá khứ, GỌI `search_memory` trước.\n\
             \n\
             [POLICY]\n\
             Trước khi tạo file hoặc thực hiện tác vụ quan trọng, kiểm tra `list_policies`.\n\
             \n\
             [GỬI FILE CHO USER]\n\
             Để hiển thị file đính kèm trên giao diện chat (cho user tải về), bạn BẮT BUỘC thực hiện:\n\
             1. Dùng tool `fs_move_file` để copy/move file mới tạo vào thư mục: `C:\\Users\\admin\\AppData\\Local\\Temp\\office_hub_exports` (hoặc %TEMP%\\office_hub_exports).\n\
             2. Trong câu trả lời, trả về markdown link định dạng: `[Tên_File.docx](/api/v1/files/download/Tên_File.docx)`\n\
             TUYỆT ĐỐI KHÔNG báo 'đã đính kèm file' nếu chưa làm 2 bước này.\n\
             \n\
             [TOOL DISCOVERY]\n\
             Tat ca tools da san sang — goi thang theo ten tool.\n\
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

        // ── ReAct Loop (max 5 turns) ──────────────────────────────────────────
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
                    // Fallback về legacy pipeline
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
                    // LLM trả lời trực tiếp → kết thúc loop
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
                        // Emit thought để UI biết đang xử lý
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(format!(
                                "⚙️ Đang {}...",
                                crate::mcp::get_tool_alias(&call.tool_name)
                            ));
                        }

                        let result_content = match call.tool_name.as_str() {
                            // ── Synthetic: search_available_tools ──────────────
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
                                            "Không tìm thấy tool nào phù hợp.".to_string()
                                        } else {
                                            format!(
                                                "Đã nạp {} tool(s): {}",
                                                found_tools.len(),
                                                found_tools
                                                    .iter()
                                                    .map(|t| t.name.as_str())
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            )
                                        }
                                    }
                                    Err(e) => format!("Lỗi tìm kiếm tool: {e}"),
                                }
                            }

                            // ── Tool Execution (Agent or MCP Server) ─────────
                            tool_name => {
                                let start_time = std::time::Instant::now();

                                // 1. Thử tìm trong AgentRegistry trước
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
                                            Err(e) => format!("⚠️ Agent '{}' lỗi: {}", agent_id, e),
                                        }
                                    } else {
                                        format!("⚠️ Agent '{}' không tồn tại.", agent_id)
                                    }
                                } else {
                                    // 2. Không tìm thấy Agent hỗ trợ -> fallback cho MCP Broker
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
                                                        "⚠️ Tool '{}' lỗi:\n{}",
                                                        tool_name,
                                                        text_buf.trim()
                                                    )
                                                } else {
                                                    text_buf.trim().to_string()
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            format!("⚠️ Lỗi khi gọi tool '{}': {}", tool_name, e)
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

                    // Đưa kết quả tool vào conversation để LLM tổng hợp
                    conv_messages.push(ToolChatMessage::ToolResults(tool_results));

                    if turn_idx == max_turns - 1 {
                        // Hết số lượt - yêu cầu LLM tổng hợp cuối cùng
                        conv_messages.push(ToolChatMessage::User(
                            "Dựa trên kết quả các tool vừa chạy, hãy đưa ra câu trả lời cuối cùng cho người dùng.".to_string()
                        ));
                    }
                }
            }
        }

        // ── Save to session ────────────────────────────────────────────────────
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
                "Tôi đã hoàn thành các tác vụ được yêu cầu.".to_string()
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

// ─────────────────────────────────────────────────────────────────────────────

// OrchestratorResponse – what the Tauri command layer receives

// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────

// AgentTask – input contract for every agent

// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────

// AgentOutput – return contract from every agent

// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────

// RouteDecision – output of the Router

// ─────────────────────────────────────────────────────────────────────────────

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

    /// Routing confidence (0.0–1.0). Low confidence may trigger a clarification step.
    pub confidence: f32,
}

// ─────────────────────────────────────────────────────────────────────────────

// Human-in-the-Loop (HITL) Manager

// ─────────────────────────────────────────────────────────────────────────────

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

    /// Wrapped in `Mutex<Option<…>>` so we can take it exactly once.
    pub sender: std::sync::Mutex<Option<oneshot::Sender<bool>>>,
}

/// Builder for `HitlRequest` – avoids leaking the internal sender.

pub struct HitlRequestBuilder {
    pub description: String,

    pub risk_level: HitlRiskLevel,

    pub payload: Option<serde_json::Value>,
}

/// Risk classification for HITL actions (mirrors the YAML rule levels).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]

pub enum HitlRiskLevel {
    /// Read-only operations – no approval needed.
    Low,

    /// Reversible write operations – soft confirmation.
    Medium,

    /// Irreversible or externally visible actions – explicit approval.
    High,

    /// Financial, authentication, or deletion operations – double confirmation.
    Critical,
}

// ─────────────────────────────────────────────────────────────────────────────

// Orchestrator Runtime Metrics

// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────

// Tests

// ─────────────────────────────────────────────────────────────────────────────

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
