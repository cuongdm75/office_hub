# Phase 3b – Gap Analysis (Updated 2026-04-24)

## Trạng thái tổng quan

| Agent | File | Gap | Mức độ |
|-------|------|-----|--------|
| FolderScanner | `folder_scanner/mod.rs` | `read_file_content` – 5 STUB readers | 🔴 High |
| FolderScanner | `folder_scanner/mod.rs` | `summarize_content` – STUB (no LLM) | 🔴 High |
| FolderScanner | `folder_scanner/mod.rs` | `generate_folder_summary` – STUB | 🟡 Medium |
| FolderScanner | `folder_scanner/mod.rs` | `generate_output_documents` – `.stub.txt` | 🟡 Medium |
| FolderScanner | `folder_scanner/mod.rs` | `extract_metrics` – STUB | 🟡 Medium |
| FolderScanner | `folder_scanner/mod.rs` | `search_content` – STUB | 🟡 Medium |
| OutlookAgent | `outlook/mod.rs` | `read_email_by_id` – action không tồn tại | 🟡 Medium |
| OutlookAgent | `outlook/mod.rs` | `reply_email` – action không tồn tại | 🟡 Medium |
| OutlookAgent | `outlook/mod.rs` | `search_emails` – action không tồn tại | 🟡 Medium |

---

## Chi tiết từng Gap

### GAP-A1: FolderScanner – `read_file_content()`
**File:** `src-tauri/src/agents/folder_scanner/mod.rs`  
**Lines:** 1138–1235  
**Hiện trạng:** Trả về STUB string cho Word/Excel/PPT/PDF/Email  

| Category | Strategy | Dependency |
|----------|----------|-----------|
| `FileCategory::Word` | `spawn_blocking` → `com_word::WordApplication::extract_content(path)` | COM đã có |
| `FileCategory::Excel` | `calamine::open_workbook_auto()` → rows thành text | `calamine = "0.26"` (đã thêm vào Cargo.toml) |
| `FileCategory::PowerPoint` | `spawn_blocking` → `com_ppt::PowerPointApplication::inspect_presentation(path)` | COM đã có |
| `FileCategory::Pdf` | Raw bytes → filter printable ASCII → lấy 100 dòng | Không cần dep |
| `FileCategory::Email` | Parse `.eml` headers + body 500 chars | Không cần dep |

**Pattern calamine (Excel):**
```rust
use calamine::{open_workbook_auto, Reader};
tokio::task::spawn_blocking(move || {
    let mut wb = open_workbook_auto(&path)?;
    let sheet_names = wb.sheet_names().to_vec();
    let mut out = Vec::new();
    for name in &sheet_names {
        if let Some(Ok(range)) = wb.worksheet_range(name) {
            for row in range.rows().take(50) {
                out.push(row.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("\t"));
            }
        }
    }
    Ok::<String, anyhow::Error>(format!("[Excel {} sheets]\n{}", sheet_names.len(), out.join("\n")))
}).await.ok().and_then(|r| r.ok())
```

**Pattern PDF (no-dep):**
```rust
let bytes = std::fs::read(&file.path).ok()?;
let text: String = bytes.iter()
    .filter(|&&b| b >= 0x20 && b < 0x7F || b == b'\n')
    .map(|&b| b as char).collect();
let lines = text.lines().filter(|l| l.trim().len() > 3).take(100)
    .collect::<Vec<_>>().join("\n");
Some(format!("[PDF extract]\n{}", lines))
```

---

### GAP-A2: FolderScanner – `summarize_content()`
**Lines:** 1243–1285  
**Hiện trạng:** Trả về STUB string không qua LLM  

**Vấn đề kiến trúc:** `FolderScannerAgent` không có `llm_gateway` field.

**Fix:**
1. Thêm field vào struct:
```rust
pub struct FolderScannerAgent {
    ...
    llm_gateway: Option<Arc<tokio::sync::RwLock<crate::llm_gateway::LlmGateway>>>,
}
```
2. Wire trong `execute()`: `self.llm_gateway = task.llm_gateway.clone();`
3. Trong `summarize_content()`: gọi `llm.complete(req).await`

---

### GAP-A3: FolderScanner – `generate_folder_summary()`
**Lines:** 1288–1333  
**Hiện trạng:** Trả về STUB text  
**Fix:** Sau khi có `llm_gateway` field từ GAP-A2, gọi LLM với file summaries concatenated.

---

### GAP-A4: FolderScanner – `generate_output_documents()`
**Lines:** 1340–1438  
**Hiện trạng:** Tạo `.stub.txt` placeholder, không tạo real Office files  

**Fix:**
```
WordReport   → com_word::WordApplication::create_report_from_template()
               → tất cả file summaries → một document Word
PptSlides    → com_ppt::PowerPointApplication::create_from_outline()
               → mỗi file = 1 SlideSpec (title=file.name, body=summary 2 lines)
ExcelSummary → analyst::excel_com::ExcelApplication::write_range_2d()
               → header row: [Tên file, Loại, Kích thước (KB), Tóm tắt]
               → 1 row mỗi file
```
**Note:** Tất cả COM call phải trong `tokio::task::spawn_blocking`.

---

### GAP-A5: FolderScanner – `extract_metrics()`
**Line:** 1567  
**Hiện trạng:** STUB warning + return  
**Fix:** Scan folder lấy tất cả `.xlsx/.csv`, dùng calamine đọc → aggregate numbers.

---

### GAP-A6: FolderScanner – `search_content()`
**Line:** 1577  
**Hiện trạng:** STUB warning + return  
**Fix:**
1. `discover_files()` với filter mặc định
2. `read_file_content()` cho từng file (plain text categories trước)
3. So khớp query string (case-insensitive) trong content
4. Trả về danh sách file + excerpt 200 chars quanh match

---

### GAP-B1: OutlookAgent – `read_email_by_id`
**File:** `src-tauri/src/agents/outlook/mod.rs`  
**Hiện trạng:** Action không tồn tại trong `execute()` match arm  

```rust
async fn handle_read_email_by_id(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
    let id = task.parameters.get("email_id").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("email_id là bắt buộc"))?;
    let token = self.fetch_graph_token().await?;
    let url = format!("https://graph.microsoft.com/v1.0/me/messages/{}", id);
    let data: serde_json::Value = self.client.get(&url)
        .bearer_auth(&token).send().await?.json().await?;
    // return subject + body.content
}
```

---

### GAP-B2: OutlookAgent – `reply_email`
```rust
// POST https://graph.microsoft.com/v1.0/me/messages/{id}/reply
// body: { "comment": "<reply text>" }
```

---

### GAP-B3: OutlookAgent – `search_emails`
```rust
// GET https://graph.microsoft.com/v1.0/me/messages?$search="<query>"
// Cần thêm header: ConsistencyLevel: eventual
```

---

## Checklist tổng hợp

### Session 15A – File Readers
- [ ] Import `calamine` trong folder_scanner/mod.rs
- [ ] `FileCategory::Word` → COM extract_content (spawn_blocking)
- [ ] `FileCategory::Excel` → calamine
- [ ] `FileCategory::PowerPoint` → COM inspect_presentation (spawn_blocking)
- [ ] `FileCategory::Pdf` → printable ASCII filter
- [ ] `FileCategory::Email` → .eml header parse
- [ ] `cargo check` → 0 errors

### Session 15B – LLM + Output
- [ ] Thêm `llm_gateway` field vào `FolderScannerAgent`
- [ ] Wire từ `AgentTask` trong `execute()`
- [ ] `summarize_content()` → real LLM
- [ ] `generate_folder_summary()` → real LLM
- [ ] `generate_output_documents()` → COM Word/PPT/Excel (spawn_blocking)
- [ ] `cargo check` → 0 errors

### Session 15C – Outlook Extended
- [ ] `handle_read_email_by_id()`
- [ ] `handle_reply_email()`
- [ ] `handle_search_emails()`
- [ ] Update `supported_actions()`
- [ ] Update `execute()` match arm
- [ ] COM fallback cho từng action (PowerShell)
- [ ] `cargo check` → 0 errors
