// ============================================================================
// Office Hub – agents/mod.rs
//
// Sub-agent module root.
//
// Declares all agent sub-modules and exposes the shared types that every agent
// implementation uses:
//
//   • `AgentId`         – newtype string identifier
//   • `AgentStatus`     – operational state
//   • `AgentStatusInfo` – lightweight DTO for the dashboard
//   • `AgentRegistry`   – thread-safe map of all registered agents
//
// Each concrete agent lives in its own sub-module:
//
//   agents/
//     analyst/        – Excel COM Automation (XLOOKUP, Power Query, VBA)
//     office_master/  – Word & PowerPoint COM Automation
//     web_researcher/ – Headless browser (Obscura/V8) — no visible window needed
//     converter/      – MCP Skill-learning & server packaging
// ============================================================================

pub mod analyst;
pub mod converter;
pub mod folder_scanner;
pub mod office_master;
pub mod outlook;
pub mod web_researcher;
pub mod system;
pub mod win32_admin;
pub mod com_utils;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::orchestrator::{AgentOutput, AgentTask};

// ─────────────────────────────────────────────────────────────────────────────
// AgentId
// ─────────────────────────────────────────────────────────────────────────────

/// Unique string identifier for an agent.
///
/// Well-known values (use these constants everywhere instead of raw strings):
/// ```rust,ignore
/// AgentId::ANALYST          = "analyst"
/// AgentId::OFFICE_MASTER    = "office_master"
/// AgentId::WEB_RESEARCHER   = "web_researcher"
/// AgentId::CONVERTER        = "converter"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub const ANALYST: &'static str = "analyst";
    pub const OFFICE_MASTER: &'static str = "office_master";
    pub const WEB_RESEARCHER: &'static str = "web_researcher";
    pub const CONVERTER: &'static str = "converter";
    pub const FOLDER_SCANNER: &'static str = "folder_scanner";
    pub const OUTLOOK: &'static str = "outlook";
    pub const SYSTEM: &'static str = "system";
    pub const WIN32_ADMIN: &'static str = "win32_admin";

    pub fn analyst() -> Self {
        Self(Self::ANALYST.to_string())
    }
    pub fn office_master() -> Self {
        Self(Self::OFFICE_MASTER.to_string())
    }
    pub fn web_researcher() -> Self {
        Self(Self::WEB_RESEARCHER.to_string())
    }
    pub fn converter() -> Self {
        Self(Self::CONVERTER.to_string())
    }
    pub fn folder_scanner() -> Self {
        Self(Self::FOLDER_SCANNER.to_string())
    }
    pub fn outlook() -> Self {
        Self(Self::OUTLOOK.to_string())
    }
    pub fn system() -> Self {
        Self(Self::SYSTEM.to_string())
    }
    pub fn win32_admin() -> Self {
        Self(Self::WIN32_ADMIN.to_string())
    }

    /// Create a custom agent ID (e.g. for dynamically-loaded MCP agents).
    pub fn custom(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AgentStatus
// ─────────────────────────────────────────────────────────────────────────────

/// Operational state of a sub-agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Ready to accept new tasks.
    #[default]
    Idle,
    /// Currently processing a task.
    Busy,
    /// Encountered a recoverable error; will retry or await recovery.
    Error,
    /// Administratively disabled (e.g. Office not installed).
    Disabled,
    /// Performing initialisation (startup probe, COM registration check, …).
    Initialising,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AgentStatus::Idle => "idle",
            AgentStatus::Busy => "busy",
            AgentStatus::Error => "error",
            AgentStatus::Disabled => "disabled",
            AgentStatus::Initialising => "initialising",
        };
        write!(f, "{s}")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AgentStatusInfo – lightweight DTO for the dashboard / IPC layer
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub last_used: Option<DateTime<Utc>>,
    pub total_tasks: u64,
    pub error_count: u32,
    pub avg_duration_ms: f64,
    pub capabilities: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait
// ─────────────────────────────────────────────────────────────────────────────

/// Every sub-agent must implement this trait.
///
/// The trait is object-safe (via `async_trait`) so agents can be stored as
/// `Box<dyn Agent>` in the registry.
#[async_trait]
pub trait Agent: Send + Sync {
    // ── Identity ─────────────────────────────────────────────────────────────

    /// Unique identifier string (e.g. `"analyst"`).
    fn id(&self) -> &AgentId;

    /// Human-readable display name (e.g. `"Analyst Agent (Excel)"`).
    fn name(&self) -> &str;

    /// Short description shown in the MCP / plugin manager UI.
    fn description(&self) -> &str {
        ""
    }

    /// Version string following SemVer convention.
    fn version(&self) -> &str {
        "0.1.0"
    }

    /// List of action identifiers this agent supports (e.g. `["analyze_workbook"]`).
    fn supported_actions(&self) -> Vec<String>;

    /// Return the JSON schemas for the tools provided by this agent.
    /// By default returns an empty list (for backward compatibility).
    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![]
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Perform any one-time initialisation (COM registration check, file I/O, …).
    /// Called once by `AgentRegistry::init_all()` at application startup.
    async fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Release resources (COM objects, file handles, …) on shutdown.
    async fn shutdown(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    // ── Execution ─────────────────────────────────────────────────────────────

    /// Execute a task dispatched by the Orchestrator.
    ///
    /// Implementations must:
    /// 1. Use only actions listed in `supported_actions()`.
    /// 2. Perform Hard-Truth Verification for any numeric writes to Office.
    /// 3. Return structured `AgentOutput` even on partial success.
    /// 4. Never panic – return `Err` instead.
    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput>;

    // ── Status ────────────────────────────────────────────────────────────────

    /// Return the agent's current operational status.
    fn status(&self) -> AgentStatus;

    /// Quick health-check: returns `true` if the agent can accept tasks right now.
    /// Default implementation checks `status() == Idle`.
    async fn health_check(&self) -> bool {
        self.status() == AgentStatus::Idle
    }

    // ── Introspection ─────────────────────────────────────────────────────────

    /// Return a lightweight status snapshot for the dashboard.
    fn status_info(&self) -> AgentStatusInfo {
        AgentStatusInfo {
            id: self.id().to_string(),
            name: self.name().to_string(),
            status: self.status().to_string(),
            last_used: None,
            total_tasks: 0,
            error_count: 0,
            avg_duration_ms: 0.0,
            capabilities: self.supported_actions(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AgentRegistry
// ─────────────────────────────────────────────────────────────────────────────

/// Thread-safe registry that holds all registered `Agent` instances.
///
/// The registry owns the agents behind `Arc<RwLock<Box<dyn Agent>>>` so that:
/// - Multiple readers (status checks, IPC queries) never block each other.
/// - A single writer (task execution) can mutate agent state exclusively.
#[derive(Clone)]
pub struct AgentRegistry {
    inner: HashMap<AgentId, Arc<RwLock<Box<dyn Agent>>>>,
}

impl AgentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    // ── Registration ──────────────────────────────────────────────────────────

    /// Register an agent. If an agent with the same ID already exists it is
    /// replaced and a warning is emitted.
    pub fn register(&mut self, agent: Box<dyn Agent>) {
        let id = agent.id().clone();
        if self.inner.contains_key(&id) {
            warn!(agent_id = %id, "Replacing existing agent in registry");
        } else {
            info!(agent_id = %id, name = agent.name(), "Agent registered");
        }
        self.inner.insert(id, Arc::new(RwLock::new(agent)));
    }

    /// Remove an agent from the registry. Returns `true` if the agent existed.
    pub fn unregister(&mut self, id: &AgentId) -> bool {
        let removed = self.inner.remove(id).is_some();
        if removed {
            info!(agent_id = %id, "Agent unregistered");
        }
        removed
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    /// Check whether an agent is registered.
    pub fn contains(&self, id: &AgentId) -> bool {
        self.inner.contains_key(id)
    }

    /// Return a cloned `Arc` handle to an agent (for concurrent read access).
    pub fn get(&self, id: &AgentId) -> Option<Arc<RwLock<Box<dyn Agent>>>> {
        self.inner.get(id).cloned()
    }

    /// Get a mutable reference to an agent for execution.
    /// Note: Returns the Arc<RwLock<...>> - caller must acquire write lock.
    pub fn get_mut(&self, id: &AgentId) -> Option<Arc<RwLock<Box<dyn Agent>>>> {
        self.inner.get(id).cloned()
    }

    /// Return all registered agent IDs.
    pub fn ids(&self) -> Vec<AgentId> {
        self.inner.keys().cloned().collect()
    }

    /// Return a snapshot of every agent's status (does **not** acquire write locks).
    pub fn all_statuses(&self) -> Vec<AgentStatusInfo> {
        // We need read access to each agent; collect synchronously where possible.
        // For async contexts prefer `all_statuses_async()`.
        self.inner
            .values()
            .filter_map(|arc| arc.try_read().ok().map(|g| g.status_info()))
            .collect()
    }

    /// Return all tool schemas from registered agents.
    pub fn all_tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        self.inner
            .values()
            .filter_map(|arc| arc.try_read().ok().map(|g| g.tool_schemas()))
            .flatten()
            .collect()
    }

    /// Return ALL tool schemas, auto-generating placeholder entries for any
    /// action in `supported_actions()` that is missing from `tool_schemas()`.
    ///
    /// This guarantees 100% action coverage in the tool catalog injected into
    /// the system prompt, so the LLM can always discover and call any action.
    pub fn all_tool_schemas_complete(&self) -> Vec<crate::mcp::McpTool> {
        use std::collections::HashSet;
        let mut result: Vec<crate::mcp::McpTool> = Vec::new();

        for arc in self.inner.values() {
            if let Ok(guard) = arc.try_read() {
                let registered = guard.tool_schemas();
                // Collect owned names BEFORE consuming registered with into_iter()
                let registered_names: HashSet<String> =
                    registered.iter().map(|t| t.name.clone()).collect();

                // Include explicitly declared schemas first (full metadata + tags)
                result.extend(registered.into_iter());

                // Auto-generate placeholder for any action without a schema
                let agent_name = guard.name().to_string();
                let agent_desc = guard.description().to_string();
                for action in guard.supported_actions() {
                    if !registered_names.contains(action.as_str()) {
                        // Derive human-readable description from action name
                        let readable = action.replace('_', " ");
                        result.push(crate::mcp::McpTool {
                            name: action.clone(),
                            description: format!(
                                "[{}] {} – {}",
                                agent_name,
                                readable,
                                agent_desc
                            ),
                            input_schema: serde_json::json!({
                                "type": "object",
                                "properties": {
                                    "file_path": {
                                        "type": "string",
                                        "description": "Đường dẫn tệp (nếu cần)"
                                    }
                                }
                            }),
                            tags: vec![],
                        });
                    }
                }
            }
        }
        result
    }

    /// Find an agent ID that supports a given action name.
    pub fn find_agent_by_action(&self, action: &str) -> Option<AgentId> {
        let action_string = action.to_string();
        for (id, arc) in self.inner.iter() {
            if let Ok(guard) = arc.try_read() {
                if guard.supported_actions().contains(&action_string) {
                    return Some(id.clone());
                }
            }
        }
        None
    }

    /// Async version of `all_statuses()` – waits for read locks.
    pub async fn all_statuses_async(&self) -> Vec<AgentStatusInfo> {
        let mut result = Vec::with_capacity(self.inner.len());
        for arc in self.inner.values() {
            let guard = arc.read().await;
            result.push(guard.status_info());
        }
        result
    }

    /// Number of agents currently registered.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    // ── Lifecycle helpers ─────────────────────────────────────────────────────

    /// Call `init()` on every registered agent sequentially.
    /// Logs but does not abort on individual failures so that a broken Office
    /// installation does not prevent the rest of the system from starting.
    pub async fn init_all(&self) {
        for (id, arc) in &self.inner {
            let mut guard = arc.write().await;
            match guard.init().await {
                Ok(()) => {
                    debug!(agent_id = %id, "Agent initialised successfully");
                }
                Err(e) => {
                    warn!(
                        agent_id = %id,
                        error    = %e,
                        "Agent initialisation failed – agent will remain in Disabled state"
                    );
                }
            }
        }
    }

    /// Call `shutdown()` on every registered agent sequentially.
    pub async fn shutdown_all(&self) {
        for (id, arc) in &self.inner {
            let mut guard = arc.write().await;
            if let Err(e) = guard.shutdown().await {
                warn!(agent_id = %id, error = %e, "Agent shutdown error (ignored)");
            } else {
                debug!(agent_id = %id, "Agent shut down cleanly");
            }
        }
    }

    // ── Task dispatch ─────────────────────────────────────────────────────────

    /// Acquire a write lock on the agent identified by `id` and call
    /// `execute(task)`.  Returns an error if the agent is not found.
    pub async fn dispatch(&self, id: &AgentId, task: AgentTask) -> anyhow::Result<AgentOutput> {
        let arc = self
            .inner
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found in registry", id))?;

        let mut guard = arc.write().await;
        guard.execute(task).await
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent capability declaration macro
// ─────────────────────────────────────────────────────────────────────────────

/// Convenience macro to declare the list of supported actions for an agent.
///
/// Usage:
/// ```rust,ignore
/// impl Agent for AnalystAgent {
///     fn supported_actions(&self) -> Vec<String> {
///         agent_actions![
///             "analyze_workbook",
///             "read_cell_range",
///             "write_cell_range",
///             "generate_formula",
///             "run_power_query",
///             "generate_vba",
///             "audit_formulas"
///         ]
///     }
/// }
/// ```
#[macro_export]
macro_rules! agent_actions {
    ($($action:expr),* $(,)?) => {
        vec![$($action.to_string()),*]
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::{AgentOutput, AgentTask};

    // ── Minimal stub agent ────────────────────────────────────────────────────

    struct StubAgent {
        id: AgentId,
        name: String,
        status: AgentStatus,
    }

    impl StubAgent {
        fn new(id: &str) -> Self {
            Self {
                id: AgentId::from(id),
                name: format!("Stub({})", id),
                status: AgentStatus::Idle,
            }
        }
    }

    #[async_trait]
    impl Agent for StubAgent {
        fn id(&self) -> &AgentId {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn supported_actions(&self) -> Vec<String> {
            agent_actions!["stub_action"]
        }

        fn status(&self) -> AgentStatus {
            self.status.clone()
        }

        async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
            Ok(AgentOutput {
                content: format!("stub response for task {}", task.task_id),
                committed: false,
                tokens_used: Some(10),
                metadata: None,
            })
        }
    }

    // ── AgentId tests ─────────────────────────────────────────────────────────

    #[test]
    fn agent_id_display() {
        let id = AgentId::analyst();
        assert_eq!(id.to_string(), "analyst");
    }

    #[test]
    fn agent_id_from_str() {
        let id: AgentId = "custom".into();
        assert_eq!(id.0, "custom");
    }

    #[test]
    fn agent_id_equality() {
        assert_eq!(AgentId::analyst(), AgentId::from(AgentId::ANALYST));
        assert_ne!(AgentId::analyst(), AgentId::office_master());
    }

    // ── AgentStatus tests ─────────────────────────────────────────────────────

    #[test]
    fn agent_status_display() {
        assert_eq!(AgentStatus::Idle.to_string(), "idle");
        assert_eq!(AgentStatus::Busy.to_string(), "busy");
        assert_eq!(AgentStatus::Error.to_string(), "error");
        assert_eq!(AgentStatus::Disabled.to_string(), "disabled");
        assert_eq!(AgentStatus::Initialising.to_string(), "initialising");
    }

    #[test]
    fn agent_status_default_is_idle() {
        assert_eq!(AgentStatus::default(), AgentStatus::Idle);
    }

    // ── AgentRegistry tests ───────────────────────────────────────────────────

    #[test]
    fn registry_register_and_contains() {
        let mut reg = AgentRegistry::new();
        assert!(reg.is_empty());
        reg.register(Box::new(StubAgent::new("analyst")));
        assert!(reg.contains(&AgentId::analyst()));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_unregister() {
        let mut reg = AgentRegistry::new();
        reg.register(Box::new(StubAgent::new("analyst")));
        assert!(reg.unregister(&AgentId::analyst()));
        assert!(!reg.contains(&AgentId::analyst()));
        assert!(!reg.unregister(&AgentId::analyst())); // second unregister → false
    }

    #[test]
    fn registry_ids() {
        let mut reg = AgentRegistry::new();
        reg.register(Box::new(StubAgent::new("analyst")));
        reg.register(Box::new(StubAgent::new("office_master")));
        let ids = reg.ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&AgentId::analyst()));
        assert!(ids.contains(&AgentId::office_master()));
    }

    #[tokio::test]
    async fn registry_dispatch_succeeds() {
        let mut reg = AgentRegistry::new();
        reg.register(Box::new(StubAgent::new("analyst")));

        let task = AgentTask {
            task_id: "t-001".to_string(),
            action: "stub_action".to_string(),
            intent: crate::orchestrator::intent::Intent::GeneralChat(Default::default()),
            message: "test".to_string(),
            context_file: None,
            session_id: "s-001".to_string(),
            parameters: Default::default(),
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        };

        let output = reg.dispatch(&AgentId::analyst(), task).await.unwrap();
        assert!(output.content.contains("t-001"));
    }

    #[tokio::test]
    async fn registry_dispatch_unknown_agent_errors() {
        let reg = AgentRegistry::new();
        let task = AgentTask {
            task_id: "t-002".to_string(),
            action: "noop".to_string(),
            intent: crate::orchestrator::intent::Intent::GeneralChat(Default::default()),
            message: "test".to_string(),
            context_file: None,
            session_id: "s-001".to_string(),
            parameters: Default::default(),
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        };

        let result = reg.dispatch(&AgentId::analyst(), task).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn registry_all_statuses_async() {
        let mut reg = AgentRegistry::new();
        reg.register(Box::new(StubAgent::new("analyst")));
        reg.register(Box::new(StubAgent::new("web_researcher")));

        let statuses = reg.all_statuses_async().await;
        assert_eq!(statuses.len(), 2);
        assert!(statuses.iter().all(|s| s.status == "idle"));
    }

    #[tokio::test]
    async fn registry_init_all_no_panic() {
        let mut reg = AgentRegistry::new();
        reg.register(Box::new(StubAgent::new("analyst")));
        reg.init_all().await; // must not panic even with stub agents
    }

    // ── agent_actions! macro ──────────────────────────────────────────────────

    #[test]
    fn agent_actions_macro() {
        let actions = agent_actions!["action_a", "action_b", "action_c"];
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0], "action_a");
        assert_eq!(actions[2], "action_c");
    }

    #[test]
    fn agent_actions_macro_trailing_comma() {
        let actions = agent_actions!["a", "b",];
        assert_eq!(actions.len(), 2);
    }
}
