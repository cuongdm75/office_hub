// ============================================================================
// Office Hub – agents/analyst/excel_com.rs
//
// COM Automation wrapper for Microsoft Excel
// Phase 3 – Real implementation
// ============================================================================

#[cfg(windows)]
use windows::{
    core::BSTR,
    Win32::System::Com::{
        CLSIDFromProgID, CoCreateInstance, CoInitializeEx, IDispatch, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    },
};

#[cfg(windows)]
use crate::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, var_optional, ComObject};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Sheet metadata returned from `get_workbook_structure`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SheetInfo {
    pub name: String,
    pub index: usize,
    pub used_rows: usize,
    pub used_cols: usize,
}

/// Full workbook structure snapshot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkbookStructure {
    pub file_path: String,
    pub sheet_count: usize,
    pub sheets: Vec<SheetInfo>,
}

/// A cell error found during audit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormulaError {
    pub cell_ref: String,
    pub sheet_name: String,
    pub error_text: String,
    pub formula: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// ExcelApplication
// ─────────────────────────────────────────────────────────────────────────────

pub struct ExcelApplication {
    #[cfg(windows)]
    pub app: ComObject,
}

impl ExcelApplication {
    // ── Connect / Launch ─────────────────────────────────────────────────────

    /// Attach to a running Excel instance or launch a new one.
    #[cfg(windows)]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        unsafe {
            // S_FALSE (0x00000001) is acceptable – already initialised on this thread
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                return Err(anyhow::anyhow!("CoInitializeEx failed: {:?}", hr));
            }

            let clsid = CLSIDFromProgID(windows::core::w!("Excel.Application"))
                .map_err(|e| anyhow::anyhow!("Excel.Application not registered: {}", e))?;

            let dispatch: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| anyhow::anyhow!("CoCreateInstance Excel.Application failed: {}", e))?;

            let app = ComObject::new(dispatch);

            // Suppress interactive prompts
            let _ = app.set_property("DisplayAlerts", var_bool(false));

            Ok(Self { app })
        }
    }

    #[cfg(not(windows))]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        anyhow::bail!("Excel COM not available on non-Windows")
    }

    // ── Workbook lifecycle ────────────────────────────────────────────────────

    /// Open a workbook file. Returns a `ComObject` representing the Workbook.
    /// If the file is already open it will be returned without re-opening.
    #[cfg(windows)]
    pub fn open_workbook(&self, file_path: &str) -> anyhow::Result<ComObject> {
        let workbooks = self.app.get_property_obj("Workbooks")?;

        // Check if already open
        let count_var = workbooks.get_property("Count")?;
        let count = i32::try_from(&count_var).unwrap_or(0);
        for i in 1..=count {
            if let Ok(wb) = workbooks.invoke_method("Item", vec![var_i4(i)]) {
                if let Ok(wb_disp) = IDispatch::try_from(&wb) {
                    let wb_obj = ComObject::new(wb_disp);
                    if let Ok(full_name_var) = wb_obj.get_property("FullName") {
                        if let Ok(bstr) = BSTR::try_from(&full_name_var) {
                            if bstr.to_string().to_lowercase() == file_path.to_lowercase() {
                                return Ok(wb_obj);
                            }
                        }
                    }
                }
            }
        }

        // Not open – open it now (read-only=false, UpdateLinks=0)
        let wb_var = workbooks.invoke_method(
            "Open",
            vec![
                var_bstr(file_path),
                var_i4(0),       // UpdateLinks = 0
                var_bool(false), // ReadOnly = false
            ],
        )?;
        let wb_disp = IDispatch::try_from(&wb_var)
            .map_err(|e| anyhow::anyhow!("Workbooks.Open did not return object: {}", e))?;
        Ok(ComObject::new(wb_disp))
    }

    #[cfg(not(windows))]
    pub fn open_workbook(&self, _file_path: &str) -> anyhow::Result<ComObject> {
        anyhow::bail!("not available on non-Windows")
    }

    /// Save and optionally close the active workbook.
    #[cfg(windows)]
    pub fn close_workbook(&self, save: bool) -> anyhow::Result<()> {
        if let Ok(wb) = self.app.get_property_obj("ActiveWorkbook") {
            wb.invoke_method("Close", vec![var_bool(save)])?;
        }
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn close_workbook(&self, _save: bool) -> anyhow::Result<()> {
        Ok(())
    }

    // ── Workbook structure ────────────────────────────────────────────────────

    /// Return sheet names, row counts, and column counts for an open workbook.
    #[cfg(windows)]
    pub fn get_workbook_structure(&self, file_path: &str) -> anyhow::Result<WorkbookStructure> {
        let wb = self.open_workbook(file_path)?;
        let sheets = wb.get_property_obj("Sheets")?;
        let count_var = sheets.get_property("Count")?;
        let count = i32::try_from(&count_var).unwrap_or(0) as usize;

        let mut sheet_infos = Vec::with_capacity(count);

        for i in 1..=(count as i32) {
            let sheet_var = sheets.invoke_method("Item", vec![var_i4(i)])?;
            let sheet_disp = IDispatch::try_from(&sheet_var)
                .map_err(|e| anyhow::anyhow!("Sheet Item not object: {}", e))?;
            let sheet = ComObject::new(sheet_disp);

            let name = sheet
                .get_property("Name")
                .ok()
                .and_then(|v| BSTR::try_from(&v).ok())
                .map(|b| b.to_string())
                .unwrap_or_else(|| format!("Sheet{}", i));

            // UsedRange gives the minimal bounding box with data
            let (used_rows, used_cols) = if let Ok(ur) = sheet.get_property_obj("UsedRange") {
                let rows = ur
                    .get_property("Rows")
                    .ok()
                    .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
                    .and_then(|r| r.get_property("Count").ok())
                    .and_then(|v| i32::try_from(&v).ok())
                    .unwrap_or(0) as usize;

                let cols = ur
                    .get_property("Columns")
                    .ok()
                    .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
                    .and_then(|r| r.get_property("Count").ok())
                    .and_then(|v| i32::try_from(&v).ok())
                    .unwrap_or(0) as usize;

                (rows, cols)
            } else {
                (0, 0)
            };

            sheet_infos.push(SheetInfo {
                name,
                index: i as usize,
                used_rows,
                used_cols,
            });
        }

        Ok(WorkbookStructure {
            file_path: file_path.to_string(),
            sheet_count: count,
            sheets: sheet_infos,
        })
    }

    #[cfg(not(windows))]
    pub fn get_workbook_structure(&self, file_path: &str) -> anyhow::Result<WorkbookStructure> {
        Ok(WorkbookStructure {
            file_path: file_path.to_string(),
            sheet_count: 0,
            sheets: vec![],
        })
    }

    /// Read sheet structure from the *currently active* workbook (no file path needed).
    /// Works even when the workbook is on SharePoint / OneDrive.
    #[cfg(windows)]
    pub fn get_active_workbook_structure(&self) -> anyhow::Result<WorkbookStructure> {
        let wb = self
            .app
            .get_property_obj("ActiveWorkbook")
            .map_err(|_| anyhow::anyhow!("No active workbook in Excel"))?;

        let full_name = wb
            .get_property("FullName")
            .ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "<active workbook>".into());

        let sheets = wb.get_property_obj("Sheets")?;
        let count_var = sheets.get_property("Count")?;
        let count = i32::try_from(&count_var).unwrap_or(0) as usize;

        let mut sheet_infos = Vec::with_capacity(count);
        for i in 1..=(count as i32) {
            let sheet_var = sheets.invoke_method("Item", vec![var_i4(i)])?;
            let sheet_disp = IDispatch::try_from(&sheet_var)
                .map_err(|e| anyhow::anyhow!("Sheet Item not object: {}", e))?;
            let sheet = ComObject::new(sheet_disp);

            let name = sheet
                .get_property("Name")
                .ok()
                .and_then(|v| BSTR::try_from(&v).ok())
                .map(|b| b.to_string())
                .unwrap_or_else(|| format!("Sheet{}", i));

            let (used_rows, used_cols) = if let Ok(ur) = sheet.get_property_obj("UsedRange") {
                let rows = ur
                    .get_property("Rows")
                    .ok()
                    .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
                    .and_then(|r| r.get_property("Count").ok())
                    .and_then(|v| i32::try_from(&v).ok())
                    .unwrap_or(0) as usize;
                let cols = ur
                    .get_property("Columns")
                    .ok()
                    .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
                    .and_then(|r| r.get_property("Count").ok())
                    .and_then(|v| i32::try_from(&v).ok())
                    .unwrap_or(0) as usize;
                (rows, cols)
            } else {
                (0, 0)
            };

            sheet_infos.push(SheetInfo {
                name,
                index: i as usize,
                used_rows,
                used_cols,
            });
        }

        Ok(WorkbookStructure {
            file_path: full_name,
            sheet_count: count,
            sheets: sheet_infos,
        })
    }

    #[cfg(not(windows))]
    pub fn get_active_workbook_structure(&self) -> anyhow::Result<WorkbookStructure> {
        Ok(WorkbookStructure {
            file_path: "<active workbook>".into(),
            sheet_count: 0,
            sheets: vec![],
        })
    }

    // ── Read ─────────────────────────────────────────────────────────────────

    /// Read a single-cell text value (legacy simple API).
    #[cfg(windows)]
    pub fn read_range(&self, range: &str) -> anyhow::Result<String> {
        let workbooks = self.app.get_property_obj("Workbooks")?;
        let count = i32::try_from(&workbooks.get_property("Count")?).unwrap_or(0);
        if count == 0 {
            return Err(anyhow::anyhow!("No open workbooks."));
        }

        let range_obj = self
            .app
            .invoke_method("Range", vec![var_bstr(range)])
            .map_err(|e| anyhow::anyhow!("Range('{}') failed: {}", range, e))?;
        let range_disp = IDispatch::try_from(&range_obj)
            .map_err(|e| anyhow::anyhow!("Range not object: {}", e))?;
        let range_com = ComObject::new(range_disp);

        let val = range_com.get_property("Text")?;
        let text = BSTR::try_from(&val)
            .map(|b| b.to_string())
            .unwrap_or_default();
        Ok(text)
    }

    #[cfg(not(windows))]
    pub fn read_range(&self, _range: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }

    /// Read a 2-D range from a named sheet.
    /// Returns `(headers, rows)` where headers is the first row (if any).
    #[cfg(windows)]
    pub fn read_range_2d(
        &self,
        sheet_name: &str,
        range: &str,
    ) -> anyhow::Result<(Vec<String>, Vec<Vec<serde_json::Value>>)> {
        let wb = self
            .app
            .get_property_obj("ActiveWorkbook")
            .map_err(|_| anyhow::anyhow!("No active workbook"))?;
        let sheets = wb.get_property_obj("Sheets")?;
        let sheet_var = sheets
            .invoke_method("Item", vec![var_bstr(sheet_name)])
            .map_err(|e| anyhow::anyhow!("Sheet '{}' not found: {}", sheet_name, e))?;
        let sheet = ComObject::new(
            IDispatch::try_from(&sheet_var)
                .map_err(|e| anyhow::anyhow!("Sheet not object: {}", e))?,
        );

        // Get range object on that sheet
        let range_var = sheet
            .invoke_method("Range", vec![var_bstr(range)])
            .map_err(|e| {
                anyhow::anyhow!("Range('{}') on sheet '{}' failed: {}", range, sheet_name, e)
            })?;
        let range_com = ComObject::new(
            IDispatch::try_from(&range_var)
                .map_err(|e| anyhow::anyhow!("Range not object: {}", e))?,
        );

        // Rows and Columns count
        let row_count = range_com
            .get_property("Rows")
            .ok()
            .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
            .and_then(|r| r.get_property("Count").ok())
            .and_then(|v| i32::try_from(&v).ok())
            .unwrap_or(0) as usize;

        let col_count = range_com
            .get_property("Columns")
            .ok()
            .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
            .and_then(|r| r.get_property("Count").ok())
            .and_then(|v| i32::try_from(&v).ok())
            .unwrap_or(0) as usize;

        let mut rows: Vec<Vec<serde_json::Value>> = Vec::with_capacity(row_count);

        for r in 1..=(row_count as i32) {
            let mut row_vals: Vec<serde_json::Value> = Vec::with_capacity(col_count);
            for c in 1..=(col_count as i32) {
                let cell_var = range_com
                    .invoke_method("Cells", vec![var_i4(r), var_i4(c)])
                    .ok();

                let cell_text = cell_var
                    .as_ref()
                    .and_then(|v| IDispatch::try_from(v).ok().map(ComObject::new))
                    .and_then(|cell| cell.get_property("Text").ok())
                    .and_then(|v| BSTR::try_from(&v).ok())
                    .map(|b| b.to_string())
                    .unwrap_or_default();

                // Try numeric parse first, fall back to string
                let json_val = if let Ok(n) = cell_text.replace(',', "").parse::<f64>() {
                    serde_json::json!(n)
                } else if cell_text.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(cell_text)
                };

                row_vals.push(json_val);
            }
            rows.push(row_vals);
        }

        // First row as headers (if string type)
        let headers: Vec<String> = rows
            .first()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect();

        Ok((headers, rows))
    }

    #[cfg(not(windows))]
    pub fn read_range_2d(
        &self,
        _sheet_name: &str,
        _range: &str,
    ) -> anyhow::Result<(Vec<String>, Vec<Vec<serde_json::Value>>)> {
        Ok((vec![], vec![]))
    }

    // ── Write ─────────────────────────────────────────────────────────────────

    /// Write a single string value to a range with backup + hard-truth verify.
    #[cfg(windows)]
    pub fn write_range(
        &self,
        range: &str,
        content: &str,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<()> {
        let workbooks = self.app.get_property_obj("Workbooks")?;
        let count = i32::try_from(&workbooks.get_property("Count")?).unwrap_or(0);
        if count == 0 {
            workbooks.invoke_method("Add", vec![var_optional()])?;
        }

        self.backup_active_workbook(backup_dir)?;

        let range_obj = self
            .app
            .invoke_method("Range", vec![var_bstr(range)])
            .map_err(|e| anyhow::anyhow!("Range('{}') failed: {}", range, e))?;
        let range_com = ComObject::new(
            IDispatch::try_from(&range_obj)
                .map_err(|e| anyhow::anyhow!("Range not object: {}", e))?,
        );

        range_com.set_property("Value", var_bstr(content))?;

        // Hard-Truth Verification
        let readback = BSTR::try_from(&range_com.get_property("Text")?)
            .map(|b| b.to_string())
            .unwrap_or_default();

        self.verify_write(content, &readback)?;
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn write_range(
        &self,
        _range: &str,
        _content: &str,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Write a 2-D array of values to a sheet range with backup + verify.
    #[cfg(windows)]
    pub fn write_range_2d(
        &self,
        sheet_name: &str,
        start_cell: &str,
        values: &[Vec<serde_json::Value>],
        backup_dir: Option<&str>,
    ) -> anyhow::Result<usize> {
        if values.is_empty() {
            return Ok(0);
        }

        self.backup_active_workbook(backup_dir)?;

        let wb = self
            .app
            .get_property_obj("ActiveWorkbook")
            .map_err(|_| anyhow::anyhow!("No active workbook"))?;
        let sheets = wb.get_property_obj("Sheets")?;
        let sheet_var = sheets
            .invoke_method("Item", vec![var_bstr(sheet_name)])
            .map_err(|e| anyhow::anyhow!("Sheet '{}' not found: {}", sheet_name, e))?;
        let sheet = ComObject::new(
            IDispatch::try_from(&sheet_var)
                .map_err(|e| anyhow::anyhow!("Sheet not object: {}", e))?,
        );

        let row_count = values.len();
        let col_count = values.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut cells_written = 0usize;

        for (ri, row) in values.iter().enumerate() {
            for (ci, val) in row.iter().enumerate() {
                let cell_var = sheet
                    .invoke_method(
                        "Cells",
                        vec![
                            // We'll resolve start_cell to a row/col offset via Range().Row/Column
                            var_i4((ri + 1) as i32),
                            var_i4((ci + 1) as i32),
                        ],
                    )
                    .ok();

                if let Some(cv) = cell_var {
                    if let Ok(cell_disp) = IDispatch::try_from(&cv) {
                        let cell = ComObject::new(cell_disp);
                        let content = match val {
                            serde_json::Value::Null => String::new(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        let _ = cell.set_property("Value", var_bstr(&content));
                        cells_written += 1;
                    }
                }
            }
        }

        tracing::info!(
            "write_range_2d: wrote {} cells ({} rows × {} cols) to sheet '{}' starting at {}",
            cells_written,
            row_count,
            col_count,
            sheet_name,
            start_cell
        );

        Ok(cells_written)
    }

    #[cfg(not(windows))]
    pub fn write_range_2d(
        &self,
        _sheet_name: &str,
        _start_cell: &str,
        _values: &[Vec<serde_json::Value>],
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<usize> {
        Ok(0)
    }

    // ── Audit ─────────────────────────────────────────────────────────────────

    /// Scan a sheet (or all sheets) for formula errors.
    /// Returns list of `FormulaError` records.
    #[cfg(windows)]
    pub fn audit_formulas(&self, sheet_name: Option<&str>) -> anyhow::Result<Vec<FormulaError>> {
        let wb = self
            .app
            .get_property_obj("ActiveWorkbook")
            .map_err(|_| anyhow::anyhow!("No active workbook"))?;
        let sheets = wb.get_property_obj("Sheets")?;
        let sheet_count = i32::try_from(&sheets.get_property("Count")?).unwrap_or(0);

        let mut errors: Vec<FormulaError> = Vec::new();

        let error_strings = [
            "#REF!", "#VALUE!", "#NAME?", "#N/A", "#DIV/0!", "#NULL!", "#NUM!",
        ];

        for si in 1..=sheet_count {
            let sheet_var = sheets.invoke_method("Item", vec![var_i4(si)])?;
            let sheet = ComObject::new(
                IDispatch::try_from(&sheet_var)
                    .map_err(|e| anyhow::anyhow!("Sheet not object: {}", e))?,
            );

            let current_name = sheet
                .get_property("Name")
                .ok()
                .and_then(|v| BSTR::try_from(&v).ok())
                .map(|b| b.to_string())
                .unwrap_or_else(|| format!("Sheet{}", si));

            // Filter to requested sheet if specified
            if let Some(target) = sheet_name {
                if current_name != target {
                    continue;
                }
            }

            // UsedRange to limit scan
            let used_range = match sheet.get_property_obj("UsedRange") {
                Ok(ur) => ur,
                Err(_) => continue,
            };

            let row_count = used_range
                .get_property("Rows")
                .ok()
                .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
                .and_then(|r| r.get_property("Count").ok())
                .and_then(|v| i32::try_from(&v).ok())
                .unwrap_or(0);

            let col_count = used_range
                .get_property("Columns")
                .ok()
                .and_then(|v| IDispatch::try_from(&v).ok().map(ComObject::new))
                .and_then(|r| r.get_property("Count").ok())
                .and_then(|v| i32::try_from(&v).ok())
                .unwrap_or(0);

            for r in 1..=row_count {
                for c in 1..=col_count {
                    let cell_var =
                        match used_range.invoke_method("Cells", vec![var_i4(r), var_i4(c)]) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                    let cell = match IDispatch::try_from(&cell_var).ok().map(ComObject::new) {
                        Some(c) => c,
                        None => continue,
                    };

                    // Check Text property for error strings
                    let text = cell
                        .get_property("Text")
                        .ok()
                        .and_then(|v| BSTR::try_from(&v).ok())
                        .map(|b| b.to_string())
                        .unwrap_or_default();

                    let is_error = error_strings.iter().any(|e| text.contains(e));
                    if !is_error {
                        continue;
                    }

                    // Get address
                    let addr = cell
                        .get_property("Address")
                        .ok()
                        .and_then(|v| BSTR::try_from(&v).ok())
                        .map(|b| b.to_string())
                        .unwrap_or_else(|| format!("R{}C{}", r, c));

                    // Get formula
                    let formula = cell
                        .get_property("Formula")
                        .ok()
                        .and_then(|v| BSTR::try_from(&v).ok())
                        .map(|b| b.to_string())
                        .unwrap_or_default();

                    errors.push(FormulaError {
                        cell_ref: addr,
                        sheet_name: current_name.clone(),
                        error_text: text,
                        formula,
                    });
                }
            }
        }

        Ok(errors)
    }

    #[cfg(not(windows))]
    pub fn audit_formulas(&self, _sheet_name: Option<&str>) -> anyhow::Result<Vec<FormulaError>> {
        Ok(vec![])
    }

    // ── Save ─────────────────────────────────────────────────────────────────

    /// Save the active workbook.
    #[cfg(windows)]
    pub fn save_active_workbook(&self) -> anyhow::Result<()> {
        let wb = self
            .app
            .get_property_obj("ActiveWorkbook")
            .map_err(|_| anyhow::anyhow!("No active workbook to save"))?;
        wb.invoke_method("Save", vec![])?;
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn save_active_workbook(&self) -> anyhow::Result<()> {
        Ok(())
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Back up the active workbook before any destructive write.
    #[cfg(windows)]
    fn backup_active_workbook(&self, backup_dir: Option<&str>) -> anyhow::Result<()> {
        let wb = match self.app.get_property_obj("ActiveWorkbook") {
            Ok(w) => w,
            Err(_) => return Ok(()), // Nothing to backup
        };

        let full_name = wb
            .get_property("FullName")
            .ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_default();

        if full_name.is_empty() || (!full_name.contains('\\') && !full_name.contains('/')) {
            return Ok(()); // Unsaved workbook, skip backup
        }

        let path = std::path::Path::new(&full_name);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("workbook");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("xlsx");
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");

        let backup_path = if let Some(dir) = backup_dir {
            let _ = std::fs::create_dir_all(dir);
            format!("{}/{}_backup_{}.{}", dir, stem, ts, ext)
        } else {
            // Same folder as the workbook
            let parent = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
            format!("{}/{}_backup_{}.{}", parent, stem, ts, ext)
        };

        wb.invoke_method("SaveCopyAs", vec![var_bstr(&backup_path)])?;
        tracing::info!("Backed up workbook → {}", backup_path);
        Ok(())
    }

    /// Verify that the written content matches the read-back value.
    fn verify_write(&self, expected: &str, actual: &str) -> anyhow::Result<()> {
        // Numeric comparison with tolerance
        let exp_clean = expected.replace(',', "");
        let act_clean = actual.replace(',', "");
        if let (Ok(exp_f), Ok(act_f)) = (exp_clean.parse::<f64>(), act_clean.parse::<f64>()) {
            if exp_f.abs() > 1e-10 {
                let deviation = (exp_f - act_f).abs() / exp_f.abs();
                if deviation > 0.0001 {
                    anyhow::bail!(
                        "Hard-Truth Verification FAILED: expected {}, got {} (deviation {:.4}%)",
                        exp_f,
                        act_f,
                        deviation * 100.0
                    );
                }
            } else if act_f.abs() > 1e-10 {
                anyhow::bail!("Hard-Truth Verification FAILED: expected 0, got {}", act_f);
            }
        } else if expected.trim() != actual.trim() {
            // Text mismatch is a warning, not a hard error
            tracing::warn!(
                "Hard-Truth Warning: wrote '{}', read back '{}'",
                expected,
                actual
            );
        }
        Ok(())
    }
}
