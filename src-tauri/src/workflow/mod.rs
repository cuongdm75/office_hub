// ============================================================================
// Office Hub – workflow/mod.rs
//
// Event-Driven Workflow Engine
//
// Trách nhiệm:
//   1. Tải và parse YAML workflow definitions từ thư mục `workflows/`
//   2. Quản lý Trigger listeners (Email, FileWatcher, Schedule, Voice, Manual)
//   3. Thực thi workflow steps tuần tự hoặc song song
//   4. Theo dõi trạng thái các workflow runs (pending → running → success/failed)
//   5. Ghi audit trail cho mọi step
//   6. Giao tiếp với Orchestrator để dispatch từng action đến đúng Agent
//
// Cấu trúc module:
//   workflow/
//     mod.rs          ← file này (public API + engine)
//     triggers/       ← các trigger implementations
//       mod.rs
//       email.rs
//       file_watcher.rs
//       schedule.rs
//       voice.rs
//     actions/        ← các action implementations
//       mod.rs
//       office.rs
//       web.rs
//       notification.rs
// ============================================================================

pub mod actions;
pub mod triggers;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Workflow '{id}' not found")]
    NotFound { id: String },

    #[error("Failed to read workflow file '{path}': {source}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse workflow YAML '{path}': {source}")]
    ParseError {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("Step '{step_id}' failed in workflow '{workflow_id}': {message}")]
    StepFailed {
        workflow_id: String,
        step_id: String,
        message: String,
    },

    #[error("Workflow '{id}' timed out after {timeout_seconds}s")]
    Timeout { id: String, timeout_seconds: u64 },

    #[error("Trigger error: {0}")]
    TriggerError(String),

    #[error("Action error: {0}")]
    ActionError(String),

    #[error("Workflow '{id}' was aborted: {reason}")]
    Aborted { id: String, reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error in workflow '{path}': {message}")]
    ValidationError { path: PathBuf, message: String },
}

pub type WorkflowResult<T> = Result<T, WorkflowError>;

// ─────────────────────────────────────────────────────────────────────────────
// YAML Schema – Workflow Definition
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level workflow definition deserialized from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Unique workflow identifier (snake_case, e.g. "email-to-report")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Semantic version
    #[serde(default = "default_version")]
    pub version: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Author / team
    #[serde(default)]
    pub author: Option<String>,

    /// Descriptive tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Workflow-level metadata and settings
    #[serde(default)]
    pub metadata: WorkflowMeta,

    /// The trigger that fires this workflow
    pub trigger: TriggerDefinition,

    /// Context variables available throughout the workflow
    #[serde(default)]
    pub context: WorkflowContext,

    /// Ordered list of steps to execute
    pub steps: Vec<StepDefinition>,

    /// Global error handlers
    #[serde(default)]
    pub error_handlers: Vec<ErrorHandler>,

    /// Logging / audit settings
    #[serde(default)]
    pub logging: LoggingConfig,

    /// Notification channel settings
    #[serde(default)]
    pub notifications: NotificationConfig,

    /// Test / dry-run settings
    #[serde(default)]
    pub test: Option<TestConfig>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Workflow-level metadata and execution settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowMeta {
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: WorkflowPriority,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub retry_on_failure: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub human_approval_required: bool,
}

fn default_priority() -> WorkflowPriority {
    WorkflowPriority::Normal
}
fn default_timeout() -> u64 {
    300
}
fn default_max_retries() -> u32 {
    2
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// Workflow-level context variables (key → template string).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowContext {
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger Definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDefinition {
    /// Trigger type identifier
    #[serde(rename = "type")]
    pub trigger_type: TriggerType,

    /// Type-specific configuration
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Fires when an email matching the filter arrives in Outlook
    EmailReceived,
    /// Fires when a watched file or directory changes
    FileChanged,
    /// Fires on a cron-like schedule
    Schedule,
    /// Fires from a voice command via the WebSocket Mobile connection
    VoiceCommand,
    /// Fires only via explicit API/UI invocation
    Manual,
}

impl std::fmt::Display for TriggerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TriggerType::EmailReceived => write!(f, "email_received"),
            TriggerType::FileChanged => write!(f, "file_changed"),
            TriggerType::Schedule => write!(f, "schedule"),
            TriggerType::VoiceCommand => write!(f, "voice_command"),
            TriggerType::Manual => write!(f, "manual"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Step Definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDefinition {
    /// Unique step identifier within this workflow
    pub id: String,

    /// Human-readable step name
    pub name: String,

    /// Which agent handles this step
    pub agent: StepAgent,

    /// The action to invoke on the agent
    pub action: String,

    /// Optional JSONPath condition – step is skipped if evaluates to false
    #[serde(default)]
    pub condition: Option<String>,

    /// Step-specific configuration
    #[serde(default)]
    pub config: serde_json::Value,

    /// Input mappings (key → template expression referencing previous steps)
    #[serde(default)]
    pub input: HashMap<String, String>,

    /// Output variable bindings (variable_name → template expression)
    #[serde(default)]
    pub output: HashMap<String, String>,

    /// Per-step timeout override (seconds)
    #[serde(default)]
    pub timeout_seconds: Option<u64>,

    /// Step-level error handling
    #[serde(default)]
    pub on_error: Option<StepErrorConfig>,

    /// Whether this step always runs regardless of prior failures
    #[serde(default)]
    pub run: Option<StepRunPolicy>,

    /// Next step to jump to (overrides sequential default)
    #[serde(default)]
    pub next_step: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAgent {
    Orchestrator,
    Analyst,
    OfficeMaster,
    WebResearcher,
    Converter,
    /// A specific MCP server by ID
    Mcp(String),
}

impl std::fmt::Display for StepAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepAgent::Orchestrator => write!(f, "orchestrator"),
            StepAgent::Analyst => write!(f, "analyst"),
            StepAgent::OfficeMaster => write!(f, "office_master"),
            StepAgent::WebResearcher => write!(f, "web_researcher"),
            StepAgent::Converter => write!(f, "converter"),
            StepAgent::Mcp(id) => write!(f, "mcp:{id}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepErrorConfig {
    /// What to do on failure: "abort" | "notify_and_pause" | "notify_and_abort" | "skip" | "retry"
    pub action: String,
    /// Optional message to include in the notification
    #[serde(default)]
    pub message: Option<String>,
    /// Whether to allow the user to manually override and continue
    #[serde(default)]
    pub allow_manual_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepRunPolicy {
    /// Only run if no previous failure (default)
    OnSuccess,
    /// Always run, even if previous steps failed (e.g. cleanup)
    Always,
    /// Only run if a previous step failed
    OnFailure,
}

// ─────────────────────────────────────────────────────────────────────────────
// Error Handler (workflow-level)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandler {
    /// Error type to match: "timeout" | "com_automation_error" | "generic" | …
    #[serde(rename = "type")]
    pub error_type: String,
    /// Action: "notify_and_abort" | "retry_with_hard_truth" | "notify_user" | …
    pub action: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub allow_retry: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Logging / Audit Config
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_true")]
    pub include_step_timing: bool,
    #[serde(default)]
    pub include_agent_responses: bool,
    #[serde(default)]
    pub audit_trail: Option<AuditTrailConfig>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            include_step_timing: true,
            include_agent_responses: false,
            audit_trail: None,
        }
    }
}

fn default_log_level() -> String {
    "info".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrailConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub log_file: Option<String>,
    #[serde(default)]
    pub include_fields: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Notification Config
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    #[serde(default)]
    pub channels: Vec<NotificationChannelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannelConfig {
    #[serde(rename = "type")]
    pub channel_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority_filter: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Test / Dry-run Config
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub mock_email: Option<serde_json::Value>,
    #[serde(default)]
    pub mock_approval: Option<String>,
    #[serde(default)]
    pub skip_steps: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow Run – runtime state of a single execution
// ─────────────────────────────────────────────────────────────────────────────

/// Status of a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    WaitingApproval,
    Success,
    PartialSuccess,
    Failed,
    Aborted,
    TimedOut,
}

/// Type alias for RunStatus (used in commands.rs)
pub type WorkflowRunStatus = RunStatus;

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RunStatus::Pending => "pending",
            RunStatus::Running => "running",
            RunStatus::WaitingApproval => "waiting_approval",
            RunStatus::Success => "success",
            RunStatus::PartialSuccess => "partial_success",
            RunStatus::Failed => "failed",
            RunStatus::Aborted => "aborted",
            RunStatus::TimedOut => "timed_out",
        };
        write!(f, "{s}")
    }
}

/// A single step's execution result within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRunResult {
    pub step_id: String,
    pub step_name: String,
    pub agent: String,
    pub action: String,
    pub status: RunStatus,
    pub output: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub skipped: bool,
    pub skip_reason: Option<String>,
}

/// Full runtime record for one workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// Unique run ID
    pub run_id: Uuid,

    /// ID of the workflow definition that was executed
    pub workflow_id: String,

    /// Name of the workflow (denormalised for display)
    pub workflow_name: String,

    /// Current overall status
    pub status: RunStatus,

    /// Trigger that fired this run
    pub trigger_type: TriggerType,

    /// Input data provided by the trigger
    pub trigger_data: Option<serde_json::Value>,

    /// Resolved context variables for this run
    pub context: HashMap<String, serde_json::Value>,

    /// Per-step results (in execution order)
    pub step_results: Vec<StepRunResult>,

    /// Human-readable summary message
    pub message: Option<String>,

    /// Error details if the run failed
    pub error: Option<String>,

    /// Number of retry attempts used
    pub retry_count: u32,

    /// Whether this was a dry-run
    pub dry_run: bool,

    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,

    /// Wall-clock duration in milliseconds
    pub duration_ms: Option<u64>,
}

impl WorkflowRun {
    pub fn new(
        workflow: &WorkflowDefinition,
        trigger_type: TriggerType,
        trigger_data: Option<serde_json::Value>,
        dry_run: bool,
    ) -> Self {
        Self {
            run_id: Uuid::new_v4(),
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            status: RunStatus::Pending,
            trigger_type,
            trigger_data,
            context: workflow
                .context
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            step_results: Vec::new(),
            message: None,
            error: None,
            retry_count: 0,
            dry_run,
            started_at: Utc::now(),
            finished_at: None,
            duration_ms: None,
        }
    }

    pub fn finish(&mut self, status: RunStatus, message: Option<String>) {
        let now = Utc::now();
        self.status = status;
        self.message = message;
        self.finished_at = Some(now);
        self.duration_ms = Some((now - self.started_at).num_milliseconds() as u64);
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            RunStatus::Success
                | RunStatus::PartialSuccess
                | RunStatus::Failed
                | RunStatus::Aborted
                | RunStatus::TimedOut
        )
    }

    /// Retrieve the output value of a named step (for downstream steps).
    pub fn step_output(&self, step_id: &str) -> Option<&serde_json::Value> {
        self.step_results
            .iter()
            .find(|s| s.step_id == step_id)
            .and_then(|s| s.output.as_ref())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger trait
// ─────────────────────────────────────────────────────────────────────────────

/// Every trigger implementation must satisfy this trait.
#[async_trait]
pub trait Trigger: Send + Sync {
    fn trigger_type(&self) -> TriggerType;

    /// Start the trigger listener.
    /// The implementation should send `TriggerEvent` values on `tx` whenever
    /// the trigger condition is met.
    async fn start(
        &self,
        workflow_id: String,
        config: serde_json::Value,
        tx: mpsc::Sender<TriggerEvent>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> WorkflowResult<()>;

    /// Gracefully stop the trigger listener.
    async fn stop(&self) -> WorkflowResult<()>;
}

/// An event emitted by a trigger when it fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerEvent {
    /// ID of the workflow that should be fired
    pub workflow_id: String,

    /// Which trigger type fired it
    pub trigger_type: TriggerType,

    /// Data payload from the trigger (email body, file path, command text…)
    pub data: serde_json::Value,

    /// Timestamp of the event
    pub fired_at: DateTime<Utc>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Action trait
// ─────────────────────────────────────────────────────────────────────────────

/// Context passed to every action during execution.
#[derive(Debug, Clone)]
pub struct ActionContext {
    /// The current workflow run (read-only snapshot)
    pub run_id: Uuid,
    pub workflow_id: String,
    pub step_id: String,
    /// Resolved context variables for this run
    pub context: HashMap<String, serde_json::Value>,
    /// Dry-run flag – if true, actions should simulate without writing
    pub dry_run: bool,
}

/// Output from a successfully executed action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionOutput {
    /// Human-readable summary of what was done
    pub summary: String,
    /// Structured output data (available to downstream steps)
    pub data: Option<serde_json::Value>,
    /// Whether this action requires HITL approval before effects are committed
    pub requires_approval: bool,
    /// Approval request ID if `requires_approval` is true
    pub approval_id: Option<String>,
}

/// Every action implementation satisfies this trait.
#[async_trait]
pub trait Action: Send + Sync {
    fn action_id(&self) -> &str;
    fn supported_agents(&self) -> &[StepAgent];

    async fn execute(
        &self,
        step: &StepDefinition,
        ctx: &ActionContext,
        input: serde_json::Value,
    ) -> WorkflowResult<ActionOutput>;
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkflowLoader – reads and parses YAML files from disk
// ─────────────────────────────────────────────────────────────────────────────

pub struct WorkflowLoader;

impl WorkflowLoader {
    /// Load all `*.yaml` files from `dir` as `WorkflowDefinition`s.
    pub async fn load_directory(dir: &Path) -> WorkflowResult<Vec<WorkflowDefinition>> {
        let mut definitions = Vec::new();

        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| WorkflowError::FileRead {
                path: dir.to_path_buf(),
                source: e,
            })?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if ext == Some("yaml") || ext == Some("md") {
                match Self::load_file(&path).await {
                    Ok(def) => {
                        info!(
                            workflow_id = %def.id,
                            path = ?path,
                            "Workflow definition loaded"
                        );
                        definitions.push(def);
                    }
                    Err(e) => {
                        warn!(path = ?path, error = %e, "Skipping invalid workflow file");
                    }
                }
            }
        }

        Ok(definitions)
    }

    /// Load a single YAML file as a `WorkflowDefinition`.
    pub async fn load_file(path: &Path) -> WorkflowResult<WorkflowDefinition> {
        let content =
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| WorkflowError::FileRead {
                    path: path.to_path_buf(),
                    source: e,
                })?;

        let yaml_str = if path.extension().and_then(|e| e.to_str()) == Some("md") {
            Self::extract_yaml_from_markdown(&content).unwrap_or(content)
        } else {
            content
        };

        let def: WorkflowDefinition =
            serde_yaml::from_str(&yaml_str).map_err(|e| WorkflowError::ParseError {
                path: path.to_path_buf(),
                source: e,
            })?;

        // Basic validation
        Self::validate(&def)?;

        Ok(def)
    }

    /// Validate a workflow definition for structural correctness.
    fn validate(def: &WorkflowDefinition) -> WorkflowResult<()> {
        if def.id.is_empty() {
            return Err(WorkflowError::ValidationError {
                path: PathBuf::from(&def.name),
                message: "workflow 'id' must not be empty".to_string(),
            });
        }

        if def.steps.is_empty() {
            return Err(WorkflowError::ValidationError {
                path: PathBuf::from(&def.id),
                message: "workflow must have at least one step".to_string(),
            });
        }

        // Check that all `next_step` references point to real step IDs
        let step_ids: std::collections::HashSet<&str> =
            def.steps.iter().map(|s| s.id.as_str()).collect();

        for step in &def.steps {
            if let Some(next) = &step.next_step {
                if !step_ids.contains(next.as_str()) {
                    return Err(WorkflowError::ValidationError {
                        path: PathBuf::from(&def.id),
                        message: format!(
                            "step '{}' references unknown next_step '{}'",
                            step.id, next
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Extacts the first ````yaml ... ```` block from a Markdown file.
    fn extract_yaml_from_markdown(md: &str) -> Option<String> {
        let mut in_block = false;
        let mut code = String::new();

        for line in md.lines() {
            if line.starts_with("```yaml") {
                in_block = true;
                continue;
            } else if line.starts_with("```") && in_block {
                return Some(code);
            }

            if in_block {
                code.push_str(line);
                code.push('\n');
            }
        }

        if !code.is_empty() {
            Some(code)
        } else {
            None
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Template Engine – resolves {{ expression }} in step inputs / outputs
// ─────────────────────────────────────────────────────────────────────────────

pub struct TemplateEngine;

impl TemplateEngine {
    /// Resolve template expressions in `template` using `vars` (context + step outputs).
    ///
    /// Supported syntax:
    ///   `{{ context.var_name }}`
    ///   `{{ steps.step_id.some_key }}`
    ///   `{{ NOW | date('HH:mm dd/MM/yyyy') }}`
    ///   `{{ DATE:yyyy-MM-dd }}`
    pub fn resolve(
        template: &str,
        context: &HashMap<String, serde_json::Value>,
        step_outputs: &HashMap<String, serde_json::Value>,
    ) -> String {
        let re = regex::Regex::new(r"\{\{\s*(.+?)\s*\}\}").unwrap();

        re.replace_all(template, |caps: &regex::Captures| {
            let expr = caps[1].trim();
            Self::eval_expression(expr, context, step_outputs)
        })
        .to_string()
    }

    fn eval_expression(
        expr: &str,
        context: &HashMap<String, serde_json::Value>,
        step_outputs: &HashMap<String, serde_json::Value>,
    ) -> String {
        // Handle DATE:format shorthand
        if let Some(fmt) = expr.strip_prefix("DATE:") {
            return Self::format_date(fmt);
        }

        // Handle NOW
        if expr.starts_with("NOW") {
            return Utc::now().to_rfc3339();
        }

        // Handle steps.step_id.field
        if let Some(rest) = expr.strip_prefix("steps.") {
            let parts: Vec<&str> = rest.splitn(2, '.').collect();
            if parts.len() == 2 {
                let step_id = parts[0];
                let field = parts[1];
                if let Some(output) = step_outputs.get(step_id) {
                    if let Some(val) = output.get(field) {
                        return val
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| val.to_string());
                    }
                }
            }
            return format!("[UNRESOLVED: steps.{rest}]");
        }

        // Handle context.var_name
        if let Some(var_name) = expr.strip_prefix("context.") {
            if let Some(val) = context.get(var_name) {
                return val
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| val.to_string());
            }
        }

        // Direct context lookup (no prefix)
        if let Some(val) = context.get(expr) {
            return val
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| val.to_string());
        }

        // Unresolved – return original expression wrapped in markers
        format!("[UNRESOLVED: {expr}]")
    }

    fn format_date(fmt: &str) -> String {
        let now = Utc::now();
        // Simple substitutions for common format tokens
        let result = fmt
            .replace("yyyy", &format!("{:04}", now.format("%Y")))
            .replace("MM", &format!("{:02}", now.format("%m")))
            .replace("dd", &format!("{:02}", now.format("%d")))
            .replace("HH", &format!("{:02}", now.format("%H")))
            .replace("mm", &format!("{:02}", now.format("%M")))
            .replace("ss", &format!("{:02}", now.format("%S")));
        result
    }

    /// Evaluate a boolean condition expression.
    /// Currently supports simple equality and existence checks.
    ///
    /// Examples:
    ///   `{{ steps.check_audit.audit_passed == true }}`
    ///   `{{ steps.request_approval.approval_action == 'approve' }}`
    pub fn eval_condition(
        condition: &str,
        context: &HashMap<String, serde_json::Value>,
        step_outputs: &HashMap<String, serde_json::Value>,
    ) -> bool {
        let resolved = Self::resolve(condition, context, step_outputs);

        // Try to parse as a boolean
        match resolved.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => return true,
            "false" | "no" | "0" | "" => return false,
            _ => {}
        }

        // If still unresolved (contains UNRESOLVED marker), default to false
        if resolved.contains("[UNRESOLVED:") {
            warn!(condition = %condition, "Condition could not be resolved, defaulting to false");
            return false;
        }

        // Non-empty string that isn't a boolean → treat as truthy
        !resolved.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkflowEngine – the main orchestration component
// ─────────────────────────────────────────────────────────────────────────────

/// Message types sent on the internal engine channel.
#[derive(Debug)]
enum EngineMessage {
    /// Execute a workflow run.
    Execute(Box<WorkflowRun>),
    /// Shut down the engine.
    #[allow(dead_code)]
    Shutdown,
}

/// The WorkflowEngine manages workflow definitions, trigger listeners, and run history.
pub struct WorkflowEngine {
    /// Loaded workflow definitions keyed by workflow ID.
    definitions: Arc<DashMap<String, WorkflowDefinition>>,

    /// Active workflow runs keyed by run_id (Uuid as string).
    active_runs: Arc<DashMap<String, WorkflowRun>>,

    /// Completed run history keyed by workflow_id → Vec<WorkflowRun>.
    run_history: Arc<DashMap<String, Vec<WorkflowRun>>>,

    /// Channel for sending run requests to the executor task.
    exec_tx: mpsc::Sender<EngineMessage>,

    /// Channel for trigger events from all active trigger listeners.
    trigger_tx: mpsc::Sender<TriggerEvent>,

    /// Broadcast channel for run status updates (consumed by UI / WebSocket layer).
    status_tx: broadcast::Sender<WorkflowProgressUpdate>,

    /// Cancel tokens for active trigger background tasks, keyed by workflow_id.
    trigger_cancel_tokens: Arc<DashMap<String, tokio_util::sync::CancellationToken>>,

    /// Path to the workflows directory.
    workflows_dir: PathBuf,

    /// Max run history entries per workflow.
    pub max_history_per_workflow: usize,

    /// Reference to the Orchestrator for action dispatching
    pub orchestrator: Arc<RwLock<Option<crate::orchestrator::OrchestratorHandle>>>,
}

/// A lightweight status update broadcast when a run's status changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowProgressUpdate {
    Run {
        run_id: String,
        workflow_id: String,
        workflow_name: String,
        status: RunStatus,
        message: Option<String>,
        updated_at: DateTime<Utc>,
    },
    Step {
        run_id: String,
        workflow_id: String,
        step_id: String,
        step_name: String,
        status: RunStatus,
        message: Option<String>,
        updated_at: DateTime<Utc>,
    },
    Thought {
        session_id: String,
        thought: String,
    },
}

impl WorkflowEngine {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a new WorkflowEngine and load definitions from `workflows_dir`.
    pub async fn new(workflows_dir: impl Into<PathBuf>) -> WorkflowResult<Arc<Self>> {
        let workflows_dir = workflows_dir.into();

        let (exec_tx, exec_rx) = mpsc::channel::<EngineMessage>(64);
        let (trigger_tx, trigger_rx) = mpsc::channel::<TriggerEvent>(256);
        let (status_tx, _) = broadcast::channel::<WorkflowProgressUpdate>(256);

        let definitions = Arc::new(DashMap::new());
        let active_runs = Arc::new(DashMap::new());
        let run_history = Arc::new(DashMap::new());

        let orchestrator = Arc::new(RwLock::new(None));

        let engine = Arc::new(Self {
            definitions: Arc::clone(&definitions),
            active_runs: Arc::clone(&active_runs),
            run_history: Arc::clone(&run_history),
            exec_tx,
            trigger_tx,
            status_tx,
            trigger_cancel_tokens: Arc::new(DashMap::new()),
            workflows_dir: workflows_dir.clone(),
            max_history_per_workflow: 100,
            orchestrator: Arc::clone(&orchestrator),
        });

        // Load definitions from disk
        engine.reload_definitions().await?;

        // Spawn the executor task
        {
            let defs = Arc::clone(&definitions);
            let active = Arc::clone(&active_runs);
            let history = Arc::clone(&run_history);
            let status_tx = engine.status_tx.clone();
            let max_history = engine.max_history_per_workflow;
            let orchestrator_clone = Arc::clone(&engine.orchestrator);
            tokio::spawn(Self::executor_loop(
                exec_rx,
                defs,
                active,
                history,
                status_tx,
                max_history,
                orchestrator_clone,
            ));
        }

        // Spawn the trigger dispatcher task
        {
            let exec_tx = engine.exec_tx.clone();
            let defs = Arc::clone(&definitions);
            tokio::spawn(Self::trigger_dispatcher_loop(trigger_rx, exec_tx, defs));
        }

        // Start all triggers for loaded definitions
        engine.start_triggers().await?;

        info!(dir = ?workflows_dir, "WorkflowEngine started");
        Ok(engine)
    }

    /// Create a minimal WorkflowEngine with no definitions and no active triggers.
    /// Used as a fallback when full initialisation fails.
    pub fn empty() -> Self {
        let (exec_tx, _) = mpsc::channel::<EngineMessage>(64);
        let (trigger_tx, _) = mpsc::channel::<TriggerEvent>(256);
        let (status_tx, _) = broadcast::channel::<WorkflowProgressUpdate>(256);

        Self {
            definitions: Arc::new(DashMap::new()),
            active_runs: Arc::new(DashMap::new()),
            run_history: Arc::new(DashMap::new()),
            exec_tx,
            trigger_tx,
            status_tx,
            trigger_cancel_tokens: Arc::new(DashMap::new()),
            workflows_dir: PathBuf::new(),
            max_history_per_workflow: 100,
            orchestrator: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the orchestrator handle for agent dispatching
    pub async fn set_orchestrator(&self, orchestrator: crate::orchestrator::OrchestratorHandle) {
        let mut lock = self.orchestrator.write().await;
        *lock = Some(orchestrator);
    }

    // ── Definition management ────────────────────────────────────────────────

    /// Reload all workflow definitions from the workflows directory.
    pub async fn reload_definitions(&self) -> WorkflowResult<usize> {
        if !self.workflows_dir.exists() {
            warn!(dir = ?self.workflows_dir, "Workflows directory does not exist – no workflows loaded");
            return Ok(0);
        }

        let defs = WorkflowLoader::load_directory(&self.workflows_dir).await?;
        let count = defs.len();

        self.definitions.clear();
        for def in defs {
            self.definitions.insert(def.id.clone(), def);
        }

        self.start_triggers().await?;

        info!(count, "Workflow definitions reloaded");
        Ok(count)
    }

    /// Register a workflow definition at runtime (e.g. from UI).
    pub fn register_definition(&self, def: WorkflowDefinition) {
        info!(workflow_id = %def.id, "Workflow definition registered");
        self.definitions.insert(def.id.clone(), def);
    }

    /// Remove a workflow definition.
    pub fn unregister_definition(&self, workflow_id: &str) -> bool {
        self.definitions.remove(workflow_id).is_some()
    }

    /// Save a workflow definition to disk and register it.
    pub async fn save_definition(&self, def: WorkflowDefinition) -> WorkflowResult<()> {
        WorkflowLoader::validate(&def)?;

        if !self.workflows_dir.exists() {
            tokio::fs::create_dir_all(&self.workflows_dir)
                .await
                .map_err(WorkflowError::Io)?;
        }

        let file_path = self.workflows_dir.join(format!("{}.yaml", def.id));
        let yaml = serde_yaml::to_string(&def)?;
        tokio::fs::write(&file_path, yaml)
            .await
            .map_err(WorkflowError::Io)?;

        self.register_definition(def);
        Ok(())
    }

    /// Get a workflow definition by its ID.
    pub fn get_definition(&self, workflow_id: &str) -> Option<WorkflowDefinition> {
        self.definitions.get(workflow_id).map(|e| e.value().clone())
    }

    /// List all registered workflow definitions (as JSON for IPC).
    pub fn list_workflows_json(&self) -> Vec<serde_json::Value> {
        self.definitions
            .iter()
            .map(|e| {
                let d = e.value();
                serde_json::json!({
                    "id":          d.id,
                    "name":        d.name,
                    "version":     d.version,
                    "description": d.description,
                    "tags":        d.tags,
                    "triggerType": d.trigger.trigger_type,
                    "priority":    d.metadata.priority,
                    "stepCount":   d.steps.len(),
                })
            })
            .collect()
    }

    // ── Trigger API ──────────────────────────────────────────────────────────

    /// Start all active triggers based on current workflow definitions.
    pub async fn start_triggers(&self) -> WorkflowResult<()> {
        info!("Starting workflow triggers...");
        self.stop_triggers().await; // Clear any existing

        for entry in self.definitions.iter() {
            let def = entry.value();
            let trigger_impl: Box<dyn Trigger> = match def.trigger.trigger_type {
                TriggerType::FileChanged => Box::new(triggers::FileWatchTrigger),
                TriggerType::Schedule => Box::new(triggers::ScheduleTrigger),
                TriggerType::EmailReceived => Box::new(triggers::EmailTrigger),
                TriggerType::VoiceCommand => Box::new(triggers::VoiceTrigger),
                TriggerType::Manual => Box::new(triggers::ManualTrigger),
            };

            let cancel_token = tokio_util::sync::CancellationToken::new();
            self.trigger_cancel_tokens
                .insert(def.id.clone(), cancel_token.clone());

            let tx = self.trigger_tx.clone();

            // start() spawns its own background tasks if needed
            if let Err(e) = trigger_impl
                .start(def.id.clone(), def.trigger.config.clone(), tx, cancel_token)
                .await
            {
                error!(workflow_id = %def.id, error = %e, "Failed to start trigger");
            }
        }

        Ok(())
    }

    /// Stop all active triggers.
    pub async fn stop_triggers(&self) {
        info!("Stopping all workflow triggers...");
        for entry in self.trigger_cancel_tokens.iter() {
            entry.value().cancel();
        }
        self.trigger_cancel_tokens.clear();
    }

    /// Manually trigger a workflow by ID with optional input data.
    pub async fn trigger(
        &self,
        workflow_id: &str,
        input_data: Option<serde_json::Value>,
    ) -> WorkflowResult<WorkflowRun> {
        let def = self
            .definitions
            .get(workflow_id)
            .ok_or_else(|| WorkflowError::NotFound {
                id: workflow_id.to_string(),
            })?
            .clone();

        let run = WorkflowRun::new(&def, TriggerType::Manual, input_data, false);
        let run_clone = run.clone();

        info!(
            run_id = %run.run_id,
            workflow_id,
            "Workflow manually triggered"
        );

        self.exec_tx
            .send(EngineMessage::Execute(Box::new(run)))
            .await
            .map_err(|e| WorkflowError::ActionError(format!("Failed to enqueue run: {e}")))?;

        Ok(run_clone)
    }

    /// Emit a trigger event (called by trigger listener tasks).
    pub async fn emit_trigger_event(&self, event: TriggerEvent) -> WorkflowResult<()> {
        self.trigger_tx
            .send(event)
            .await
            .map_err(|e| WorkflowError::TriggerError(format!("Failed to emit trigger: {e}")))
    }

    // ── Run history ──────────────────────────────────────────────────────────

    /// Get all runs for a specific workflow (most recent first).
    pub fn get_runs(&self, workflow_id: &str) -> Vec<WorkflowRun> {
        self.run_history
            .get(workflow_id)
            .map(|r| r.value().iter().rev().cloned().collect())
            .unwrap_or_default()
    }

    /// Get a specific run by its UUID string.
    pub fn get_run(&self, run_id: &str) -> Option<WorkflowRun> {
        // Check active runs first
        if let Some(run) = self.active_runs.get(run_id) {
            return Some(run.clone());
        }
        // Then search history
        for entry in self.run_history.iter() {
            if let Some(run) = entry
                .value()
                .iter()
                .find(|r| r.run_id.to_string() == run_id)
            {
                return Some(run.clone());
            }
        }
        None
    }

    /// Subscribe to run status updates.
    pub fn subscribe_status(&self) -> broadcast::Receiver<WorkflowProgressUpdate> {
        self.status_tx.subscribe()
    }

    /// Total number of loaded workflow definitions.
    pub fn definition_count(&self) -> usize {
        self.definitions.len()
    }

    // ── Internal executor loop ───────────────────────────────────────────────

    /// Background task: processes queued workflow runs one by one.
    async fn executor_loop(
        mut rx: mpsc::Receiver<EngineMessage>,
        definitions: Arc<DashMap<String, WorkflowDefinition>>,
        active_runs: Arc<DashMap<String, WorkflowRun>>,
        run_history: Arc<DashMap<String, Vec<WorkflowRun>>>,
        status_tx: broadcast::Sender<WorkflowProgressUpdate>,
        max_history: usize,
        orchestrator: Arc<RwLock<Option<crate::orchestrator::OrchestratorHandle>>>,
    ) {
        info!("Workflow executor loop started");

        while let Some(msg) = rx.recv().await {
            match msg {
                EngineMessage::Shutdown => {
                    info!("Workflow executor loop shutting down");
                    break;
                }
                EngineMessage::Execute(mut run) => {
                    let workflow_id = run.workflow_id.clone();
                    let run_id = run.run_id.to_string();

                    // Retrieve definition
                    let def = match definitions.get(&workflow_id) {
                        Some(d) => d.clone(),
                        None => {
                            error!(workflow_id, "Workflow definition missing at execution time");
                            run.finish(RunStatus::Failed, Some("Definition not found".into()));
                            Self::archive_run(&run, &run_history, max_history);
                            continue;
                        }
                    };

                    // Mark as running
                    run.status = RunStatus::Running;
                    active_runs.insert(run_id.clone(), (*run).clone());
                    let _ = status_tx.send(WorkflowProgressUpdate::Run {
                        run_id: run_id.clone(),
                        workflow_id: workflow_id.clone(),
                        workflow_name: def.name.clone(),
                        status: RunStatus::Running,
                        message: None,
                        updated_at: Utc::now(),
                    });

                    // Execute steps
                    let timeout = std::time::Duration::from_secs(def.metadata.timeout_seconds);
                    let execution = Self::execute_workflow(
                        &mut run,
                        &def,
                        orchestrator.clone(),
                        status_tx.clone(),
                    );

                    let result = tokio::time::timeout(timeout, execution).await;

                    match result {
                        Ok(Ok(())) => {
                            let all_success = run
                                .step_results
                                .iter()
                                .all(|s| s.status == RunStatus::Success || s.skipped);
                            let final_status = if all_success {
                                RunStatus::Success
                            } else {
                                RunStatus::PartialSuccess
                            };
                            run.finish(
                                final_status,
                                Some(format!("Completed {} steps", run.step_results.len())),
                            );
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            error!(workflow_id, run_id, error = %msg, "Workflow run failed");
                            run.error = Some(msg.clone());
                            run.finish(RunStatus::Failed, Some(msg));
                        }
                        Err(_) => {
                            let msg = format!(
                                "Workflow timed out after {}s",
                                def.metadata.timeout_seconds
                            );
                            warn!(workflow_id, run_id, %msg);
                            run.finish(RunStatus::TimedOut, Some(msg));
                        }
                    }

                    // Broadcast final status
                    let _ = status_tx.send(WorkflowProgressUpdate::Run {
                        run_id: run_id.clone(),
                        workflow_id: workflow_id.clone(),
                        workflow_name: def.name.clone(),
                        status: run.status.clone(),
                        message: run.message.clone(),
                        updated_at: Utc::now(),
                    });

                    // Move from active → history
                    active_runs.remove(&run_id);
                    Self::archive_run(&run, &run_history, max_history);

                    info!(
                        run_id,
                        workflow_id,
                        status = %run.status,
                        duration_ms = ?run.duration_ms,
                        "Workflow run complete"
                    );
                }
            }
        }
    }

    /// Execute all steps of a workflow run sequentially.
    #[instrument(skip(run, def, orchestrator_ref), fields(run_id = %run.run_id, workflow_id = %run.workflow_id))]
    async fn execute_workflow(
        run: &mut WorkflowRun,
        def: &WorkflowDefinition,
        orchestrator_ref: Arc<RwLock<Option<crate::orchestrator::OrchestratorHandle>>>,
        status_tx: broadcast::Sender<WorkflowProgressUpdate>,
    ) -> WorkflowResult<()> {
        let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();
        let mut previous_failed = false;

        for step in &def.steps {
            // Determine run policy
            let run_policy = step.run.as_ref().unwrap_or(&StepRunPolicy::OnSuccess);

            let should_skip_due_to_failure =
                previous_failed && *run_policy == StepRunPolicy::OnSuccess;
            let should_run_only_on_failure =
                !previous_failed && *run_policy == StepRunPolicy::OnFailure;

            if should_skip_due_to_failure || should_run_only_on_failure {
                run.step_results.push(StepRunResult {
                    step_id: step.id.clone(),
                    step_name: step.name.clone(),
                    agent: step.agent.to_string(),
                    action: step.action.clone(),
                    status: RunStatus::Aborted,
                    output: None,
                    error_message: None,
                    duration_ms: 0,
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                    skipped: true,
                    skip_reason: Some("Run policy not met".into()),
                });
                let _ = status_tx.send(WorkflowProgressUpdate::Step {
                    run_id: run.run_id.to_string(),
                    workflow_id: run.workflow_id.clone(),
                    step_id: step.id.clone(),
                    step_name: step.name.clone(),
                    status: RunStatus::Aborted,
                    message: Some("Run policy not met".into()),
                    updated_at: Utc::now(),
                });
                continue;
            }

            // Evaluate step condition
            if let Some(ref condition) = step.condition {
                let ok = TemplateEngine::eval_condition(condition, &run.context, &step_outputs);
                if !ok {
                    debug!(step_id = %step.id, condition, "Step skipped: condition false");
                    run.step_results.push(StepRunResult {
                        step_id: step.id.clone(),
                        step_name: step.name.clone(),
                        agent: step.agent.to_string(),
                        action: step.action.clone(),
                        status: RunStatus::Aborted,
                        output: None,
                        error_message: None,
                        duration_ms: 0,
                        started_at: Utc::now(),
                        finished_at: Some(Utc::now()),
                        skipped: true,
                        skip_reason: Some(format!("Condition evaluated to false: {condition}")),
                    });
                    let _ = status_tx.send(WorkflowProgressUpdate::Step {
                        run_id: run.run_id.to_string(),
                        workflow_id: run.workflow_id.clone(),
                        step_id: step.id.clone(),
                        step_name: step.name.clone(),
                        status: RunStatus::Aborted,
                        message: Some(format!("Condition evaluated to false: {condition}")),
                        updated_at: Utc::now(),
                    });
                    continue;
                }
            }

            // Dry-run shortcut
            if run.dry_run {
                info!(step_id = %step.id, "[DRY RUN] Simulating step");
                run.step_results.push(StepRunResult {
                    step_id: step.id.clone(),
                    step_name: step.name.clone(),
                    agent: step.agent.to_string(),
                    action: step.action.clone(),
                    status: RunStatus::Success,
                    output: Some(serde_json::json!({ "dry_run": true })),
                    error_message: None,
                    duration_ms: 1,
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                    skipped: false,
                    skip_reason: None,
                });
                step_outputs.insert(step.id.clone(), serde_json::json!({ "dry_run": true }));
                let _ = status_tx.send(WorkflowProgressUpdate::Step {
                    run_id: run.run_id.to_string(),
                    workflow_id: run.workflow_id.clone(),
                    step_id: step.id.clone(),
                    step_name: step.name.clone(),
                    status: RunStatus::Success,
                    message: Some("Simulated (Dry Run)".into()),
                    updated_at: Utc::now(),
                });
                continue;
            }

            // Execute the step
            let step_start = Utc::now();
            let _timeout_secs = step.timeout_seconds.unwrap_or(120);

            info!(step_id = %step.id, agent = %step.agent, action = %step.action, "Executing step");
            let _ = status_tx.send(WorkflowProgressUpdate::Step {
                run_id: run.run_id.to_string(),
                workflow_id: run.workflow_id.clone(),
                step_id: step.id.clone(),
                step_name: step.name.clone(),
                status: RunStatus::Running,
                message: None,
                updated_at: Utc::now(),
            });

            // Route to actual agents via Orchestrator.
            let step_result =
                Self::real_execute_step(step, run, &step_outputs, &orchestrator_ref).await;

            let step_end = Utc::now();
            let duration_ms = (step_end - step_start).num_milliseconds() as u64;

            match step_result {
                Ok(output) => {
                    let output_val =
                        serde_json::to_value(&output).unwrap_or(serde_json::Value::Null);
                    step_outputs.insert(step.id.clone(), output_val.clone());
                    run.step_results.push(StepRunResult {
                        step_id: step.id.clone(),
                        step_name: step.name.clone(),
                        agent: step.agent.to_string(),
                        action: step.action.clone(),
                        status: RunStatus::Success,
                        output: Some(output_val),
                        error_message: None,
                        duration_ms,
                        started_at: step_start,
                        finished_at: Some(step_end),
                        skipped: false,
                        skip_reason: None,
                    });
                    let _ = status_tx.send(WorkflowProgressUpdate::Step {
                        run_id: run.run_id.to_string(),
                        workflow_id: run.workflow_id.clone(),
                        step_id: step.id.clone(),
                        step_name: step.name.clone(),
                        status: RunStatus::Success,
                        message: None,
                        updated_at: Utc::now(),
                    });

                    // Handle next_step override
                    if let Some(ref next) = step.next_step {
                        debug!(current = %step.id, next, "Step jump requested – handled by condition on target step");
                    }
                }
                Err(e) => {
                    warn!(step_id = %step.id, error = %e, "Step failed");
                    previous_failed = true;

                    let error_msg = e.to_string();
                    run.step_results.push(StepRunResult {
                        step_id: step.id.clone(),
                        step_name: step.name.clone(),
                        agent: step.agent.to_string(),
                        action: step.action.clone(),
                        status: RunStatus::Failed,
                        output: None,
                        error_message: Some(error_msg.clone()),
                        duration_ms,
                        started_at: step_start,
                        finished_at: Some(step_end),
                        skipped: false,
                        skip_reason: None,
                    });
                    let _ = status_tx.send(WorkflowProgressUpdate::Step {
                        run_id: run.run_id.to_string(),
                        workflow_id: run.workflow_id.clone(),
                        step_id: step.id.clone(),
                        step_name: step.name.clone(),
                        status: RunStatus::Failed,
                        message: Some(error_msg.clone()),
                        updated_at: Utc::now(),
                    });

                    // Check step-level error handler
                    if let Some(ref on_err) = step.on_error {
                        match on_err.action.as_str() {
                            "abort" | "notify_and_abort" => {
                                return Err(WorkflowError::StepFailed {
                                    workflow_id: run.workflow_id.clone(),
                                    step_id: step.id.clone(),
                                    message: error_msg,
                                });
                            }
                            "skip" => {
                                warn!(step_id = %step.id, "Step failed but policy is 'skip' – continuing");
                                previous_failed = false; // reset for next step
                            }
                            _ => {
                                // notify_and_pause, retry → handled by Orchestrator HITL
                                warn!(step_id = %step.id, action = %on_err.action, "Step error handler: will notify");
                            }
                        }
                    } else {
                        // Default: abort on error
                        return Err(WorkflowError::StepFailed {
                            workflow_id: run.workflow_id.clone(),
                            step_id: step.id.clone(),
                            message: error_msg,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Real step executor – dispatches to the AgentRegistry via OrchestratorHandle.
    async fn real_execute_step(
        step: &StepDefinition,
        run: &WorkflowRun,
        step_outputs: &HashMap<String, serde_json::Value>,
        orchestrator_ref: &Arc<RwLock<Option<crate::orchestrator::OrchestratorHandle>>>,
    ) -> WorkflowResult<ActionOutput> {
        // Resolve input templates
        let mut resolved_input: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, template) in &step.input {
            let resolved = TemplateEngine::resolve(template, &run.context, step_outputs);
            resolved_input.insert(key.clone(), serde_json::Value::String(resolved));
        }

        // Add config overrides
        for (key, val) in step.config.as_object().unwrap_or(&serde_json::Map::new()) {
            resolved_input.insert(key.clone(), val.clone());
        }

        let orch_lock: tokio::sync::RwLockReadGuard<
            '_,
            Option<crate::orchestrator::OrchestratorHandle>,
        > = orchestrator_ref.read().await;
        let orch = orch_lock.as_ref().ok_or_else(|| {
            WorkflowError::ActionError("Orchestrator not attached to WorkflowEngine".into())
        })?;

        let task = crate::orchestrator::AgentTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            action: step.action.clone(),
            intent: crate::orchestrator::intent::Intent::Ambiguous(Default::default()),
            message: format!("Workflow Step: {}", step.name),
            context_file: None,
            session_id: format!("wf-{}", run.run_id),
            parameters: resolved_input.clone(),
            llm_gateway: None, // Will be populated by OrchestratorHandle::execute_agent_action
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        };

        info!(step_id = %step.id, agent = %step.agent, action = %step.action, "Dispatching to agent");

        match orch
            .execute_agent_action(&step.agent.to_string(), task)
            .await
        {
            Ok(output) => Ok(ActionOutput {
                summary: output.content,
                data: output.metadata,
                requires_approval: !output.committed,
                approval_id: None,
            }),
            Err(e) => Err(WorkflowError::ActionError(e.to_string())),
        }
    }

    /// Trigger dispatcher: receives trigger events and converts them to run requests.
    async fn trigger_dispatcher_loop(
        mut rx: mpsc::Receiver<TriggerEvent>,
        exec_tx: mpsc::Sender<EngineMessage>,
        definitions: Arc<DashMap<String, WorkflowDefinition>>,
    ) {
        info!("Trigger dispatcher loop started");

        while let Some(event) = rx.recv().await {
            let workflow_id = event.workflow_id.clone();

            match definitions.get(&workflow_id) {
                Some(def) => {
                    let run = WorkflowRun::new(
                        &def,
                        event.trigger_type.clone(),
                        Some(event.data.clone()),
                        false,
                    );
                    info!(
                        run_id = %run.run_id,
                        workflow_id,
                        trigger = %event.trigger_type,
                        "Trigger event dispatching workflow run"
                    );
                    let _ = exec_tx.send(EngineMessage::Execute(Box::new(run))).await;
                }
                None => {
                    warn!(workflow_id, "Trigger fired for unknown workflow – ignoring");
                }
            }
        }

        info!("Trigger dispatcher loop stopped");
    }

    /// Archive a completed run into the history store.
    fn archive_run(
        run: &WorkflowRun,
        history: &DashMap<String, Vec<WorkflowRun>>,
        max_history: usize,
    ) {
        let mut entry = history.entry(run.workflow_id.clone()).or_default();

        entry.push(run.clone());

        // Trim to max_history (keep most recent)
        if entry.len() > max_history {
            let excess = entry.len() - max_history;
            entry.drain(..excess);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── WorkflowDefinition parsing ───────────────────────────────────────────

    fn minimal_workflow_yaml() -> &'static str {
        r#"
id: test-workflow
name: Test Workflow
trigger:
  type: manual
steps:
  - id: step1
    name: First Step
    agent: orchestrator
    action: noop
"#
    }

    #[test]
    fn test_parse_minimal_workflow() {
        let def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        assert_eq!(def.id, "test-workflow");
        assert_eq!(def.steps.len(), 1);
        assert_eq!(def.trigger.trigger_type, TriggerType::Manual);
    }

    #[test]
    fn test_workflow_validation_empty_id_fails() {
        let mut def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        def.id = String::new();
        let result = WorkflowLoader::validate(&def);
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_validation_no_steps_fails() {
        let mut def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        def.steps.clear();
        let result = WorkflowLoader::validate(&def);
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_validation_bad_next_step_fails() {
        let mut def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        def.steps[0].next_step = Some("nonexistent_step".into());
        let result = WorkflowLoader::validate(&def);
        assert!(result.is_err());
    }

    // ── Template Engine ──────────────────────────────────────────────────────

    #[test]
    fn test_template_resolve_context_variable() {
        let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
        ctx.insert(
            "report_template".into(),
            serde_json::json!("templates/weekly.docx"),
        );
        let step_outputs: HashMap<String, serde_json::Value> = HashMap::new();

        let resolved = TemplateEngine::resolve("{{ report_template }}", &ctx, &step_outputs);
        assert_eq!(resolved, "templates/weekly.docx");
    }

    #[test]
    fn test_template_resolve_step_output() {
        let ctx: HashMap<String, serde_json::Value> = HashMap::new();
        let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();
        step_outputs.insert(
            "step1".into(),
            serde_json::json!({ "report_path": "C:/Reports/report.docx" }),
        );

        let resolved =
            TemplateEngine::resolve("{{ steps.step1.report_path }}", &ctx, &step_outputs);
        assert_eq!(resolved, "C:/Reports/report.docx");
    }

    #[test]
    fn test_template_resolve_unresolved_marker() {
        let ctx: HashMap<String, serde_json::Value> = HashMap::new();
        let step_outputs: HashMap<String, serde_json::Value> = HashMap::new();

        let resolved = TemplateEngine::resolve("{{ missing_var }}", &ctx, &step_outputs);
        assert!(resolved.contains("[UNRESOLVED:"), "got: {}", resolved);
    }

    #[test]
    fn test_template_eval_condition_true() {
        let ctx: HashMap<String, serde_json::Value> = HashMap::new();
        let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();
        step_outputs.insert("check".into(), serde_json::json!({ "audit_passed": true }));

        // Direct boolean from step output
        let ok =
            TemplateEngine::eval_condition("{{ steps.check.audit_passed }}", &ctx, &step_outputs);
        assert!(ok);
    }

    #[test]
    fn test_template_eval_condition_false_for_unresolved() {
        let ctx: HashMap<String, serde_json::Value> = HashMap::new();
        let step_outputs: HashMap<String, serde_json::Value> = HashMap::new();

        let ok =
            TemplateEngine::eval_condition("{{ steps.nonexistent.value }}", &ctx, &step_outputs);
        assert!(!ok);
    }

    // ── WorkflowRun ──────────────────────────────────────────────────────────

    #[test]
    fn test_workflow_run_new() {
        let def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        let run = WorkflowRun::new(&def, TriggerType::Manual, None, false);
        assert_eq!(run.workflow_id, "test-workflow");
        assert_eq!(run.status, RunStatus::Pending);
        assert!(!run.is_terminal());
    }

    #[test]
    fn test_workflow_run_finish() {
        let def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        let mut run = WorkflowRun::new(&def, TriggerType::Manual, None, false);
        run.finish(RunStatus::Success, Some("Done".into()));
        assert_eq!(run.status, RunStatus::Success);
        assert!(run.is_terminal());
        assert!(run.finished_at.is_some());
        assert!(run.duration_ms.is_some());
    }

    #[test]
    fn test_workflow_run_step_output_lookup() {
        let def: WorkflowDefinition = serde_yaml::from_str(minimal_workflow_yaml()).unwrap();
        let mut run = WorkflowRun::new(&def, TriggerType::Manual, None, false);

        run.step_results.push(StepRunResult {
            step_id: "step1".into(),
            step_name: "First Step".into(),
            agent: "orchestrator".into(),
            action: "noop".into(),
            status: RunStatus::Success,
            output: Some(serde_json::json!({ "file": "out.docx" })),
            error_message: None,
            duration_ms: 42,
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            skipped: false,
            skip_reason: None,
        });

        let output = run.step_output("step1");
        assert!(output.is_some());
        assert_eq!(output.unwrap()["file"], "out.docx");
        assert!(run.step_output("nonexistent").is_none());
    }

    // ── Run status display ───────────────────────────────────────────────────

    #[test]
    fn test_run_status_display() {
        assert_eq!(RunStatus::Success.to_string(), "success");
        assert_eq!(RunStatus::WaitingApproval.to_string(), "waiting_approval");
        assert_eq!(RunStatus::TimedOut.to_string(), "timed_out");
    }

    // ── WorkflowEngine (in-memory, no real triggers) ─────────────────────────

    #[tokio::test]
    async fn test_engine_no_workflows_dir_returns_ok() {
        let tmp = std::env::temp_dir().join("oh_test_empty_workflows_dir");
        tokio::fs::create_dir_all(&tmp).await.ok();

        let engine = WorkflowEngine::new(&tmp).await;
        assert!(engine.is_ok());
        let e = engine.unwrap();
        assert_eq!(e.definition_count(), 0);

        // cleanup
        tokio::fs::remove_dir_all(&tmp).await.ok();
    }

    #[tokio::test]
    async fn test_engine_trigger_unknown_workflow_returns_err() {
        let tmp = std::env::temp_dir().join("oh_test_trigger_unknown");
        tokio::fs::create_dir_all(&tmp).await.ok();

        let engine = WorkflowEngine::new(&tmp).await.unwrap();
        let result = engine.trigger("does-not-exist", None).await;
        assert!(result.is_err());

        tokio::fs::remove_dir_all(&tmp).await.ok();
    }

    #[tokio::test]
    async fn test_engine_trigger_known_workflow_returns_run() {
        let tmp = std::env::temp_dir().join("oh_test_trigger_known");
        tokio::fs::create_dir_all(&tmp).await.ok();

        // Write a minimal YAML workflow to the temp dir
        let yaml_path = tmp.join("test-workflow.yaml");
        tokio::fs::write(&yaml_path, minimal_workflow_yaml())
            .await
            .unwrap();

        let engine = WorkflowEngine::new(&tmp).await.unwrap();
        assert_eq!(engine.definition_count(), 1);

        let run = engine.trigger("test-workflow", None).await.unwrap();
        assert_eq!(run.workflow_id, "test-workflow");
        assert_eq!(run.status, RunStatus::Pending);

        // Allow executor loop to process the run
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // After execution, run should be in history
        let history = engine.get_runs("test-workflow");
        assert!(
            !history.is_empty(),
            "Expected at least one completed run in history"
        );
        assert!(
            history[0].is_terminal(),
            "Run should be in a terminal state"
        );

        tokio::fs::remove_dir_all(&tmp).await.ok();
    }
}
