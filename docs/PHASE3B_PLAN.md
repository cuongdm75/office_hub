# Phase 3b – Implementation Plan (Multi-Session)
**Tạo:** 2026-04-24  
**Conversation context:** `340a1531-6823-4969-9bfa-325389ab3232`  
**Prerequisite:** Phase 3 COM (Excel/Word/PPT) hoàn thành – xem `HANDOFF_SESSION_15.md`

---

## Tổng quan

Phase 3b gồm 3 sessions độc lập, thực thi theo thứ tự 15A → 15B → 15C (15C có thể làm song song với 15A/15B).

```
Session 15A: FolderScanner – File Readers (Gap A1)
Session 15B: FolderScanner – LLM + Output Documents (Gaps A2–A4)
Session 15C: OutlookAgent – Extended Actions (Gaps B1–B3)
```

---

## Session 15A – FolderScanner File Readers

### File target
`e:\Office hub\src-tauri\src\agents\folder_scanner\mod.rs`  
**Lines:** 1138–1235 (function `read_file_content`)

### Thay đổi cần làm

**1. Thêm import đầu file:**
```rust
use calamine::{open_workbook_auto, Reader};
use crate::agents::office_master::{com_word, com_ppt};
```

**2. Thay toàn bộ match body của `read_file_content()`:**

```rust
async fn read_file_content(&self, file: &ScannedFileInfo) -> Option<String> {
    match file.category {
        FileCategory::PlainText | FileCategory::Markdown => {
            tokio::fs::read_to_string(&file.path).await.ok().map(|c| {
                if c.len() > 10_000 { format!("{}\n[truncated {} chars]", &c[..10_000], c.len() - 10_000) }
                else { c }
            })
        }
        FileCategory::Csv => {
            tokio::fs::read_to_string(&file.path).await.ok().map(|c| {
                let lines: Vec<&str> = c.lines().take(50).collect();
                format!("[CSV {} rows preview]\n{}", lines.len(), lines.join("\n"))
            })
        }
        FileCategory::Json => {
            tokio::fs::read_to_string(&file.path).await.ok()
                .map(|c| serde_json::from_str::<serde_json::Value>(&c)
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or(c.clone()))
                    .unwrap_or(c))
                .map(|s| if s.len() > 8_000 { format!("{}\n[truncated]", &s[..8_000]) } else { s })
        }
        FileCategory::Yaml => tokio::fs::read_to_string(&file.path).await.ok(),
        FileCategory::Word => {
            let path = file.path.to_string_lossy().to_string();
            tokio::task::spawn_blocking(move || {
                let word = com_word::WordApplication::connect_or_launch().ok()?;
                let content = word.extract_content(&path).ok()?;
                let text = content.paragraphs.join("\n");
                Some(format!("[Word {} pages, {} words]\n{}", 
                    content.page_count, content.word_count,
                    if text.len() > 8_000 { format!("{}\n[truncated]", &text[..8_000]) } else { text }
                ))
            }).await.ok().flatten()
        }
        FileCategory::Excel => {
            let path = file.path.clone();
            tokio::task::spawn_blocking(move || {
                let mut wb = open_workbook_auto(&path).ok()?;
                let sheet_names = wb.sheet_names().to_vec();
                let mut rows_text = Vec::new();
                for name in &sheet_names {
                    rows_text.push(format!("=== Sheet: {} ===", name));
                    if let Some(Ok(range)) = wb.worksheet_range(name) {
                        for row in range.rows().take(30) {
                            rows_text.push(row.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("\t"));
                        }
                    }
                }
                Some(format!("[Excel {} sheets]\n{}", sheet_names.len(), rows_text.join("\n")))
            }).await.ok().flatten()
        }
        FileCategory::PowerPoint => {
            let path = file.path.to_string_lossy().to_string();
            tokio::task::spawn_blocking(move || {
                let ppt = com_ppt::PowerPointApplication::connect_or_launch().ok()?;
                let info = ppt.inspect_presentation(&path).ok()?;
                let titles = info.slide_titles.iter().enumerate()
                    .map(|(i, t)| format!("  Slide {}: {}", i + 1, t))
                    .collect::<Vec<_>>().join("\n");
                Some(format!("[PowerPoint {} slides]\n{}", info.slide_count, titles))
            }).await.ok().flatten()
        }
        FileCategory::Pdf => {
            let path = file.path.clone();
            tokio::task::spawn_blocking(move || {
                let bytes = std::fs::read(&path).ok()?;
                let text: String = bytes.iter()
                    .filter(|&&b| b >= 0x20 && b < 0x7F || b == b'\n')
                    .map(|&b| b as char).collect();
                let lines = text.lines()
                    .filter(|l| l.trim().len() > 3)
                    .take(100)
                    .collect::<Vec<_>>().join("\n");
                Some(format!("[PDF text extract]\n{}", lines))
            }).await.ok().flatten()
        }
        FileCategory::Email => {
            tokio::fs::read_to_string(&file.path).await.ok().map(|raw| {
                let headers: Vec<String> = raw.lines()
                    .take_while(|l| !l.is_empty())
                    .filter(|l| l.starts_with("From:") || l.starts_with("Subject:") || l.starts_with("Date:") || l.starts_with("To:"))
                    .map(String::from).collect();
                let body_start = raw.find("\r\n\r\n").or_else(|| raw.find("\n\n")).unwrap_or(raw.len());
                let body: String = raw[body_start..].chars().take(500).collect();
                format!("[Email]\n{}\n\n{}", headers.join("\n"), body)
            })
        }
        FileCategory::Unknown => None,
    }
}
```

### Verify
```powershell
cd "e:\Office hub"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | Select-Object -Last 5
```

---

## Session 15B – FolderScanner LLM + Output Docs

### Prerequisite: Session 15A done

### Thay đổi 1: Thêm field vào struct

```rust
// Thêm vào FolderScannerAgent struct (sau active_scans field)
llm_gateway: Option<Arc<tokio::sync::RwLock<crate::llm_gateway::LlmGateway>>>,

// Thêm vào FolderScannerAgent::new()
llm_gateway: None,
```

### Thay đổi 2: Wire LLM trong execute()

```rust
// Đầu hàm execute(), trước khi gọi dispatch_action:
if self.llm_gateway.is_none() {
    self.llm_gateway = task.llm_gateway.clone();
}
```

### Thay đổi 3: summarize_content() – gọi LLM thật

```rust
async fn summarize_content(&self, file: &ScannedFileInfo, content: Option<&str>, config: &FolderScanConfig) -> anyhow::Result<String> {
    let content = match content {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(format!("Không thể đọc nội dung `{}`.", file.name)),
    };

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
            &content[..content.len().min(6000)],
            detail, lang
        );
        let req = crate::llm_gateway::LlmRequest::new(vec![
            crate::llm_gateway::LlmMessage::system("Bạn là trợ lý tóm tắt tài liệu chuyên nghiệp. Chỉ trả lời phần tóm tắt, không giải thích thêm."),
            crate::llm_gateway::LlmMessage::user(prompt),
        ]).with_max_tokens(512).with_temperature(0.3);
        if let Ok(resp) = llm.complete(req).await {
            return Ok(resp.content);
        }
    }

    // Fallback khi không có LLM
    Ok(format!(
        "`{}` ({:?}, {:.1} KB) – {} dòng. [LLM chưa được cấu hình]",
        file.name, file.category,
        file.size_bytes as f64 / 1024.0,
        content.lines().count()
    ))
}
```

### Thay đổi 4: generate_folder_summary() – LLM thật

```rust
async fn generate_folder_summary(&self, files: &[ScannedFileInfo], config: &FolderScanConfig) -> Option<String> {
    let done_files: Vec<&ScannedFileInfo> = files.iter().filter(|f| f.status == FileProcessStatus::Done).collect();
    if done_files.is_empty() { return None; }

    let file_list = done_files.iter().take(20)
        .map(|f| format!("- **{}**: {}", f.name, f.summary.as_deref().unwrap_or("(không có tóm tắt)")))
        .collect::<Vec<_>>().join("\n");

    if let Some(llm_arc) = &self.llm_gateway {
        let llm = llm_arc.read().await;
        let lang = if config.report_language == "vi" { "tiếng Việt" } else { "English" };
        let prompt = format!(
            "Đây là tóm tắt {} file trong folder `{}`:\n{}\n\nViết tổng quan 5-8 câu về nội dung và chủ đề chính bằng {}.",
            done_files.len(), config.folder_path.display(), file_list, lang
        );
        let req = crate::llm_gateway::LlmRequest::new(vec![
            crate::llm_gateway::LlmMessage::user(prompt),
        ]).with_max_tokens(768).with_temperature(0.4);
        if let Ok(resp) = llm.complete(req).await {
            return Some(resp.content);
        }
    }

    Some(format!("Đã xử lý {}/{} file.\n\n{}", done_files.len(), files.len(), file_list))
}
```

### Thay đổi 5: generate_output_documents() – real COM files

```rust
// Thay toàn bộ for loop từ line 1376 (for format in formats_to_generate):
for format in formats_to_generate {
    let output_path = output_dir.join(format!("{}_{}_TongHop.{}", prefix, timestamp, match &format {
        ScanOutputFormat::WordReport   => "docx",
        ScanOutputFormat::PptSlides    => "pptx",
        ScanOutputFormat::ExcelSummary => "xlsx",
        ScanOutputFormat::All          => unreachable!(),
    }));

    let done_files: Vec<&ScannedFileInfo> = files.iter()
        .filter(|f| f.status == FileProcessStatus::Done).collect();

    match &format {
        ScanOutputFormat::WordReport => {
            let report = done_files.iter()
                .map(|f| format!("## {}\n{}\n", f.name, f.summary.as_deref().unwrap_or("(không có tóm tắt)")))
                .collect::<Vec<_>>().join("\n");
            let out_str = output_path.to_string_lossy().to_string();
            let _ = tokio::task::spawn_blocking(move || {
                let word = com_word::WordApplication::connect_or_launch()?;
                word.create_report_from_template(None, &report, None)?;
                // TODO: SaveAs to out_str once Selection.TypeText is done
                Ok::<(), anyhow::Error>(())
            }).await;
        }
        ScanOutputFormat::PptSlides => {
            let slides: Vec<crate::agents::office_master::com_ppt::SlideSpec> = done_files.iter()
                .map(|f| crate::agents::office_master::com_ppt::SlideSpec {
                    title: f.name.clone(),
                    body_lines: f.summary.as_deref().unwrap_or("").lines().take(4).map(String::from).collect(),
                    layout: 2,
                }).collect();
            let out_str = output_path.to_string_lossy().to_string();
            let _ = tokio::task::spawn_blocking(move || {
                let ppt = com_ppt::PowerPointApplication::connect_or_launch()?;
                ppt.create_from_outline(None, &slides, &out_str, None)
            }).await;
        }
        ScanOutputFormat::ExcelSummary => {
            let headers = vec!["Tên file".to_string(), "Loại".to_string(), "Kích thước (KB)".to_string(), "Tóm tắt".to_string()];
            let rows: Vec<Vec<String>> = done_files.iter().map(|f| vec![
                f.name.clone(),
                format!("{:?}", f.category),
                format!("{:.1}", f.size_bytes as f64 / 1024.0),
                f.summary.as_deref().unwrap_or("").chars().take(200).collect(),
            ]).collect();
            // Use analyst excel_com to write
            let out_str = output_path.to_string_lossy().to_string();
            let _ = tokio::task::spawn_blocking(move || {
                let excel = crate::agents::analyst::excel_com::ExcelApplication::connect_or_launch()?;
                excel.open_workbook(&out_str)?; // creates new if not exists
                let values: Vec<Vec<serde_json::Value>> = std::iter::once(headers.iter().map(|h| serde_json::Value::String(h.clone())).collect())
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
        page_count: if matches!(&format, ScanOutputFormat::WordReport) { Some(1) } else { None },
        sheet_count: if matches!(&format, ScanOutputFormat::ExcelSummary) { Some(1) } else { None },
        slide_count: if matches!(&format, ScanOutputFormat::PptSlides) { Some(done_files.len() as u32) } else { None },
    });
}
```

---

## Session 15C – OutlookAgent Extended Actions

### File target
`e:\Office hub\src-tauri\src\agents\outlook\mod.rs`

### Thay đổi 1: supported_actions()
```rust
fn supported_actions(&self) -> Vec<String> {
    crate::agent_actions![
        "read_inbox",
        "send_email",
        "read_email_by_id",
        "reply_email",
        "search_emails"
    ]
}
```

### Thay đổi 2: execute() match arm
```rust
let result = match task.action.as_str() {
    "read_inbox"        => self.handle_read_inbox(&task).await,
    "send_email"        => self.handle_send_email(&task).await,
    "read_email_by_id"  => self.handle_read_email_by_id(&task).await,
    "reply_email"       => self.handle_reply_email(&task).await,
    "search_emails"     => self.handle_search_emails(&task).await,
    unknown => { ... }
};
```

### Thay đổi 3: 3 handler mới (thêm vào impl OutlookAgent)

```rust
async fn handle_read_email_by_id(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
    let id = task.parameters.get("email_id").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Thiếu email_id"))?;
    let token = self.fetch_graph_token().await?;
    let url = format!("https://graph.microsoft.com/v1.0/me/messages/{}", id);
    let data: serde_json::Value = self.client.get(&url).bearer_auth(&token).send().await?.json().await?;
    let subject = data["subject"].as_str().unwrap_or("(no subject)");
    let body = data["body"]["content"].as_str().unwrap_or("");
    Ok(AgentOutput {
        content: format!("📧 **{}**\n\n{}", subject, &body[..body.len().min(2000)]),
        committed: false, tokens_used: None,
        metadata: Some(serde_json::json!({ "email_id": id, "subject": subject })),
    })
}

async fn handle_reply_email(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
    let id = task.parameters.get("email_id").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Thiếu email_id"))?;
    let comment = task.parameters.get("body").and_then(|v| v.as_str())
        .unwrap_or(&task.message);
    let token = self.fetch_graph_token().await?;
    let url = format!("https://graph.microsoft.com/v1.0/me/messages/{}/reply", id);
    let payload = serde_json::json!({ "comment": comment });
    let res = self.client.post(&url).bearer_auth(&token).json(&payload).send().await?;
    if !res.status().is_success() {
        return Err(anyhow::anyhow!("Graph reply error: {}", res.status()));
    }
    Ok(AgentOutput {
        content: format!("✅ Đã reply email `{}`.", id),
        committed: true, tokens_used: None,
        metadata: Some(serde_json::json!({ "email_id": id })),
    })
}

async fn handle_search_emails(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
    let query = task.parameters.get("query").and_then(|v| v.as_str())
        .unwrap_or(&task.message);
    let max = task.parameters.get("max_results").and_then(|v| v.as_u64()).unwrap_or(10);
    let token = self.fetch_graph_token().await?;
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/messages?$search=\"{}\"%$top={}",
        urlencoding::encode(query), max
    );
    let data: serde_json::Value = self.client.get(&url)
        .bearer_auth(&token)
        .header("ConsistencyLevel", "eventual")
        .send().await?.json().await?;
    let emails = data["value"].as_array().cloned().unwrap_or_default();
    let list = emails.iter().map(|e| format!(
        "- **{}** (from: {})",
        e["subject"].as_str().unwrap_or(""),
        e["sender"]["emailAddress"]["address"].as_str().unwrap_or("")
    )).collect::<Vec<_>>().join("\n");
    Ok(AgentOutput {
        content: format!("🔍 Tìm \"{}\" → {} kết quả:\n{}", query, emails.len(), list),
        committed: false, tokens_used: None,
        metadata: Some(serde_json::json!({ "query": query, "count": emails.len(), "emails": emails })),
    })
}
```

---

## Verify cuối session

```powershell
cd "e:\Office hub"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | Select-Object -Last 5
# Expected: Finished dev profile – 0 errors
```

---

## Files cần đọc khi bắt đầu session mới

```
docs/HANDOFF_SESSION_15.md   ← context phiên trước
docs/PHASE3B_GAP_ANALYSIS.md ← danh sách gaps chi tiết
docs/PHASE3B_PLAN.md         ← file này (plan)

src-tauri/src/agents/folder_scanner/mod.rs    ← file chính session 15A/15B
src-tauri/src/agents/outlook/mod.rs           ← file chính session 15C
src-tauri/src/agents/office_master/com_word.rs  ← tham khảo COM pattern
src-tauri/src/agents/office_master/com_ppt.rs   ← tham khảo COM pattern
src-tauri/src/agents/analyst/excel_com.rs       ← tham khảo COM pattern
```
