# Phase 4 Completion Plan: Advanced UIA and Workflows

This plan covers the remaining tasks for Phase 4 to fully empower the Web Researcher Agent.

## User Review Required

> [!IMPORTANT]
> The Web-to-Excel workflow will be implemented as a **Pipeline Dispatch** in the Orchestrator. When the user requests "Lấy bảng dữ liệu web và lưu vào excel", the Orchestrator will decompose this into two steps:
> 1. `WebExtract` (Web Researcher) -> Extracts JSON table data.
> 2. `ExcelWrite` (Analyst) -> Takes the JSON data from the session context and writes it to Excel.
> 
> Is this pipeline decomposition acceptable, or would you prefer a single dedicated agent script for Web-to-Excel?

> [!WARNING]  
> To save screenshots as PNGs for the grounding feature, we will need to add the `image` crate (e.g., `image = "0.24"`) to `Cargo.toml`. Let me know if you want to avoid external dependencies by writing a raw BMP file manually instead.

## Proposed Changes

---

### 1. `Cargo.toml`
#### [MODIFY] `e:\Office hub\src-tauri\Cargo.toml`
- Add `image = "0.24"` to dependencies to support saving captured GDI bitmaps as PNG files.

---

### 2. UI Automation (UIA) Bindings
#### [MODIFY] `e:\Office hub\src-tauri\src\agents\web_researcher\uia.rs`
- Add `IUIAutomationGridPattern` and `IUIAutomationTablePattern` imports.
- Add `extract_browser_tables(&self, hwnd: HWND) -> anyhow::Result<Vec<Vec<Vec<String>>>>`:
  - Search for elements with `UIA_TablePatternId` or `UIA_GridPatternId`.
  - Use `GetItem(row, col)` to recursively fetch the cell text.
  - Return a list of 2D string arrays representing the tables.
- Add `capture_screenshot(&self, hwnd: HWND, output_path: &str) -> anyhow::Result<()>`:
  - Use `GetWindowRect` to get the browser dimensions.
  - Use GDI (`GetDC`, `CreateCompatibleDC`, `CreateCompatibleBitmap`, `BitBlt`) to capture the window pixels.
  - Use `GetDIBits` to extract raw pixels and `image::RgbaImage` to save as a PNG file.

---

### 3. Web Researcher Agent Logic
#### [MODIFY] `e:\Office hub\src-tauri\src\agents\web_researcher\mod.rs`
- Update `handle_extract_text` or create a new action `extract_table`:
  - Attempt to extract structured tables using `uia.extract_browser_tables()`.
  - Use `uia.capture_screenshot()` to save a snapshot to `config.screenshot_dir` for grounding and audit logging.
  - Append the grounding evidence to the AgentOutput.

---

### 4. Orchestrator Pipeline (Web-to-Excel)
#### [MODIFY] `e:\Office hub\src-tauri\src\orchestrator\mod.rs`
- In the `process_message` logic (where intents are currently routed), intercept `IntentCategory::WebToExcel`.
- Instead of standard dispatch, invoke `router.dispatch_pipeline` with two intents:
  1. `Intent::WebExtract` (targeted at the active web page).
  2. `Intent::ExcelWrite` (taking the extracted JSON data to write to a new or active Excel workbook).
- Pass the context seamlessly through the `Session` metadata.

## Verification Plan

### Automated Tests
- `cargo check` to ensure all new GDI and UIA pattern bindings compile correctly.

### Manual Verification
- **Table Extraction**: Open a webpage with an HTML `<table>` (e.g., Wikipedia) and ask the agent to extract the table. Verify the JSON output maintains the grid structure.
- **Screenshot**: Verify a `.png` file is correctly saved in the AppData grounding folder when extraction occurs.
- **Pipeline**: Ask the chat interface "Trích xuất bảng và lưu ra Excel". Verify that the Web Researcher runs first, followed by the Analyst Agent opening Excel and writing the data.
