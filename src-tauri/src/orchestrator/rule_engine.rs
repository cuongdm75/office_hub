// ============================================================================
// orchestrator/rule_engine.rs
//
// YAML-driven Rule Engine – validates every agent output before it is written
// to an Office document, a browser, or the outside world.
//
// Design goals
// ────────────
// • Zero-panic: all paths return Result<_, RuleEngineError>.
// • Hot-reload: rules are re-read from disk on every `validate()` call so that
//   operators can tweak rules without restarting the app.
// • Extensible: new rule kinds are added by implementing the `Rule` trait and
//   registering a factory in `RuleRegistry`.
// • Audit-first: every violation and every pass is structured-logged so the
//   audit trail is always complete.
// ============================================================================

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RuleEngineError {
    #[error("Failed to read rule file '{path}': {source}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse rule file '{path}': {source}")]
    ParseError {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("Unknown rule kind '{0}'")]
    UnknownRuleKind(String),

    #[error("Rule '{rule_id}' configuration error: {message}")]
    ConfigError { rule_id: String, message: String },

    #[error("Regex compilation error in rule '{rule_id}': {source}")]
    RegexError {
        rule_id: String,
        #[source]
        source: regex::Error,
    },

    #[error("Validation aborted: {0}")]
    Aborted(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Raw YAML schema (deserialized from rules/*.yaml)
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level structure of a rule file (e.g. `rules/default.yaml`).
#[derive(Debug, Clone, Deserialize)]
pub struct RuleFile {
    pub meta: RuleFileMeta,
    #[serde(default)]
    pub global: Option<GlobalRules>,
    #[serde(default)]
    pub agents: Option<AgentRules>,
    #[serde(default)]
    pub llm: Option<LlmRules>,
    #[serde(default)]
    pub security: Option<SecurityRules>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleFileMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Global resource and audit limits.
#[derive(Debug, Clone, Deserialize)]
pub struct GlobalRules {
    pub resource_limits: Option<ResourceLimits>,
    pub audit: Option<AuditConfig>,
    pub locale: Option<LocaleConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceLimits {
    #[serde(default = "defaults::max_concurrent_agents")]
    pub max_concurrent_agents: usize,
    #[serde(default = "defaults::max_llm_tokens")]
    pub max_llm_tokens_per_request: usize,
    #[serde(default = "defaults::max_session_history")]
    pub max_session_history_turns: usize,
    #[serde(default = "defaults::max_file_size_mb")]
    pub max_file_size_mb: u64,
    #[serde(default = "defaults::request_timeout_seconds")]
    pub request_timeout_seconds: u64,
    #[serde(default = "defaults::llm_retry_attempts")]
    pub llm_retry_attempts: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuditConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default = "defaults::log_level")]
    pub log_level: String,
    #[serde(default)]
    pub log_all_llm_outputs: bool,
    #[serde(default = "defaults::bool_true")]
    pub log_all_office_writes: bool,
    #[serde(default = "defaults::bool_true")]
    pub log_all_uia_actions: bool,
    #[serde(default = "defaults::log_retention_days")]
    pub log_retention_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocaleConfig {
    #[serde(default = "defaults::default_language")]
    pub default_language: String,
    #[serde(default = "defaults::date_format")]
    pub date_format: String,
}

/// Agent-specific rules.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentRules {
    pub analyst: Option<AnalystRules>,
    pub office_master: Option<OfficeMasterRules>,
    pub web_researcher: Option<WebResearcherRules>,
    pub converter: Option<ConverterRules>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnalystRules {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    pub operation_limits: Option<AnalystOperationLimits>,
    pub hard_truth_verification: Option<HardTruthConfig>,
    pub data_protection: Option<DataProtectionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnalystOperationLimits {
    #[serde(default = "defaults::max_rows")]
    pub max_rows_per_operation: usize,
    #[serde(default = "defaults::max_cols")]
    pub max_columns_per_operation: usize,
    #[serde(default = "defaults::max_formula_complexity")]
    pub max_formula_complexity: usize,
    #[serde(default)]
    pub allowed_vba_commands: Vec<String>,
    #[serde(default)]
    pub blocked_vba_commands: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HardTruthConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default = "defaults::bool_true")]
    pub verify_numeric_outputs: bool,
    #[serde(default = "defaults::tolerance")]
    pub tolerance_percentage: f64,
    #[serde(default = "defaults::bool_true")]
    pub verify_formula_results: bool,
    #[serde(default)]
    pub flag_suspicious_values: Vec<SuspiciousValuePattern>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SuspiciousValuePattern {
    pub pattern: String,
    pub context: String,
    pub warning: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataProtectionConfig {
    #[serde(default = "defaults::bool_true")]
    pub always_backup_before_write: bool,
    #[serde(default)]
    pub protect_named_ranges: Vec<String>,
    #[serde(default)]
    pub prevent_formula_overwrite: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfficeMasterRules {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    pub word: Option<WordRules>,
    pub powerpoint: Option<PowerPointRules>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WordRules {
    #[serde(default = "defaults::bool_true")]
    pub preserve_styles: bool,
    #[serde(default = "defaults::bool_true")]
    pub preserve_section_breaks: bool,
    #[serde(default = "defaults::bool_true")]
    pub preserve_headers_footers: bool,
    #[serde(default)]
    pub allowed_style_modifications: Vec<String>,
    #[serde(default = "defaults::max_document_pages")]
    pub max_document_pages: u32,
    #[serde(default = "defaults::bool_true")]
    pub backup_before_write: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PowerPointRules {
    #[serde(default = "defaults::bool_true")]
    pub preserve_master_slides: bool,
    #[serde(default = "defaults::bool_true")]
    pub preserve_brand_colors: bool,
    #[serde(default = "defaults::max_slides")]
    pub max_slides_per_deck: u32,
    #[serde(default = "defaults::max_text_per_slide")]
    pub max_text_per_slide_chars: usize,
    #[serde(default)]
    pub allowed_transitions: Vec<String>,
    #[serde(default)]
    pub brand_color_palette: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebResearcherRules {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    pub allowed_domains: Option<DomainPolicy>,
    #[serde(default)]
    pub blocked_actions: Vec<String>,
    #[serde(default)]
    pub require_human_approval: Vec<String>,
    pub extraction_limits: Option<ExtractionLimits>,
    pub grounding: Option<GroundingConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainPolicy {
    #[serde(default = "defaults::domain_mode")]
    pub mode: String, // "whitelist" | "blacklist" | "all"
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub blacklist: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtractionLimits {
    #[serde(default = "defaults::max_table_rows")]
    pub max_rows_per_table: usize,
    #[serde(default = "defaults::max_pages_navigate")]
    pub max_pages_to_navigate: u32,
    #[serde(default = "defaults::extraction_timeout")]
    pub extraction_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroundingConfig {
    #[serde(default = "defaults::bool_true")]
    pub always_take_screenshot: bool,
    #[serde(default = "defaults::bool_true")]
    pub attach_url_to_data: bool,
    #[serde(default = "defaults::bool_true")]
    pub attach_timestamp_to_data: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConverterRules {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_sources: Vec<String>,
    #[serde(default = "defaults::bool_true")]
    pub sandbox_mcp_servers: bool,
    #[serde(default = "defaults::bool_true")]
    pub require_approval_for_new_server: bool,
    #[serde(default = "defaults::max_mcp_servers")]
    pub max_mcp_servers: usize,
}

/// LLM output validation rules.
#[derive(Debug, Clone, Deserialize)]
pub struct LlmRules {
    pub output_validation: Option<LlmOutputValidation>,
    pub token_management: Option<TokenManagement>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmOutputValidation {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    pub hallucination_detection: Option<HallucinationDetection>,
    #[serde(default)]
    pub blocked_content: Option<BlockedContentRules>,
    pub length_limits: Option<LengthLimits>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HallucinationDetection {
    #[serde(default = "defaults::bool_true")]
    pub check_numeric_consistency: bool,
    #[serde(default = "defaults::bool_true")]
    pub check_date_validity: bool,
    #[serde(default = "defaults::bool_true")]
    pub check_percentage_range: bool,
    #[serde(default = "defaults::bool_true")]
    pub flag_fabricated_citations: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockedContentRules {
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub patterns: Vec<BlockedPattern>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockedPattern {
    pub regex: String,
    pub action: String, // "block" | "flag" | "redact"
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LengthLimits {
    #[serde(default = "defaults::excel_cell_chars")]
    pub max_excel_cell_chars: usize,
    #[serde(default = "defaults::word_paragraph_chars")]
    pub max_word_paragraph_chars: usize,
    #[serde(default = "defaults::ppt_textbox_chars")]
    pub max_ppt_text_box_chars: usize,
    #[serde(default = "defaults::chat_response_chars")]
    pub max_chat_response_chars: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenManagement {
    #[serde(default = "defaults::bool_true")]
    pub enable_prompt_caching: bool,
    #[serde(default = "defaults::cache_ttl_minutes")]
    pub cache_ttl_minutes: u64,
    #[serde(default = "defaults::compress_after_turns")]
    pub compress_context_after_turns: usize,
}

/// Security rules.
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityRules {
    pub human_in_the_loop: Option<HitlConfig>,
    pub rate_limiting: Option<RateLimits>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HitlConfig {
    #[serde(default = "defaults::bool_true")]
    pub enabled: bool,
    #[serde(default = "defaults::approval_timeout")]
    pub approval_timeout_seconds: u64,
    #[serde(default = "defaults::default_timeout_action")]
    pub default_action_on_timeout: String,
    pub approval_levels: Option<ApprovalLevels>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalLevels {
    #[serde(default)]
    pub low: Vec<String>,
    #[serde(default)]
    pub medium: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimits {
    #[serde(default = "defaults::llm_rpm")]
    pub llm_requests_per_minute: u32,
    #[serde(default = "defaults::com_rpm")]
    pub office_com_operations_per_minute: u32,
    #[serde(default = "defaults::uia_rpm")]
    pub uia_actions_per_minute: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Default value functions (required by serde)
// ─────────────────────────────────────────────────────────────────────────────

pub mod defaults {
    pub fn bool_true() -> bool {
        true
    }
    pub fn max_concurrent_agents() -> usize {
        4
    }
    pub fn max_llm_tokens() -> usize {
        32_768
    }
    pub fn max_session_history() -> usize {
        20
    }
    pub fn max_file_size_mb() -> u64 {
        100
    }
    pub fn request_timeout_seconds() -> u64 {
        120
    }
    pub fn llm_retry_attempts() -> u32 {
        3
    }
    pub fn log_level() -> String {
        "info".into()
    }
    pub fn log_retention_days() -> u32 {
        30
    }
    pub fn default_language() -> String {
        "vi".into()
    }
    pub fn date_format() -> String {
        "DD/MM/YYYY".into()
    }
    pub fn max_rows() -> usize {
        100_000
    }
    pub fn max_cols() -> usize {
        1_000
    }
    pub fn max_formula_complexity() -> usize {
        10
    }
    pub fn tolerance() -> f64 {
        0.01
    }
    pub fn max_document_pages() -> u32 {
        500
    }
    pub fn max_slides() -> u32 {
        100
    }
    pub fn max_text_per_slide() -> usize {
        500
    }
    pub fn domain_mode() -> String {
        "whitelist".into()
    }
    pub fn max_table_rows() -> usize {
        10_000
    }
    pub fn max_pages_navigate() -> u32 {
        10
    }
    pub fn extraction_timeout() -> u64 {
        30
    }
    pub fn max_mcp_servers() -> usize {
        50
    }
    pub fn excel_cell_chars() -> usize {
        32_767
    }
    pub fn word_paragraph_chars() -> usize {
        5_000
    }
    pub fn ppt_textbox_chars() -> usize {
        500
    }
    pub fn chat_response_chars() -> usize {
        8_000
    }
    pub fn cache_ttl_minutes() -> u64 {
        60
    }
    pub fn compress_after_turns() -> usize {
        10
    }
    pub fn approval_timeout() -> u64 {
        300
    }
    pub fn default_timeout_action() -> String {
        "reject".into()
    }
    pub fn llm_rpm() -> u32 {
        60
    }
    pub fn com_rpm() -> u32 {
        300
    }
    pub fn uia_rpm() -> u32 {
        60
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation context – describes *what* is being validated and *by whom*
// ─────────────────────────────────────────────────────────────────────────────

/// The kind of content being validated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ValidationTarget {
    /// A single Excel cell value.
    ExcelCell { sheet: String, cell_ref: String },
    /// A block of text to be inserted into a Word paragraph.
    WordParagraph { bookmark: Option<String> },
    /// Text for a PowerPoint text box.
    PptTextBox { slide_index: u32 },
    /// A VBA macro script to be executed.
    VbaScript,
    /// An LLM response before it is shown to the user or written anywhere.
    LlmResponse,
    /// A URL the Web Researcher wants to navigate to.
    WebUrl { url: String },
    /// A UIA action to be performed (click, fill, etc.)
    UiaAction { action_type: String },
    /// A chat message to be sent to the user.
    ChatMessage,
    /// A workflow YAML definition.
    WorkflowDefinition,
    /// A generic string value (catch-all).
    Generic { label: String },
}

/// Input to the rule engine.
#[derive(Debug, Clone)]
pub struct ValidationRequest {
    /// Unique ID for this validation run (for audit correlation).
    pub request_id: Uuid,
    /// The agent or component requesting validation.
    pub agent_id: String,
    /// What is being validated.
    pub target: ValidationTarget,
    /// The actual content to validate.
    pub content: String,
    /// Optional structured data (JSON) for richer checks.
    pub metadata: Option<serde_json::Value>,
    /// Timestamp of the request.
    pub created_at: DateTime<Utc>,
}

impl ValidationRequest {
    pub fn new(
        agent_id: impl Into<String>,
        target: ValidationTarget,
        content: impl Into<String>,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            agent_id: agent_id.into(),
            target,
            content: content.into(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_metadata(mut self, meta: serde_json::Value) -> Self {
        self.metadata = Some(meta);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation result
// ─────────────────────────────────────────────────────────────────────────────

/// Severity of a rule violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational – proceed but note the issue.
    Info,
    /// Warning – proceed with caution.
    Warning,
    /// Error – block the action and notify the user.
    Error,
    /// Critical – block immediately and require human review.
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warning => write!(f, "WARN"),
            Severity::Error => write!(f, "ERROR"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A single rule violation found during validation.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    /// ID of the rule that produced this violation.
    pub rule_id: String,
    /// Human-readable rule name.
    pub rule_name: String,
    /// Severity of this violation.
    pub severity: Severity,
    /// Explanation of what went wrong.
    pub message: String,
    /// If `true`, the content MUST NOT be written and the action is blocked.
    pub blocking: bool,
    /// Optional suggested replacement or fix.
    pub suggestion: Option<String>,
}

/// Overall result returned by `RuleEngine::validate()`.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub request_id: Uuid,
    pub agent_id: String,
    pub target: ValidationTarget,
    /// `true` when the content passes all blocking rules.
    pub passed: bool,
    /// All violations found (blocking + non-blocking).
    pub violations: Vec<Violation>,
    /// Content after any non-blocking transformations (redaction, trimming).
    pub sanitized_content: String,
    /// Whether Human-in-the-Loop approval is required before proceeding.
    pub requires_human_approval: bool,
    /// Approval level required (if `requires_human_approval` is true).
    pub approval_level: Option<String>,
    pub validated_at: DateTime<Utc>,
    pub duration_ms: u64,
}

impl ValidationResult {
    /// Convenience: collect all blocking violations.
    pub fn blocking_violations(&self) -> Vec<&Violation> {
        self.violations.iter().filter(|v| v.blocking).collect()
    }

    /// Convenience: highest severity across all violations.
    pub fn max_severity(&self) -> Option<&Severity> {
        self.violations.iter().map(|v| &v.severity).max()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rule trait – every concrete rule implements this
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Rule: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    /// Evaluate the rule against `req` and return any violations.
    /// An empty Vec means "passed".
    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Concrete rule implementations
// ─────────────────────────────────────────────────────────────────────────────

// ── 1. Content-length rule ───────────────────────────────────────────────────

struct LengthRule {
    rule_id: String,
    max_chars: usize,
    targets: Vec<ValidationTarget>,
}

impl LengthRule {
    fn new(id: impl Into<String>, max_chars: usize, targets: Vec<ValidationTarget>) -> Self {
        Self {
            rule_id: id.into(),
            max_chars,
            targets,
        }
    }

    fn applies_to(&self, target: &ValidationTarget) -> bool {
        // Match by discriminant
        self.targets
            .iter()
            .any(|t| std::mem::discriminant(t) == std::mem::discriminant(target))
    }
}

#[async_trait]
impl Rule for LengthRule {
    fn id(&self) -> &str {
        &self.rule_id
    }
    fn name(&self) -> &str {
        "Content Length Limit"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        if !self.applies_to(&req.target) {
            return vec![];
        }
        let len = req.content.chars().count();
        if len > self.max_chars {
            vec![Violation {
                rule_id: self.rule_id.clone(),
                rule_name: self.name().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Content is {} characters, exceeding the limit of {}.",
                    len, self.max_chars
                ),
                blocking: true,
                suggestion: Some(format!(
                    "Truncate or summarise to under {} characters.",
                    self.max_chars
                )),
            }]
        } else {
            vec![]
        }
    }
}

// ── 2. Regex-based blocked-content rule ─────────────────────────────────────

struct BlockedPatternRule {
    rule_id: String,
    rule_name: String,
    pattern: Regex,
    action: String, // "block" | "flag" | "redact"
    reason: String,
}

impl BlockedPatternRule {
    fn try_new(
        id: impl Into<String>,
        name: impl Into<String>,
        pattern_str: &str,
        action: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<Self, RuleEngineError> {
        let id_str = id.into();
        let pattern = Regex::new(pattern_str).map_err(|e| RuleEngineError::RegexError {
            rule_id: id_str.clone(),
            source: e,
        })?;
        Ok(Self {
            rule_id: id_str,
            rule_name: name.into(),
            pattern,
            action: action.into(),
            reason: reason.into(),
        })
    }
}

#[async_trait]
impl Rule for BlockedPatternRule {
    fn id(&self) -> &str {
        &self.rule_id
    }
    fn name(&self) -> &str {
        &self.rule_name
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        if !self.pattern.is_match(&req.content) {
            return vec![];
        }
        let (severity, blocking) = match self.action.as_str() {
            "block" => (Severity::Critical, true),
            "flag" => (Severity::Warning, false),
            "redact" => (Severity::Warning, false), // engine will sanitize
            _ => (Severity::Warning, false),
        };
        vec![Violation {
            rule_id: self.rule_id.clone(),
            rule_name: self.rule_name.clone(),
            severity,
            message: format!("Blocked content detected: {}", self.reason),
            blocking,
            suggestion: if self.action == "redact" {
                Some("Sensitive data has been automatically redacted.".into())
            } else {
                None
            },
        }]
    }
}

// ── 3. VBA command whitelist/blacklist rule ──────────────────────────────────

struct VbaCommandRule {
    _allowed: Vec<Regex>,
    blocked: Vec<(Regex, String)>, // (pattern, description)
}

impl VbaCommandRule {
    fn try_new(
        allowed_patterns: &[String],
        blocked_patterns: &[String],
    ) -> Result<Self, RuleEngineError> {
        let allowed = allowed_patterns
            .iter()
            .map(|p| {
                Regex::new(&format!("(?i){}", regex::escape(p).replace(r"\*", ".*"))).map_err(|e| {
                    RuleEngineError::RegexError {
                        rule_id: "vba_whitelist".into(),
                        source: e,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut all_blocked = blocked_patterns.to_vec();
        // Cứng hóa Sandbox: Chặn tuyệt đối các lời gọi Shell, Filesystem, Network từ VBA
        let hardcoded_blocks = vec![
            r"WScript\.Shell".to_string(),
            r"(?i)CreateObject\s*\(\s*.(?:WScript\.Shell|Shell\.Application).*\)".to_string(),
            r"(?i)\bKill\b\s+".to_string(),
            r"(?i)\bOpen\b\s+".to_string(),
            r"(?i)MSXML2\.XMLHTTP".to_string(),
            r"(?i)WinHttp\.WinHttpRequest".to_string(),
            r"(?i)\bEnviron\b\s*\(".to_string(),
            r"(?i)\bShell\b\s*\(".to_string(),
            r"(?i)FileSystemObject".to_string(),
        ];
        for b in hardcoded_blocks {
            if !all_blocked.contains(&b) {
                all_blocked.push(b);
            }
        }

        let blocked = all_blocked
            .iter()
            .map(|p| {
                let re = Regex::new(&format!("(?i){}", regex::escape(p))).map_err(|e| {
                    RuleEngineError::RegexError {
                        rule_id: "vba_blacklist".into(),
                        source: e,
                    }
                })?;
                Ok((re, p.clone()))
            })
            .collect::<Result<Vec<_>, RuleEngineError>>()?;

        Ok(Self { _allowed: allowed, blocked })
    }
}

#[async_trait]
impl Rule for VbaCommandRule {
    fn id(&self) -> &str {
        "vba_command_policy"
    }
    fn name(&self) -> &str {
        "VBA Command Policy"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        if !matches!(req.target, ValidationTarget::VbaScript) {
            return vec![];
        }

        let mut violations = vec![];

        // Check blacklist first
        for (pattern, description) in &self.blocked {
            if pattern.is_match(&req.content) {
                violations.push(Violation {
                    rule_id: self.id().to_string(),
                    rule_name: self.name().to_string(),
                    severity: Severity::Critical,
                    message: format!(
                        "VBA script contains a blocked command: '{}'. This command is forbidden for security reasons.",
                        description
                    ),
                    blocking: true,
                    suggestion: Some(
                        "Remove the blocked VBA command and use a safe alternative.".into(),
                    ),
                });
            }
        }

        violations
    }
}

// ── 4. URL domain policy rule ────────────────────────────────────────────────

struct DomainPolicyRule {
    mode: String,
    whitelist_patterns: Vec<Regex>,
    blacklist_patterns: Vec<Regex>,
}

impl DomainPolicyRule {
    fn try_new(policy: &DomainPolicy) -> Result<Self, RuleEngineError> {
        let compile = |patterns: &[String], rule_id: &str| {
            patterns
                .iter()
                .map(|p| {
                    // Convert glob-style "*.example.com" → regex
                    let re_str = format!(
                        "(?i)^https?://{}",
                        regex::escape(p).replace(r"\*\.", r"([a-z0-9-]+\.)*")
                    );
                    Regex::new(&re_str).map_err(|e| RuleEngineError::RegexError {
                        rule_id: rule_id.into(),
                        source: e,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        };

        Ok(Self {
            mode: policy.mode.clone(),
            whitelist_patterns: compile(&policy.whitelist, "domain_whitelist")?,
            blacklist_patterns: compile(&policy.blacklist, "domain_blacklist")?,
        })
    }

    fn is_url_allowed(&self, url: &str) -> bool {
        match self.mode.as_str() {
            "all" => {
                // Only check blacklist
                !self.blacklist_patterns.iter().any(|re| re.is_match(url))
            }
            "blacklist" => !self.blacklist_patterns.iter().any(|re| re.is_match(url)),
            _ => {
                // whitelist mode (default)
                self.whitelist_patterns.iter().any(|re| re.is_match(url))
            }
        }
    }
}

#[async_trait]
impl Rule for DomainPolicyRule {
    fn id(&self) -> &str {
        "domain_policy"
    }
    fn name(&self) -> &str {
        "Web Researcher Domain Policy"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        let url = match &req.target {
            ValidationTarget::WebUrl { url } => url.clone(),
            _ => return vec![],
        };

        if !self.is_url_allowed(&url) {
            return vec![Violation {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: Severity::Error,
                message: format!(
                    "URL '{}' is not permitted under the current domain policy (mode: {}).",
                    url, self.mode
                ),
                blocking: true,
                suggestion: Some("Add the domain to the whitelist in rules/default.yaml under \
                     agents.web_researcher.allowed_domains.whitelist.".to_string()),
            }];
        }

        vec![]
    }
}

// ── 5. Percentage range sanity check ────────────────────────────────────────

struct PercentageRangeRule;

#[async_trait]
impl Rule for PercentageRangeRule {
    fn id(&self) -> &str {
        "percentage_range"
    }
    fn name(&self) -> &str {
        "Percentage Range Sanity Check"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        // Only applies to LLM responses and Excel cells
        match req.target {
            ValidationTarget::LlmResponse | ValidationTarget::ExcelCell { .. } => {}
            _ => return vec![],
        }

        // Find all decimal numbers followed by % in the content
        let re = Regex::new(r"(-?\d+(?:\.\d+)?)\s*%").unwrap();
        let mut violations = vec![];

        for cap in re.captures_iter(&req.content) {
            if let Some(m) = cap.get(1) {
                if let Ok(value) = m.as_str().parse::<f64>() {
                    if !(0.0..=100.0).contains(&value) {
                        violations.push(Violation {
                            rule_id: self.id().to_string(),
                            rule_name: self.name().to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "Suspicious percentage value: {}%. Expected range is 0%–100%.",
                                value
                            ),
                            blocking: false,
                            suggestion: Some(
                                "Verify the percentage value against the source data.".into(),
                            ),
                        });
                    }
                }
            }
        }

        violations
    }
}

// ── 6. Placeholder leakage rule ──────────────────────────────────────────────
// Detects un-substituted template placeholders like {{VAR}} or ${VAR}.

struct PlaceholderLeakageRule;

#[async_trait]
impl Rule for PlaceholderLeakageRule {
    fn id(&self) -> &str {
        "placeholder_leakage"
    }
    fn name(&self) -> &str {
        "Template Placeholder Leakage"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        match req.target {
            ValidationTarget::WordParagraph { .. }
            | ValidationTarget::PptTextBox { .. }
            | ValidationTarget::ExcelCell { .. }
            | ValidationTarget::ChatMessage => {}
            _ => return vec![],
        }

        let re = Regex::new(r"\{\{[^}]+\}\}|\$\{[^}]+\}").unwrap();
        if re.is_match(&req.content) {
            return vec![Violation {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: Severity::Error,
                message: "Un-substituted template placeholder(s) detected in output. \
                          The content must not be written until all placeholders are resolved."
                    .to_string(),
                blocking: true,
                suggestion: Some(
                    "Ensure the template engine has resolved all {{ }} / ${ } expressions \
                     before passing content to the agent."
                        .into(),
                ),
            }];
        }

        vec![]
    }
}

// ── 7. Hard-truth numeric verification rule ──────────────────────────────────
// After a write, the orchestrator calls this rule with both the intended value
// (in `content`) and the value actually read back from COM (in `metadata.actual`).

struct HardTruthVerificationRule {
    tolerance: f64,
}

#[async_trait]
impl Rule for HardTruthVerificationRule {
    fn id(&self) -> &str {
        "hard_truth_verification"
    }
    fn name(&self) -> &str {
        "Hard-Truth Numeric Verification"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        if !matches!(req.target, ValidationTarget::ExcelCell { .. }) {
            return vec![];
        }

        // Expect metadata = { "actual": <number>, "intended": <number> }
        let meta = match &req.metadata {
            Some(m) => m,
            None => return vec![],
        };

        let intended: f64 = match meta.get("intended").and_then(|v| v.as_f64()) {
            Some(v) => v,
            None => return vec![],
        };
        let actual: f64 = match meta.get("actual").and_then(|v| v.as_f64()) {
            Some(v) => v,
            None => return vec![],
        };

        if intended == 0.0 && actual == 0.0 {
            return vec![];
        }

        let reference = if intended.abs() > 1e-10 {
            intended.abs()
        } else {
            actual.abs()
        };
        let deviation_pct = ((intended - actual).abs() / reference) * 100.0;

        if deviation_pct > self.tolerance {
            return vec![Violation {
                rule_id: self.id().to_string(),
                rule_name: self.name().to_string(),
                severity: Severity::Critical,
                message: format!(
                    "Hard-truth mismatch: intended {:.4} but COM read back {:.4} \
                     (deviation {:.4}%, tolerance {:.4}%).",
                    intended, actual, deviation_pct, self.tolerance
                ),
                blocking: true,
                suggestion: Some(
                    "Do NOT use the LLM-generated value. Re-read the cell from Excel via COM."
                        .into(),
                ),
            }];
        }

        vec![]
    }
}

// ── 8. Human-in-the-Loop approval level classifier ──────────────────────────

struct HitlClassifierRule {
    low: Vec<String>,
    medium: Vec<String>,
    high: Vec<String>,
    critical: Vec<String>,
}

impl HitlClassifierRule {
    fn classify_action(&self, action: &str) -> Option<String> {
        if self.critical.iter().any(|a| a == action) {
            Some("critical".into())
        } else if self.high.iter().any(|a| a == action) {
            Some("high".into())
        } else if self.medium.iter().any(|a| a == action) {
            Some("medium".into())
        } else if self.low.iter().any(|a| a == action) {
            Some("low".into())
        } else {
            None
        }
    }
}

#[async_trait]
impl Rule for HitlClassifierRule {
    fn id(&self) -> &str {
        "hitl_classifier"
    }
    fn name(&self) -> &str {
        "Human-in-the-Loop Classifier"
    }

    async fn evaluate(&self, req: &ValidationRequest) -> Vec<Violation> {
        // This rule never blocks; it only annotates `requires_human_approval` via a
        // special metadata violation that the engine interprets after collecting all violations.
        let action_type = match &req.target {
            ValidationTarget::UiaAction { action_type } => action_type.as_str().to_string(),
            _ => return vec![],
        };

        if let Some(level) = self.classify_action(&action_type) {
            if level == "high" || level == "critical" {
                return vec![Violation {
                    rule_id: format!("hitl_required:{}", level),
                    rule_name: self.name().to_string(),
                    severity: if level == "critical" { Severity::Critical } else { Severity::Warning },
                    message: format!(
                        "Action '{}' is classified as '{}' – human approval required before execution.",
                        action_type, level
                    ),
                    blocking: false, // engine sets `requires_human_approval` separately
                    suggestion: None,
                }];
            }
        }

        vec![]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sanitizer – applies non-blocking transformations (redaction, trimming)
// ─────────────────────────────────────────────────────────────────────────────

pub struct Sanitizer {
    redact_patterns: Vec<(Regex, String)>, // (pattern, replacement)
}

impl Sanitizer {
    pub fn new(blocked_patterns: &[BlockedPattern]) -> Result<Self, RuleEngineError> {
        let redact_patterns = blocked_patterns
            .iter()
            .filter(|p| p.action == "redact")
            .map(|p| {
                let re = Regex::new(&p.regex).map_err(|e| RuleEngineError::RegexError {
                    rule_id: "sanitizer".into(),
                    source: e,
                })?;
                Ok((re, "[REDACTED]".to_string()))
            })
            .collect::<Result<Vec<_>, RuleEngineError>>()?;

        Ok(Self { redact_patterns })
    }

    pub fn sanitize(&self, content: &str) -> String {
        let mut result = content.to_string();
        for (pattern, replacement) in &self.redact_patterns {
            result = pattern
                .replace_all(&result, replacement.as_str())
                .to_string();
        }
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rule Engine
// ─────────────────────────────────────────────────────────────────────────────

pub struct RuleEngine {
    /// Path to the primary rule YAML file.
    rules_path: PathBuf,
    /// Compiled rules (rebuilt whenever the rule file is reloaded).
    rules: Arc<RwLock<Vec<Box<dyn Rule>>>>,
    /// Sanitizer for redaction.
    sanitizer: Arc<RwLock<Sanitizer>>,
    /// HITL classifier (kept separate for quick approval-level lookup).
    hitl_classifier: Arc<RwLock<Option<HitlClassifierRule>>>,
    /// Parsed rule file (cached for config lookups).
    rule_file: Arc<RwLock<Option<RuleFile>>>,
}

impl RuleEngine {
    // ── Construction ────────────────────────────────────────────────────────

    /// Create a new `RuleEngine` and load the rule file immediately.
    pub async fn new(rules_path: impl Into<PathBuf>) -> Result<Self, RuleEngineError> {
        let path = rules_path.into();
        let engine = Self {
            rules_path: path,
            rules: Arc::new(RwLock::new(vec![])),
            sanitizer: Arc::new(RwLock::new(Sanitizer::new(&[])?)),
            hitl_classifier: Arc::new(RwLock::new(None)),
            rule_file: Arc::new(RwLock::new(None)),
        };
        engine.reload().await?;
        Ok(engine)
    }

    /// Create a `RuleEngine` with only the built-in default rules (no file needed).
    pub async fn with_defaults() -> Result<Self, RuleEngineError> {
        let engine = Self {
            rules_path: PathBuf::new(),
            rules: Arc::new(RwLock::new(vec![])),
            sanitizer: Arc::new(RwLock::new(Sanitizer::new(&[])?)),
            hitl_classifier: Arc::new(RwLock::new(None)),
            rule_file: Arc::new(RwLock::new(None)),
        };
        engine.apply_defaults().await?;
        Ok(engine)
    }

    // ── Hot-reload ──────────────────────────────────────────────────────────

    /// Re-read the YAML file and rebuild all compiled rules.
    /// Safe to call while other threads are using the engine.
    pub async fn reload(&self) -> Result<(), RuleEngineError> {
        if self.rules_path.as_os_str().is_empty() {
            return self.apply_defaults().await;
        }

        let content = tokio::fs::read_to_string(&self.rules_path)
            .await
            .map_err(|e| RuleEngineError::FileRead {
                path: self.rules_path.clone(),
                source: e,
            })?;

        let rule_file: RuleFile =
            serde_yaml::from_str(&content).map_err(|e| RuleEngineError::ParseError {
                path: self.rules_path.clone(),
                source: e,
            })?;

        info!(
            path = ?self.rules_path,
            name = %rule_file.meta.name,
            version = %rule_file.meta.version,
            "Rule file loaded"
        );

        let new_rules = self.compile_rules(&rule_file)?;
        let new_sanitizer = self.build_sanitizer(&rule_file)?;
        let new_hitl = self.build_hitl_classifier(&rule_file);

        // Atomic swap
        *self.rules.write().await = new_rules;
        *self.sanitizer.write().await = new_sanitizer;
        *self.hitl_classifier.write().await = new_hitl;
        *self.rule_file.write().await = Some(rule_file);

        info!("Rule engine reloaded successfully.");
        Ok(())
    }

    // ── Rule compilation ────────────────────────────────────────────────────

    fn compile_rules(&self, rf: &RuleFile) -> Result<Vec<Box<dyn Rule>>, RuleEngineError> {
        let mut rules: Vec<Box<dyn Rule>> = vec![];

        // ── LLM output rules ────────────────────────────────────────────────
        if let Some(llm) = &rf.llm {
            if let Some(validation) = &llm.output_validation {
                if validation.enabled {
                    // Length limits
                    if let Some(limits) = &validation.length_limits {
                        rules.push(Box::new(LengthRule::new(
                            "llm_response_length",
                            limits.max_chat_response_chars,
                            vec![ValidationTarget::LlmResponse, ValidationTarget::ChatMessage],
                        )));
                        rules.push(Box::new(LengthRule::new(
                            "excel_cell_length",
                            limits.max_excel_cell_chars,
                            vec![ValidationTarget::ExcelCell {
                                sheet: String::new(),
                                cell_ref: String::new(),
                            }],
                        )));
                        rules.push(Box::new(LengthRule::new(
                            "word_paragraph_length",
                            limits.max_word_paragraph_chars,
                            vec![ValidationTarget::WordParagraph { bookmark: None }],
                        )));
                        rules.push(Box::new(LengthRule::new(
                            "ppt_textbox_length",
                            limits.max_ppt_text_box_chars,
                            vec![ValidationTarget::PptTextBox { slide_index: 0 }],
                        )));
                    }

                    // Blocked content patterns
                    if let Some(blocked) = &validation.blocked_content {
                        for (i, pat) in blocked.patterns.iter().enumerate() {
                            let rule_id = format!("blocked_pattern_{}", i);
                            let rule = BlockedPatternRule::try_new(
                                &rule_id,
                                format!("Blocked Pattern #{}", i + 1),
                                &pat.regex,
                                &pat.action,
                                &pat.reason,
                            )?;
                            rules.push(Box::new(rule));
                        }
                    }

                    // Percentage range check
                    if let Some(hd) = &validation.hallucination_detection {
                        if hd.check_percentage_range {
                            rules.push(Box::new(PercentageRangeRule));
                        }
                    }
                }
            }
        }

        // ── Analyst agent rules ─────────────────────────────────────────────
        if let Some(agents) = &rf.agents {
            if let Some(analyst) = &agents.analyst {
                if analyst.enabled {
                    // VBA command policy
                    if let Some(limits) = &analyst.operation_limits {
                        let vba_rule = VbaCommandRule::try_new(
                            &limits.allowed_vba_commands,
                            &limits.blocked_vba_commands,
                        )?;
                        rules.push(Box::new(vba_rule));
                    }

                    // Hard-truth verification
                    if let Some(ht) = &analyst.hard_truth_verification {
                        if ht.enabled && ht.verify_numeric_outputs {
                            rules.push(Box::new(HardTruthVerificationRule {
                                tolerance: ht.tolerance_percentage,
                            }));
                        }
                    }
                }
            }

            // Web researcher domain policy
            if let Some(web) = &agents.web_researcher {
                if web.enabled {
                    if let Some(domain_policy) = &web.allowed_domains {
                        let domain_rule = DomainPolicyRule::try_new(domain_policy)?;
                        rules.push(Box::new(domain_rule));
                    }
                }
            }
        }

        // ── Always-on rules ─────────────────────────────────────────────────
        rules.push(Box::new(PlaceholderLeakageRule));

        // HITL classifier (non-blocking annotation)
        if let Some(hitl) = self.build_hitl_classifier(rf) {
            rules.push(Box::new(HitlClassifierRule {
                low: hitl.low,
                medium: hitl.medium,
                high: hitl.high,
                critical: hitl.critical,
            }));
        }

        debug!("Compiled {} rules", rules.len());
        Ok(rules)
    }

    fn build_sanitizer(&self, rf: &RuleFile) -> Result<Sanitizer, RuleEngineError> {
        let patterns = rf
            .llm
            .as_ref()
            .and_then(|l| l.output_validation.as_ref())
            .and_then(|v| v.blocked_content.as_ref())
            .map(|b| b.patterns.as_slice())
            .unwrap_or(&[]);
        Sanitizer::new(patterns)
    }

    fn build_hitl_classifier(&self, rf: &RuleFile) -> Option<HitlClassifierRule> {
        let levels = rf
            .security
            .as_ref()
            .and_then(|s| s.human_in_the_loop.as_ref())
            .filter(|h| h.enabled)
            .and_then(|h| h.approval_levels.as_ref())?;

        Some(HitlClassifierRule {
            low: levels.low.clone(),
            medium: levels.medium.clone(),
            high: levels.high.clone(),
            critical: levels.critical.clone(),
        })
    }

    async fn apply_defaults(&self) -> Result<(), RuleEngineError> {
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(PlaceholderLeakageRule),
            Box::new(PercentageRangeRule),
            Box::new(HardTruthVerificationRule { tolerance: 0.01 }),
            Box::new(LengthRule::new(
                "default_llm_length",
                8_000,
                vec![ValidationTarget::LlmResponse, ValidationTarget::ChatMessage],
            )),
        ];
        *self.rules.write().await = rules;
        *self.sanitizer.write().await = Sanitizer::new(&[])?;
        Ok(())
    }

    // ── Main validation entry point ─────────────────────────────────────────

    /// Validate `req` against all loaded rules.
    pub async fn validate(&self, req: ValidationRequest) -> ValidationResult {
        let start = std::time::Instant::now();
        let rules = self.rules.read().await;
        let sanitizer = self.sanitizer.read().await;

        let mut all_violations: Vec<Violation> = vec![];

        // Run all rules concurrently
        let futures: Vec<_> = rules.iter().map(|rule| rule.evaluate(&req)).collect();
        for violations in futures::future::join_all(futures).await {
            all_violations.extend(violations);
        }

        // Determine pass/fail
        let passed = all_violations.iter().all(|v| !v.blocking);

        // Determine HITL requirements
        let (requires_human_approval, approval_level) = self.determine_hitl(&all_violations);

        // Sanitize content
        let sanitized_content = if passed {
            sanitizer.sanitize(&req.content)
        } else {
            req.content.clone()
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        // Structured audit log
        if !all_violations.is_empty() {
            for v in &all_violations {
                if v.blocking {
                    warn!(
                        request_id = %req.request_id,
                        agent_id  = %req.agent_id,
                        rule_id   = %v.rule_id,
                        severity  = %v.severity,
                        blocking  = true,
                        message   = %v.message,
                        "Rule violation (BLOCKING)"
                    );
                } else {
                    debug!(
                        request_id = %req.request_id,
                        agent_id  = %req.agent_id,
                        rule_id   = %v.rule_id,
                        severity  = %v.severity,
                        blocking  = false,
                        message   = %v.message,
                        "Rule violation (non-blocking)"
                    );
                }
            }
        } else {
            debug!(
                request_id = %req.request_id,
                agent_id  = %req.agent_id,
                duration_ms,
                "Validation passed with no violations"
            );
        }

        ValidationResult {
            request_id: req.request_id,
            agent_id: req.agent_id,
            target: req.target,
            passed,
            violations: all_violations,
            sanitized_content,
            requires_human_approval,
            approval_level,
            validated_at: Utc::now(),
            duration_ms,
        }
    }

    /// Determine HITL requirements from the set of violations.
    fn determine_hitl(&self, violations: &[Violation]) -> (bool, Option<String>) {
        let hitl_violation = violations
            .iter()
            .find(|v| v.rule_id.starts_with("hitl_required:"));

        if let Some(v) = hitl_violation {
            let level = v.rule_id.split(':').nth(1).unwrap_or("high").to_string();
            return (true, Some(level));
        }

        (false, None)
    }

    // ── Config accessors ────────────────────────────────────────────────────

    /// Return the resource limits from the loaded rule file (or defaults).
    pub async fn resource_limits(&self) -> ResourceLimits {
        self.rule_file
            .read()
            .await
            .as_ref()
            .and_then(|rf| rf.global.as_ref())
            .and_then(|g| g.resource_limits.as_ref())
            .cloned()
            .unwrap_or_else(|| ResourceLimits {
                max_concurrent_agents: defaults::max_concurrent_agents(),
                max_llm_tokens_per_request: defaults::max_llm_tokens(),
                max_session_history_turns: defaults::max_session_history(),
                max_file_size_mb: defaults::max_file_size_mb(),
                request_timeout_seconds: defaults::request_timeout_seconds(),
                llm_retry_attempts: defaults::llm_retry_attempts(),
            })
    }

    /// Return HITL config from the loaded rule file.
    pub async fn hitl_config(&self) -> Option<HitlConfig> {
        self.rule_file
            .read()
            .await
            .as_ref()
            .and_then(|rf| rf.security.as_ref())
            .and_then(|s| s.human_in_the_loop.as_ref())
            .cloned()
    }

    /// Return token management config.
    pub async fn token_management(&self) -> Option<TokenManagement> {
        self.rule_file
            .read()
            .await
            .as_ref()
            .and_then(|rf| rf.llm.as_ref())
            .and_then(|l| l.token_management.as_ref())
            .cloned()
    }

    /// Return a clone of the entire parsed rule file for inspection.
    pub async fn rule_file_snapshot(&self) -> Option<RuleFile> {
        self.rule_file.read().await.clone()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Default implementation for RuleEngine (empty/minimal state)
// ─────────────────────────────────────────────────────────────────────────────

impl Default for RuleEngine {
    fn default() -> Self {
        use std::path::PathBuf;
        Self {
            rules_path: PathBuf::from("rules/default.yaml"),
            rules: Arc::new(RwLock::new(vec![])),
            sanitizer: Arc::new(RwLock::new(Sanitizer::new(&[]).unwrap())),
            hitl_classifier: Arc::new(RwLock::new(None)),
            rule_file: Arc::new(RwLock::new(None)),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn default_engine() -> RuleEngine {
        RuleEngine::with_defaults().await.unwrap()
    }

    #[tokio::test]
    async fn test_placeholder_leakage_blocked() {
        let engine = default_engine().await;
        let req = ValidationRequest::new(
            "office_master",
            ValidationTarget::WordParagraph { bookmark: None },
            "Kính gửi {{NAME}}, báo cáo ngày ${DATE}.",
        );
        let result = engine.validate(req).await;
        assert!(!result.passed);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule_id == "placeholder_leakage"));
    }

    #[tokio::test]
    async fn test_placeholder_leakage_passes_clean_content() {
        let engine = default_engine().await;
        let req = ValidationRequest::new(
            "office_master",
            ValidationTarget::WordParagraph { bookmark: None },
            "Kính gửi anh Nguyễn Văn A, báo cáo ngày 01/01/2025.",
        );
        let result = engine.validate(req).await;
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[tokio::test]
    async fn test_hard_truth_mismatch_blocking() {
        let engine = default_engine().await;
        let req = ValidationRequest::new(
            "analyst",
            ValidationTarget::ExcelCell {
                sheet: "Sheet1".into(),
                cell_ref: "B5".into(),
            },
            "1000000",
        )
        .with_metadata(serde_json::json!({
            "intended": 1_000_000.0,
            "actual":   1_200_000.0   // 20% deviation → above 0.01% tolerance
        }));
        let result = engine.validate(req).await;
        assert!(!result.passed);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule_id == "hard_truth_verification"));
    }

    #[tokio::test]
    async fn test_hard_truth_within_tolerance_passes() {
        let engine = default_engine().await;
        let req = ValidationRequest::new(
            "analyst",
            ValidationTarget::ExcelCell {
                sheet: "Sheet1".into(),
                cell_ref: "B5".into(),
            },
            "1000000",
        )
        .with_metadata(serde_json::json!({
            "intended": 1_000_000.0,
            "actual":   1_000_000.0001   // negligible deviation
        }));
        let result = engine.validate(req).await;
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_percentage_range_warning() {
        let engine = default_engine().await;
        let req = ValidationRequest::new(
            "llm_gateway",
            ValidationTarget::LlmResponse,
            "Tỷ lệ hoàn thành đạt 150%, vượt mục tiêu đề ra.",
        );
        let result = engine.validate(req).await;
        // percentage warning is non-blocking so passed = true
        assert!(result.passed);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule_id == "percentage_range"));
    }

    #[tokio::test]
    async fn test_content_length_exceeded() {
        let engine = default_engine().await;
        let long_text = "x".repeat(9_000); // exceeds default 8_000 char limit
        let req = ValidationRequest::new("llm_gateway", ValidationTarget::LlmResponse, long_text);
        let result = engine.validate(req).await;
        assert!(!result.passed);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule_id == "default_llm_length" && v.blocking));
    }

    #[tokio::test]
    async fn test_domain_policy_whitelist() {
        let policy = DomainPolicy {
            mode: "whitelist".into(),
            whitelist: vec!["*.gov.vn".into(), "cafef.vn".into()],
            blacklist: vec![],
        };
        let rule = DomainPolicyRule::try_new(&policy).unwrap();

        let req_allowed = ValidationRequest::new(
            "web_researcher",
            ValidationTarget::WebUrl {
                url: "https://mof.gov.vn/page".into(),
            },
            "https://mof.gov.vn/page",
        );
        let violations = rule.evaluate(&req_allowed).await;
        assert!(violations.is_empty(), "gov.vn should be allowed");

        let req_blocked = ValidationRequest::new(
            "web_researcher",
            ValidationTarget::WebUrl {
                url: "https://unknownsite.com/prices".into(),
            },
            "https://unknownsite.com/prices",
        );
        let violations = rule.evaluate(&req_blocked).await;
        assert!(!violations.is_empty(), "unknown domain should be blocked");
        assert!(violations[0].blocking);
    }

    #[tokio::test]
    async fn test_empty_content_validation() {
        let engine = default_engine().await;
        let req = ValidationRequest::new(
            "generic",
            ValidationTarget::Generic { label: "test".into() },
            "",
        );
        let result = engine.validate(req).await;
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[tokio::test]
    async fn test_missing_rules_config_fallback() {
        // Create an engine with an empty RuleFile to simulate missing or empty config
        let engine = default_engine().await;
        *engine.rule_file.write().await = None; // No rules loaded
        
        let req = ValidationRequest::new(
            "generic",
            ValidationTarget::Generic { label: "test".into() },
            "some content",
        );
        let result = engine.validate(req).await;
        // Should pass gracefully without panicking
        assert!(result.passed);
    }

    // ── LengthRule Tests ──────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_length_rule_applies_selectively() {
        let rule = LengthRule::new(
            "length_limit",
            10,
            vec![ValidationTarget::LlmResponse],
        );

        // Target it applies to
        let req1 = ValidationRequest::new("gateway", ValidationTarget::LlmResponse, "0123456789A");
        let result1 = rule.evaluate(&req1).await;
        assert_eq!(result1.len(), 1);
        assert!(result1[0].blocking);

        // Target it does not apply to
        let req2 = ValidationRequest::new("gateway", ValidationTarget::Generic { label: "any".into() }, "0123456789A");
        let result2 = rule.evaluate(&req2).await;
        assert!(result2.is_empty());
    }

    // ── BlockedPatternRule Tests ──────────────────────────────────────────────
    #[tokio::test]
    async fn test_blocked_pattern_rule_severity_and_blocking() {
        // Block action
        let rule_block = BlockedPatternRule::try_new(
            "block_bad",
            "Block Bad",
            "badword",
            "block",
            "reason block",
        ).unwrap();
        let req_block = ValidationRequest::new("gateway", ValidationTarget::LlmResponse, "this is a badword.");
        let res_block = rule_block.evaluate(&req_block).await;
        assert_eq!(res_block.len(), 1);
        assert!(res_block[0].blocking);
        assert_eq!(res_block[0].severity, Severity::Critical);

        // Flag action
        let rule_flag = BlockedPatternRule::try_new(
            "flag_bad",
            "Flag Bad",
            "flagword",
            "flag",
            "reason flag",
        ).unwrap();
        let req_flag = ValidationRequest::new("gateway", ValidationTarget::LlmResponse, "this is a flagword.");
        let res_flag = rule_flag.evaluate(&req_flag).await;
        assert_eq!(res_flag.len(), 1);
        assert!(!res_flag[0].blocking);
        assert_eq!(res_flag[0].severity, Severity::Warning);
    }

    // ── ValidationResult Helpers Tests ────────────────────────────────────────
    #[test]
    fn test_validation_result_helpers() {
        let violations = vec![
            Violation {
                rule_id: "rule1".into(),
                rule_name: "Rule 1".into(),
                severity: Severity::Info,
                message: "msg1".into(),
                blocking: false,
                suggestion: None,
            },
            Violation {
                rule_id: "rule2".into(),
                rule_name: "Rule 2".into(),
                severity: Severity::Warning,
                message: "msg2".into(),
                blocking: false,
                suggestion: None,
            },
            Violation {
                rule_id: "rule3".into(),
                rule_name: "Rule 3".into(),
                severity: Severity::Critical,
                message: "msg3".into(),
                blocking: true,
                suggestion: None,
            },
        ];

        let result = ValidationResult {
            request_id: uuid::Uuid::new_v4(),
            agent_id: "test".into(),
            target: ValidationTarget::LlmResponse,
            passed: false,
            violations,
            sanitized_content: "".into(),
            requires_human_approval: false,
            approval_level: None,
            validated_at: chrono::Utc::now(),
            duration_ms: 10,
        };

        assert_eq!(result.max_severity(), Some(&Severity::Critical));
        
        let blocking = result.blocking_violations();
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0].rule_id, "rule3");
    }

    // ── ValidationRequest Metadata Tests ──────────────────────────────────────
    #[test]
    fn test_validation_request_metadata() {
        let req = ValidationRequest::new("gateway", ValidationTarget::LlmResponse, "test")
            .with_metadata(serde_json::json!({"key": "value"}));
        
        assert!(req.metadata.is_some());
        assert_eq!(req.metadata.unwrap()["key"], "value");
    }
}
