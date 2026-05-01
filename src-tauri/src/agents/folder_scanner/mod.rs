// ============================================================================
// Office Hub – agents/folder_scanner/mod.rs
//
// Folder Scanner Agent
//
// Trách nhiệm:
//   1. Quét đệ quy một folder và thu thập danh sách file theo filter
//   2. Đọc nội dung từng file (Word, Excel, PDF, TXT, Markdown, CSV…)
//   3. Tóm tắt từng file qua LLM Gateway
//   4. Tổng hợp kết quả thành một trong ba dạng output:
//      a) Báo cáo Word (.docx)  – tóm tắt + phân tích
//      b) Slide trình chiếu (.pptx) – executive summary
//      c) Tổng hợp số liệu (.xlsx) – bảng dữ liệu tổng hợp
//   5. Hiển thị tiến trình real-time qua Tauri event channel
//      (để cả Desktop UI và Mobile App theo dõi)
//
// Supported input file types:
//   .docx / .doc   – Word (COM read hoặc Open XML parse)
//   .xlsx / .xls   – Excel (COM read)
//   .pptx / .ppt   – PowerPoint (COM read)
//   .pdf           – PDF (pdfium-render hoặc text extraction)
//   .txt / .md     – Plain text / Markdown
//   .csv           – CSV (csv crate)
//   .json / .yaml  – Structured data
//   .eml           – Email file
//
// Status: STUB – Phase 3 implementation pending
//   Phase 3 sẽ tích hợp COM read cho Office files
//   Phase 3 sẽ tích hợp PDF text extraction
// ============================================================================

use std::{
    collections::HashMap,
    path::PathBuf,
    time::Instant,
};

use async_trait::async_trait;
use calamine::{open_workbook_auto, Reader};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::agents::office_master::{com_ppt, com_word};

use crate::agents::{Agent, AgentId, AgentStatus, AgentStatusInfo};
use crate::orchestrator::{AgentOutput, AgentTask};

// ─────────────────────────────────────────────────────────────────────────────
// Output format enum
// ─────────────────────────────────────────────────────────────────────────────

/// Định dạng output mà Folder Scanner Agent có thể tạo ra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanOutputFormat {
    /// Báo cáo Word tổng hợp (.docx)
    WordReport,
    /// Slide trình chiếu PowerPoint (.pptx)
    PptSlides,
    /// Bảng tổng hợp số liệu Excel (.xlsx)
    ExcelSummary,
    /// Tất cả ba định dạng cùng lúc
    All,
}

impl ScanOutputFormat {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::WordReport => "Báo cáo Word (.docx)",
            Self::PptSlides => "Slide PowerPoint (.pptx)",
            Self::ExcelSummary => "Bảng số liệu Excel (.xlsx)",
            Self::All => "Tất cả định dạng",
        }
    }

    pub fn file_extensions(&self) -> Vec<&'static str> {
        match self {
            Self::WordReport => vec!["docx"],
            Self::PptSlides => vec!["pptx"],
            Self::ExcelSummary => vec!["xlsx"],
            Self::All => vec!["docx", "pptx", "xlsx"],
        }
    }
}

impl std::fmt::Display for ScanOutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// File type categories
// ─────────────────────────────────────────────────────────────────────────────

/// Loại file để chọn reader phù hợp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Word,
    Excel,
    PowerPoint,
    Pdf,
    PlainText,
    Markdown,
    Csv,
    Json,
    Yaml,
    Email,
    Image,
    Unknown,
}

impl FileCategory {
    /// Phát hiện loại file từ extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "docx" | "doc" | "docm" => Self::Word,
            "xlsx" | "xls" | "xlsm" | "xlsb" => Self::Excel,
            "pptx" | "ppt" | "pptm" => Self::PowerPoint,
            "pdf" => Self::Pdf,
            "txt" | "log" | "text" => Self::PlainText,
            "md" | "markdown" | "mdx" => Self::Markdown,
            "csv" | "tsv" => Self::Csv,
            "json" | "jsonl" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "eml" | "msg" => Self::Email,
            "png" | "jpg" | "jpeg" | "webp" | "gif" => Self::Image,
            _ => Self::Unknown,
        }
    }

    /// Kiểm tra file có được hỗ trợ không.
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// Ước tính thời gian đọc file theo MB (milliseconds per MB).
    pub fn read_time_ms_per_mb(&self) -> u64 {
        match self {
            Self::PlainText | Self::Markdown | Self::Csv | Self::Json | Self::Yaml => 50,
            Self::Word | Self::PowerPoint => 200,
            Self::Excel => 300,
            Self::Pdf => 500,
            Self::Email => 100,
            Self::Image => 800,
            Self::Unknown => 0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scan configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Cấu hình cho một lần quét folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderScanConfig {
    /// Folder cần quét
    pub folder_path: PathBuf,

    /// Quét đệ quy vào các subfolder không?
    pub recursive: bool,

    /// Độ sâu quét tối đa (None = không giới hạn)
    pub max_depth: Option<usize>,

    /// Chỉ quét các extension này (None = tất cả supported types)
    pub include_extensions: Option<Vec<String>>,

    /// Bỏ qua các extension này
    pub exclude_extensions: Vec<String>,

    /// Bỏ qua các pattern file/folder này (glob-style)
    pub exclude_patterns: Vec<String>,

    /// Kích thước file tối đa (bytes). File lớn hơn sẽ bị skip
    pub max_file_size_bytes: u64,

    /// Số file tối đa để xử lý trong một lần scan
    pub max_files: usize,

    /// Định dạng output cần tạo
    pub output_format: ScanOutputFormat,

    /// Thư mục lưu output (None = cùng folder với input)
    pub output_dir: Option<PathBuf>,

    /// Tên file output (None = tự động tạo từ tên folder + timestamp)
    pub output_filename_prefix: Option<String>,

    /// Template Word cho báo cáo (None = dùng template mặc định)
    pub word_template: Option<PathBuf>,

    /// Template PPT cho slide (None = dùng brand template mặc định)
    pub ppt_template: Option<PathBuf>,

    /// Ngôn ngữ báo cáo: "vi" | "en"
    pub report_language: String,

    /// Mức độ chi tiết: "brief" | "standard" | "detailed"
    pub detail_level: String,

    /// Có bao gồm TOC (Table of Contents) trong báo cáo Word không?
    pub include_toc: bool,

    /// Có bao gồm ảnh chụp màn hình/thumbnail của từng file không?
    pub include_thumbnails: bool,

    /// Prompt hướng dẫn tùy chỉnh từ người dùng (bổ sung vào system prompt)
    pub custom_instructions: Option<String>,
}

impl Default for FolderScanConfig {
    fn default() -> Self {
        Self {
            folder_path: PathBuf::from("."),
            recursive: true,
            max_depth: Some(5),
            include_extensions: None,
            exclude_extensions: vec![
                "exe".into(),
                "dll".into(),
                "sys".into(),
                "tmp".into(),
                "bak".into(),
                "~".into(),
                "lock".into(),
                "lnk".into(),
            ],
            exclude_patterns: vec![
                ".git/**".into(),
                "node_modules/**".into(),
                "__pycache__/**".into(),
                "target/**".into(),
            ],
            max_file_size_bytes: 50 * 1024 * 1024, // 50 MB
            max_files: 200,
            output_format: ScanOutputFormat::WordReport,
            output_dir: None,
            output_filename_prefix: None,
            word_template: None,
            ppt_template: None,
            report_language: "vi".into(),
            detail_level: "standard".into(),
            include_toc: true,
            include_thumbnails: false,
            custom_instructions: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scan result types
// ─────────────────────────────────────────────────────────────────────────────

/// Thông tin metadata của một file đã được quét.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFileInfo {
    /// Đường dẫn tuyệt đối
    pub path: PathBuf,

    /// Tên file (không kèm thư mục)
    pub name: String,

    /// Relative path từ folder gốc
    pub relative_path: PathBuf,

    /// Extension (lowercase, không có dấu chấm)
    pub extension: String,

    /// Loại file
    pub category: FileCategory,

    /// Kích thước file (bytes)
    pub size_bytes: u64,

    /// Thời điểm tạo file
    pub created_at: Option<DateTime<Utc>>,

    /// Thời điểm sửa đổi cuối
    pub modified_at: Option<DateTime<Utc>>,

    /// Tóm tắt nội dung (do LLM tạo ra)
    pub summary: Option<String>,

    /// Các key topics / keywords phát hiện
    pub keywords: Vec<String>,

    /// Số liệu quan trọng được trích xuất (nếu là file Excel/CSV)
    pub extracted_metrics: Option<serde_json::Value>,

    /// Trạng thái xử lý file này
    pub status: FileProcessStatus,

    /// Lý do bỏ qua (nếu status = Skipped)
    pub skip_reason: Option<String>,

    /// Thời gian xử lý (milliseconds)
    pub processing_ms: u64,
}

/// Trạng thái xử lý của từng file trong quá trình scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileProcessStatus {
    /// Đang chờ xử lý
    Pending,
    /// Đang đọc file
    Reading,
    /// Đang tóm tắt qua LLM
    Summarizing,
    /// Đã hoàn thành
    Done,
    /// Bị bỏ qua (file quá lớn, không hỗ trợ, v.v.)
    Skipped,
    /// Lỗi trong quá trình xử lý
    Error(String),
}

/// Kết quả tổng thể của một lần scan folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderScanResult {
    /// ID duy nhất cho lần scan này
    pub scan_id: String,

    /// Folder đã quét
    pub folder_path: PathBuf,

    /// Thống kê
    pub stats: ScanStats,

    /// Danh sách các file đã xử lý (kèm metadata và tóm tắt)
    pub files: Vec<ScannedFileInfo>,

    /// Tóm tắt tổng quan toàn bộ folder (do LLM tạo ra)
    pub folder_summary: Option<String>,

    /// Các chủ đề / theme chính xuất hiện qua nhiều file
    pub common_themes: Vec<String>,

    /// Các số liệu tổng hợp (từ các file Excel/CSV)
    pub aggregated_metrics: Option<serde_json::Value>,

    /// Đường dẫn các file output đã tạo
    pub output_files: Vec<OutputFileInfo>,

    /// Thời điểm bắt đầu scan
    pub started_at: DateTime<Utc>,

    /// Thời điểm hoàn thành
    pub completed_at: Option<DateTime<Utc>>,

    /// Tổng thời gian (milliseconds)
    pub total_duration_ms: Option<u64>,
}

/// Thống kê kết quả scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    /// Tổng số file phát hiện
    pub total_discovered: usize,
    /// Số file được xử lý thành công
    pub processed: usize,
    /// Số file bị bỏ qua
    pub skipped: usize,
    /// Số file lỗi
    pub errors: usize,
    /// Tổng kích thước file đã xử lý (bytes)
    pub total_size_bytes: u64,
    /// Số token LLM đã dùng
    pub total_llm_tokens: u64,
    /// Phân loại theo loại file
    pub by_category: HashMap<String, usize>,
}

/// Thông tin về một file output đã được tạo ra.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFileInfo {
    pub format: ScanOutputFormat,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    pub sheet_count: Option<u32>,
    pub slide_count: Option<u32>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Progress event (streamed real-time via Tauri event / WebSocket)
// ─────────────────────────────────────────────────────────────────────────────

/// Event tiến trình được phát ra trong quá trình scan.
/// Desktop UI và Mobile App đều subscribe để hiển thị progress bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ScanProgressEvent {
    /// Bắt đầu quét folder
    Started {
        scan_id: String,
        folder_path: String,
        estimated_files: usize,
    },
    /// Phát hiện danh sách file
    FilesDiscovered {
        scan_id: String,
        total_files: usize,
        supported_files: usize,
        skipped_files: usize,
    },
    /// Đang xử lý một file cụ thể
    FileProcessing {
        scan_id: String,
        file_name: String,
        file_index: usize,
        total_files: usize,
        percent: f32,
        current_stage: String, // "reading" | "summarizing" | "extracting_metrics"
    },
    /// Một file đã xử lý xong
    FileCompleted {
        scan_id: String,
        file_name: String,
        file_index: usize,
        total_files: usize,
        percent: f32,
        summary_preview: Option<String>, // 100 ký tự đầu của tóm tắt
        status: String,
    },
    /// Đang tạo output document
    GeneratingOutput {
        scan_id: String,
        format: String,
        stage: String, // "generating_summary" | "writing_document" | "applying_template"
    },
    /// Hoàn thành toàn bộ
    Completed {
        scan_id: String,
        output_files: Vec<String>,
        total_files_processed: usize,
        total_duration_ms: u64,
        stats: ScanStats,
    },
    /// Lỗi nghiêm trọng (dừng scan)
    Failed {
        scan_id: String,
        error: String,
        files_processed_so_far: usize,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Folder Scanner Agent
// ─────────────────────────────────────────────────────────────────────────────

/// Supported actions của Folder Scanner Agent.
pub const ACTIONS: &[&str] = &[
    // Scan và tổng hợp toàn folder
    "scan_folder_to_word",
    "scan_folder_to_ppt",
    "scan_folder_to_excel",
    "scan_folder_all_formats",
    // Scan với config tùy chỉnh
    "scan_folder_custom",
    // Quét nhanh: chỉ liệt kê không tóm tắt
    "list_folder_files",
    // Đọc và tóm tắt một file đơn lẻ
    "read_and_summarize_file",
    // Trích xuất số liệu từ folder (chỉ Excel/CSV)
    "extract_metrics_from_folder",
    // Tìm kiếm nội dung trong folder
    "search_folder_content",
    // Lấy trạng thái scan đang chạy
    "get_scan_progress",
    // Hủy scan đang chạy
    "cancel_scan",
];

/// Folder Scanner Agent – tổng hợp thông tin từ nhiều file trong một folder.
pub struct FolderScannerAgent {
    id: AgentId,
    status: AgentStatus,
    config: FolderScannerConfig,
    metrics: FolderScannerMetrics,
    progress_tx: Option<mpsc::Sender<ScanProgressEvent>>,
    active_scans: HashMap<String, DateTime<Utc>>,
    llm_gateway: Option<std::sync::Arc<tokio::sync::RwLock<crate::llm_gateway::LlmGateway>>>,
}

/// Agent-level configuration (phân biệt với per-scan FolderScanConfig).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderScannerConfig {
    /// Số file xử lý song song tối đa (dùng tokio::task::spawn_blocking)
    pub max_parallel_readers: usize,

    /// Số token LLM tối đa cho một file summary
    pub max_summary_tokens: u32,

    /// Số token LLM tối đa cho folder overview summary
    pub max_folder_summary_tokens: u32,

    /// Output folder mặc định (None = cùng thư mục với folder được scan)
    pub default_output_dir: Option<PathBuf>,

    /// Template Word mặc định
    pub default_word_template: Option<PathBuf>,

    /// Template PPT mặc định
    pub default_ppt_template: Option<PathBuf>,

    /// Giới hạn kích thước file (bytes)
    pub max_file_size_bytes: u64,

    /// Giới hạn số file tối đa mỗi scan
    pub max_files_per_scan: usize,
}

impl Default for FolderScannerConfig {
    fn default() -> Self {
        Self {
            max_parallel_readers: 4,
            max_summary_tokens: 512,
            max_folder_summary_tokens: 1024,
            default_output_dir: None,
            default_word_template: None,
            default_ppt_template: None,
            max_file_size_bytes: 50 * 1024 * 1024, // 50 MB
            max_files_per_scan: 200,
        }
    }
}

/// Metrics thống kê của agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FolderScannerMetrics {
    pub total_scans: u64,
    pub successful_scans: u64,
    pub failed_scans: u64,
    pub total_files_processed: u64,
    pub total_tokens_used: u64,
    pub total_output_files_created: u64,
    pub word_reports_created: u64,
    pub ppt_slides_created: u64,
    pub excel_summaries_created: u64,
}

impl FolderScannerAgent {
    /// Tạo mới với cấu hình mặc định.
    pub fn new() -> Self {
        Self {
            id: AgentId::custom("folder_scanner"),
            status: AgentStatus::Idle,
            config: FolderScannerConfig::default(),
            metrics: FolderScannerMetrics::default(),
            progress_tx: None,
            active_scans: HashMap::new(),
            llm_gateway: None,
        }
    }

    /// Tạo mới với kênh phát progress events.
    pub fn with_progress_channel(mut self, tx: mpsc::Sender<ScanProgressEvent>) -> Self {
        self.progress_tx = Some(tx);
        self
    }

    /// Thiết lập kênh progress sau khi tạo.
    pub fn set_progress_channel(&mut self, tx: mpsc::Sender<ScanProgressEvent>) {
        self.progress_tx = Some(tx);
    }

    // ── Action dispatch ───────────────────────────────────────────────────────

    async fn dispatch_action(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        match task.action.as_str() {
            "scan_folder_to_word" => {
                let cfg = self.build_scan_config(task, ScanOutputFormat::WordReport)?;
                self.run_scan(cfg).await
            }
            "scan_folder_to_ppt" => {
                let cfg = self.build_scan_config(task, ScanOutputFormat::PptSlides)?;
                self.run_scan(cfg).await
            }
            "scan_folder_to_excel" => {
                let cfg = self.build_scan_config(task, ScanOutputFormat::ExcelSummary)?;
                self.run_scan(cfg).await
            }
            "scan_folder_all_formats" => {
                let cfg = self.build_scan_config(task, ScanOutputFormat::All)?;
                self.run_scan(cfg).await
            }
            "scan_folder_custom" => {
                let cfg: FolderScanConfig = task
                    .parameters
                    .get("config")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                self.run_scan(cfg).await
            }
            "list_folder_files" => self.list_files(task).await,
            "read_and_summarize_file" => self.summarize_single_file(task).await,
            "extract_metrics_from_folder" => self.extract_metrics(task).await,
            "search_folder_content" => self.search_content(task).await,
            "get_scan_progress" => self.get_scan_progress(task).await,
            "cancel_scan" => self.cancel_scan(task).await,
            unknown => Err(anyhow::anyhow!(
                "FolderScannerAgent: unknown action '{}'. Supported: {:?}",
                unknown,
                ACTIONS
            )),
        }
    }

    // ── Config builder ────────────────────────────────────────────────────────

    /// Xây dựng FolderScanConfig từ task parameters.
    fn build_scan_config(
        &self,
        task: &AgentTask,
        format: ScanOutputFormat,
    ) -> anyhow::Result<FolderScanConfig> {
        let params = &task.parameters;

        // Folder path: từ parameters hoặc context_file
        let folder_path = params
            .get("folder_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow::anyhow!("folder_path là bắt buộc. Hãy chỉ định folder cần quét.")
            })?;

        if !folder_path.exists() {
            return Err(anyhow::anyhow!(
                "Folder không tồn tại: {}",
                folder_path.display()
            ));
        }

        if !folder_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Đường dẫn không phải là folder: {}",
                folder_path.display()
            ));
        }

        let recursive = params
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let max_depth = params
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .map(|d| d as usize);

        let include_extensions: Option<Vec<String>> = params
            .get("include_extensions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            });

        let output_dir = params
            .get("output_dir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.config.default_output_dir.clone());

        let output_filename_prefix = params
            .get("output_filename_prefix")
            .and_then(|v| v.as_str())
            .map(String::from);

        let report_language = params
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("vi")
            .to_string();

        let detail_level = params
            .get("detail_level")
            .and_then(|v| v.as_str())
            .unwrap_or("standard")
            .to_string();

        let custom_instructions = params
            .get("custom_instructions")
            .and_then(|v| v.as_str())
            .map(String::from)
            // Also extract from the user's message
            .or_else(|| {
                if !task.message.is_empty() {
                    Some(task.message.clone())
                } else {
                    None
                }
            });

        Ok(FolderScanConfig {
            folder_path,
            recursive,
            max_depth,
            include_extensions,
            output_dir,
            output_filename_prefix,
            output_format: format,
            report_language,
            detail_level,
            custom_instructions,
            word_template: self.config.default_word_template.clone(),
            ppt_template: self.config.default_ppt_template.clone(),
            max_file_size_bytes: self.config.max_file_size_bytes,
            max_files: self.config.max_files_per_scan,
            ..FolderScanConfig::default()
        })
    }

    // ── Core scan pipeline ────────────────────────────────────────────────────

    /// Chạy toàn bộ pipeline scan: discover → read → summarize → generate output.
    ///
    /// TODO(phase-3): Implement file readers cho từng loại:
    ///   - Word: COM read hoặc `docx` crate
    ///   - Excel: COM read hoặc `calamine` crate
    ///   - PPT: COM read hoặc `pptx` crate
    ///   - PDF: `pdfium-render` hoặc `pdf-extract` crate
    ///   - Text/MD/CSV/JSON: std::fs::read_to_string
    #[instrument(skip(self, config), fields(folder = %config.folder_path.display()))]
    async fn run_scan(&mut self, config: FolderScanConfig) -> anyhow::Result<AgentOutput> {
        let scan_id = Uuid::new_v4().to_string();
        let started_at = Utc::now();
        let start_instant = Instant::now();

        info!(
            scan_id = %scan_id,
            folder = %config.folder_path.display(),
            format = %config.output_format,
            "FolderScannerAgent: starting scan"
        );

        self.active_scans.insert(scan_id.clone(), started_at);
        self.metrics.total_scans += 1;

        // Emit: Started
        self.emit_progress(ScanProgressEvent::Started {
            scan_id: scan_id.clone(),
            folder_path: config.folder_path.to_string_lossy().to_string(),
            estimated_files: 0,
        })
        .await;

        // ── Step 1: Discover files ─────────────────────────────────────────
        let discovered = self.discover_files(&config).await?;
        let total_supported = discovered
            .iter()
            .filter(|f| f.category.is_supported())
            .count();
        let total_skipped = discovered.len() - total_supported;

        self.emit_progress(ScanProgressEvent::FilesDiscovered {
            scan_id: scan_id.clone(),
            total_files: discovered.len(),
            supported_files: total_supported,
            skipped_files: total_skipped,
        })
        .await;

        info!(
            scan_id = %scan_id,
            discovered = discovered.len(),
            supported = total_supported,
            "Files discovered"
        );

        // ── Step 2: Read and summarize each file ──────────────────────────
        let mut stats = ScanStats {
            total_discovered: discovered.len(),
            ..Default::default()
        };

        use futures::stream::StreamExt;
        let this = &*self;
        let max_concurrent = 4; // TODO: configurable limit

        let results: Vec<(ScannedFileInfo, bool, Option<String>)> = futures::stream::iter(discovered.into_iter().enumerate())
            .map(|(idx, mut file_info)| {
                let scan_id_clone = scan_id.clone();
                let config_clone = config.clone();
                async move {
                    if !file_info.category.is_supported() {
                        file_info.status = FileProcessStatus::Skipped;
                        file_info.skip_reason = Some("Unsupported file type".into());
                        return (file_info, false, None);
                    }

                    let percent = (idx as f32 / total_supported as f32) * 100.0;

                    // Emit: FileProcessing (reading)
                    this.emit_progress(ScanProgressEvent::FileProcessing {
                        scan_id: scan_id_clone.clone(),
                        file_name: file_info.name.clone(),
                        file_index: idx + 1,
                        total_files: total_supported,
                        percent,
                        current_stage: "reading".into(),
                    })
                    .await;

                    // TODO(phase-3): real file read
                    let file_content = this.read_file_content(&file_info).await;

                    // Emit: FileProcessing (summarizing)
                    this.emit_progress(ScanProgressEvent::FileProcessing {
                        scan_id: scan_id_clone.clone(),
                        file_name: file_info.name.clone(),
                        file_index: idx + 1,
                        total_files: total_supported,
                        percent,
                        current_stage: "summarizing".into(),
                    })
                    .await;

                    // TODO(phase-3): real LLM summarization
                    let summary = this
                        .summarize_content(&file_info, file_content.as_deref(), &config_clone)
                        .await;

                    match summary {
                        Ok(s) => {
                            let preview = s.chars().take(120).collect::<String>();
                            file_info.summary = Some(s);
                            file_info.status = FileProcessStatus::Done;

                            this.emit_progress(ScanProgressEvent::FileCompleted {
                                scan_id: scan_id_clone.clone(),
                                file_name: file_info.name.clone(),
                                file_index: idx + 1,
                                total_files: total_supported,
                                percent: percent + (1.0 / total_supported as f32) * 100.0,
                                summary_preview: Some(preview),
                                status: "done".into(),
                            })
                            .await;
                            
                            (file_info, true, None)
                        }
                        Err(e) => {
                            file_info.status = FileProcessStatus::Error(e.to_string());
                            
                            this.emit_progress(ScanProgressEvent::FileCompleted {
                                scan_id: scan_id_clone.clone(),
                                file_name: file_info.name.clone(),
                                file_index: idx + 1,
                                total_files: total_supported,
                                percent,
                                summary_preview: None,
                                status: "error".into(),
                            })
                            .await;

                            (file_info, false, Some(e.to_string()))
                        }
                    }
                }
            })
            .buffer_unordered(max_concurrent)
            .collect()
            .await;

        let mut processed_files: Vec<ScannedFileInfo> = Vec::new();
        for (file_info, is_success, error) in results {
            if is_success {
                stats.processed += 1;
                stats.total_size_bytes += file_info.size_bytes;
                *stats
                    .by_category
                    .entry(format!("{:?}", file_info.category))
                    .or_insert(0) += 1;
            } else if file_info.status == FileProcessStatus::Skipped {
                stats.skipped += 1;
            } else {
                warn!(
                    file = %file_info.name,
                    error = ?error,
                    "Failed to summarize file"
                );
                stats.errors += 1;
            }
            processed_files.push(file_info);
        }

        // ── Step 3: Generate folder-level summary ─────────────────────────
        self.emit_progress(ScanProgressEvent::GeneratingOutput {
            scan_id: scan_id.clone(),
            format: config.output_format.to_string(),
            stage: "generating_summary".into(),
        })
        .await;

        // TODO(phase-3): real LLM folder summary
        let folder_summary = self
            .generate_folder_summary(&processed_files, &config)
            .await;

        // ── Step 4: Generate output documents ─────────────────────────────
        self.emit_progress(ScanProgressEvent::GeneratingOutput {
            scan_id: scan_id.clone(),
            format: config.output_format.to_string(),
            stage: "writing_document".into(),
        })
        .await;

        let output_files = self
            .generate_output_documents(&processed_files, &folder_summary, &config, &scan_id)
            .await?;

        // ── Finalize ──────────────────────────────────────────────────────
        let total_duration_ms = start_instant.elapsed().as_millis() as u64;
        let output_paths: Vec<String> = output_files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();

        self.active_scans.remove(&scan_id);
        self.metrics.successful_scans += 1;
        self.metrics.total_files_processed += stats.processed as u64;
        self.metrics.total_output_files_created += output_files.len() as u64;

        for out in &output_files {
            match out.format {
                ScanOutputFormat::WordReport => self.metrics.word_reports_created += 1,
                ScanOutputFormat::PptSlides => self.metrics.ppt_slides_created += 1,
                ScanOutputFormat::ExcelSummary => self.metrics.excel_summaries_created += 1,
                ScanOutputFormat::All => {
                    self.metrics.word_reports_created += 1;
                    self.metrics.ppt_slides_created += 1;
                    self.metrics.excel_summaries_created += 1;
                }
            }
        }

        self.emit_progress(ScanProgressEvent::Completed {
            scan_id: scan_id.clone(),
            output_files: output_paths.clone(),
            total_files_processed: stats.processed,
            total_duration_ms,
            stats: stats.clone(),
        })
        .await;

        info!(
            scan_id = %scan_id,
            processed = stats.processed,
            outputs = output_files.len(),
            duration_ms = total_duration_ms,
            "FolderScannerAgent: scan completed"
        );

        // Build human-readable reply
        let output_list = output_paths
            .iter()
            .map(|p| format!("  • `{p}`"))
            .collect::<Vec<_>>()
            .join("\n");

        let content = format!(
            "✅ Đã quét và tổng hợp folder **{}**\n\n\
             📊 **Kết quả:**\n\
             • Phát hiện: {} file\n\
             • Xử lý thành công: {} file\n\
             • Bỏ qua: {} file\n\
             • Lỗi: {} file\n\
             • Tổng thời gian: {:.1}s\n\n\
             📄 **File output đã tạo:**\n{}",
            config.folder_path.display(),
            stats.total_discovered,
            stats.processed,
            stats.skipped,
            stats.errors,
            total_duration_ms as f64 / 1000.0,
            output_list
        );

        Ok(AgentOutput {
            content,
            committed: true,
            tokens_used: Some(self.metrics.total_tokens_used as u32),
            metadata: Some(serde_json::json!({
                "scan_id":        scan_id,
                "folder_path":    config.folder_path,
                "stats":          stats,
                "output_files":   output_paths,
                "duration_ms":    total_duration_ms,
            })),
        })
    }

    // ── File discovery ────────────────────────────────────────────────────────

    /// Quét folder và thu thập danh sách file theo filter.
    async fn discover_files(
        &self,
        config: &FolderScanConfig,
    ) -> anyhow::Result<Vec<ScannedFileInfo>> {
        let mut results: Vec<ScannedFileInfo> = Vec::new();
        let mut queue: Vec<(PathBuf, usize)> = vec![(config.folder_path.clone(), 0)];

        while let Some((current_dir, depth)) = queue.pop() {
            if let Some(max_depth) = config.max_depth {
                if depth >= max_depth {
                    continue;
                }
            }

            let mut entries = match tokio::fs::read_dir(&current_dir).await {
                Ok(e) => e,
                Err(e) => {
                    warn!(dir = %current_dir.display(), error = %e, "Failed to read directory");
                    continue;
                }
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    if config.recursive {
                        queue.push((path, depth + 1));
                    }
                    continue;
                }

                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase().to_string())
                    .unwrap_or_default();

                // Apply extension filter
                if let Some(ref include) = config.include_extensions {
                    if !include.contains(&extension) && !include.is_empty() {
                        continue;
                    }
                }

                if config.exclude_extensions.contains(&extension) {
                    continue;
                }

                let category = FileCategory::from_extension(&extension);

                let metadata = entry.metadata().await.ok();
                let size_bytes = metadata.as_ref().map_or(0, |m| m.len());

                // Skip files that are too large
                if size_bytes > config.max_file_size_bytes {
                    results.push(ScannedFileInfo {
                        path: path.clone(),
                        name: name.clone(),
                        relative_path: path.strip_prefix(&config.folder_path).unwrap_or(&path).to_path_buf(),
                        extension: extension.clone(),
                        category,
                        size_bytes,
                        created_at: None,
                        modified_at: None,
                        summary: None,
                        keywords: vec![],
                        extracted_metrics: None,
                        status: FileProcessStatus::Skipped,
                        skip_reason: Some(format!(
                            "File quá lớn: {:.1} MB (giới hạn: {:.1} MB)",
                            size_bytes as f64 / 1024.0 / 1024.0,
                            config.max_file_size_bytes as f64 / 1024.0 / 1024.0
                        )),
                        processing_ms: 0,
                    });
                    continue;
                }

                // Stop if we've hit the limit
                if results.len() >= config.max_files {
                    warn!(
                        max = config.max_files,
                        "Max file limit reached, stopping discovery"
                    );
                    break;
                }

                let modified_at = metadata.as_ref().and_then(|m| m.modified().ok()).map(|t| {
                    let dt: DateTime<Utc> = t.into();
                    dt
                });

                let created_at = metadata.as_ref().and_then(|m| m.created().ok()).map(|t| {
                    let dt: DateTime<Utc> = t.into();
                    dt
                });

                results.push(ScannedFileInfo {
                    path: path.clone(),
                    name,
                    relative_path: path.strip_prefix(&config.folder_path).unwrap_or(&path).to_path_buf(),
                    extension,
                    category,
                    size_bytes,
                    created_at,
                    modified_at,
                    summary: None,
                    keywords: vec![],
                    extracted_metrics: None,
                    status: FileProcessStatus::Pending,
                    skip_reason: None,
                    processing_ms: 0,
                });
            }
        }

        // Sort: directories first, then by name
        results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        Ok(results)
    }

    // ── File reading ──────────────────────────────────────────────────────────

    /// Đọc nội dung file thành text.
    ///
    /// TODO(phase-3): Implement real readers:
    ///   - Word: `docx` crate hoặc COM
    ///   - Excel: `calamine` crate hoặc COM
    ///   - PDF: `pdfium-render` crate
    ///   - PPT: `pptx` crate hoặc COM
    async fn read_file_content(&self, file: &ScannedFileInfo) -> Option<String> {
        match file.category {
            FileCategory::PlainText | FileCategory::Markdown => {
                // Plain text: đọc trực tiếp
                tokio::fs::read_to_string(&file.path)
                    .await
                    .ok()
                    .map(|content| {
                        // Giới hạn nội dung để tránh vượt context window
                        if content.len() > 10_000 {
                            format!(
                                "{}\n\n[... truncated {} chars ...]",
                                &content[..10_000],
                                content.len() - 10_000
                            )
                        } else {
                            content
                        }
                    })
            }
            FileCategory::Csv => {
                // CSV: đọc và format thành bảng text
                tokio::fs::read_to_string(&file.path)
                    .await
                    .ok()
                    .map(|content| {
                        let lines: Vec<&str> = content.lines().take(50).collect();
                        format!(
                            "[CSV Preview – {} dòng đầu]\n{}",
                            lines.len(),
                            lines.join("\n")
                        )
                    })
            }
            FileCategory::Json => {
                tokio::fs::read_to_string(&file.path)
                    .await
                    .ok()
                    .map(|content| {
                        // Format JSON để dễ đọc hơn
                        serde_json::from_str::<serde_json::Value>(&content)
                            .map(|v| serde_json::to_string_pretty(&v).unwrap_or(content.clone()))
                            .unwrap_or(content)
                    })
                    .map(|s| {
                        if s.len() > 8_000 {
                            format!("{}\n[... truncated]", &s[..8_000])
                        } else {
                            s
                        }
                    })
            }
            FileCategory::Yaml => tokio::fs::read_to_string(&file.path).await.ok(),
            FileCategory::Word => {
                let path = file.path.to_string_lossy().to_string();
                tokio::task::spawn_blocking(move || {
                    let word = com_word::WordApplication::connect_or_launch().ok()?;
                    let content = word.extract_content(&path).ok()?;
                    let text = content.paragraphs.join("\n");
                    let preview = if text.len() > 8_000 {
                        format!("{}\n[truncated]", &text[..8_000])
                    } else {
                        text
                    };
                    Some(format!(
                        "[Word {} pages, {} words]\n{}",
                        content.page_count, content.word_count, preview
                    ))
                })
                .await
                .ok()
                .flatten()
            }
            FileCategory::Excel => {
                let path = file.path.clone();
                tokio::task::spawn_blocking(move || {
                    let mut wb = open_workbook_auto(&path).ok()?;
                    let sheet_names = wb.sheet_names().to_vec();
                    let mut rows_text = Vec::new();
                    for name in &sheet_names {
                        rows_text.push(format!("=== Sheet: {} ===", name));
                        if let Ok(range) = wb.worksheet_range(name) {
                            for row in range.rows().take(30) {
                                rows_text.push(
                                    row.iter()
                                        .map(|c: &calamine::Data| c.to_string())
                                        .collect::<Vec<_>>()
                                        .join("\t"),
                                );
                            }
                        }
                    }
                    Some(format!(
                        "[Excel {} sheets]\n{}",
                        sheet_names.len(),
                        rows_text.join("\n")
                    ))
                })
                .await
                .ok()
                .flatten()
            }
            FileCategory::PowerPoint => {
                let path = file.path.to_string_lossy().to_string();
                tokio::task::spawn_blocking(move || {
                    let ppt = com_ppt::PowerPointApplication::connect_or_launch().ok()?;
                    let info = ppt.inspect_presentation(&path).ok()?;
                    let titles = info
                        .slide_titles
                        .iter()
                        .enumerate()
                        .map(|(i, t)| format!("  Slide {}: {}", i + 1, t))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some(format!(
                        "[PowerPoint {} slides]\n{}",
                        info.slide_count, titles
                    ))
                })
                .await
                .ok()
                .flatten()
            }
            FileCategory::Pdf => {
                let path = file.path.clone();
                tokio::task::spawn_blocking(move || {
                    let bytes = std::fs::read(&path).ok()?;
                    let text: String = bytes
                        .iter()
                        .filter(|&&b| (0x20..0x7F).contains(&b) || b == b'\n')
                        .map(|&b| b as char)
                        .collect();
                    let lines = text
                        .lines()
                        .filter(|l| l.trim().len() > 3)
                        .take(100)
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some(format!("[PDF text extract]\n{}", lines))
                })
                .await
                .ok()
                .flatten()
            }
            FileCategory::Email => {
                tokio::fs::read_to_string(&file.path)
                    .await
                    .ok()
                    .map(|raw| {
                        let headers: Vec<String> = raw
                            .lines()
                            .take_while(|l| !l.is_empty())
                            .filter(|l| {
                                l.starts_with("From:")
                                    || l.starts_with("Subject:")
                                    || l.starts_with("Date:")
                                    || l.starts_with("To:")
                            })
                            .map(String::from)
                            .collect();
                        let body_start = raw
                            .find("\r\n\r\n")
                            .or_else(|| raw.find("\n\n"))
                            .unwrap_or(raw.len());
                        let body: String = raw[body_start..].chars().take(500).collect();
                        format!("[Email]\n{}\n\n{}", headers.join("\n"), body)
                    })
            }
            FileCategory::Image => {
                tokio::fs::read(&file.path)
                    .await
                    .ok()
                    .map(|bytes| {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                        format!("[IMAGE_BASE64]\n{}", b64)
                    })
            }
            FileCategory::Unknown => None,
        }
    }

    // ── LLM summarization ─────────────────────────────────────────────────────

    /// Tóm tắt nội dung một file qua LLM.
    async fn summarize_content(
        &self,
        file: &ScannedFileInfo,
        content: Option<&str>,
        config: &FolderScanConfig,
    ) -> anyhow::Result<String> {
        let content_str = match content {
            Some(c) if !c.is_empty() => c,
            _ => return Ok(format!("Không thể đọc nội dung `{}`.", file.name)),
        };

        let mut text_content = content_str.to_string();
        let mut image_base64s = vec![];

        if text_content.starts_with("[IMAGE_BASE64]\n") {
            let b64 = text_content.trim_start_matches("[IMAGE_BASE64]\n").trim().to_string();
            image_base64s.push(b64);
            text_content = format!("(Hình ảnh đính kèm: {})", file.name);
        }

        let lang = if config.report_language == "vi" { "tiếng Việt" } else { "English" };
        let detail = match config.detail_level.as_str() {
            "brief"    => "1-2 câu ngắn gọn",
            "detailed" => "5-10 câu chi tiết kèm số liệu",
            _          => "3-5 câu nêu nội dung chính và số liệu",
        };

        if let Some(llm_arc) = &self.llm_gateway {
            let llm = llm_arc.read().await;
            let prompt = format!(
                "File: `{}`\nLoại: {:?}\nNội dung:\n---\n{}\n---\nViết tóm tắt {} bằng {}.",
                file.name, file.category,
                &text_content[..text_content.len().min(6000)],
                detail, lang
            );
            let req = crate::llm_gateway::LlmRequest::new(vec![
                crate::llm_gateway::LlmMessage::system(
                    "Bạn là trợ lý tóm tắt tài liệu chuyên nghiệp. Nếu là hình ảnh, hãy mô tả chi tiết nội dung hình ảnh đó. Chỉ trả lời phần tóm tắt, không giải thích thêm."
                ),
                crate::llm_gateway::LlmMessage::user_with_images(prompt, image_base64s),
            ])
            .with_max_tokens(512)
            .with_temperature(0.3);
            if let Ok(resp) = llm.complete(req).await {
                return Ok(resp.content);
            }
        }

        // Fallback khi không có LLM
        Ok(format!(
            "`{}` ({:?}, {:.1} KB) – {} dòng.",
            file.name, file.category,
            file.size_bytes as f64 / 1024.0,
            content_str.lines().count()
        ))
    }

    /// Tạo tóm tắt tổng quan toàn bộ folder từ các file summaries.
    async fn generate_folder_summary(
        &self,
        files: &[ScannedFileInfo],
        config: &FolderScanConfig,
    ) -> Option<String> {
        let done_files: Vec<&ScannedFileInfo> = files
            .iter()
            .filter(|f| f.status == FileProcessStatus::Done)
            .collect();
        if done_files.is_empty() {
            return None;
        }

        let file_list = done_files
            .iter()
            .take(20)
            .map(|f| format!("- **{}**: {}", f.name, f.summary.as_deref().unwrap_or("(không có tóm tắt)")))
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(llm_arc) = &self.llm_gateway {
            let llm = llm_arc.read().await;
            let lang = if config.report_language == "vi" { "tiếng Việt" } else { "English" };
            let prompt = format!(
                "Đây là tóm tắt {} file trong folder `{}`:\n{}\n\nViết tổng quan 5-8 câu về nội dung và chủ đề chính bằng {}.",
                done_files.len(), config.folder_path.display(), file_list, lang
            );
            let req = crate::llm_gateway::LlmRequest::new(vec![
                crate::llm_gateway::LlmMessage::user(prompt),
            ])
            .with_max_tokens(768)
            .with_temperature(0.4);
            if let Ok(resp) = llm.complete(req).await {
                return Some(resp.content);
            }
        }

        Some(format!("Đã xử lý {}/{} file.\n\n{}", done_files.len(), files.len(), file_list))
    }

    // ── Output generation ─────────────────────────────────────────────────────

    /// Tạo các file output (Word, PPT, Excel) từ kết quả scan.
    async fn generate_output_documents(
        &mut self,
        files: &[ScannedFileInfo],
        _folder_summary: &Option<String>,
        config: &FolderScanConfig,
        _scan_id: &str,
    ) -> anyhow::Result<Vec<OutputFileInfo>> {
        let mut outputs: Vec<OutputFileInfo> = Vec::new();

        let folder_name = config
            .folder_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "scan".to_string());

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let prefix = config.output_filename_prefix.as_deref().unwrap_or(&folder_name);
        let output_dir = config.output_dir.clone().unwrap_or_else(|| config.folder_path.clone());

        if let Err(e) = tokio::fs::create_dir_all(&output_dir).await {
            warn!(error = %e, "Could not create output directory");
        }

        let formats_to_generate = match config.output_format {
            ScanOutputFormat::All => vec![
                ScanOutputFormat::WordReport,
                ScanOutputFormat::PptSlides,
                ScanOutputFormat::ExcelSummary,
            ],
            ref fmt => vec![fmt.clone()],
        };

        let done_files: Vec<&ScannedFileInfo> = files
            .iter()
            .filter(|f| f.status == FileProcessStatus::Done)
            .collect();

        for format in formats_to_generate {
            let ext = match &format {
                ScanOutputFormat::WordReport   => "docx",
                ScanOutputFormat::PptSlides    => "pptx",
                ScanOutputFormat::ExcelSummary => "xlsx",
                ScanOutputFormat::All          => unreachable!(),
            };
            let output_path = output_dir.join(format!("{}_{}_TongHop.{}", prefix, timestamp, ext));

            match &format {
                ScanOutputFormat::WordReport => {
                    let report = done_files.iter()
                        .map(|f| format!("## {}\n{}\n", f.name, f.summary.as_deref().unwrap_or("(không có tóm tắt)")))
                        .collect::<Vec<_>>().join("\n");
                    let out_str = output_path.to_string_lossy().to_string();
                    let _ = tokio::task::spawn_blocking(move || {
                        let word = com_word::WordApplication::connect_or_launch()?;
                        word.create_report_from_template(None, &report, Some(&out_str))
                    }).await;
                }
                ScanOutputFormat::PptSlides => {
                    let slides: Vec<crate::agents::office_master::com_ppt::SlideSpec> = done_files.iter()
                        .map(|f| crate::agents::office_master::com_ppt::SlideSpec {
                            title: f.name.clone(),
                            body_lines: f.summary.as_deref().unwrap_or("")
                                .lines().take(4).map(String::from).collect(),
                            layout: 2,
                        }).collect();
                    let out_str = output_path.to_string_lossy().to_string();
                    let _ = tokio::task::spawn_blocking(move || {
                        let ppt = com_ppt::PowerPointApplication::connect_or_launch()?;
                        ppt.create_from_outline(None, &slides, &out_str, None)
                    }).await;
                }
                ScanOutputFormat::ExcelSummary => {
                    let headers = ["Tên file".to_string(), "Loại".to_string(),
                        "Kích thước (KB)".to_string(), "Tóm tắt".to_string()];
                    let rows: Vec<Vec<String>> = done_files.iter().map(|f| vec![
                        f.name.clone(),
                        format!("{:?}", f.category),
                        format!("{:.1}", f.size_bytes as f64 / 1024.0),
                        f.summary.as_deref().unwrap_or("").chars().take(200).collect(),
                    ]).collect();
                    let out_str = output_path.to_string_lossy().to_string();
                    let _ = tokio::task::spawn_blocking(move || {
                        let excel = crate::agents::analyst::excel_com::ExcelApplication::connect_or_launch()?;
                        excel.open_workbook(&out_str)?;
                        let values: Vec<Vec<serde_json::Value>> =
                            std::iter::once(headers.iter().map(|h| serde_json::Value::String(h.clone())).collect())
                            .chain(rows.iter().map(|r| r.iter().map(|c| serde_json::Value::String(c.clone())).collect()))
                            .collect();
                        excel.write_range_2d("Sheet1", "A1", &values, None)
                    }).await;
                }
                ScanOutputFormat::All => unreachable!(),
            }

            let size_bytes = output_path.metadata().ok().map_or(0, |m| m.len());
            outputs.push(OutputFileInfo {
                format: format.clone(),
                path: output_path,
                size_bytes,
                page_count:  if matches!(&format, ScanOutputFormat::WordReport)   { Some(1) } else { None },
                sheet_count: if matches!(&format, ScanOutputFormat::ExcelSummary) { Some(1) } else { None },
                slide_count: if matches!(&format, ScanOutputFormat::PptSlides)    { Some(done_files.len() as u32) } else { None },
            });
        }

        Ok(outputs)
    }

    // ── Other action handlers ─────────────────────────────────────────────────

    async fn list_files(&self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let folder = task
            .parameters
            .get("folder_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("folder_path là bắt buộc"))?;

        let config = FolderScanConfig {
            folder_path: folder.clone(),
            ..FolderScanConfig::default()
        };

        let files = self.discover_files(&config).await?;

        let file_list: String = files
            .iter()
            .map(|f| {
                format!(
                    "  • {} ({:?}, {:.1} KB)",
                    f.name,
                    f.category,
                    f.size_bytes as f64 / 1024.0
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let supported = files.iter().filter(|f| f.category.is_supported()).count();
        let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();

        Ok(AgentOutput {
            content: format!(
                "📁 **Danh sách file trong `{}`**\n\n\
                 Tổng: {} file | Hỗ trợ: {} file | Tổng kích thước: {:.1} MB\n\n\
                 {}",
                folder.display(),
                files.len(),
                supported,
                total_size as f64 / 1024.0 / 1024.0,
                file_list
            ),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "folder": folder,
                "total_files": files.len(),
                "supported_files": supported,
                "total_size_bytes": total_size,
            })),
        })
    }

    async fn summarize_single_file(&self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let file_path = task
            .parameters
            .get("file_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("file_path là bắt buộc"))?;

        if !file_path.exists() {
            return Err(anyhow::anyhow!(
                "File không tồn tại: {}",
                file_path.display()
            ));
        }

        let ext = file_path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase().to_string())
            .unwrap_or_default();

        let category = FileCategory::from_extension(&ext);
        let metadata = file_path.metadata().ok();
        let size_bytes = metadata.map_or(0, |m| m.len());

        let file_info = ScannedFileInfo {
            path: file_path.clone(),
            name: file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            relative_path: PathBuf::from(file_path.file_name().unwrap_or_default()),
            extension: ext,
            category,
            size_bytes,
            created_at: None,
            modified_at: None,
            summary: None,
            keywords: vec![],
            extracted_metrics: None,
            status: FileProcessStatus::Pending,
            skip_reason: None,
            processing_ms: 0,
        };

        let config = FolderScanConfig::default();
        let content = self.read_file_content(&file_info).await;
        let summary = self
            .summarize_content(&file_info, content.as_deref(), &config)
            .await?;

        Ok(AgentOutput {
            content: format!(
                "📄 **Tóm tắt file: `{}`**\n\n{}\n\n\
                 *(Loại: {:?} | Kích thước: {:.1} KB)*",
                file_info.name,
                summary,
                file_info.category,
                size_bytes as f64 / 1024.0
            ),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file_path,
                "category": format!("{:?}", file_info.category),
                "size_bytes": size_bytes,
            })),
        })
    }

    async fn extract_metrics(&self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let folder = task
            .parameters
            .get("folder_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("folder_path là bắt buộc"))?;

        let config = FolderScanConfig {
            folder_path: folder.clone(),
            ..FolderScanConfig::default()
        };

        let files = self.discover_files(&config).await?;

        let excel_files: Vec<_> = files
            .into_iter()
            .filter(|f| matches!(f.category, FileCategory::Excel | FileCategory::Csv))
            .collect();

        let mut all_metrics = serde_json::Map::new();
        for f in &excel_files {
            let path = f.path.clone();
            if let Some(metrics) = tokio::task::spawn_blocking(move || {
                let wb = open_workbook_auto(&path).ok()?;
                Some(serde_json::json!({ "sheets": wb.sheet_names() }))
            })
            .await
            .ok()
            .flatten()
            {
                all_metrics.insert(f.name.clone(), metrics);
            }
        }
        
        Ok(AgentOutput {
            content: format!("Trích xuất {} file Excel/CSV", excel_files.len()),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::Value::Object(all_metrics)),
        })
    }

    async fn search_content(&self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let query = task
            .parameters
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or(&task.message)
            .to_lowercase(); // Make case-insensitive

        let folder = task
            .parameters
            .get("folder_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("folder_path là bắt buộc"))?;

        let config = FolderScanConfig {
            folder_path: folder.clone(),
            ..FolderScanConfig::default()
        };

        let files = self.discover_files(&config).await?;
        let mut results = Vec::new();

        for file in files {
            if !file.category.is_supported() {
                continue;
            }
            if let Some(content) = self.read_file_content(&file).await {
                let content_lower = content.to_lowercase();
                if content_lower.contains(&query) {
                    let count = content_lower.matches(&query).count();
                    // snippet
                    let idx = content_lower.find(&query).unwrap();
                    let start = idx.saturating_sub(50);
                    let end = (idx + query.len() + 50).min(content.len());
                    let snippet = &content[start..end];
                    let snippet_clean = snippet.replace('\n', " ");

                    results.push(format!(
                        "- **{}** ({} matches):\n  `...{}...`",
                        file.name, count, snippet_clean
                    ));
                }
            }
        }

        let content = if results.is_empty() {
            format!("Không tìm thấy `{}` trong `{}`", query, folder.display())
        } else {
            format!(
                "Tìm thấy `{}` trong {} file:\n\n{}",
                query,
                results.len(),
                results.join("\n\n")
            )
        };

        Ok(AgentOutput {
            content,
            committed: false,
            tokens_used: None,
            metadata: None,
        })
    }

    async fn get_scan_progress(&self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let scan_id = task
            .parameters
            .get("scan_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if let Some(started_at) = self.active_scans.get(scan_id) {
            let elapsed = (Utc::now() - *started_at).num_seconds();
            Ok(AgentOutput {
                content: format!("Scan `{}` đang chạy, đã {elapsed}s.", scan_id),
                committed: false,
                tokens_used: None,
                metadata: Some(serde_json::json!({
                    "scan_id": scan_id,
                    "status": "running",
                    "elapsed_seconds": elapsed,
                })),
            })
        } else if scan_id.is_empty() {
            let active: Vec<&String> = self.active_scans.keys().collect();
            Ok(AgentOutput {
                content: format!("Đang có {} scan hoạt động: {:?}", active.len(), active),
                committed: false,
                tokens_used: None,
                metadata: Some(serde_json::json!({ "active_scans": active })),
            })
        } else {
            Ok(AgentOutput {
                content: format!("Không tìm thấy scan `{}`.", scan_id),
                committed: false,
                tokens_used: None,
                metadata: None,
            })
        }
    }

    async fn cancel_scan(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let scan_id = task
            .parameters
            .get("scan_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("scan_id là bắt buộc"))?;

        if self.active_scans.remove(scan_id).is_some() {
            Ok(AgentOutput {
                content: format!("✅ Đã huỷ scan `{}`.", scan_id),
                committed: false,
                tokens_used: None,
                metadata: Some(serde_json::json!({ "scan_id": scan_id, "cancelled": true })),
            })
        } else {
            Err(anyhow::anyhow!(
                "Không tìm thấy scan đang chạy với id `{}`.",
                scan_id
            ))
        }
    }

    // ── Progress emitter ──────────────────────────────────────────────────────

    async fn emit_progress(&self, event: ScanProgressEvent) {
        if let Some(ref tx) = self.progress_tx {
            if let Err(e) = tx.send(event).await {
                debug!(error = %e, "Progress channel closed, skipping emit");
            }
        }
    }

    pub fn metrics(&self) -> &FolderScannerMetrics {
        &self.metrics
    }
}

impl Default for FolderScannerAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl Agent for FolderScannerAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn name(&self) -> &str {
        "Folder Scanner Agent"
    }

    fn description(&self) -> &str {
        "Quét folder, đọc và tóm tắt từng file, tạo báo cáo tổng hợp \
         dưới dạng Word report, PowerPoint slides hoặc Excel summary."
    }

    fn version(&self) -> &str {
        "0.1.0-stub"
    }

    fn supported_actions(&self) -> Vec<String> {
        ACTIONS.iter().map(|s| s.to_string()).collect()
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "scan_folder_to_word".to_string(),
                description: "Quét toàn bộ file trong một thư mục và tạo báo cáo tổng hợp dạng Word (.docx). Tham số: `folder_path` (bắt buộc), `output_path` (tuỳ chọn).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string", "description": "Đường dẫn thư mục cần quét" },
                        "output_path": { "type": "string", "description": "Đường dẫn file Word đầu ra (tuỳ chọn)" }
                    },
                    "required": ["folder_path"]
                }),
                tags: vec!["folder".into(), "thư mục".into(), "word".into(), "docx".into(), "báo cáo".into(), "tổng hợp".into(), "scan".into(), "quét".into()],
            },
            crate::mcp::McpTool {
                name: "scan_folder_to_excel".to_string(),
                description: "Quét thư mục và xuất danh sách file + metadata ra file Excel. Tham số: `folder_path`, `output_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" },
                        "output_path": { "type": "string" }
                    },
                    "required": ["folder_path"]
                }),
                tags: vec!["folder".into(), "thư mục".into(), "excel".into(), "xlsx".into(), "danh sách".into(), "scan".into(), "quét".into()],
            },
            crate::mcp::McpTool {
                name: "list_folder_files".to_string(),
                description: "Liệt kê nhanh danh sách file trong thư mục (không tóm tắt nội dung). Tham số: `folder_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" }
                    },
                    "required": ["folder_path"]
                }),
                tags: vec!["folder".into(), "thư mục".into(), "list".into(), "danh sách".into(), "liệt kê".into(), "file".into()],
            },
            crate::mcp::McpTool {
                name: "read_and_summarize_file".to_string(),
                description: "Đọc và tóm tắt nội dung của một file cụ thể (Word, Excel, PDF, TXT). Tham số: `file_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Đường dẫn đầy đủ tới file cần đọc" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["read".into(), "đọc".into(), "tóm tắt".into(), "summarize".into(), "word".into(), "pdf".into(), "excel".into(), "file".into()],
            },
            crate::mcp::McpTool {
                name: "search_folder_content".to_string(),
                description: "Tìm kiếm từ khóa trong nội dung tất cả file của một thư mục. Tham số: `folder_path`, `query`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" },
                        "query": { "type": "string", "description": "Từ khóa cần tìm" }
                    },
                    "required": ["folder_path", "query"]
                }),
                tags: vec!["folder".into(), "thư mục".into(), "search".into(), "tìm kiếm".into(), "tìm".into(), "content".into()],
            },
            crate::mcp::McpTool {
                name: "extract_metrics_from_folder".to_string(),
                description: "Trích xuất và tổng hợp số liệu từ các file Excel/CSV trong thư mục. Tham số: `folder_path`, `metric_columns`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" },
                        "metric_columns": { "type": "array", "items": { "type": "string" }, "description": "Danh sách tên cột cần trích xuất" }
                    },
                    "required": ["folder_path"]
                }),
                tags: vec!["folder".into(), "excel".into(), "metrics".into(), "số liệu".into(), "tổng hợp".into(), "extract".into()],
            },
        ]
    }


    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        info!("FolderScannerAgent initialising");
        self.status = AgentStatus::Idle;
        info!("FolderScannerAgent ready");
        Ok(())
    }

    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        if self.llm_gateway.is_none() {
            self.llm_gateway = task.llm_gateway.clone();
        }
        self.status = AgentStatus::Busy;
        let result = self.dispatch_action(&task).await;
        self.status = AgentStatus::Idle;

        if result.is_err() {
            self.metrics.failed_scans += 1;
        }

        result
    }

    fn status_info(&self) -> AgentStatusInfo {
        AgentStatusInfo {
            id: self.id.to_string(),
            name: self.name().to_string(),
            status: self.status.to_string(),
            last_used: None,
            total_tasks: self.metrics.total_scans,
            error_count: self.metrics.failed_scans as u32,
            avg_duration_ms: 0.0,
            capabilities: self.supported_actions(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_task(action: &str, params: HashMap<String, serde_json::Value>) -> AgentTask {
        AgentTask {
            task_id: Uuid::new_v4().to_string(),
            action: action.to_string(),
            intent: crate::orchestrator::intent::Intent::GeneralChat(Default::default()),
            message: String::new(),
            context_file: None,
            session_id: "test-session".into(),
            parameters: params,
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        }
    }

    // ── FileCategory detection ────────────────────────────────────────────────

    #[test]
    fn test_file_category_from_extension() {
        assert_eq!(FileCategory::from_extension("docx"), FileCategory::Word);
        assert_eq!(FileCategory::from_extension("XLSX"), FileCategory::Excel);
        assert_eq!(FileCategory::from_extension("pdf"), FileCategory::Pdf);
        assert_eq!(FileCategory::from_extension("md"), FileCategory::Markdown);
        assert_eq!(FileCategory::from_extension("csv"), FileCategory::Csv);
        assert_eq!(FileCategory::from_extension("json"), FileCategory::Json);
        assert_eq!(FileCategory::from_extension("exe"), FileCategory::Unknown);
        assert_eq!(FileCategory::from_extension(""), FileCategory::Unknown);
    }

    #[test]
    fn test_file_category_supported() {
        assert!(FileCategory::Word.is_supported());
        assert!(FileCategory::PlainText.is_supported());
        assert!(!FileCategory::Unknown.is_supported());
    }

    // ── ScanOutputFormat ──────────────────────────────────────────────────────

    #[test]
    fn test_output_format_display() {
        assert!(ScanOutputFormat::WordReport.display_name().contains("Word"));
        assert!(
            ScanOutputFormat::PptSlides.display_name().contains("PPT")
                || ScanOutputFormat::PptSlides.display_name().contains("Slide")
        );
        assert!(ScanOutputFormat::ExcelSummary
            .display_name()
            .contains("Excel"));
    }

    #[test]
    fn test_output_format_extensions() {
        assert!(ScanOutputFormat::WordReport
            .file_extensions()
            .contains(&"docx"));
        assert!(ScanOutputFormat::PptSlides
            .file_extensions()
            .contains(&"pptx"));
        assert!(ScanOutputFormat::ExcelSummary
            .file_extensions()
            .contains(&"xlsx"));
        assert_eq!(ScanOutputFormat::All.file_extensions().len(), 3);
    }

    // ── Agent construction ────────────────────────────────────────────────────

    #[test]
    fn test_agent_creates_ok() {
        let agent = FolderScannerAgent::new();
        assert_eq!(agent.id().to_string(), "folder_scanner");
        assert_eq!(agent.status(), AgentStatus::Idle);
    }

    #[test]
    fn test_supported_actions_not_empty() {
        let agent = FolderScannerAgent::new();
        let actions = agent.supported_actions();
        assert!(!actions.is_empty());
        assert!(actions.contains(&"scan_folder_to_word".into()));
        assert!(actions.contains(&"scan_folder_to_ppt".into()));
        assert!(actions.contains(&"scan_folder_to_excel".into()));
        assert!(actions.contains(&"list_folder_files".into()));
        assert!(actions.contains(&"read_and_summarize_file".into()));
    }

    // ── Build scan config ─────────────────────────────────────────────────────

    #[test]
    fn test_build_scan_config_missing_folder_errors() {
        let agent = FolderScannerAgent::new();
        let task = make_task("scan_folder_to_word", HashMap::new());
        let result = agent.build_scan_config(&task, ScanOutputFormat::WordReport);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("folder_path"));
    }

    #[test]
    fn test_build_scan_config_with_temp_dir() {
        let agent = FolderScannerAgent::new();
        let tmp = std::env::temp_dir();
        let mut params = HashMap::new();
        params.insert(
            "folder_path".into(),
            serde_json::json!(tmp.to_string_lossy().as_ref()),
        );
        params.insert("recursive".into(), serde_json::json!(false));
        params.insert("language".into(), serde_json::json!("en"));

        let task = make_task("scan_folder_to_word", params);
        let result = agent.build_scan_config(&task, ScanOutputFormat::WordReport);
        assert!(result.is_ok());

        let cfg = result.unwrap();
        assert_eq!(cfg.report_language, "en");
        assert!(!cfg.recursive);
        assert_eq!(cfg.output_format, ScanOutputFormat::WordReport);
    }

    // ── File discovery ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_discover_files_in_temp_dir() {
        let agent = FolderScannerAgent::new();
        let tmp = std::env::temp_dir();

        let cfg = FolderScanConfig {
            folder_path: tmp.clone(),
            recursive: false,
            max_files: 10,
            ..FolderScanConfig::default()
        };

        // Should not panic even if temp dir has many files
        let result = agent.discover_files(&cfg).await;
        assert!(result.is_ok());
        // We can't assert exact count but it should be bounded by max_files
        assert!(result.unwrap().len() <= 10);
    }

    #[tokio::test]
    async fn test_discover_files_extension_filter() {
        let agent = FolderScannerAgent::new();
        let tmp = std::env::temp_dir();

        let cfg = FolderScanConfig {
            folder_path: tmp,
            recursive: false,
            include_extensions: Some(vec!["xlsx".into()]),
            max_files: 100,
            ..FolderScanConfig::default()
        };

        let result = agent.discover_files(&cfg).await.unwrap();
        // All discovered files should be xlsx or skipped
        for file in &result {
            if file.extension != "xlsx" {
                assert!(matches!(
                    file.status,
                    FileProcessStatus::Skipped | FileProcessStatus::Pending
                ));
            }
        }
    }

    // ── File content reading ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_plain_text_file() {
        let agent = FolderScannerAgent::new();

        // Create a temp text file
        let tmp_path = std::env::temp_dir().join("oh_test_read.txt");
        tokio::fs::write(&tmp_path, "Nội dung test tiếng Việt 123")
            .await
            .unwrap();

        let file_info = ScannedFileInfo {
            path: tmp_path.clone(),
            name: "oh_test_read.txt".into(),
            relative_path: PathBuf::from("oh_test_read.txt"),
            extension: "txt".into(),
            category: FileCategory::PlainText,
            size_bytes: 50,
            created_at: None,
            modified_at: None,
            summary: None,
            keywords: vec![],
            extracted_metrics: None,
            status: FileProcessStatus::Pending,
            skip_reason: None,
            processing_ms: 0,
        };

        let content = agent.read_file_content(&file_info).await;
        assert!(content.is_some());
        assert!(content.unwrap().contains("Nội dung test"));

        tokio::fs::remove_file(&tmp_path).await.ok();
    }

    #[tokio::test]
    async fn test_read_unknown_file_returns_none() {
        let agent = FolderScannerAgent::new();

        let file_info = ScannedFileInfo {
            path: PathBuf::from("test.xyz"),
            name: "test.xyz".into(),
            relative_path: PathBuf::from("test.xyz"),
            extension: "xyz".into(),
            category: FileCategory::Unknown,
            size_bytes: 100,
            created_at: None,
            modified_at: None,
            summary: None,
            keywords: vec![],
            extracted_metrics: None,
            status: FileProcessStatus::Pending,
            skip_reason: None,
            processing_ms: 0,
        };

        let content = agent.read_file_content(&file_info).await;
        assert!(content.is_none());
    }

    // ── Execute ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_execute_list_files_on_temp_dir() {
        let mut agent = FolderScannerAgent::new();
        let tmp = std::env::temp_dir();
        let mut params = HashMap::new();
        params.insert(
            "folder_path".into(),
            serde_json::json!(tmp.to_string_lossy().as_ref()),
        );

        let task = make_task("list_folder_files", params);
        let result = agent.execute(task).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("file"));
    }

    #[tokio::test]
    async fn test_execute_unknown_action_errors() {
        let mut agent = FolderScannerAgent::new();
        let task = make_task("totally_unknown_action", HashMap::new());
        let result = agent.execute(task).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown action"));
    }

    #[tokio::test]
    async fn test_execute_scan_folder_to_word_stub() {
        let mut agent = FolderScannerAgent::new();
        let tmp = std::env::temp_dir();
        let mut params = HashMap::new();
        params.insert(
            "folder_path".into(),
            serde_json::json!(tmp.to_string_lossy().as_ref()),
        );

        let task = make_task("scan_folder_to_word", params);
        let result = agent.execute(task).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.content.contains("Đã quét") || output.content.contains("folder"));
    }

    // ── Scan progress ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_scan_progress_no_active_scans() {
        let agent = FolderScannerAgent::new();
        let task = make_task("get_scan_progress", HashMap::new());
        let result = agent.get_scan_progress(&task).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("0"));
    }

    #[tokio::test]
    async fn test_cancel_scan_unknown_id_errors() {
        let mut agent = FolderScannerAgent::new();
        let mut params = HashMap::new();
        params.insert("scan_id".into(), serde_json::json!("nonexistent-id"));
        let task = make_task("cancel_scan", params);
        let result = agent.cancel_scan(&task).await;
        assert!(result.is_err());
    }

    // ── Progress channel ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_progress_channel_receives_events() {
        let (tx, mut rx) = mpsc::channel::<ScanProgressEvent>(32);
        let agent = FolderScannerAgent::new().with_progress_channel(tx);

        agent
            .emit_progress(ScanProgressEvent::Started {
                scan_id: "test-scan".into(),
                folder_path: "/tmp/test".into(),
                estimated_files: 5,
            })
            .await;

        let event = rx.try_recv();
        assert!(event.is_ok());
        if let ScanProgressEvent::Started { scan_id, .. } = event.unwrap() {
            assert_eq!(scan_id, "test-scan");
        } else {
            panic!("Wrong event type");
        }
    }

    // ── Metrics ───────────────────────────────────────────────────────────────

    #[test]
    fn test_metrics_initial_state() {
        let agent = FolderScannerAgent::new();
        let m = agent.metrics();
        assert_eq!(m.total_scans, 0);
        assert_eq!(m.total_files_processed, 0);
        assert_eq!(m.word_reports_created, 0);
    }

    // ── FolderScanConfig default ──────────────────────────────────────────────

    #[test]
    fn test_scan_config_default_sensible() {
        let cfg = FolderScanConfig::default();
        assert!(cfg.recursive);
        assert_eq!(cfg.max_files, 200);
        assert_eq!(cfg.max_file_size_bytes, 50 * 1024 * 1024);
        assert!(!cfg.include_thumbnails);
        assert!(cfg.include_toc);
        assert_eq!(cfg.report_language, "vi");
    }
}
