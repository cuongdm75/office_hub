// ============================================================================
// Office Hub – agents/web_researcher/mod.rs
//
// Web Researcher Agent – Obscura Engine (Phase 5)
// Replaces UIA/Edge dependency with embedded headless browser (obscura.exe)
// ============================================================================

pub mod browser_engine;
pub mod uia;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument};

use self::browser_engine::BrowserEngine;
use crate::agents::{Agent, AgentId, AgentStatus};
use crate::orchestrator::{AgentOutput, AgentTask};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserKind {
    Obscura,
    MicrosoftEdge, // Legacy UIA (kept for compat)
    GoogleChrome,  // Legacy UIA (kept for compat)
    Auto,
}

impl std::fmt::Display for BrowserKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrowserKind::Obscura => write!(f, "Obscura (headless)"),
            BrowserKind::MicrosoftEdge => write!(f, "Microsoft Edge"),
            BrowserKind::GoogleChrome => write!(f, "Google Chrome"),
            BrowserKind::Auto => write!(f, "Auto-detect"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedData {
    pub source_url: Option<String>,
    pub page_title: Option<String>,
    pub extraction_type: String,
    pub data: serde_json::Value,
    pub captured_at: DateTime<Utc>,
    pub browser: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserStatus {
    Ready,
    Unavailable(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiaAuditEntry {
    pub id: String,
    pub action_type: String,
    pub target_element: Option<String>,
    pub target_url: Option<String>,
    pub approved_by: Option<String>,
    pub performed_at: DateTime<Utc>,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebResearcherConfig {
    pub preferred_browser: BrowserKind,
    pub screenshot_grounding: bool,
    pub screenshot_dir: String,
    pub require_approval_for_navigation: bool,
    pub require_approval_for_interaction: bool,
    pub max_rows_per_table: usize,
    pub max_pages_to_navigate: u32,
    pub extraction_timeout_seconds: u64,
    pub allowed_domains: Vec<String>,
    pub blocked_actions: Vec<String>,
}

impl Default for WebResearcherConfig {
    fn default() -> Self {
        Self {
            preferred_browser: BrowserKind::Obscura,
            screenshot_grounding: false, // Obscura doesn't need screenshot grounding
            screenshot_dir: "$APPDATA/office-hub/grounding".to_string(),
            require_approval_for_navigation: false, // Obscura is headless, no approval needed
            require_approval_for_interaction: false,
            max_rows_per_table: 10_000,
            max_pages_to_navigate: 10,
            extraction_timeout_seconds: 30,
            allowed_domains: vec![], // Empty = allow all (Obscura handles its own safety)
            blocked_actions: vec![
                "form_submit".to_string(),
                "file_download".to_string(),
                "authentication".to_string(),
                "payment".to_string(),
            ],
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WebResearcherAgent
// ─────────────────────────────────────────────────────────────────────────────

pub struct WebResearcherAgent {
    id: AgentId,
    config: WebResearcherConfig,
    status: AgentStatus,
    browser_status: BrowserStatus,
    audit_log: Vec<UiaAuditEntry>,
    total_tasks: u64,
    error_count: u32,
    last_used: Option<DateTime<Utc>>,
    engine: Option<BrowserEngine>,
}

impl WebResearcherAgent {
    pub fn new(config: WebResearcherConfig) -> Self {
        info!("WebResearcherAgent created (Phase 5 – Obscura engine)");
        Self {
            id: AgentId::web_researcher(),
            config,
            status: AgentStatus::Idle,
            browser_status: BrowserStatus::Unavailable("Not initialised".to_string()),
            audit_log: Vec::new(),
            total_tasks: 0,
            error_count: 0,
            last_used: None,
            engine: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(WebResearcherConfig::default())
    }

    /// Ensure the browser engine is ready.
    fn ensure_engine(&mut self) -> anyhow::Result<&BrowserEngine> {
        if self.engine.is_none() {
            match BrowserEngine::new() {
                Ok(engine) => {
                    self.engine = Some(engine);
                    self.browser_status = BrowserStatus::Ready;
                }
                Err(e) => {
                    self.browser_status = BrowserStatus::Unavailable(e.to_string());
                    return Err(e);
                }
            }
        }
        Ok(self.engine.as_ref().unwrap())
    }

    pub fn is_domain_allowed(&self, url: &str) -> bool {
        if self.config.allowed_domains.is_empty() {
            return true;
        }
        let url_lower = url.to_lowercase();
        self.config.allowed_domains.iter().any(|pattern| {
            let p = pattern.to_lowercase();
            if let Some(suffix) = p.strip_prefix("*.") {
                url_lower.contains(suffix)
            } else {
                url_lower.contains(&p)
            }
        })
    }

    pub fn log_audit_action(
        &mut self,
        action_type: &str,
        target_element: Option<&str>,
        target_url: Option<&str>,
    ) {
        self.audit_log.push(UiaAuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            action_type: action_type.to_string(),
            target_element: target_element.map(String::from),
            target_url: target_url.map(String::from),
            approved_by: None,
            performed_at: Utc::now(),
            success: false,
            error: None,
        });
    }

    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "agent":          "web_researcher",
            "version":        "0.5.0-obscura",
            "engine":         "obscura",
            "status":         self.status.to_string(),
            "browserStatus":  self.browser_status,
            "totalTasks":     self.total_tasks,
            "errorCount":     self.error_count,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl Agent for WebResearcherAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn name(&self) -> &str {
        "Web Researcher Agent"
    }

    fn description(&self) -> &str {
        "Trích xuất nội dung trang web bằng Obscura headless browser engine (V8, stealth mode)."
    }

    fn version(&self) -> &str {
        "0.5.0-obscura"
    }

    fn supported_actions(&self) -> Vec<String> {
        crate::agent_actions![
            "navigate_to_url",
            "fetch_page",
            "fetch_links",
            "eval_js",
            "extract_text",
            "extract_table",
            "search_google",
            "web_download_file"
        ]
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "fetch_page".to_string(),
                description: "Truy cập và trích xuất nội dung văn bản từ một trang web. Tham số: `url`, `prompt` (tùy chọn, để LLM tóm tắt).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" },
                        "prompt": { "type": "string" }
                    },
                    "required": ["url"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "fetch_links".to_string(),
                description: "Trích xuất danh sách các liên kết từ một trang web. Tham số: `url`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "eval_js".to_string(),
                description: "Thực thi JavaScript trên trang web. Tham số: `url`, `js`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" },
                        "js": { "type": "string" }
                    },
                    "required": ["url", "js"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "search_google".to_string(),
                description: "Tìm kiếm Google và lấy kết quả top đầu. Tham số: `query`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "web_download_file".to_string(),
                description: "Tải file (ví dụ: hình ảnh minh họa) từ URL web về máy. Tham số: `url`, `filename` (tùy chọn). Trả về đường dẫn file đã tải trên đĩa cứng.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" },
                        "filename": { "type": "string" }
                    },
                    "required": ["url"]
                }),
                tags: vec![],
            }
        ]
    }

    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        info!("WebResearcherAgent initialising (Obscura engine)…");
        // Eagerly initialise engine to fail-fast
        match BrowserEngine::new() {
            Ok(engine) => {
                self.engine = Some(engine);
                self.browser_status = BrowserStatus::Ready;
                info!("WebResearcherAgent: Obscura engine ready ✓");
            }
            Err(e) => {
                error!("WebResearcherAgent: Obscura engine not found — {}", e);
                self.browser_status = BrowserStatus::Unavailable(e.to_string());
                // Non-fatal: agent still starts, but will error on first use
            }
        }
        self.status = AgentStatus::Idle;
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        info!("WebResearcherAgent shutting down");
        self.engine = None;
        self.browser_status = BrowserStatus::Unavailable("Shut down".to_string());
        self.status = AgentStatus::Idle;
        Ok(())
    }

    #[instrument(skip(self, task), fields(task_id = %task.task_id))]
    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        self.total_tasks += 1;
        self.last_used = Some(Utc::now());
        self.status = AgentStatus::Busy;

        let result = match task.action.as_str() {
            "navigate_to_url" | "fetch_page" => self.handle_fetch_page(&task).await,
            "fetch_links" => self.handle_fetch_links(&task).await,
            "eval_js" => self.handle_eval_js(&task).await,
            "extract_text" => self.handle_fetch_page(&task).await, // alias
            "extract_table" => self.handle_fetch_page(&task).await, // full page for now
            "search_google" => self.handle_search_google(&task).await,
            "web_download_file" => self.handle_download_file(&task).await,
            unknown => {
                self.error_count += 1;
                Err(anyhow::anyhow!(
                    "WebResearcherAgent does not support action '{}'",
                    unknown
                ))
            }
        };

        if result.is_err() {
            self.error_count += 1;
        }
        self.status = AgentStatus::Idle;
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Action handlers
// ─────────────────────────────────────────────────────────────────────────────

impl WebResearcherAgent {
    /// Fetch a page via Obscura and optionally run it through the LLM.
    async fn handle_fetch_page(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let url = task
            .parameters
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://google.com")
            .to_string();

        self.log_audit_action("fetch_page", None, Some(&url));

        if !self.is_domain_allowed(&url) {
            anyhow::bail!("Domain không được phép theo config: {}", url);
        }

        // Fetch via Obscura
        let engine = self.ensure_engine()?;
        let result = engine.fetch_text(&url).await?;

        let page_title = result.title.clone().unwrap_or_else(|| url.clone());
        let raw_text = result.content.clone();

        // Optional LLM synthesis
        let user_prompt = task
            .parameters
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("Hãy tóm tắt nội dung chính trên trang web này.");

        let final_content = if let Some(llm_arc) = &task.llm_gateway {
            let llm_prompt = format!(
                "Nguồn: {}\nTiêu đề: {}\n\nYêu cầu: {}\n\nNội dung trang:\n---\n{}\n---",
                url, page_title, user_prompt, raw_text
            );
            let llm = llm_arc.read().await;
            let req = crate::llm_gateway::LlmRequest::new(vec![
                crate::llm_gateway::LlmMessage::system(
                    "Bạn là Web Researcher Agent. Phân tích và trích xuất thông tin từ nội dung trang web. Trả lời ngắn gọn, chính xác bằng tiếng Việt nếu được yêu cầu.".to_string()
                ),
                crate::llm_gateway::LlmMessage::user(llm_prompt),
            ]).with_temperature(0.2);

            match llm.complete(req).await {
                Ok(resp) => resp.content,
                Err(e) => {
                    error!("LLM error in WebResearcherAgent: {}", e);
                    format!(
                        "Đã fetch trang thành công nhưng LLM lỗi: {}\n\nNội dung thô:\n{}",
                        e,
                        &raw_text[..raw_text.len().min(2000)]
                    )
                }
            }
        } else {
            // No LLM: return raw text directly
            format!("# {}\n\n{}", page_title, raw_text)
        };

        Ok(AgentOutput {
            content: final_content,
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action":    "fetch_page",
                "url":       url,
                "title":     page_title,
                "engine":    "obscura",
                "text_len":  raw_text.len(),
            })),
        })
    }

    /// Fetch all links from a page.
    async fn handle_fetch_links(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let url = task
            .parameters
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://google.com")
            .to_string();

        self.log_audit_action("fetch_links", None, Some(&url));

        let engine = self.ensure_engine()?;
        let links = engine.fetch_links(&url).await?;

        let links_text: String = links
            .iter()
            .map(|(href, text)| format!("- [{}]({})", text, href))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(AgentOutput {
            content: format!("## Links từ {}\n\n{}", url, links_text),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action":     "fetch_links",
                "url":        url,
                "link_count": links.len(),
                "engine":     "obscura",
            })),
        })
    }

    /// Execute JavaScript on a page and return the result.
    async fn handle_eval_js(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let url = task
            .parameters
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("about:blank")
            .to_string();
        let js = task
            .parameters
            .get("js")
            .and_then(|v| v.as_str())
            .unwrap_or("document.title")
            .to_string();

        self.log_audit_action("eval_js", None, Some(&url));

        let engine = self.ensure_engine()?;
        let result = engine.eval_js(&url, &js).await?;

        Ok(AgentOutput {
            content: result.content,
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action":  "eval_js",
                "url":     url,
                "js":      js,
                "engine":  "obscura",
            })),
        })
    }

    /// Search Google and fetch the top result (via Obscura, stealth mode).
    async fn handle_search_google(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let query = task
            .parameters
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        self.log_audit_action("search_google", None, Some(&query));

        let search_url = format!(
            "https://www.google.com/search?q={}",
            urlencoding::encode(&query)
        );

        let engine = self.ensure_engine()?;

        // Fetch search results page
        let result = engine.fetch_text(&search_url).await?;

        // Extract links to actual results
        let links = engine.fetch_links(&search_url).await.unwrap_or_default();
        let result_links: Vec<_> = links
            .into_iter()
            .filter(|(href, _)| {
                href.starts_with("http")
                    && !href.contains("google.com")
                    && !href.contains("youtube.com")
            })
            .take(5)
            .collect();

        let links_summary = result_links
            .iter()
            .enumerate()
            .map(|(i, (href, text))| format!("{}. [{}]({})", i + 1, text, href))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(AgentOutput {
            content: format!(
                "## Kết quả tìm kiếm: {}\n\n{}\n\n---\nNội dung tóm tắt:\n{}",
                query,
                links_summary,
                &result.content[..result.content.len().min(3000)]
            ),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action":   "search_google",
                "query":    query,
                "engine":   "obscura",
                "results":  result_links.len(),
            })),
        })
    }

    /// Tải một file từ web về thư mục tạm cục bộ (rất hữu ích để tải hình ảnh minh họa chèn vào Word/PPT).
    async fn handle_download_file(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let url = task
            .parameters
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: url"))?
            .to_string();

        let filename = task
            .parameters
            .get("filename")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("downloaded_{}.jpg", uuid::Uuid::new_v4()));

        self.log_audit_action("web_download_file", None, Some(&url));

        let temp_dir = std::env::temp_dir().join("office_hub_exports");
        let _ = std::fs::create_dir_all(&temp_dir);
        let save_path = temp_dir.join(filename);

        // Perform HTTP GET
        let response = reqwest::get(&url).await?;
        if !response.status().is_success() {
            anyhow::bail!("Tải file thất bại, HTTP Status: {}", response.status());
        }

        let bytes = response.bytes().await?;
        std::fs::write(&save_path, &bytes)?;

        Ok(AgentOutput {
            content: format!(
                "Đã tải file thành công và lưu tại đường dẫn: {}",
                save_path.to_string_lossy()
            ),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "web_download_file",
                "url": url,
                "saved_path": save_path.to_string_lossy(),
                "bytes": bytes.len()
            })),
        })
    }
}
