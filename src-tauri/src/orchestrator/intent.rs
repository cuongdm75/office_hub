// ============================================================================
// Office Hub – orchestrator/intent.rs
//
// Intent Schema & Classifier
//
// Trách nhiệm:
//   1. Định nghĩa toàn bộ taxonomy Intent (enum + structs)
//   2. Phân loại Intent từ raw user message (rule-based + LLM-assisted)
//   3. Trích xuất entities (file path, sheet name, URL, …) từ message
//   4. Tính Confidence Score để Orchestrator quyết định route hay hỏi thêm
//
// Flow:
//   raw message
//       │
//       ▼
//   FastClassifier (regex / keyword rules) ──► high confidence → Intent
//       │ low confidence
//       ▼
//   LlmClassifier (structured JSON prompt) ──► Intent + entities + confidence
//       │
//       ▼
//   IntentClassifyResult { intent, entities, confidence, clarification? }
// ============================================================================

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{llm_gateway::LlmGateway, orchestrator::session::Session, AppResult};

// ─────────────────────────────────────────────────────────────────────────────
// Intent Priority & Category (for Router compatibility)
// ─────────────────────────────────────────────────────────────────────────────

/// Priority level for an intent (used by router for scheduling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum IntentPriority {
    /// Normal priority, default for most intents.
    #[default]
    Normal,
    /// High priority, used for time-sensitive operations.
    High,
    /// Low priority, used for background tasks.
    Low,
}

/// Category of intent (used by router for routing decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentCategory {
    // Excel / Analyst Agent
    ExcelAnalyze,
    ExcelWrite,
    ExcelFormula,
    ExcelVba,
    ExcelPowerQuery,
    ExcelRead,
    ExcelAudit,
    ExcelMacro,

    // Word / Office Master Agent
    WordCreate,
    WordEdit,
    WordFormat,
    WordExtract,

    // PowerPoint / Office Master Agent
    PptCreate,
    PptEdit,
    PptFormat,
    PptConvertFrom,

    // Web Researcher Agent
    WebExtract,
    WebNavigate,
    WebFormFill,
    WebToExcel,
    WebToWord,
    WebSearch,
    WebScreenshot,

    // Folder Scanner Agent
    FolderScan,

    // Outlook Agent
    OutlookAction,

    // MCP / Converter Agent
    McpToolCall,
    McpInstall,
    McpList,
    SkillLearn,

    // Workflow Engine
    WorkflowTrigger,
    WorkflowStatus,
    WorkflowEdit,

    // Orchestrator / System
    GeneralChat,
    SystemConfig,
    HelpRequest,
    Ambiguous,
    Unknown,
}

impl From<&Intent> for IntentCategory {
    fn from(intent: &Intent) -> Self {
        match intent {
            Intent::ExcelRead(_) => IntentCategory::ExcelRead,
            Intent::ExcelWrite(_) => IntentCategory::ExcelWrite,
            Intent::ExcelFormula(_) => IntentCategory::ExcelFormula,
            Intent::ExcelPowerQuery(_) => IntentCategory::ExcelPowerQuery,
            Intent::ExcelMacro(_) => IntentCategory::ExcelMacro,
            Intent::ExcelAnalyze(_) => IntentCategory::ExcelAnalyze,
            Intent::ExcelAudit(_) => IntentCategory::ExcelAudit,

            Intent::WordCreate(_) => IntentCategory::WordCreate,
            Intent::WordEdit(_) => IntentCategory::WordEdit,
            Intent::WordFormat(_) => IntentCategory::WordFormat,
            Intent::WordExtract(_) => IntentCategory::WordExtract,

            Intent::PptCreate(_) => IntentCategory::PptCreate,
            Intent::PptEdit(_) => IntentCategory::PptEdit,
            Intent::PptFormat(_) => IntentCategory::PptFormat,
            Intent::PptConvertFrom(_) => IntentCategory::PptConvertFrom,

            Intent::WebExtractData(_) => IntentCategory::WebExtract,
            Intent::WebNavigate(_) => IntentCategory::WebNavigate,
            Intent::WebSearch(_) => IntentCategory::WebSearch,
            Intent::WebScreenshot(_) => IntentCategory::WebScreenshot,
            Intent::WebToExcel(_) => IntentCategory::WebToExcel,

            Intent::FolderScan(_) => IntentCategory::FolderScan,
            Intent::OutlookAction(_) => IntentCategory::OutlookAction,

            Intent::McpInstall(_) => IntentCategory::McpInstall,
            Intent::McpCallTool(_) => IntentCategory::McpToolCall,
            Intent::McpListServers => IntentCategory::McpList,

            Intent::WorkflowTrigger(_) => IntentCategory::WorkflowTrigger,
            Intent::WorkflowStatus(_) => IntentCategory::WorkflowStatus,
            Intent::WorkflowEdit(_) => IntentCategory::WorkflowEdit,

            Intent::GeneralChat(_) => IntentCategory::GeneralChat,
            Intent::SystemConfig(_) => IntentCategory::SystemConfig,
            Intent::HelpRequest(_) => IntentCategory::HelpRequest,
            Intent::Ambiguous(_) => IntentCategory::Ambiguous,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// IntentWithMeta – Wrapper for router compatibility
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper struct that adds metadata fields to Intent for router compatibility.
/// The router expects fields like .id, .raw_text, .category, .context, .priority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentWithMeta {
    /// Unique identifier for this intent instance
    pub id: String,
    /// Original raw text from the user
    pub raw_text: String,
    /// The actual intent enum
    pub intent: Intent,
    /// Category derived from the intent (for routing)
    pub category: IntentCategory,
    /// Priority level
    pub priority: IntentPriority,
    /// Optional context data (e.g., conversation history, active file)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

impl IntentWithMeta {
    /// Create a new IntentWithMeta from raw text and an Intent.
    pub fn new(raw_text: String, intent: Intent) -> Self {
        let category = IntentCategory::from(&intent);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            raw_text,
            intent,
            category,
            priority: IntentPriority::Normal,
            context: None,
        }
    }

    /// Set the priority level.
    pub fn with_priority(mut self, priority: IntentPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the context data.
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. INTENT TAXONOMY
// ─────────────────────────────────────────────────────────────────────────────

/// Toàn bộ Intent mà hệ thống hiểu được.
/// Mỗi variant có thể chứa metadata bổ sung (payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Intent {
    // ── Analyst Agent (Excel) ────────────────────────────────────────────────
    /// Đọc / truy vấn dữ liệu từ file Excel (không ghi)
    ExcelRead(ExcelReadPayload),
    /// Ghi / cập nhật giá trị ô hoặc dải ô Excel
    ExcelWrite(ExcelWritePayload),
    /// Tạo / chỉnh sửa công thức (XLOOKUP, SUMIF, LAMBDA, …)
    ExcelFormula(ExcelFormulaPayload),
    /// Xây dựng hoặc làm mới Power Query / Query M-code
    ExcelPowerQuery(ExcelPowerQueryPayload),
    /// Sinh hoặc chạy macro VBA / Office Scripts
    ExcelMacro(ExcelMacroPayload),
    /// Phân tích dữ liệu: thống kê, xu hướng, anomaly detection
    ExcelAnalyze(ExcelAnalyzePayload),
    /// Kiểm tra / audit công thức đang có trong workbook
    ExcelAudit(ExcelAuditPayload),

    // ── Office Master Agent (Word) ───────────────────────────────────────────
    /// Tạo tài liệu Word mới từ template hoặc từ đầu
    WordCreate(WordCreatePayload),
    /// Chỉnh sửa nội dung đoạn văn, bảng, section trong Word
    WordEdit(WordEditPayload),
    /// Định dạng: Styles, Sections, Cross-references, TOC
    WordFormat(WordFormatPayload),
    /// Trích xuất nội dung / số liệu từ file Word
    WordExtract(WordExtractPayload),

    // ── Office Master Agent (PowerPoint) ────────────────────────────────────
    /// Tạo bộ slide mới từ template hoặc outline
    PptCreate(PptCreatePayload),
    /// Thêm / chỉnh sửa / xoá slide, text box, hình ảnh
    PptEdit(PptEditPayload),
    /// Định dạng: Grid, Morph transition, Brand palette, layout
    PptFormat(PptFormatPayload),
    /// Chuyển đổi nội dung (Word / Markdown / JSON) thành slide
    PptConvertFrom(PptConvertFromPayload),

    // ── Web Researcher Agent (UIA) ───────────────────────────────────────────
    /// Trích xuất bảng / danh sách dữ liệu từ trang web đang mở
    WebExtractData(WebExtractPayload),
    /// Điều hướng trình duyệt đến URL (cần Human-in-the-Loop)
    WebNavigate(WebNavigatePayload),
    /// Tìm kiếm thông tin trên web và tóm tắt kết quả
    WebSearch(WebSearchPayload),
    /// Chụp ảnh màn hình vùng dữ liệu (grounding evidence)
    WebScreenshot(WebScreenshotPayload),
    /// Luồng trích xuất dữ liệu web và ghi vào Excel (Pipeline)
    WebToExcel(WebToExcelPayload),

    // ── Folder Scanner Agent ─────────────────────────────────────────────────
    /// Quét đệ quy và tóm tắt dữ liệu trong folder
    FolderScan(FolderScanPayload),

    // ── Outlook Agent ────────────────────────────────────────────────────────
    /// Thao tác với Email và Lịch trình
    OutlookAction(OutlookPayload),

    // ── Converter Agent (MCP) ────────────────────────────────────────────────
    /// Cài đặt / đăng ký một MCP Server mới
    McpInstall(McpInstallPayload),
    /// Gọi trực tiếp một tool được đăng ký trong MCP Server
    McpCallTool(McpCallToolPayload),
    /// Liệt kê các MCP Servers / tools đang có
    McpListServers,

    // ── Workflow Engine ──────────────────────────────────────────────────────
    /// Kích hoạt một workflow đã định nghĩa theo ID
    WorkflowTrigger(WorkflowTriggerPayload),
    /// Xem lịch sử / trạng thái các workflow runs
    WorkflowStatus(WorkflowStatusPayload),
    /// Tạo / chỉnh sửa định nghĩa workflow YAML
    WorkflowEdit(WorkflowEditPayload),

    // ── Orchestrator / System ────────────────────────────────────────────────
    /// Câu hỏi chung, trò chuyện, không liên quan đến Office
    GeneralChat(GeneralChatPayload),
    /// Cấu hình hệ thống (LLM provider, API key, …)
    SystemConfig(SystemConfigPayload),
    /// Yêu cầu giải thích / hướng dẫn sử dụng Office Hub
    HelpRequest(HelpRequestPayload),
    /// Ý định chưa xác định rõ – cần hỏi thêm người dùng
    Ambiguous(AmbiguousPayload),
}

impl Intent {
    /// Trả về chuỗi action tương ứng để router và agent xử lý.
    pub fn action_str(&self) -> &'static str {
        match self {
            Intent::ExcelRead(_) => "read_cell_range",
            Intent::ExcelWrite(_) => "write_cell_range",
            Intent::ExcelFormula(_) => "generate_formula",
            Intent::ExcelPowerQuery(_) => "run_power_query",
            Intent::ExcelMacro(_) => "run_vba",
            Intent::ExcelAnalyze(_) => "analyze_workbook",
            Intent::ExcelAudit(_) => "audit_formulas",

            Intent::WordCreate(_) => "create_document",
            Intent::WordEdit(_) => "edit_document",
            Intent::WordFormat(_) => "format_document",
            Intent::WordExtract(_) => "extract_text",

            Intent::PptCreate(_) => "create_presentation",
            Intent::PptEdit(_) => "edit_presentation",
            Intent::PptFormat(_) => "format_presentation",
            Intent::PptConvertFrom(_) => "convert_to_presentation",

            Intent::WebExtractData(_) => "extract_data",
            Intent::WebNavigate(_) => "navigate_to_url",
            Intent::WebSearch(_) => "search",
            Intent::WebScreenshot(_) => "take_screenshot",
            Intent::WebToExcel(_) => "extract_and_write",

            Intent::FolderScan(_) => "scan_folder_all_formats",
            Intent::OutlookAction(_) => "handle_outlook_action",

            Intent::McpInstall(_) => "install",
            Intent::McpCallTool(_) => "call_tool",
            Intent::McpListServers => "list",

            Intent::WorkflowTrigger(_) => "trigger",
            Intent::WorkflowStatus(_) => "status",
            Intent::WorkflowEdit(_) => "edit",

            Intent::GeneralChat(_) => "chat",
            Intent::SystemConfig(_) => "config",
            Intent::HelpRequest(_) => "help",
            Intent::Ambiguous(_) => "clarify",
        }
    }

    /// Trả về tên agent chịu trách nhiệm xử lý Intent này.
    pub fn target_agent(&self) -> AgentTarget {
        match self {
            Intent::ExcelRead(_)
            | Intent::ExcelWrite(_)
            | Intent::ExcelFormula(_)
            | Intent::ExcelPowerQuery(_)
            | Intent::ExcelMacro(_)
            | Intent::ExcelAnalyze(_)
            | Intent::ExcelAudit(_) => AgentTarget::Analyst,

            Intent::WordCreate(_)
            | Intent::WordEdit(_)
            | Intent::WordFormat(_)
            | Intent::WordExtract(_)
            | Intent::PptCreate(_)
            | Intent::PptEdit(_)
            | Intent::PptFormat(_)
            | Intent::PptConvertFrom(_) => AgentTarget::OfficeMaster,

            Intent::WebExtractData(_)
            | Intent::WebNavigate(_)
            | Intent::WebSearch(_)
            | Intent::WebScreenshot(_) => AgentTarget::WebResearcher,

            Intent::WebToExcel(_) => AgentTarget::Orchestrator,

            Intent::FolderScan(_) => AgentTarget::FolderScanner,
            Intent::OutlookAction(_) => AgentTarget::Outlook,

            Intent::McpInstall(_) | Intent::McpCallTool(_) | Intent::McpListServers => {
                AgentTarget::Converter
            }

            Intent::WorkflowTrigger(_)
            | Intent::WorkflowStatus(_)
            | Intent::WorkflowEdit(_)
            | Intent::GeneralChat(_)
            | Intent::SystemConfig(_)
            | Intent::HelpRequest(_)
            | Intent::Ambiguous(_) => AgentTarget::Orchestrator,
        }
    }

    /// Mức độ nhạy cảm của Intent – ảnh hưởng đến Human-in-the-Loop policy.
    pub fn sensitivity(&self) -> SensitivityLevel {
        match self {
            // Chỉ đọc → thấp
            Intent::ExcelRead(_)
            | Intent::ExcelAudit(_)
            | Intent::WordExtract(_)
            | Intent::WebScreenshot(_)
            | Intent::McpListServers
            | Intent::WorkflowStatus(_)
            | Intent::GeneralChat(_)
            | Intent::HelpRequest(_) => SensitivityLevel::Low,

            // Ghi vào Office → trung bình
            Intent::ExcelWrite(_)
            | Intent::ExcelFormula(_)
            | Intent::ExcelAnalyze(_)
            | Intent::ExcelPowerQuery(_)
            | Intent::WordCreate(_)
            | Intent::WordEdit(_)
            | Intent::WordFormat(_)
            | Intent::PptCreate(_)
            | Intent::PptEdit(_)
            | Intent::PptFormat(_)
            | Intent::PptConvertFrom(_)
            | Intent::WorkflowTrigger(_)
            | Intent::WorkflowEdit(_)
            | Intent::FolderScan(_)
            | Intent::SystemConfig(_) => SensitivityLevel::Medium,

            // Chạy macro / VBA → cao
            Intent::ExcelMacro(_)
            | Intent::WebSearch(_)
            | Intent::OutlookAction(_)
            | Intent::McpInstall(_)
            | Intent::McpCallTool(_) => SensitivityLevel::High,

            // Điều khiển trình duyệt → rất cao
            Intent::WebNavigate(_) | Intent::WebExtractData(_) | Intent::WebToExcel(_) => {
                SensitivityLevel::Critical
            }

            Intent::Ambiguous(_) => SensitivityLevel::Low,
        }
    }

    /// Trả về chuỗi mô tả ngắn gọn để log / hiển thị.
    pub fn display_name(&self) -> &'static str {
        match self {
            Intent::ExcelRead(_) => "Excel: Đọc dữ liệu",
            Intent::ExcelWrite(_) => "Excel: Ghi dữ liệu",
            Intent::ExcelFormula(_) => "Excel: Công thức",
            Intent::ExcelPowerQuery(_) => "Excel: Power Query",
            Intent::ExcelMacro(_) => "Excel: Macro VBA",
            Intent::ExcelAnalyze(_) => "Excel: Phân tích dữ liệu",
            Intent::ExcelAudit(_) => "Excel: Kiểm tra công thức",
            Intent::WordCreate(_) => "Word: Tạo tài liệu",
            Intent::WordEdit(_) => "Word: Chỉnh sửa",
            Intent::WordFormat(_) => "Word: Định dạng",
            Intent::WordExtract(_) => "Word: Trích xuất",
            Intent::PptCreate(_) => "PPT: Tạo slide",
            Intent::PptEdit(_) => "PPT: Chỉnh sửa slide",
            Intent::PptFormat(_) => "PPT: Định dạng slide",
            Intent::PptConvertFrom(_) => "PPT: Chuyển đổi thành slide",
            Intent::WebExtractData(_) => "Web: Trích xuất dữ liệu",
            Intent::WebNavigate(_) => "Web: Điều hướng trình duyệt",
            Intent::WebSearch(_) => "Web: Tìm kiếm",
            Intent::WebScreenshot(_) => "Web: Chụp màn hình",
            Intent::WebToExcel(_) => "Pipeline: Web to Excel",
            Intent::FolderScan(_) => "Folder: Quét và tổng hợp",
            Intent::OutlookAction(_) => "Outlook: Xử lý Email/Lịch",
            Intent::McpInstall(_) => "MCP: Cài đặt server",
            Intent::McpCallTool(_) => "MCP: Gọi tool",
            Intent::McpListServers => "MCP: Liệt kê servers",
            Intent::WorkflowTrigger(_) => "Workflow: Kích hoạt",
            Intent::WorkflowStatus(_) => "Workflow: Trạng thái",
            Intent::WorkflowEdit(_) => "Workflow: Chỉnh sửa",
            Intent::GeneralChat(_) => "Chat: Trò chuyện",
            Intent::SystemConfig(_) => "Hệ thống: Cấu hình",
            Intent::HelpRequest(_) => "Trợ giúp",
            Intent::Ambiguous(_) => "Không rõ ý định",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. AGENT TARGET & SENSITIVITY
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTarget {
    Analyst,
    OfficeMaster,
    WebResearcher,
    FolderScanner,
    Outlook,
    Converter,
    Orchestrator,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityLevel {
    Low,
    Medium,
    High,
    Critical,
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. INTENT PAYLOAD TYPES
// ─────────────────────────────────────────────────────────────────────────────

// ── Excel payloads ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelReadPayload {
    /// Đường dẫn file Excel (có thể rỗng nếu đang làm việc với file hiện tại)
    pub file_path: Option<String>,
    /// Tên sheet (None = sheet đang active)
    pub sheet_name: Option<String>,
    /// Dải ô cần đọc, ví dụ "A1:D20" hoặc "A:A"
    pub range: Option<String>,
    /// Named range hoặc Table name
    pub named_range: Option<String>,
    /// Câu hỏi / yêu cầu cụ thể về dữ liệu
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelWritePayload {
    pub file_path: Option<String>,
    pub sheet_name: Option<String>,
    pub range: Option<String>,
    /// Nội dung cần ghi (raw text, JSON array, …)
    pub content: Option<String>,
    /// Ghi đè hay chèn thêm dòng
    pub write_mode: ExcelWriteMode,
    pub create_if_missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExcelWriteMode {
    #[default]
    Overwrite,
    Append,
    Insert,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelFormulaPayload {
    pub file_path: Option<String>,
    pub sheet_name: Option<String>,
    pub target_range: Option<String>,
    /// Mô tả yêu cầu công thức bằng ngôn ngữ tự nhiên
    pub description: String,
    /// Loại công thức gợi ý (nếu user đề cập)
    pub formula_type: Option<String>,
    /// Các dải ô liên quan
    pub source_ranges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelPowerQueryPayload {
    pub file_path: Option<String>,
    pub query_name: Option<String>,
    /// Mô tả yêu cầu transform dữ liệu
    pub description: String,
    /// Nguồn dữ liệu (file path, table name, URL)
    pub data_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelMacroPayload {
    pub file_path: Option<String>,
    pub macro_name: Option<String>,
    /// Mô tả hành vi macro cần tạo
    pub description: String,
    /// Chỉ tạo code (không chạy), hay tạo và chạy luôn?
    pub execute_immediately: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelAnalyzePayload {
    pub file_path: Option<String>,
    pub sheet_name: Option<String>,
    pub range: Option<String>,
    /// Loại phân tích: summary | trend | anomaly | correlation | forecast
    pub analysis_types: Vec<String>,
    /// Cột chứa nhãn thời gian (nếu có)
    pub time_column: Option<String>,
    /// Cột chứa giá trị cần phân tích
    pub value_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExcelAuditPayload {
    pub file_path: Option<String>,
    pub sheet_name: Option<String>,
    pub range: Option<String>,
    /// Kiểm tra tất cả công thức hay chỉ một dải ô?
    pub full_audit: bool,
    /// Báo cáo chi tiết hay tóm tắt?
    pub detailed: bool,
}

// ── Word payloads ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WordCreatePayload {
    pub output_path: Option<String>,
    pub template_path: Option<String>,
    /// Tiêu đề / chủ đề tài liệu
    pub title: Option<String>,
    /// Outline hoặc nội dung chi tiết cần tạo
    pub outline: Option<String>,
    /// Ngôn ngữ: "vi" | "en"
    pub language: Option<String>,
    pub page_orientation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WordEditPayload {
    pub file_path: Option<String>,
    /// Bookmark, heading text, hoặc đoạn cần sửa
    pub target_section: Option<String>,
    /// Nội dung mới hoặc chỉ dẫn thay đổi
    pub instruction: String,
    pub preserve_format: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WordFormatPayload {
    pub file_path: Option<String>,
    /// Loại định dạng: styles | toc | sections | cross_refs | page_numbers
    pub format_tasks: Vec<String>,
    pub template_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WordExtractPayload {
    pub file_path: Option<String>,
    /// Loại nội dung cần trích xuất: text | tables | images | metadata
    pub extract_types: Vec<String>,
    pub section_filter: Option<String>,
}

// ── PowerPoint payloads ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PptCreatePayload {
    pub output_path: Option<String>,
    pub template_path: Option<String>,
    pub title: Option<String>,
    pub outline: Option<String>,
    pub slide_count_hint: Option<u32>,
    pub theme: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PptEditPayload {
    pub file_path: Option<String>,
    pub slide_index: Option<u32>,
    /// Tên placeholder / shape / text box cần sửa
    pub target_element: Option<String>,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PptFormatPayload {
    pub file_path: Option<String>,
    pub format_tasks: Vec<String>,
    pub apply_brand_palette: bool,
    pub align_to_grid: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PptConvertFromPayload {
    pub source_path: Option<String>,
    pub source_type: String, // "word" | "markdown" | "json" | "excel"
    pub output_path: Option<String>,
    pub template_path: Option<String>,
    pub slides_per_section: Option<u32>,
}

// ── Web Researcher payloads ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebExtractPayload {
    /// URL cụ thể (None = trang đang mở trong browser)
    pub url: Option<String>,
    /// Tên / loại dữ liệu cần lấy
    pub data_description: String,
    /// Loại element: "table" | "list" | "text" | "auto"
    pub element_type: String,
    /// Index bảng trên trang (0-based), None = tất cả
    pub table_index: Option<u32>,
    /// Xuất ra Excel hay JSON?
    pub export_format: String,
    /// Đích lưu sau khi extract
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebNavigatePayload {
    pub url: String,
    pub browser: Option<String>, // "edge" | "chrome"
    pub wait_for_load: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebSearchPayload {
    pub query: String,
    pub num_results: Option<u32>,
    /// Trích xuất nội dung trang kết quả không?
    pub fetch_content: bool,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebScreenshotPayload {
    pub url: Option<String>,
    /// Vùng chụp: "full" | "viewport" | "element"
    pub capture_mode: String,
    pub element_selector: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WebToExcelPayload {
    pub url: Option<String>,
    pub data_description: String,
    pub output_path: Option<String>,
}

// ── Folder Scanner payloads ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FolderScanPayload {
    pub folder_path: String,
    pub output_format: Option<String>,
    pub recursive: Option<bool>,
    pub max_depth: Option<usize>,
}

// ── Outlook Agent payloads ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OutlookPayload {
    pub action_type: String, // "read_emails", "send_email", "calendar"
    pub target: Option<String>,
    pub query: Option<String>,
    pub details: Option<String>,
}

// ── MCP payloads ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct McpInstallPayload {
    /// Nguồn: filesystem path, "npm:package-name", "github:org/repo"
    pub source: String,
    pub version: Option<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct McpCallToolPayload {
    pub server_id: String,
    pub tool_name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

// ── Workflow payloads ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkflowTriggerPayload {
    pub workflow_id: String,
    pub input_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkflowStatusPayload {
    pub workflow_id: Option<String>,
    pub run_id: Option<String>,
    pub last_n_runs: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkflowEditPayload {
    pub workflow_id: Option<String>,
    pub description: String,
    pub yaml_content: Option<String>,
}

// ── General / System payloads ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GeneralChatPayload {
    pub message: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemConfigPayload {
    pub config_key: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HelpRequestPayload {
    pub topic: Option<String>,
    pub question: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AmbiguousPayload {
    pub original_message: String,
    /// Danh sách các Intent có thể, để hỏi lại người dùng
    pub candidates: Vec<String>,
    pub clarification_question: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. EXTRACTED ENTITIES
// ─────────────────────────────────────────────────────────────────────────────

/// Các entity được trích xuất từ raw message.
/// Phục vụ việc điền vào payload mà không cần LLM parse lại.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedEntities {
    /// Đường dẫn file được đề cập (Excel, Word, PPT, …)
    pub file_paths: Vec<String>,
    /// Tên sheet / tab Excel
    pub sheet_names: Vec<String>,
    /// Dải ô Excel dạng "A1:D20"
    pub cell_ranges: Vec<String>,
    /// Named ranges / Table names
    pub named_ranges: Vec<String>,
    /// URL web
    pub urls: Vec<String>,
    /// Tên workflow
    pub workflow_ids: Vec<String>,
    /// Tên MCP server / tool
    pub mcp_server_ids: Vec<String>,
    /// Ngày / khoảng thời gian đề cập
    pub date_references: Vec<String>,
    /// Số / giá trị quan trọng
    pub numeric_values: Vec<f64>,
    /// Từ khoá chủ đề (báo cáo, doanh thu, chi phí, …)
    pub topic_keywords: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. CLASSIFY REQUEST / RESULT
// ─────────────────────────────────────────────────────────────────────────────

/// Đầu vào cho bộ phân loại Intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassifyRequest {
    /// Tin nhắn thô từ người dùng
    pub message: String,
    /// Session context: các lượt hội thoại trước (tóm tắt)
    pub session_context: Option<String>,
    /// File đang được mở / focus trong File Browser
    pub active_file: Option<String>,
    /// Ngôn ngữ người dùng: "vi" | "en" | "auto"
    pub language: Option<String>,
}

/// Kết quả phân loại Intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassifyResult {
    /// Intent đã được xác định
    pub intent: Intent,
    /// Confidence score [0.0 – 1.0]
    pub confidence: f32,
    /// Các entity được trích xuất từ message
    pub entities: ExtractedEntities,
    /// Phương pháp phân loại đã dùng
    pub method: ClassificationMethod,
    /// Câu hỏi clarification nếu cần hỏi lại người dùng (confidence thấp)
    pub clarification_needed: bool,
    pub clarification_question: Option<String>,
}

impl IntentClassifyResult {
    /// Ngưỡng confidence để coi là "chắc chắn" (không cần hỏi thêm)
    pub const CONFIDENT_THRESHOLD: f32 = 0.75;
    /// Ngưỡng confidence tối thiểu (dưới này → Ambiguous)
    pub const MINIMUM_THRESHOLD: f32 = 0.30;

    pub fn is_confident(&self) -> bool {
        self.confidence >= Self::CONFIDENT_THRESHOLD
    }

    pub fn is_too_uncertain(&self) -> bool {
        self.confidence < Self::MINIMUM_THRESHOLD
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationMethod {
    /// Phân loại bằng regex / keyword rules (nhanh, offline)
    FastRule,
    /// Phân loại bằng LLM (chậm hơn nhưng chính xác hơn)
    LlmAssisted,
    /// Kết hợp: FastRule cho category, LLM cho entity extraction
    Hybrid,
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. FAST CLASSIFIER (Rule-based, no LLM needed)
// ─────────────────────────────────────────────────────────────────────────────

/// Tập hợp regex patterns dùng cho FastClassifier.
/// Được compiled một lần khi khởi động (Lazy).
struct FastClassifierPatterns {
    // Excel patterns
    excel_read: Regex,
    excel_write: Regex,
    excel_formula: Regex,
    excel_power_query: Regex,
    excel_macro: Regex,
    excel_analyze: Regex,
    excel_audit: Regex,
    // Word patterns
    word_create: Regex,
    word_edit: Regex,
    word_format: Regex,
    word_extract: Regex,
    // PPT patterns
    ppt_create: Regex,
    ppt_edit: Regex,
    ppt_format: Regex,
    ppt_convert: Regex,
    // Web patterns
    web_extract: Regex,
    web_navigate: Regex,
    web_search: Regex,
    web_screenshot: Regex,
    // MCP patterns
    mcp_install: Regex,
    mcp_call: Regex,
    mcp_list: Regex,
    // Folder & Outlook
    folder_scan: Regex,
    outlook_action: Regex,
    // Workflow patterns
    workflow_trigger: Regex,
    workflow_status: Regex,
    workflow_edit: Regex,
    // Help / System
    help: Regex,
    config: Regex,
    // Entity patterns
    file_path: Regex,
    cell_range: Regex,
    sheet_name: Regex,
    url_pattern: Regex,
}

impl FastClassifierPatterns {
    fn new() -> Self {
        let re =
            |pattern: &str| Regex::new(pattern).expect("Invalid regex pattern in FastClassifier");

        Self {
            // ── Excel
            excel_read: re(
                r"(?i)(đọc|lấy|xem|hiển thị|show|get|read|truy vấn|query)\s.*(excel|xlsx|xls|\bô\b|cell|\bbảng\b|sheet|\bcột\b|\bhàng\b|\bdòng\b|row|column)",
            ),
            excel_write: re(
                r"(?i)(ghi|điền|cập nhật|update|write|nhập|insert|thêm vào ô|fill)\s.*(excel|xlsx|xls|\bô\b|cell|range|\bbảng\b)",
            ),
            excel_formula: re(
                r"(?i)(công thức|formula|xlookup|vlookup|sumif|countif|index.*match|lambda|dynamic array|spill|let\s*=|hàm\s+\w+|tính tổng)",
            ),
            excel_power_query: re(
                r"(?i)(power query|query editor|m code|m-code|transform data|lọc dữ liệu từ|import từ)",
            ),
            excel_macro: re(
                r"(?i)(macro|vba|sub\s+\w+|office scripts|tự động hóa excel|tạo.*script.*excel)",
            ),
            excel_analyze: re(
                r"(?i)(phân tích|analyze|thống kê|statistics|xu hướng|trend|anomaly|bất thường|dự báo|forecast|tương quan|correlation)\s.*(dữ liệu|data|excel|xlsx|\bsố liệu\b)",
            ),
            excel_audit: re(
                r"(?i)(kiểm tra|audit|check|review|lỗi công thức|formula error|trace|precedent|dependent)\s.*(công thức|formula|excel|xlsx)",
            ),

            // ── Word
            word_create: re(
                r"(?i)(tạo|viết|soạn|create|write|generate)\s.*(word|docx|tài liệu|văn bản|báo cáo|hợp đồng|biên bản|quyết định|thông báo|công văn)",
            ),
            word_edit: re(
                r"(?i)(sửa|chỉnh|edit|update|thay đổi|modify)\s.*(word|docx|tài liệu|văn bản|đoạn|paragraph|bảng trong word)",
            ),
            word_format: re(
                r"(?i)(định dạng|format|style|font|heading|mục lục|table of contents|toc|section|header|footer|page number|cross.*ref)",
            ),
            word_extract: re(
                r"(?i)(trích xuất|extract|lấy nội dung|đọc|read)\s.*(word|docx|tài liệu|văn bản)",
            ),

            // ── PPT
            ppt_create: re(
                r"(?i)(tạo|làm|create|generate|soạn)\s.*(slide|ppt|pptx|powerpoint|bài thuyết trình|presentation|deck)",
            ),
            ppt_edit: re(
                r"(?i)(sửa|thêm|xóa|edit|add|delete|update)\s.*(slide|ppt|pptx|powerpoint|thuyết trình)",
            ),
            ppt_format: re(
                r"(?i)(định dạng|format|theme|transition|animation|layout|grid|brand|màu sắc slide)\s.*(slide|ppt|pptx|powerpoint)",
            ),
            ppt_convert: re(
                r"(?i)(chuyển|convert|tạo slide.*từ|make.*slides from|turn into slides)\s.*(word|excel|markdown|text|outline)",
            ),

            // ── Web
            web_extract: re(
                r"(?i)(lấy dữ liệu từ web|scrape|trích xuất từ trang|extract from.*(web|page|site)|lấy bảng.*từ)",
            ),
            web_navigate: re(
                r"(?i)(mở trang|navigate|điều hướng|go to|open.*(url|website|page)|truy cập trang)",
            ),
            web_search: re(
                r"(?i)(tìm kiếm trên web|search the web|google|tìm thông tin.*(online|web|internet)|research)",
            ),
            web_screenshot: re(r"(?i)(chụp màn hình|screenshot|capture|chụp trang|chụp ảnh web)"),

            // ── MCP
            mcp_install: re(
                r"(?i)(cài đặt|install|thêm|add)\s.*(mcp|plugin|extension|server|tool mới)",
            ),
            mcp_call: re(r"(?i)(gọi|call|run|chạy)\s+tool\s+\w+"),
            mcp_list: re(r"(?i)(liệt kê|list|xem danh sách)\s.*(mcp|plugin|tool|server)"),

            // ── Folder & Outlook
            folder_scan: re(r"(?i)(quét|scan|tóm tắt|summarize)\s.*(thư mục|folder|directory)"),
            outlook_action: re(
                r"(?i)(\bemail\b|\bmail\b|hộp thư|thư điện tử|lịch|calendar|outlook)",
            ),

            // ── Workflow
            workflow_trigger: re(
                r"(?i)(chạy|kích hoạt|trigger|run|bắt đầu)\s.*(workflow|quy trình|automation|tự động hóa)",
            ),
            workflow_status: re(
                r"(?i)(trạng thái|status|lịch sử|history|kết quả)\s.*(workflow|quy trình|lần chạy|run)",
            ),
            workflow_edit: re(
                r"(?i)(tạo|sửa|chỉnh|create|edit|modify)\s.*(workflow|quy trình|automation)",
            ),

            // ── Help / Config
            help: re(
                r"(?i)(giúp|help|hướng dẫn|guide|how to|làm thế nào|cách|what is|là gì|office hub)",
            ),
            config: re(
                r"(?i)(cấu hình|configure|settings|api key|model|endpoint|ollama|gemini|openai|lm studio)",
            ),

            // ── Entity extractors
            file_path: re(
                r#"(?i)([A-Za-z]:\\[^\s"'<>|?*]+\.(xlsx?|xlsm|docx?|pptx?|pdf|csv|json|yaml|txt)|[^\s"'<>|?*]+\.(xlsx?|xlsm|docx?|pptx?|pdf|csv|json|yaml|txt))"#,
            ),
            cell_range: re(
                r"(?i)\b([A-Z]{1,3}\d+:[A-Z]{1,3}\d+|[A-Z]{1,3}:\s*[A-Z]{1,3}|\$?[A-Z]{1,3}\$?\d+)\b",
            ),
            sheet_name: re(r#"(?i)(?:sheet|tab|trang tính)\s*[""']?([^\s""'!]+)[""']?"#),
            url_pattern: re(r"https?://[^\s]+"),
        }
    }
}

static PATTERNS: Lazy<FastClassifierPatterns> = Lazy::new(FastClassifierPatterns::new);

// ─────────────────────────────────────────────────────────────────────────────
// 7. IntentClassifier – public API
// ─────────────────────────────────────────────────────────────────────────────

pub struct IntentClassifier;

impl IntentClassifier {
    /// Phân loại nhanh bằng rule-based (không cần LLM).
    ///
    /// Trả về `None` nếu không tìm thấy pattern nào phù hợp.
    pub fn classify_fast(req: &IntentClassifyRequest) -> Option<IntentClassifyResult> {
        let msg = &req.message;
        let p = &*PATTERNS;

        // Trích xuất entities trước
        let entities = Self::extract_entities(msg, req.active_file.as_deref());

        // Thử từng pattern theo thứ tự ưu tiên (từ cụ thể → chung)
        let matched: Option<(Intent, f32)> = None
            // Folder & Outlook (ưu tiên cao để tránh nhầm với Excel read/write)
            .or_else(|| {
                Self::try_match(
                    p.folder_scan.is_match(msg),
                    Intent::FolderScan(FolderScanPayload {
                        folder_path: entities
                            .file_paths
                            .first()
                            .cloned()
                            .unwrap_or_else(|| msg.clone()),
                        ..Default::default()
                    }),
                    0.85,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.outlook_action.is_match(msg),
                    Intent::OutlookAction(OutlookPayload {
                        action_type: "auto".to_string(),
                        query: Some(msg.clone()),
                        ..Default::default()
                    }),
                    0.85,
                )
            })
            // Web
            .or_else(|| {
                Self::try_match(
                    p.web_screenshot.is_match(msg),
                    Intent::WebScreenshot(WebScreenshotPayload {
                        url: entities.urls.first().cloned(),
                        capture_mode: "viewport".to_string(),
                        ..Default::default()
                    }),
                    0.88,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.web_navigate.is_match(msg),
                    Intent::WebNavigate(WebNavigatePayload {
                        url: entities.urls.first().cloned().unwrap_or_default(),
                        browser: None,
                        wait_for_load: true,
                    }),
                    0.85,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.web_extract.is_match(msg),
                    Intent::WebExtractData(WebExtractPayload {
                        url: entities.urls.first().cloned(),
                        data_description: msg.clone(),
                        element_type: "table".to_string(),
                        export_format: "excel".to_string(),
                        ..Default::default()
                    }),
                    0.82,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.web_search.is_match(msg),
                    Intent::WebSearch(WebSearchPayload {
                        query: msg.clone(),
                        num_results: Some(5),
                        fetch_content: true,
                        language: req.language.clone(),
                    }),
                    0.78,
                )
            })
            // Excel (cụ thể trước)
            .or_else(|| {
                Self::try_match(
                    p.excel_power_query.is_match(msg),
                    Intent::ExcelPowerQuery(ExcelPowerQueryPayload {
                        file_path: entities.file_paths.first().cloned(),
                        description: msg.clone(),
                        ..Default::default()
                    }),
                    0.85,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.excel_macro.is_match(msg),
                    Intent::ExcelMacro(ExcelMacroPayload {
                        file_path: entities.file_paths.first().cloned(),
                        macro_name: None,
                        description: msg.clone(),
                        execute_immediately: false,
                    }),
                    0.85,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.excel_formula.is_match(msg),
                    Intent::ExcelFormula(ExcelFormulaPayload {
                        file_path: entities.file_paths.first().cloned(),
                        sheet_name: entities.sheet_names.first().cloned(),
                        target_range: entities.cell_ranges.first().cloned(),
                        description: msg.clone(),
                        source_ranges: entities.cell_ranges.clone(),
                        ..Default::default()
                    }),
                    0.88,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.excel_audit.is_match(msg),
                    Intent::ExcelAudit(ExcelAuditPayload {
                        file_path: entities.file_paths.first().cloned(),
                        sheet_name: entities.sheet_names.first().cloned(),
                        range: entities.cell_ranges.first().cloned(),
                        full_audit: true,
                        detailed: true,
                    }),
                    0.82,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.excel_analyze.is_match(msg),
                    Intent::ExcelAnalyze(ExcelAnalyzePayload {
                        file_path: entities.file_paths.first().cloned(),
                        sheet_name: entities.sheet_names.first().cloned(),
                        analysis_types: vec!["summary".to_string()],
                        value_columns: vec![],
                        ..Default::default()
                    }),
                    0.80,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.excel_write.is_match(msg),
                    Intent::ExcelWrite(ExcelWritePayload {
                        file_path: entities.file_paths.first().cloned(),
                        sheet_name: entities.sheet_names.first().cloned(),
                        range: entities.cell_ranges.first().cloned(),
                        ..Default::default()
                    }),
                    0.82,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.excel_read.is_match(msg),
                    Intent::ExcelRead(ExcelReadPayload {
                        file_path: entities.file_paths.first().cloned(),
                        sheet_name: entities.sheet_names.first().cloned(),
                        range: entities.cell_ranges.first().cloned(),
                        query: Some(msg.clone()),
                        ..Default::default()
                    }),
                    0.78,
                )
            })
            // PPT Convert is prioritized to prevent generic word_create match
            .or_else(|| {
                Self::try_match(
                    p.ppt_convert.is_match(msg),
                    Intent::PptConvertFrom(PptConvertFromPayload {
                        source_path: entities.file_paths.first().cloned(),
                        source_type: "auto".to_string(),
                        ..Default::default()
                    }),
                    0.82,
                )
            })
            // Word
            .or_else(|| {
                Self::try_match(
                    p.word_extract.is_match(msg),
                    Intent::WordExtract(WordExtractPayload {
                        file_path: entities.file_paths.first().cloned(),
                        extract_types: vec!["text".to_string(), "tables".to_string()],
                        ..Default::default()
                    }),
                    0.78,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.word_format.is_match(msg),
                    Intent::WordFormat(WordFormatPayload {
                        file_path: entities.file_paths.first().cloned(),
                        format_tasks: vec![msg.clone()],
                        ..Default::default()
                    }),
                    0.78,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.word_edit.is_match(msg),
                    Intent::WordEdit(WordEditPayload {
                        file_path: entities.file_paths.first().cloned(),
                        instruction: msg.clone(),
                        preserve_format: true,
                        ..Default::default()
                    }),
                    0.78,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.word_create.is_match(msg),
                    Intent::WordCreate(WordCreatePayload {
                        output_path: entities.file_paths.first().cloned(),
                        title: Some(msg.clone()),
                        language: req.language.clone(),
                        ..Default::default()
                    }),
                    0.80,
                )
            })
            // PPT
            .or_else(|| {
                Self::try_match(
                    p.ppt_format.is_match(msg),
                    Intent::PptFormat(PptFormatPayload {
                        file_path: entities.file_paths.first().cloned(),
                        format_tasks: vec![msg.clone()],
                        apply_brand_palette: false,
                        align_to_grid: false,
                    }),
                    0.78,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.ppt_edit.is_match(msg),
                    Intent::PptEdit(PptEditPayload {
                        file_path: entities.file_paths.first().cloned(),
                        instruction: msg.clone(),
                        ..Default::default()
                    }),
                    0.78,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.ppt_create.is_match(msg),
                    Intent::PptCreate(PptCreatePayload {
                        title: Some(msg.clone()),
                        outline: Some(msg.clone()),
                        ..Default::default()
                    }),
                    0.80,
                )
            })
            // MCP
            .or_else(|| Self::try_match(p.mcp_list.is_match(msg), Intent::McpListServers, 0.90))
            .or_else(|| {
                Self::try_match(
                    p.mcp_install.is_match(msg),
                    Intent::McpInstall(McpInstallPayload {
                        source: msg.clone(),
                        ..Default::default()
                    }),
                    0.82,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.mcp_call.is_match(msg),
                    Intent::McpCallTool(McpCallToolPayload {
                        server_id: String::new(),
                        tool_name: String::new(),
                        arguments: HashMap::new(),
                    }),
                    0.70,
                )
            })
            // Workflow
            .or_else(|| {
                Self::try_match(
                    p.workflow_status.is_match(msg),
                    Intent::WorkflowStatus(WorkflowStatusPayload {
                        workflow_id: None,
                        run_id: None,
                        last_n_runs: Some(5),
                    }),
                    0.85,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.workflow_edit.is_match(msg),
                    Intent::WorkflowEdit(WorkflowEditPayload {
                        description: msg.clone(),
                        ..Default::default()
                    }),
                    0.78,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.workflow_trigger.is_match(msg),
                    Intent::WorkflowTrigger(WorkflowTriggerPayload {
                        workflow_id: String::new(),
                        input_data: None,
                    }),
                    0.80,
                )
            })
            // Config / Help (chung nhất, kiểm tra cuối)
            .or_else(|| {
                Self::try_match(
                    p.config.is_match(msg),
                    Intent::SystemConfig(SystemConfigPayload {
                        description: msg.clone(),
                        ..Default::default()
                    }),
                    0.80,
                )
            })
            .or_else(|| {
                Self::try_match(
                    p.help.is_match(msg),
                    Intent::HelpRequest(HelpRequestPayload {
                        question: msg.clone(),
                        ..Default::default()
                    }),
                    0.78,
                )
            });

        matched.map(|(intent, confidence)| IntentClassifyResult {
            clarification_needed: confidence < IntentClassifyResult::CONFIDENT_THRESHOLD,
            clarification_question: if confidence < IntentClassifyResult::CONFIDENT_THRESHOLD {
                Some(format!(
                    "Tôi hiểu bạn muốn thực hiện \"{}\". Bạn có thể xác nhận thêm không?",
                    intent.display_name()
                ))
            } else {
                None
            },
            intent,
            confidence,
            entities,
            method: ClassificationMethod::FastRule,
        })
    }

    /// Xây dựng prompt để gửi cho LLM phân loại Intent.
    /// LLM sẽ trả về JSON theo schema `LlmClassifyResponse`.
    pub fn build_llm_prompt(req: &IntentClassifyRequest) -> String {
        let context_block = req
            .session_context
            .as_deref()
            .map(|c| format!("\n\n**Context hội thoại trước:**\n{c}"))
            .unwrap_or_default();

        let file_block = req
            .active_file
            .as_deref()
            .map(|f| format!("\n\n**File đang mở:** `{f}`"))
            .unwrap_or_default();

        format!(
            r#"Bạn là Intent Classifier của hệ thống Office Hub.
Nhiệm vụ: Phân loại tin nhắn của người dùng thành một Intent cụ thể.{context_block}{file_block}

**Tin nhắn người dùng:**
{message}

**Danh sách Intent hợp lệ:**
excel_read, excel_write, excel_formula, excel_power_query, excel_macro, excel_analyze, excel_audit,
word_create, word_edit, word_format, word_extract,
ppt_create, ppt_edit, ppt_format, ppt_convert_from,
web_extract_data, web_navigate, web_search, web_screenshot,
folder_scan, outlook_action,
mcp_install, mcp_call_tool, mcp_list_servers,
workflow_trigger, workflow_status, workflow_edit,
general_chat, system_config, help_request, ambiguous

**Yêu cầu:** Trả về JSON theo đúng format sau, không có text ngoài JSON:
{{
  "intent_type": "<một trong các Intent ở trên>",
  "confidence": <số thực 0.0-1.0>,
  "reasoning": "<giải thích ngắn gọn>",
  "entities": {{
    "file_paths": [],
    "sheet_names": [],
    "cell_ranges": [],
    "named_ranges": [],
    "urls": [],
    "workflow_ids": [],
    "topic_keywords": []
  }},
  "clarification_needed": <true|false>,
  "clarification_question": "<câu hỏi nếu cần, null nếu không>"
}}"#,
            message = req.message
        )
    }

    pub fn parse_llm_response(
        req: &IntentClassifyRequest,
        llm_json: &str,
    ) -> Result<IntentClassifyResult, String> {
        // Strip markdown backticks if present
        let cleaned_json = llm_json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let raw: serde_json::Value =
            serde_json::from_str(cleaned_json).map_err(|e| format!("JSON parse error: {e}"))?;

        let intent_type_val = raw.get("intent_type").or_else(|| raw.get("intent"));
        let intent_type = intent_type_val
            .and_then(|v| v.as_str())
            .ok_or("Missing 'intent_type' or 'intent' field")?;

        let confidence = raw["confidence"].as_f64().unwrap_or(0.5) as f32;

        let clarification_needed = raw["clarification_needed"].as_bool().unwrap_or(false);
        let clarification_question = raw["clarification_question"].as_str().map(str::to_string);

        let entities = Self::parse_entities_from_json(&raw["entities"], req.active_file.as_deref());

        let intent = Self::build_intent_from_type(intent_type, req, &entities);

        Ok(IntentClassifyResult {
            intent,
            confidence,
            entities,
            method: ClassificationMethod::LlmAssisted,
            clarification_needed,
            clarification_question,
        })
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn try_match(matched: bool, intent: Intent, confidence: f32) -> Option<(Intent, f32)> {
        if matched {
            Some((intent, confidence))
        } else {
            None
        }
    }

    fn extract_entities(message: &str, active_file: Option<&str>) -> ExtractedEntities {
        let p = &*PATTERNS;
        let mut entities = ExtractedEntities::default();

        // File paths
        for cap in p.file_path.captures_iter(message) {
            if let Some(m) = cap.get(0) {
                entities.file_paths.push(m.as_str().to_string());
            }
        }
        // Fall back to the active file if no path was mentioned
        if entities.file_paths.is_empty() {
            if let Some(f) = active_file {
                entities.file_paths.push(f.to_string());
            }
        }

        // Cell ranges
        for cap in p.cell_range.captures_iter(message) {
            if let Some(m) = cap.get(1) {
                entities.cell_ranges.push(m.as_str().to_uppercase());
            }
        }

        // Sheet names
        for cap in p.sheet_name.captures_iter(message) {
            if let Some(m) = cap.get(1) {
                entities.sheet_names.push(m.as_str().to_string());
            }
        }

        // URLs
        for m in p.url_pattern.find_iter(message) {
            entities.urls.push(m.as_str().to_string());
        }

        // Simple keyword extraction for topic_keywords
        let keywords = [
            "doanh thu",
            "chi phí",
            "lợi nhuận",
            "tồn kho",
            "công nợ",
            "báo cáo",
            "hợp đồng",
            "hóa đơn",
            "ngân sách",
            "kế hoạch",
            "revenue",
            "cost",
            "profit",
            "inventory",
            "budget",
        ];
        for kw in &keywords {
            if message.to_lowercase().contains(kw) {
                entities.topic_keywords.push(kw.to_string());
            }
        }

        entities
    }

    fn parse_entities_from_json(
        json: &serde_json::Value,
        active_file: Option<&str>,
    ) -> ExtractedEntities {
        let arr_to_vec = |key: &str| -> Vec<String> {
            json[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut file_paths = arr_to_vec("file_paths");
        if file_paths.is_empty() {
            if let Some(f) = active_file {
                file_paths.push(f.to_string());
            }
        }

        ExtractedEntities {
            file_paths,
            sheet_names: arr_to_vec("sheet_names"),
            cell_ranges: arr_to_vec("cell_ranges"),
            named_ranges: arr_to_vec("named_ranges"),
            urls: arr_to_vec("urls"),
            workflow_ids: arr_to_vec("workflow_ids"),
            mcp_server_ids: arr_to_vec("mcp_server_ids"),
            topic_keywords: arr_to_vec("topic_keywords"),
            ..Default::default()
        }
    }

    fn build_intent_from_type(
        intent_type: &str,
        req: &IntentClassifyRequest,
        entities: &ExtractedEntities,
    ) -> Intent {
        let file = entities.file_paths.first().cloned();
        let sheet = entities.sheet_names.first().cloned();
        let range = entities.cell_ranges.first().cloned();
        let msg = req.message.clone();

        match intent_type {
            "excel_read" => Intent::ExcelRead(ExcelReadPayload {
                file_path: file,
                sheet_name: sheet,
                range,
                query: Some(msg),
                ..Default::default()
            }),
            "excel_write" => Intent::ExcelWrite(ExcelWritePayload {
                file_path: file,
                sheet_name: sheet,
                range,
                ..Default::default()
            }),
            "excel_formula" => Intent::ExcelFormula(ExcelFormulaPayload {
                file_path: file,
                sheet_name: sheet,
                target_range: range,
                description: msg,
                source_ranges: entities.cell_ranges.clone(),
                ..Default::default()
            }),
            "excel_power_query" => Intent::ExcelPowerQuery(ExcelPowerQueryPayload {
                file_path: file,
                description: msg,
                ..Default::default()
            }),
            "excel_macro" => Intent::ExcelMacro(ExcelMacroPayload {
                file_path: file,
                macro_name: None,
                description: msg,
                execute_immediately: false,
            }),
            "excel_analyze" => Intent::ExcelAnalyze(ExcelAnalyzePayload {
                file_path: file,
                sheet_name: sheet,
                range,
                ..Default::default()
            }),
            "excel_audit" => Intent::ExcelAudit(ExcelAuditPayload {
                file_path: file,
                sheet_name: sheet,
                range,
                full_audit: true,
                detailed: true,
            }),
            "word_create" => Intent::WordCreate(WordCreatePayload {
                title: Some(msg),
                language: req.language.clone(),
                ..Default::default()
            }),
            "word_edit" => Intent::WordEdit(WordEditPayload {
                file_path: file,
                instruction: msg,
                preserve_format: true,
                ..Default::default()
            }),
            "word_format" => Intent::WordFormat(WordFormatPayload {
                file_path: file,
                format_tasks: vec![msg],
                ..Default::default()
            }),
            "word_extract" => Intent::WordExtract(WordExtractPayload {
                file_path: file,
                extract_types: vec!["text".into(), "tables".into()],
                ..Default::default()
            }),
            "ppt_create" => Intent::PptCreate(PptCreatePayload {
                title: Some(msg),
                ..Default::default()
            }),
            "ppt_edit" => Intent::PptEdit(PptEditPayload {
                file_path: file,
                instruction: msg,
                ..Default::default()
            }),
            "ppt_format" => Intent::PptFormat(PptFormatPayload {
                file_path: file,
                format_tasks: vec![msg],
                apply_brand_palette: false,
                align_to_grid: false,
            }),
            "ppt_convert_from" => Intent::PptConvertFrom(PptConvertFromPayload {
                source_path: file,
                source_type: "auto".into(),
                ..Default::default()
            }),
            "web_extract_data" => Intent::WebExtractData(WebExtractPayload {
                url: entities.urls.first().cloned(),
                data_description: msg,
                element_type: "auto".into(),
                export_format: "excel".into(),
                ..Default::default()
            }),
            "web_navigate" => Intent::WebNavigate(WebNavigatePayload {
                url: entities.urls.first().cloned().unwrap_or_default(),
                browser: None,
                wait_for_load: true,
            }),
            "web_search" => Intent::WebSearch(WebSearchPayload {
                query: msg,
                num_results: Some(5),
                fetch_content: true,
                language: req.language.clone(),
            }),
            "web_screenshot" => Intent::WebScreenshot(WebScreenshotPayload {
                url: entities.urls.first().cloned(),
                capture_mode: "viewport".into(),
                ..Default::default()
            }),
            "mcp_install" => Intent::McpInstall(McpInstallPayload {
                source: msg,
                ..Default::default()
            }),
            "mcp_call_tool" => Intent::McpCallTool(McpCallToolPayload {
                server_id: String::new(),
                tool_name: String::new(),
                arguments: HashMap::new(),
            }),
            "mcp_list_servers" => Intent::McpListServers,
            "folder_scan" => Intent::FolderScan(FolderScanPayload {
                folder_path: entities
                    .file_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| msg.clone()),
                ..Default::default()
            }),
            "outlook_action" => Intent::OutlookAction(OutlookPayload {
                action_type: "auto".to_string(),
                query: Some(msg.clone()),
                ..Default::default()
            }),
            "workflow_trigger" => Intent::WorkflowTrigger(WorkflowTriggerPayload {
                workflow_id: entities.workflow_ids.first().cloned().unwrap_or_default(),
                input_data: None,
            }),
            "workflow_status" => Intent::WorkflowStatus(WorkflowStatusPayload {
                workflow_id: entities.workflow_ids.first().cloned(),
                run_id: None,
                last_n_runs: Some(5),
            }),
            "workflow_edit" => Intent::WorkflowEdit(WorkflowEditPayload {
                description: msg,
                ..Default::default()
            }),
            "system_config" => Intent::SystemConfig(SystemConfigPayload {
                description: msg,
                ..Default::default()
            }),
            "help_request" => Intent::HelpRequest(HelpRequestPayload {
                question: msg,
                ..Default::default()
            }),
            _ => Intent::GeneralChat(GeneralChatPayload {
                message: msg,
                context: req.session_context.clone(),
            }),
        }
    }

    /// Phân loại intent với sự hỗ trợ của LLM.
    pub async fn classify(
        &self,
        message: &str,
        session: &Session,
        llm: &LlmGateway,
    ) -> AppResult<IntentClassifyResult> {
        let session_context = if session.messages.len() > 1 {
            Some(
                session
                    .messages
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else {
            None
        };

        let req = IntentClassifyRequest {
            message: message.to_string(),
            session_context,
            active_file: None,
            language: Some("vi".to_string()),
        };

        // 1. First try fast rule-based classification
        if let Some(result) = Self::classify_fast(&req) {
            // If we are confident, use the fast result
            if result.is_confident() {
                return Ok(result);
            }
        }

        // 2. If fast classification fails or is low confidence, use LLM
        let prompt_text = Self::build_llm_prompt(&req);

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "intent_type": { "type": "string" },
                "confidence": { "type": "number" },
                "reasoning": { "type": "string" },
                "entities": {
                    "type": "object",
                    "properties": {
                        "file_paths": { "type": "array", "items": { "type": "string" } },
                        "sheet_names": { "type": "array", "items": { "type": "string" } },
                        "cell_ranges": { "type": "array", "items": { "type": "string" } },
                        "named_ranges": { "type": "array", "items": { "type": "string" } },
                        "urls": { "type": "array", "items": { "type": "string" } },
                        "workflow_ids": { "type": "array", "items": { "type": "string" } },
                        "mcp_server_ids": { "type": "array", "items": { "type": "string" } },
                        "topic_keywords": { "type": "array", "items": { "type": "string" } }
                    }
                },
                "clarification_needed": { "type": "boolean" },
                "clarification_question": { "type": "string" }
            },
            "required": ["intent_type", "confidence", "reasoning", "entities", "clarification_needed"]
        });

        let llm_req = crate::llm_gateway::LlmRequest::new(vec![
            crate::llm_gateway::LlmMessage::system(
                "You are an intent classifier that always replies in valid JSON.",
            ),
            crate::llm_gateway::LlmMessage::user(prompt_text),
        ])
        .with_temperature(0.1)
        .with_json_schema(schema);

        match llm.complete(llm_req).await {
            Ok(resp) => match Self::parse_llm_response(&req, &resp.content) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!("Failed to parse LLM intent response: {}. Falling back.", e);
                }
            },
            Err(e) => {
                tracing::warn!("LLM classification failed: {}. Falling back.", e);
            }
        }

        // Fallback: trả về intent GeneralChat mặc định nếu LLM lỗi
        Ok(IntentClassifyResult {
            intent: Intent::GeneralChat(GeneralChatPayload {
                message: message.to_string(),
                context: None,
            }),
            confidence: 0.5,
            entities: ExtractedEntities::default(),
            method: ClassificationMethod::FastRule,
            clarification_needed: false,
            clarification_question: None,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. UNIT TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn req(msg: &str) -> IntentClassifyRequest {
        IntentClassifyRequest {
            message: msg.to_string(),
            session_context: None,
            active_file: None,
            language: Some("vi".to_string()),
        }
    }

    #[test]
    fn test_classify_excel_read() {
        let r =
            IntentClassifier::classify_fast(&req("Đọc dữ liệu từ file bao_cao.xlsx sheet Sheet1"));
        assert!(r.is_some());
        let result = r.unwrap();
        assert!(matches!(result.intent, Intent::ExcelRead(_)));
        assert!(result.confidence >= 0.7);
    }

    #[test]
    fn test_classify_excel_formula() {
        let r = IntentClassifier::classify_fast(&req(
            "Viết công thức XLOOKUP để tìm giá trong bảng giá",
        ));
        assert!(r.is_some());
        let result = r.unwrap();
        assert!(matches!(result.intent, Intent::ExcelFormula(_)));
    }

    #[test]
    fn test_classify_word_create() {
        let r = IntentClassifier::classify_fast(&req(
            "Tạo báo cáo Word tuần này dựa trên số liệu Excel",
        ));
        assert!(r.is_some());
        let result = r.unwrap();
        assert!(matches!(result.intent, Intent::WordCreate(_)));
    }

    #[test]
    fn test_classify_ppt_create() {
        let r = IntentClassifier::classify_fast(&req(
            "Tạo slide thuyết trình từ nội dung file Word này",
        ));
        assert!(r.is_some());
        let result = r.unwrap();
        println!("ACTUAL INTENT: {:?}", result.intent);
        // Could be PptCreate or PptConvertFrom – both are valid
        assert!(
            matches!(result.intent, Intent::PptCreate(_))
                || matches!(result.intent, Intent::PptConvertFrom(_))
        );
    }

    #[test]
    fn test_classify_web_extract() {
        let r = IntentClassifier::classify_fast(&req(
            "Lấy bảng giá xăng dầu từ https://petrolimex.com.vn",
        ));
        assert!(r.is_some());
        let result = r.unwrap();
        println!("ACTUAL WEB EXTRACT INTENT: {:?}", result.intent);
        assert!(matches!(result.intent, Intent::WebExtractData(_)));
        // Entity extraction should have found the URL
        assert!(!result.entities.urls.is_empty());
    }

    #[test]
    fn test_classify_web_navigate_requires_hitl() {
        let r = IntentClassifier::classify_fast(&req("Mở trang https://example.com trong Edge"));
        let result = r.unwrap();
        assert_eq!(result.intent.sensitivity(), SensitivityLevel::Critical);
    }

    #[test]
    fn test_extract_cell_range() {
        let r = IntentClassifier::classify_fast(&req(
            "Tính tổng cột doanh thu từ B2:B100 trong file.xlsx",
        ));
        let result = r.unwrap();
        assert!(result.entities.cell_ranges.contains(&"B2:B100".to_string()));
    }

    #[test]
    fn test_extract_file_path() {
        let r = IntentClassifier::classify_fast(&req(
            "Phân tích dữ liệu trong C:\\Users\\test\\Desktop\\bao_cao.xlsx",
        ));
        let result = r.unwrap();
        assert!(!result.entities.file_paths.is_empty());
        assert!(result.entities.file_paths[0].ends_with("bao_cao.xlsx"));
    }

    #[test]
    fn test_mcp_list() {
        let r = IntentClassifier::classify_fast(&req("Liệt kê các MCP server đang có"));
        assert!(r.is_some());
        let result = r.unwrap();
        assert!(matches!(result.intent, Intent::McpListServers));
        assert!(result.confidence >= 0.85);
    }

    #[test]
    fn test_target_agent_routing() {
        assert_eq!(
            Intent::ExcelRead(Default::default()).target_agent(),
            AgentTarget::Analyst
        );
        assert_eq!(
            Intent::WordCreate(Default::default()).target_agent(),
            AgentTarget::OfficeMaster
        );
        assert_eq!(
            Intent::WebExtractData(Default::default()).target_agent(),
            AgentTarget::WebResearcher
        );
        assert_eq!(
            Intent::McpListServers.target_agent(),
            AgentTarget::Converter
        );
        assert_eq!(
            Intent::GeneralChat(Default::default()).target_agent(),
            AgentTarget::Orchestrator
        );
    }

    #[test]
    fn test_llm_prompt_generation() {
        let req = req("Lấy báo giá xăng dầu và cập nhật vào file báo cáo");
        let prompt = IntentClassifier::build_llm_prompt(&req);
        assert!(prompt.contains("intent_type"));
        assert!(prompt.contains("confidence"));
        assert!(prompt.contains("entities"));
    }
}
