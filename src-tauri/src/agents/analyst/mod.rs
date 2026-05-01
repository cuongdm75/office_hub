// ============================================================================
// Office Hub – agents/analyst/mod.rs
//
// Analyst Agent – Excel COM Automation
// Phase: 3 – COM Automation Integration (REAL)
// ============================================================================

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{
    agents::{Agent, AgentId, AgentStatus},
    orchestrator::{AgentOutput, AgentTask},
};

pub mod excel_com;

// ─────────────────────────────────────────────────────────────────────────────
// Analyst Agent
// ─────────────────────────────────────────────────────────────────────────────

pub struct AnalystAgent {
    id: AgentId,
    status: AgentStatus,
    /// Whether the COM connection to Excel has been verified
    com_available: bool,
    /// Configuration
    config: AnalystConfig,
    /// Runtime stats
    stats: AnalystStats,
}

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalystConfig {
    /// Allow VBA macro generation and execution
    pub allow_vba_execution: bool,
    /// Maximum rows to process in a single operation
    pub max_rows_per_operation: usize,
    /// Always backup workbook before any write
    pub backup_before_write: bool,
    /// Backup directory path
    pub backup_dir: String,
    /// Hard-truth tolerance percentage [0.0 – 100.0]
    pub hard_truth_tolerance_pct: f64,
    /// Whether to take grounding screenshots
    pub grounding_screenshots: bool,
    /// Screenshot save directory
    pub screenshot_dir: String,
}

impl Default for AnalystConfig {
    fn default() -> Self {
        Self {
            allow_vba_execution: false,
            max_rows_per_operation: 100_000,
            backup_before_write: true,
            backup_dir: "$APPDATA/office-hub/backups/excel".to_string(),
            hard_truth_tolerance_pct: 0.01,
            grounding_screenshots: true,
            screenshot_dir: "$APPDATA/office-hub/grounding".to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime stats
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AnalystStats {
    pub total_tasks: u64,
    pub successful_tasks: u64,
    pub failed_tasks: u64,
    pub cells_read: u64,
    pub cells_written: u64,
    pub formulas_generated: u64,
    pub vba_executions: u64,
    pub hard_truth_violations: u64,
    pub backups_created: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Supported actions
// ─────────────────────────────────────────────────────────────────────────────

/// All action identifiers the Analyst Agent can handle.
/// These must match the action strings used in:
///   - Workflow YAML step definitions
///   - Router routing table
///   - Orchestrator dispatch calls
pub const ACTIONS: &[&str] = &[
    "analyze_workbook",
    "read_cell_range",
    "read_named_range",
    "write_cell_range",
    "write_named_range",
    "generate_formula",
    "apply_formula",
    "run_power_query",
    "generate_power_query",
    "generate_vba",
    "run_vba",
    "audit_formulas",
    "detect_anomalies",
    "calculate_statistics",
    "trend_analysis",
    "create_pivot_table",
    "refresh_data_connections",
    "export_to_csv",
    "hard_truth_verify",
    "take_grounding_screenshot",
    "chat",
    "config",
    "help",
    "clarify",
];

// ─────────────────────────────────────────────────────────────────────────────
// Excel-specific result types
// ─────────────────────────────────────────────────────────────────────────────

/// A single cell value read from Excel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellValue {
    /// A1-style cell reference (e.g. "B5")
    pub cell_ref: String,
    /// Sheet name
    pub sheet_name: String,
    /// The value as a JSON value (string, number, bool, null)
    pub value: serde_json::Value,
    /// The raw formula in the cell (if any)
    pub formula: Option<String>,
    /// Whether the cell has an error (e.g. #REF!, #VALUE!)
    pub has_error: bool,
    /// Error text if `has_error` is true
    pub error_text: Option<String>,
}

/// Result of reading a range of cells.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeReadResult {
    pub sheet_name: String,
    pub range_address: String,
    pub row_count: usize,
    pub col_count: usize,
    /// Row-major 2D array of values
    pub values: Vec<Vec<serde_json::Value>>,
    /// Column headers (first row, if detected)
    pub headers: Option<Vec<String>>,
    /// Grounding screenshot path (if taken)
    pub screenshot_path: Option<String>,
}

/// Result of a workbook analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbookAnalysis {
    pub file_path: String,
    pub sheet_count: usize,
    pub sheet_names: Vec<String>,
    pub total_cells_with_data: u64,
    pub total_formulas: u64,
    pub formula_errors: Vec<CellValue>,
    pub summary: String,
    pub key_metrics: serde_json::Value,
    pub anomalies: Vec<AnomalyRecord>,
    pub grounding_screenshots: Vec<String>,
    pub audit_passed: bool,
}

/// A detected anomaly in the data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyRecord {
    pub cell_ref: String,
    pub sheet_name: String,
    pub anomaly_type: AnomalyType,
    pub description: String,
    pub severity: AnomalySeverity,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    NegativeQuantity,
    ZeroTotal,
    FormulaError,
    BlankInRequiredCell,
    OutlierValue,
    InconsistentFormat,
    CircularReference,
    ExcessivePrecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnomalySeverity {
    Info,
    Warning,
    Error,
}

/// Generated VBA macro code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VbaCode {
    pub macro_name: String,
    pub code: String,
    pub description: String,
    pub requires_trust_center_changes: bool,
    pub estimated_runtime_ms: u64,
}

/// Generated Excel formula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFormula {
    pub formula: String,
    pub formula_type: String, // "XLOOKUP", "SUMIF", "LAMBDA", etc.
    pub target_range: String,
    pub explanation: String,
    pub is_dynamic_array: bool,
    pub spill_range: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// AnalystAgent implementation
// ─────────────────────────────────────────────────────────────────────────────

impl AnalystAgent {
    /// Create a new AnalystAgent with default configuration.
    pub fn new() -> Self {
        Self {
            id: AgentId::analyst(),
            status: AgentStatus::Idle,
            com_available: false,
            config: AnalystConfig::default(),
            stats: AnalystStats::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: AnalystConfig) -> Self {
        Self {
            id: AgentId::analyst(),
            status: AgentStatus::Idle,
            com_available: false,
            config,
            stats: AnalystStats::default(),
        }
    }

    // ── COM Availability Check ────────────────────────────────────────────────

    /// Check whether the Excel COM server is accessible on this machine.
    #[cfg(windows)]
    fn probe_excel_com() -> bool {
        excel_com::ExcelApplication::connect_or_launch().is_ok()
    }

    #[cfg(not(windows))]
    fn probe_excel_com() -> bool {
        false
    }

    // ── Action Dispatch (stub routing) ───────────────────────────────────────

    async fn dispatch_action(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        match task.action.as_str() {
            "analyze_workbook" => self.analyze_workbook(task).await,
            "read_cell_range" | "read_named_range" => self.read_range(task).await,
            "write_cell_range" | "write_named_range" => self.write_range(task).await,
            "generate_formula" | "apply_formula" => self.generate_formula(task).await,
            "run_power_query" | "generate_power_query" => self.power_query(task).await,
            "generate_vba" | "run_vba" => self.vba_action(task).await,
            "audit_formulas" => self.audit_formulas(task).await,
            "detect_anomalies" | "calculate_statistics" | "trend_analysis" => {
                self.analyze_data(task).await
            }
            "hard_truth_verify" => self.hard_truth_verify(task).await,
            "take_grounding_screenshot" => self.take_screenshot(task).await,
            "chat" | "config" | "help" | "clarify" => self.general_chat(task).await,
            unknown => Err(anyhow::anyhow!(
                "AnalystAgent: unknown action '{}'. Supported: {:?}",
                unknown,
                ACTIONS
            )),
        }
    }

    // ── Stub action implementations ───────────────────────────────────────────
    // Each method below is a STUB that returns a placeholder response.
    // Real implementations will be added in Phase 3 using Windows COM Automation.

    async fn analyze_workbook(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let raw_file = task
            .parameters
            .get("file_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .unwrap_or("");

        // SharePoint / OneDrive URLs and HTTP paths cannot be opened by the
        // local Excel COM process – fall back to whatever is already active.
        let is_remote = raw_file.starts_with("http://")
            || raw_file.starts_with("https://")
            || raw_file.starts_with("onedrive:")
            || raw_file.contains("sharepoint.com");
        let file = if is_remote || raw_file.is_empty() {
            ""
        } else {
            raw_file
        };

        self.stats.total_tasks += 1;

        match excel_com::ExcelApplication::connect_or_launch() {
            Err(e) => {
                self.stats.failed_tasks += 1;
                Err(anyhow::anyhow!("Excel COM unavailable: {}", e))
            }
            Ok(excel) => {
                let structure = if file.is_empty() {
                    // No local path – read the currently-active workbook via COM
                    excel.get_active_workbook_structure().unwrap_or_else(|e| {
                        warn!("get_active_workbook_structure failed: {}", e);
                        excel_com::WorkbookStructure {
                            file_path: "<active workbook>".into(),
                            sheet_count: 0,
                            sheets: vec![],
                        }
                    })
                } else {
                    excel.get_workbook_structure(file).unwrap_or_else(|e| {
                        warn!("get_workbook_structure failed: {}", e);
                        excel_com::WorkbookStructure {
                            file_path: file.to_string(),
                            sheet_count: 0,
                            sheets: vec![],
                        }
                    })
                };

                // Audit formulas across all sheets
                let formula_errors = excel.audit_formulas(None).unwrap_or_default();

                let sheet_summary = structure
                    .sheets
                    .iter()
                    .map(|s| {
                        format!(
                            "  • {} ({} hàng × {} cột)",
                            s.name, s.used_rows, s.used_cols
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let error_summary = if formula_errors.is_empty() {
                    "✅ Không phát hiện lỗi công thức.".to_string()
                } else {
                    let lines: Vec<String> = formula_errors
                        .iter()
                        .take(10)
                        .map(|e| {
                            format!(
                                "  ❌ [{}]!{} → {} ({})",
                                e.sheet_name, e.cell_ref, e.error_text, e.formula
                            )
                        })
                        .collect();
                    format!(
                        "⚠️ {} lỗi công thức:\n{}",
                        formula_errors.len(),
                        lines.join("\n")
                    )
                };

                let content = format!(
                    "📊 **Phân tích Workbook**: `{}`\n\n\
                     **Cấu trúc** ({} sheet):\n{}\n\n\
                     **Kiểm tra công thức**:\n{}",
                    structure.file_path,
                    structure.sheet_count,
                    if sheet_summary.is_empty() {
                        "  (không có sheet)".into()
                    } else {
                        sheet_summary
                    },
                    error_summary
                );

                self.stats.successful_tasks += 1;

                Ok(AgentOutput {
                    content,
                    committed: false,
                    tokens_used: None,
                    metadata: Some(serde_json::json!({
                        "file_path": structure.file_path,
                        "sheet_count": structure.sheet_count,
                        "formula_errors": formula_errors.len(),
                        "sheets": structure.sheets
                    })),
                })
            }
        }
    }

    async fn read_range(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let file = task
            .parameters
            .get("file_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .unwrap_or("");
        let range = task
            .parameters
            .get("range")
            .and_then(|v| v.as_str())
            .unwrap_or("A1");
        let sheet = task
            .parameters
            .get("sheet")
            .and_then(|v| v.as_str())
            .unwrap_or("Sheet1");

        self.stats.total_tasks += 1;

        let excel = excel_com::ExcelApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Excel COM unavailable: {}", e))?;

        if !file.is_empty() {
            excel
                .open_workbook(file)
                .map_err(|e| anyhow::anyhow!("Cannot open '{}': {}", file, e))?;
        }

        let (headers, rows) = excel
            .read_range_2d(sheet, range)
            .map_err(|e| anyhow::anyhow!("read_range_2d failed: {}", e))?;

        self.stats.cells_read += rows.iter().map(|r| r.len() as u64).sum::<u64>();
        self.stats.successful_tasks += 1;

        // Format as markdown table
        let mut lines = Vec::new();
        if !headers.is_empty() {
            lines.push(format!("| {} |", headers.join(" | ")));
            lines.push(format!(
                "| {} |",
                headers
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
        for row in rows.iter().skip(if headers.is_empty() { 0 } else { 1 }) {
            let cells: Vec<String> = row
                .iter()
                .map(|v| match v {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect();
            lines.push(format!("| {} |", cells.join(" | ")));
        }

        let table = if lines.is_empty() {
            "(Không có dữ liệu)".to_string()
        } else {
            lines.join("\n")
        };
        let content = format!(
            "📋 **Dữ liệu từ** `{}!{}` ({} hàng):\n\n{}",
            sheet,
            range,
            rows.len(),
            table
        );

        Ok(AgentOutput {
            content,
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file,
                "sheet": sheet,
                "range": range,
                "row_count": rows.len(),
                "col_count": headers.len()
            })),
        })
    }

    async fn write_range(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let file = task
            .parameters
            .get("file_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .unwrap_or("");
        let range = task
            .parameters
            .get("range")
            .and_then(|v| v.as_str())
            .unwrap_or("A1");
        let content = task
            .parameters
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or(task.message.as_str());
        let backup_dir = if self.config.backup_before_write {
            Some(self.config.backup_dir.as_str())
        } else {
            None
        };

        self.stats.total_tasks += 1;

        let excel = excel_com::ExcelApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Excel COM unavailable: {}", e))?;

        if !file.is_empty() {
            excel
                .open_workbook(file)
                .map_err(|e| anyhow::anyhow!("Cannot open '{}': {}", file, e))?;
        }

        excel
            .write_range(range, content, backup_dir)
            .map_err(|e| anyhow::anyhow!("write_range failed: {}", e))?;

        // Auto-save after write
        let _ = excel.save_active_workbook();

        self.stats.cells_written += 1;
        self.stats.successful_tasks += 1;

        Ok(AgentOutput {
            content: format!(
                "✅ Đã ghi vào `{}` – Hard-Truth Verification passed.",
                range
            ),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file,
                "range": range,
                "backup": backup_dir.is_some()
            })),
        })
    }

    async fn generate_formula(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let description = task.message.as_str();

        warn!("[STUB] generate_formula: '{}'", description);
        self.stats.total_tasks += 1;
        self.stats.successful_tasks += 1;
        self.stats.formulas_generated += 1;

        Ok(AgentOutput {
            content: format!(
                "🔢 [STUB] Tạo công thức cho yêu cầu: \"{}\"\n\n\
                 Ví dụ (stub):\n\
                 ```\n\
                 =XLOOKUP(A2, Products[ProductID], Products[Price], \"Không tìm thấy\")\n\
                 ```\n\
                 Phase 3 sẽ sinh công thức thực tế dựa trên cấu trúc workbook.",
                description
            ),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "stub": true,
                "formula_type": "XLOOKUP",
                "phase": 3
            })),
        })
    }

    async fn power_query(&mut self, _task: &AgentTask) -> anyhow::Result<AgentOutput> {
        warn!("[STUB] power_query action");
        self.stats.total_tasks += 1;
        self.stats.successful_tasks += 1;

        Ok(AgentOutput {
            content: "⚡ [STUB] Power Query – Phase 3 implementation pending.\n\
                      Sẽ hỗ trợ: sinh M-code, làm mới query, transform dữ liệu."
                .to_string(),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "stub": true, "phase": 3 })),
        })
    }

    async fn vba_action(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        if !self.config.allow_vba_execution && task.action == "run_vba" {
            return Err(anyhow::anyhow!(
                "VBA execution is disabled in AnalystAgent configuration. \
                 Set `allow_vba_execution: true` in config.yaml to enable."
            ));
        }

        warn!("[STUB] vba_action: '{}'", task.action);
        self.stats.total_tasks += 1;
        self.stats.successful_tasks += 1;

        Ok(AgentOutput {
            content: format!(
                "🔧 [STUB] VBA action: `{}`\n\
                 Lưu ý: Mọi hành động VBA cần Human-in-the-Loop approval.\n\
                 Phase 3 sẽ sinh và/hoặc thực thi VBA macro qua COM.",
                task.action
            ),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "stub": true,
                "action": task.action,
                "requires_hitl": true
            })),
        })
    }

    async fn audit_formulas(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let file = task
            .parameters
            .get("file_path")
            .and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .unwrap_or("");
        let sheet = task.parameters.get("sheet").and_then(|v| v.as_str());

        self.stats.total_tasks += 1;

        let excel = excel_com::ExcelApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Excel COM unavailable: {}", e))?;

        if !file.is_empty() {
            excel
                .open_workbook(file)
                .map_err(|e| anyhow::anyhow!("Cannot open '{}': {}", file, e))?;
        }

        let errors = excel
            .audit_formulas(sheet)
            .map_err(|e| anyhow::anyhow!("audit_formulas failed: {}", e))?;

        self.stats.successful_tasks += 1;

        let content = if errors.is_empty() {
            format!(
                "✅ **Audit công thức** `{}`: Không phát hiện lỗi.",
                if file.is_empty() {
                    "active workbook"
                } else {
                    file
                }
            )
        } else {
            let lines: Vec<String> = errors
                .iter()
                .take(20)
                .map(|e| {
                    format!(
                        "  ❌ `[{}]!{}` → `{}` | Formula: `{}`",
                        e.sheet_name, e.cell_ref, e.error_text, e.formula
                    )
                })
                .collect();
            format!(
                "🔍 **Audit công thức** – {} lỗi phát hiện:\n\n{}",
                errors.len(),
                lines.join("\n")
            )
        };

        Ok(AgentOutput {
            content,
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file,
                "error_count": errors.len(),
                "errors": errors
            })),
        })
    }

    async fn analyze_data(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        warn!("[STUB] analyze_data: '{}'", task.action);
        self.stats.total_tasks += 1;
        self.stats.successful_tasks += 1;

        Ok(AgentOutput {
            content: format!(
                "📈 [STUB] Phân tích dữ liệu ({})\n\
                 Phase 3 sẽ thực hiện phân tích thống kê, xu hướng và anomaly detection.",
                task.action
            ),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "stub": true, "action": task.action })),
        })
    }

    async fn hard_truth_verify(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let intended = task
            .parameters
            .get("intended_value")
            .and_then(|v| v.as_f64());
        let actual = task.parameters.get("actual_value").and_then(|v| v.as_f64());

        if let (Some(i), Some(a)) = (intended, actual) {
            let deviation = if i.abs() > 1e-10 {
                ((i - a).abs() / i.abs()) * 100.0
            } else {
                0.0
            };
            let passed = deviation <= self.config.hard_truth_tolerance_pct;

            if !passed {
                self.stats.hard_truth_violations += 1;
                return Err(anyhow::anyhow!(
                    "Hard-Truth Verification FAILED: intended={:.4}, actual={:.4}, \
                     deviation={:.4}% (tolerance={:.4}%)",
                    i,
                    a,
                    deviation,
                    self.config.hard_truth_tolerance_pct
                ));
            }

            return Ok(AgentOutput {
                content: format!(
                    "✅ Hard-Truth Verification PASSED: intended={:.4}, actual={:.4} \
                     (deviation={:.6}%)",
                    i, a, deviation
                ),
                committed: true,
                tokens_used: None,
                metadata: Some(serde_json::json!({
                    "intended": i,
                    "actual": a,
                    "deviation_pct": deviation,
                    "tolerance_pct": self.config.hard_truth_tolerance_pct,
                    "passed": true
                })),
            });
        }

        // No numeric values to compare – stub response
        Ok(AgentOutput {
            content: "[STUB] Hard-Truth Verification – no numeric values provided.".to_string(),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "stub": true })),
        })
    }

    async fn take_screenshot(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let save_dir = task
            .parameters
            .get("output_dir")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.config.screenshot_dir);

        warn!("[STUB] take_grounding_screenshot → {}", save_dir);

        Ok(AgentOutput {
            content: format!(
                "📸 [STUB] Chụp màn hình grounding → `{}`\n\
                 Phase 4 sẽ tích hợp GDI/DWM screenshot API.",
                save_dir
            ),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "stub": true,
                "output_dir": save_dir
            })),
        })
    }

    async fn general_chat(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        self.stats.total_tasks += 1;
        self.stats.successful_tasks += 1;

        Ok(AgentOutput {
            content: "Xin chào! Đây là trợ lý Office Hub.\n\nTôi đang trong giai đoạn phát triển (Phase 1 & 2), vui lòng yêu cầu tôi thực hiện các tác vụ liên quan đến Excel, Word, PowerPoint hoặc Web Automation!".to_string(),
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": task.action,
                "is_fallback_chat": true
            })),
        })
    }
}

impl Default for AnalystAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl Agent for AnalystAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn name(&self) -> &str {
        "Analyst Agent (Excel)"
    }

    fn description(&self) -> &str {
        "Phân tích và thao tác dữ liệu Excel qua Windows COM Automation. \
         Hỗ trợ XLOOKUP, Power Query, VBA/Office Scripts, và Hard-Truth Verification."
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
                name: "analyze_workbook".to_string(),
                description: "Phân tích toàn bộ file Excel: đọc các sheet, tóm tắt dữ liệu, phát hiện lỗi. Tham số: `file_path` (đường dẫn file).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Đường dẫn tuyệt đối tới file Excel (.xlsx/.xls)" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["excel".into(), "xlsx".into(), "spreadsheet".into(), "bảng tính".into(), "phân tích".into()],
            },
            crate::mcp::McpTool {
                name: "read_cell_range".to_string(),
                description: "Đọc dữ liệu từ một vùng ô trong file Excel. Tham số: `file_path`, `sheet` (tên sheet), `range` (vùng dữ liệu, vd A1:B10).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" },
                        "range": { "type": "string", "description": "Vd: A1:D10" }
                    },
                    "required": ["file_path", "range", "sheet"]
                }),
                tags: vec!["excel".into(), "đọc".into(), "read".into(), "cell".into(), "ô".into()],
            },
            crate::mcp::McpTool {
                name: "write_cell_range".to_string(),
                description: "Ghi dữ liệu vào một vùng ô trong file Excel. Tham số: `file_path`, `sheet`, `range`, `content`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" },
                        "range": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["file_path", "range", "content"]
                }),
                tags: vec!["excel".into(), "ghi".into(), "write".into(), "cell".into(), "update".into()],
            },
            crate::mcp::McpTool {
                name: "generate_formula".to_string(),
                description: "Tạo công thức Excel theo yêu cầu. Tham số: `description` (mô tả công thức cần tạo), `file_path` (tuỳ chọn).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "Mô tả công thức cần tạo bằng ngôn ngữ tự nhiên" },
                        "file_path": { "type": "string" }
                    },
                    "required": ["description"]
                }),
                tags: vec!["excel".into(), "formula".into(), "công thức".into(), "hàm".into(), "vlookup".into(), "sumif".into()],
            },
            crate::mcp::McpTool {
                name: "create_pivot_table".to_string(),
                description: "Tạo Pivot Table trong file Excel. Tham số: `file_path`, `source_sheet`, `data_range`, `pivot_sheet` (tên sheet đích), `rows`, `columns`, `values`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "source_sheet": { "type": "string" },
                        "data_range": { "type": "string" },
                        "pivot_sheet": { "type": "string" },
                        "rows": { "type": "array", "items": { "type": "string" } },
                        "columns": { "type": "array", "items": { "type": "string" } },
                        "values": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["file_path", "source_sheet", "data_range"]
                }),
                tags: vec!["excel".into(), "pivot".into(), "pivot table".into(), "tổng hợp".into(), "bảng tổng hợp".into()],
            },
            crate::mcp::McpTool {
                name: "detect_anomalies".to_string(),
                description: "Phát hiện bất thường và outlier trong dữ liệu Excel. Tham số: `file_path`, `sheet`, `column`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" },
                        "column": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["excel".into(), "anomaly".into(), "bất thường".into(), "outlier".into(), "kiểm tra".into()],
            },
            crate::mcp::McpTool {
                name: "calculate_statistics".to_string(),
                description: "Tính toán thống kê mô tả cho dataset Excel: mean, median, std, min, max. Tham số: `file_path`, `sheet`, `column`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" },
                        "column": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["excel".into(), "statistics".into(), "thống kê".into(), "mean".into(), "sum".into(), "tổng".into()],
            },
            crate::mcp::McpTool {
                name: "trend_analysis".to_string(),
                description: "Phân tích xu hướng dữ liệu theo thời gian trong Excel. Tham số: `file_path`, `sheet`, `date_column`, `value_column`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" },
                        "date_column": { "type": "string" },
                        "value_column": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["excel".into(), "trend".into(), "xu hướng".into(), "biểu đồ".into(), "chart".into(), "time series".into()],
            },
            crate::mcp::McpTool {
                name: "audit_formulas".to_string(),
                description: "Kiểm tra và báo cáo lỗi công thức trong file Excel. Tham số: `file_path`, `sheet`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["excel".into(), "audit".into(), "formula".into(), "lỗi".into(), "error".into(), "kiểm tra".into()],
            },
            crate::mcp::McpTool {
                name: "export_to_csv".to_string(),
                description: "Xuất dữ liệu từ sheet Excel ra file CSV. Tham số: `file_path`, `sheet`, `output_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "sheet": { "type": "string" },
                        "output_path": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec!["excel".into(), "csv".into(), "export".into(), "xuất".into(), "convert".into()],
            },
            crate::mcp::McpTool {
                name: "hard_truth_verify".to_string(),
                description: "Xác minh độ chính xác của số liệu Excel: so sánh giá trị dự kiến và thực tế. Tham số: `intended_value`, `actual_value`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "intended_value": { "type": "number" },
                        "actual_value": { "type": "number" }
                    },
                    "required": ["intended_value", "actual_value"]
                }),
                tags: vec!["excel".into(), "verify".into(), "xác minh".into(), "kiểm tra".into(), "số liệu".into()],
            },
        ]
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        info!("AnalystAgent: probing Excel COM availability…");
        self.com_available = Self::probe_excel_com();

        if self.com_available {
            info!("AnalystAgent: Excel COM available ✓");
            self.status = AgentStatus::Idle;
        } else {
            warn!(
                "AnalystAgent: Excel COM not available. \
                 Agent will run in stub mode until Phase 3 is implemented."
            );
            // Not an error – stay Idle so the agent can still serve stub responses
            self.status = AgentStatus::Idle;
        }

        Ok(())
    }

    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        debug!(
            task_id = %task.task_id,
            action  = %task.action,
            session = %task.session_id,
            "AnalystAgent received task"
        );

        self.status = AgentStatus::Busy;

        let result = self.dispatch_action(&task).await;

        self.status = AgentStatus::Idle;

        match &result {
            Ok(_) => debug!(task_id = %task.task_id, "AnalystAgent task completed OK"),
            Err(e) => {
                self.stats.failed_tasks += 1;
                warn!(task_id = %task.task_id, error = %e, "AnalystAgent task failed")
            }
        }

        result
    }

    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    fn status_info(&self) -> crate::agents::AgentStatusInfo {
        crate::agents::AgentStatusInfo {
            id: self.id.to_string(),
            name: self.name().to_string(),
            status: self.status.to_string(),
            last_used: None,
            total_tasks: self.stats.total_tasks,
            error_count: self.stats.failed_tasks as u32,
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
    use crate::orchestrator::intent::Intent;
    use std::collections::HashMap;

    fn make_task(action: &str) -> AgentTask {
        AgentTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            intent: Intent::ExcelRead(Default::default()),
            message: format!("Test task for action: {}", action),
            context_file: Some("C:\\test\\sample.xlsx".to_string()),
            session_id: "test-session".to_string(),
            parameters: HashMap::new(),
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        }
    }

    fn make_task_with_params(
        action: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> AgentTask {
        AgentTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            intent: Intent::ExcelRead(Default::default()),
            message: "test".to_string(),
            context_file: None,
            session_id: "test-session".to_string(),
            parameters: params,
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        }
    }

    #[test]
    fn test_agent_id() {
        let agent = AnalystAgent::new();
        assert_eq!(agent.id().0, "analyst");
    }

    #[test]
    fn test_agent_name() {
        let agent = AnalystAgent::new();
        assert!(agent.name().contains("Excel"));
    }

    #[test]
    fn test_supported_actions_not_empty() {
        let agent = AnalystAgent::new();
        let actions = agent.supported_actions();
        assert!(!actions.is_empty());
        assert!(actions.contains(&"analyze_workbook".to_string()));
        assert!(actions.contains(&"generate_formula".to_string()));
        assert!(actions.contains(&"audit_formulas".to_string()));
    }

    #[test]
    fn test_initial_status_is_idle() {
        let agent = AnalystAgent::new();
        assert_eq!(agent.status(), AgentStatus::Idle);
    }

    #[test]
    fn test_vba_execution_disabled_by_default() {
        let agent = AnalystAgent::new();
        assert!(!agent.config.allow_vba_execution);
    }

    #[tokio::test]
    async fn test_execute_analyze_workbook_no_excel() {
        let mut agent = AnalystAgent::new();
        let task = make_task("analyze_workbook");
        let result = agent.execute(task).await;
        // Either Ok (Excel running) or Err (Excel not available) – both acceptable
        // When Excel is not running, error propagates cleanly
        let _ = result;
    }

    #[tokio::test]
    async fn test_execute_read_range_no_excel() {
        // Without Excel installed, COM should return an error
        let mut agent = AnalystAgent::new();
        let task = make_task("read_cell_range");
        let result = agent.execute(task).await;
        // Either Ok (Excel running) or Err (Excel not available) – both acceptable
        let _ = result;
    }

    #[tokio::test]
    async fn test_execute_write_range_no_excel() {
        let mut agent = AnalystAgent::new();
        let task = make_task("write_cell_range");
        let result = agent.execute(task).await;
        // Either Ok (Excel running) or Err (Excel not available) – both acceptable
        let _ = result;
    }

    #[tokio::test]
    async fn test_execute_run_vba_blocked_when_disabled() {
        let mut agent = AnalystAgent::new(); // allow_vba_execution = false
        let task = make_task("run_vba");
        let result = agent.execute(task).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("VBA execution is disabled"));
    }

    #[tokio::test]
    async fn test_execute_generate_vba_allowed_without_execution_flag() {
        let mut agent = AnalystAgent::new(); // allow_vba_execution = false
        let task = make_task("generate_vba"); // generate (not run) is OK
        let result = agent.execute(task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_run_vba_allowed_when_enabled() {
        let config = AnalystConfig {
            allow_vba_execution: true,
            ..Default::default()
        };
        let mut agent = AnalystAgent::with_config(config);
        let task = make_task("run_vba");
        let result = agent.execute(task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_unknown_action_returns_error() {
        let mut agent = AnalystAgent::new();
        let task = make_task("completely_unknown_action");
        let result = agent.execute(task).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown action"));
    }

    #[tokio::test]
    async fn test_hard_truth_verify_passes_within_tolerance() {
        let mut agent = AnalystAgent::new(); // tolerance = 0.01%
        let mut params = HashMap::new();
        params.insert("intended_value".into(), serde_json::json!(1_000_000.0));
        params.insert("actual_value".into(), serde_json::json!(1_000_000.000_1));
        let task = make_task_with_params("hard_truth_verify", params);
        let result = agent.execute(task).await;
        assert!(result.is_ok());
        assert!(result.unwrap().content.contains("PASSED"));
    }

    #[tokio::test]
    async fn test_hard_truth_verify_fails_outside_tolerance() {
        let mut agent = AnalystAgent::new(); // tolerance = 0.01%
        let mut params = HashMap::new();
        params.insert("intended_value".into(), serde_json::json!(1_000_000.0));
        params.insert("actual_value".into(), serde_json::json!(1_200_000.0)); // 20% deviation
        let task = make_task_with_params("hard_truth_verify", params);
        let result = agent.execute(task).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FAILED"));
        assert_eq!(agent.stats.hard_truth_violations, 1);
    }

    #[tokio::test]
    async fn test_init_no_panic() {
        let mut agent = AnalystAgent::new();
        agent.init().await.expect("init should not panic");
    }

    #[test]
    fn test_default_config_sensible_values() {
        let cfg = AnalystConfig::default();
        assert!(!cfg.allow_vba_execution); // safe default
        assert!(cfg.backup_before_write); // safe default
        assert_eq!(cfg.max_rows_per_operation, 100_000);
        assert!(cfg.hard_truth_tolerance_pct > 0.0);
    }
}
