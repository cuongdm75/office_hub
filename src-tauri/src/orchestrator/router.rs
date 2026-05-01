//! router.rs — Task Router
//!
//! Nhận một `Intent` đã được phân loại từ `IntentParser`, chọn Agent phù hợp,
//! xây dựng `AgentRequest`, gửi đến Agent và trả về `AgentResponse` chuẩn hoá.
//!
//! ## Luồng xử lý
//!
//! ```text
//! Intent
//!   │
//!   ▼
//! RouteSelector ──► AgentRegistry (kiểm tra agent idle/busy)
//!   │
//!   ▼
//! AgentRequest (build từ intent + session context)
//!   │
//!   ├──► AnalystAgent        (Excel COM)
//!   ├──► OfficeMasterAgent   (Word / PPT COM)
//!   ├──► WebResearcherAgent  (UI Automation)
//!   ├──► ConverterAgent      (MCP skill builder)
//!   └──► McpAgent            (dynamic MCP server)
//!   │
//!   ▼
//! AgentResponse
//!   │
//!   ▼
//! RuleEngine.validate()  ──► nếu pass → trả về caller
//!                            nếu fail → reject hoặc retry
//! ```

use std::{
    collections::HashMap,
    sync::Arc,
};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tracing::info;
use uuid::Uuid;

use crate::{
    agents::{Agent, AgentId, AgentRegistry, AgentStatusInfo},
    orchestrator::{
        intent::{Intent, IntentCategory},
        rule_engine::RuleEngine,
        session::Session, RouteDecision,
    },
    AppError, AppResult,
};

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait – mọi agent đều phải implement trait này
// ─────────────────────────────────────────────────────────────────────────────

/// Loại agent trong hệ thống.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// Phân tích và thao tác Excel qua COM Automation.
    Analyst,
    /// Tạo và chỉnh sửa Word / PowerPoint qua COM Automation.
    OfficeMaster,
    /// Trích xuất dữ liệu từ trình duyệt qua Windows UI Automation.
    WebResearcher,
    /// Tự học kỹ năng mới và đóng gói thành MCP Server.
    Converter,
    /// Gọi MCP Server bên ngoài (dynamic plugin).
    Mcp(String), // server_id
    /// Xử lý Outlook email & calendar
    Outlook,
    /// Quét và tổng hợp dữ liệu folder
    FolderScanner,
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentKind::Analyst => write!(f, "AnalystAgent"),
            AgentKind::OfficeMaster => write!(f, "OfficeMasterAgent"),
            AgentKind::WebResearcher => write!(f, "WebResearcherAgent"),
            AgentKind::Converter => write!(f, "ConverterAgent"),
            AgentKind::Mcp(id) => write!(f, "McpAgent({id})"),
            AgentKind::Outlook => write!(f, "OutlookAgent"),
            AgentKind::FolderScanner => write!(f, "FolderScannerAgent"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AgentRequest / AgentResponse
// ─────────────────────────────────────────────────────────────────────────────

/// Request gửi tới một Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    /// Unique ID cho request này (dùng cho audit log và HITL tracking).
    pub request_id: Uuid,

    /// Agent đích.
    pub target_agent: AgentKind,

    /// Intent gốc của người dùng.
    pub intent: Intent,

    /// Session hiện tại (context: lịch sử hội thoại, file đang mở…).
    pub session: Arc<Session>,

    /// Dữ liệu đầu vào bổ sung (VD: đường dẫn file, dữ liệu từ Web Researcher…).
    pub payload: serde_json::Value,

    /// Liệu tác vụ này có cần Human-in-the-Loop approval không.
    pub requires_hitl: bool,

    /// Thời điểm tạo request.
    pub created_at: DateTime<Utc>,

    /// Thời gian timeout tối đa (giây).
    pub _timeout_secs: u64,
}

impl AgentRequest {
    pub fn new(
        intent: Intent,
        session: Arc<Session>,
        target_agent: AgentKind,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            target_agent,
            intent,
            session,
            payload,
            requires_hitl: false,
            created_at: Utc::now(),
            _timeout_secs: 120,
        }
    }

    pub fn with_hitl(mut self) -> Self {
        self.requires_hitl = true;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self._timeout_secs = secs;
        self
    }
}

/// Response trả về từ Agent sau khi xử lý xong.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// ID của request đã xử lý.
    pub request_id: Uuid,

    /// Agent đã xử lý.
    pub agent_kind: AgentKind,

    /// Nội dung trả lời dạng văn bản (hiển thị trong Chat Pane).
    pub content: String,

    /// Dữ liệu có cấu trúc (cho workflow downstream sử dụng).
    pub data: Option<serde_json::Value>,

    /// Trạng thái xử lý.
    pub status: AgentResponseStatus,

    /// Thông tin grounding – bằng chứng xác minh dữ liệu (screenshot path, URL…).
    pub grounding: Vec<GroundingEvidence>,

    /// Số token LLM đã tiêu thụ.
    pub tokens_used: Option<u32>,

    /// Thời gian xử lý.
    pub duration_ms: u64,

    /// Thời điểm hoàn thành.
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentResponseStatus {
    Success,
    PartialSuccess,
    RequiresApproval,
    Failed,
    TimedOut,
}

/// Bằng chứng grounding đính kèm vào AgentResponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingEvidence {
    /// Loại bằng chứng.
    pub kind: GroundingKind,
    /// Mô tả.
    pub description: String,
    /// Đường dẫn file hoặc URL.
    pub reference: String,
    /// Timestamp thu thập.
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundingKind {
    Screenshot,
    ExcelCellValue,
    WebUrl,
    DocumentBookmark,
    AuditLog,
}

// ─────────────────────────────────────────────────────────────────────────────
// Note: Agent trait is defined in crate::agents::Agent
// We use the canonical Agent trait from agents/mod.rs
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Routing table entry
// ─────────────────────────────────────────────────────────────────────────────

/// Ánh xạ một IntentCategory sang Agent và chiến lược routing.
#[derive(Debug, Clone)]
struct RouteEntry {
    /// Agent chính xử lý intent này.
    primary_agent: AgentKind,
    /// Agent dự phòng nếu primary bận hoặc lỗi.
    _fallback_agent: Option<AgentKind>,
    /// Intent này có cần HITL không (override rule_engine).
    force_hitl: bool,
    /// Timeout riêng cho loại intent này (giây).
    _timeout_secs: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────────────────────────────────────

/// Số lượng tác vụ agent có thể chạy đồng thời.
const MAX_CONCURRENT_AGENT_TASKS: usize = 4;

/// Bộ điều phối tác vụ: nhận `Intent`, chọn `Agent`, thực thi và trả về kết quả.
pub struct Router {
    /// Registry chứa tất cả agents đã đăng ký.
    agents: Arc<DashMap<AgentKind, Arc<dyn Agent>>>,

    /// Bảng routing: IntentCategory → RouteEntry.
    /// STUB: Currently uses simplified routing
    routing_table: HashMap<IntentCategory, RouteEntry>,

    /// Rule engine để validate output trước khi trả về.
    /// Note: Full validation will be implemented in Phase 1
    _rule_engine: Arc<RuleEngine>,

    /// Semaphore kiểm soát số tác vụ đồng thời.
    concurrency_limit: Arc<Semaphore>,

    /// Số liệu thống kê: agent_kind → AgentStatusInfo.
    stats: Arc<DashMap<AgentKind, AgentStatusInfo>>,
}

impl Router {
    /// Khởi tạo Router với routing table mặc định.
    pub fn new(_rule_engine: Arc<RuleEngine>) -> Self {
        let routing_table = Self::build_default_routing_table();
        Self {
            agents: Arc::new(DashMap::new()),
            routing_table,
            _rule_engine,
            concurrency_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_AGENT_TASKS)),
            stats: Arc::new(DashMap::new()),
        }
    }

    // ── Quản lý Agent registry ───────────────────────────────────────────────

    /// Đăng ký một Agent vào registry.
    pub fn register_agent(&self, agent: Arc<dyn Agent>) {
        // STUB: Use Analyst as default kind - full routing in Phase 1
        let kind = AgentKind::Analyst;
        let status = AgentStatusInfo {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            status: "idle".to_string(),
            last_used: None,
            error_count: 0,
            total_tasks: 0,
            avg_duration_ms: 0.0,
            capabilities: agent.supported_actions(),
        };
        self.stats.insert(kind.clone(), status);
        self.agents.insert(kind.clone(), agent);
        info!(agent = %kind, "Agent registered");
    }

    /// Huỷ đăng ký một Agent.
    pub fn unregister_agent(&self, kind: &AgentKind) {
        self.agents.remove(kind);
        self.stats.remove(kind);
        info!(agent = %kind, "Agent unregistered");
    }

    /// Trả về danh sách AgentStatusInfo cho tất cả agents đã đăng ký.
    pub fn list_agent_statuses(&self) -> Vec<AgentStatusInfo> {
        self.stats
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    // ── Routing chính ────────────────────────────────────────────────────────

    /// Dispatch một `Intent` đến Agent phù hợp.
    ///
    /// STUB for Phase 1 - Full implementation in Phase 1
    /// This is a placeholder that returns a stub response.
    pub async fn dispatch(
        &self,
        intent: Intent,
        session: Arc<Session>,
        registry: &AgentRegistry,
    ) -> AppResult<AgentResponse> {
        let route = self.resolve(&intent, registry).await?;
        let agent_arc = self.resolve_agent(registry, &route.agent_id).await?;

        let _permit = self.concurrency_limit.acquire().await.map_err(|e| {
            AppError::Orchestrator(format!("Failed to acquire concurrency permit: {}", e))
        })?;

        let _payload = self.build_payload(&intent, &session).await?;

        // Start duration timer
        let start_time = std::time::Instant::now();

        // Convert AgentRequest semantics to AgentTask
        let task = crate::orchestrator::AgentTask {
            task_id: Uuid::new_v4().to_string(),
            action: route.action.clone(),
            intent: intent.clone(),
            message: "".to_string(),
            context_file: session.active_file_path.clone(),
            session_id: session.id.clone(),
            parameters: route.parameters.clone(),
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        };

        let mut agent_guard = agent_arc.write().await;
        
        let agent_kind = match route.agent_id.to_string().as_str() {
            "analyst" => AgentKind::Analyst,
            "office_master" => AgentKind::OfficeMaster,
            "web_researcher" => AgentKind::WebResearcher,
            "converter" => AgentKind::Converter,
            "outlook" => AgentKind::Outlook,
            "folder_scanner" => AgentKind::FolderScanner,
            other => {
                if other.starts_with("mcp_") {
                    AgentKind::Mcp(other.to_string())
                } else {
                    AgentKind::Analyst
                }
            }
        };

        self.set_agent_status(&agent_kind, "busy");

        let agent_result = agent_guard.execute(task).await;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        match agent_result {
            Ok(output) => {
                self.record_success(&agent_kind, duration_ms);
                
                // Construct response
                let resp = AgentResponse {
                    request_id: Uuid::new_v4(),
                    agent_kind: agent_kind.clone(),
                    content: output.content.clone(),
                    data: output.metadata.clone(),
                    status: AgentResponseStatus::Success,
                    grounding: vec![],
                    tokens_used: output.tokens_used,
                    duration_ms,
                    completed_at: Utc::now(),
                };

                // Validate with rule engine
                let _val_req = crate::orchestrator::rule_engine::ValidationRequest::new(
                    agent_kind.to_string(),
                    crate::orchestrator::rule_engine::ValidationTarget::LlmResponse,
                    output.content,
                );
                // self.rule_engine.validate(val_req).await; 

                Ok(resp)
            }
            Err(e) => {
                self.record_error(&agent_kind, duration_ms);
                Err(AppError::Agent {
                    agent: route.agent_id.to_string(),
                    message: e.to_string(),
                })
            }
        }
    }

    /// Dispatch nhiều intents độc lập song song (multi-agent coordination).
    ///
    /// Tất cả intents chạy đồng thời, kết quả được gộp lại.
    /// Nếu bất kỳ intent nào fail, trả về lỗi đó (fail-fast).
    pub async fn dispatch_parallel(
        &self,
        tasks: Vec<(Intent, Arc<Session>)>,
        registry: &AgentRegistry,
    ) -> AppResult<Vec<AgentResponse>> {
        use futures::future::try_join_all;

        info!(task_count = tasks.len(), "Dispatching parallel tasks");

        let futures: Vec<_> = tasks
            .into_iter()
            .map(|(intent, session)| self.dispatch(intent, session, registry))
            .collect();

        try_join_all(futures).await
    }

    /// Dispatch một chuỗi intents tuần tự, output của step trước
    /// được inject vào payload của step sau.
    pub async fn dispatch_pipeline(
        &self,
        steps: Vec<(Intent, Arc<Session>)>,
        registry: &AgentRegistry,
    ) -> AppResult<AgentResponse> {
        let mut last_response: Option<AgentResponse> = None;
        let mut accumulated_data = serde_json::json!({});

        for (intent, session) in steps {
            // Note: Pipeline execution passes data between steps via session context
            // The previous step's data is available in session.metadata for the next step

            let response = self.dispatch(intent, session, registry).await?;
            if let Some(ref data) = response.data {
                // Tích luỹ data qua các bước
                if let serde_json::Value::Object(map) = data {
                    if let serde_json::Value::Object(acc) = &mut accumulated_data {
                        acc.extend(map.clone());
                    }
                }
            }
            last_response = Some(response);
        }

        last_response
            .ok_or_else(|| AppError::Orchestrator("Pipeline had no steps to execute".to_string()))
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    async fn resolve_agent(
        &self,
        registry: &AgentRegistry,
        agent_id: &AgentId,
    ) -> AppResult<Arc<tokio::sync::RwLock<Box<dyn Agent>>>> {
        if let Some(agent) = registry.get(agent_id) {
            return Ok(agent);
        }

        Err(AppError::Agent {
            agent: agent_id.to_string(),
            message: "Agent not found in registry".to_string(),
        })
    }

    /// Xây dựng payload JSON từ Intent và Session context.
    async fn build_payload(
        &self,
        intent: &Intent,
        session: &Session,
    ) -> AppResult<serde_json::Value> {
        // Serialize intent directly and wrap with session context
        let intent_json = serde_json::to_value(intent)
            .unwrap_or_else(|_| serde_json::json!({ "error": "failed to serialize intent" }));

        let payload = serde_json::json!({
            "intent":           intent_json,
            "intent_category":  format!("{:?}", IntentCategory::from(intent)),
            "active_file":      session.active_file_path,
            "session_summary":  session.context_summary,
            "language":         session.language.clone(),
        });
        Ok(payload)
    }

    // ── Routing table mặc định ───────────────────────────────────────────────

    /// Xây dựng bảng routing mặc định: ánh xạ mỗi IntentCategory → RouteEntry.
    fn build_default_routing_table() -> HashMap<IntentCategory, RouteEntry> {
        let mut table = HashMap::new();

        // ── Excel-related intents ─────────────────────────────────────────
        table.insert(
            IntentCategory::ExcelAnalyze,
            RouteEntry {
                primary_agent: AgentKind::Analyst,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 120,
            },
        );

        table.insert(
            IntentCategory::ExcelWrite,
            RouteEntry {
                primary_agent: AgentKind::Analyst,
                _fallback_agent: None,
                force_hitl: false, // rule_engine sẽ quyết định nếu cần
                _timeout_secs: 60,
            },
        );

        table.insert(
            IntentCategory::ExcelFormula,
            RouteEntry {
                primary_agent: AgentKind::Analyst,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 60,
            },
        );

        table.insert(
            IntentCategory::ExcelVba,
            RouteEntry {
                primary_agent: AgentKind::Analyst,
                _fallback_agent: None,
                force_hitl: true, // VBA execution luôn cần HITL
                _timeout_secs: 180,
            },
        );

        table.insert(
            IntentCategory::ExcelPowerQuery,
            RouteEntry {
                primary_agent: AgentKind::Analyst,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 120,
            },
        );

        // ── Word-related intents ─────────────────────────────────────────
        table.insert(
            IntentCategory::WordCreate,
            RouteEntry {
                primary_agent: AgentKind::OfficeMaster,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 120,
            },
        );

        table.insert(
            IntentCategory::WordEdit,
            RouteEntry {
                primary_agent: AgentKind::OfficeMaster,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 90,
            },
        );

        table.insert(
            IntentCategory::WordFormat,
            RouteEntry {
                primary_agent: AgentKind::OfficeMaster,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 60,
            },
        );

        // ── PowerPoint-related intents ───────────────────────────────────
        table.insert(
            IntentCategory::PptCreate,
            RouteEntry {
                primary_agent: AgentKind::OfficeMaster,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 120,
            },
        );

        table.insert(
            IntentCategory::PptEdit,
            RouteEntry {
                primary_agent: AgentKind::OfficeMaster,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 90,
            },
        );

        // ── Web research intents ─────────────────────────────────────────
        table.insert(
            IntentCategory::WebExtract,
            RouteEntry {
                primary_agent: AgentKind::WebResearcher,
                _fallback_agent: None,
                force_hitl: false, // navigation riêng lẻ không cần HITL
                _timeout_secs: 60,
            },
        );

        table.insert(
            IntentCategory::WebNavigate,
            RouteEntry {
                primary_agent: AgentKind::WebResearcher,
                _fallback_agent: None,
                force_hitl: true, // điều hướng browser luôn cần HITL
                _timeout_secs: 60,
            },
        );

        table.insert(
            IntentCategory::WebFormFill,
            RouteEntry {
                primary_agent: AgentKind::WebResearcher,
                _fallback_agent: None,
                force_hitl: true, // điền form tuyệt đối cần HITL
                _timeout_secs: 120,
            },
        );

        // ── Cross-agent (Web → Office) ───────────────────────────────────
        // Các intent này do Orchestrator phân rã thành sub-intents,
        // sau đó dispatch_pipeline() xử lý tuần tự.
        table.insert(
            IntentCategory::WebToExcel,
            RouteEntry {
                primary_agent: AgentKind::WebResearcher, // step 1
                _fallback_agent: None,
                force_hitl: true,
                _timeout_secs: 300,
            },
        );

        table.insert(
            IntentCategory::WebToWord,
            RouteEntry {
                primary_agent: AgentKind::WebResearcher, // step 1
                _fallback_agent: None,
                force_hitl: true,
                _timeout_secs: 300,
            },
        );

        // ── MCP / Converter intents ──────────────────────────────────────
        table.insert(
            IntentCategory::McpToolCall,
            RouteEntry {
                primary_agent: AgentKind::Converter,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 60,
            },
        );

        table.insert(
            IntentCategory::SkillLearn,
            RouteEntry {
                primary_agent: AgentKind::Converter,
                _fallback_agent: None,
                force_hitl: true, // cài đặt MCP server mới cần HITL
                _timeout_secs: 300,
            },
        );

        table.insert(
            IntentCategory::WorkflowEdit,
            RouteEntry {
                primary_agent: AgentKind::Converter,
                _fallback_agent: None,
                force_hitl: true,
                _timeout_secs: 300,
            },
        );

        // ── Folder & Email ───────────────────────────────────────────────
        table.insert(
            IntentCategory::FolderScan,
            RouteEntry {
                primary_agent: AgentKind::FolderScanner,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 300,
            },
        );

        table.insert(
            IntentCategory::OutlookAction,
            RouteEntry {
                primary_agent: AgentKind::Outlook,
                _fallback_agent: None,
                force_hitl: true,
                _timeout_secs: 120,
            },
        );

        // ── Chit-chat / General ──────────────────────────────────────────
        table.insert(
            IntentCategory::GeneralChat,
            RouteEntry {
                primary_agent: AgentKind::Analyst, // dùng tạm; Orchestrator xử lý trực tiếp
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 30,
            },
        );

        table.insert(
            IntentCategory::Unknown,
            RouteEntry {
                primary_agent: AgentKind::Analyst,
                _fallback_agent: None,
                force_hitl: false,
                _timeout_secs: 30,
            },
        );

        table
    }

    // ── Stats helpers ─────────────────────────────────────────────────────────

    fn set_agent_status(&self, kind: &AgentKind, status: &str) {
        if let Some(mut entry) = self.stats.get_mut(kind) {
            entry.status = status.to_string();
            if status == "busy" {
                entry.last_used = Some(Utc::now());
            }
        }
    }

    fn record_success(&self, kind: &AgentKind, duration_ms: u64) {
        if let Some(mut entry) = self.stats.get_mut(kind) {
            entry.total_tasks += 1;
            let n = entry.total_tasks as f64;
            entry.avg_duration_ms = ((n - 1.0) * entry.avg_duration_ms + duration_ms as f64) / n;
        }
    }

    fn record_error(&self, kind: &AgentKind, duration_ms: u64) {
        if let Some(mut entry) = self.stats.get_mut(kind) {
            entry.total_tasks += 1;
            entry.error_count += 1;
            entry.status = "idle".to_string();
            let n = entry.total_tasks as f64;
            entry.avg_duration_ms = ((n - 1.0) * entry.avg_duration_ms + duration_ms as f64) / n;
        }
    }

    pub async fn resolve(
        &self,
        intent: &Intent,
        _registry: &AgentRegistry,
    ) -> AppResult<RouteDecision> {
        let category = IntentCategory::from(intent);
        let route_entry = self.routing_table.get(&category).cloned().unwrap_or(RouteEntry {
            primary_agent: AgentKind::Analyst,
            _fallback_agent: None,
            force_hitl: false,
            _timeout_secs: 30,
        });

        let agent_id = match route_entry.primary_agent {
            AgentKind::Analyst => AgentId::analyst(),
            AgentKind::OfficeMaster => AgentId::office_master(),
            AgentKind::WebResearcher => AgentId::web_researcher(),
            AgentKind::Converter => AgentId::converter(),
            AgentKind::Mcp(id) => AgentId::custom(id),
            AgentKind::Outlook => AgentId::custom("outlook".to_string()),
            AgentKind::FolderScanner => AgentId::custom("folder_scanner".to_string()),
        };

        Ok(RouteDecision {
            agent_id,
            action: intent.action_str().to_string(),
            parameters: HashMap::new(),
            requires_hitl: route_entry.force_hitl,
            confidence: 0.8,
        })
    }
}


