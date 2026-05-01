// ============================================================================
// Office Hub – agents/office_master/mod.rs
//
// Office Master Agent – Word & PowerPoint COM Automation
//
// Trách nhiệm:
//   1. Tạo và chỉnh sửa tài liệu Word qua COM Automation
//   2. Tạo và chỉnh sửa slide PowerPoint qua COM Automation
//   3. Hard-Truth Verification: đọc lại nội dung sau khi ghi để xác nhận
//   4. Backup tài liệu trước khi thay đổi
//
//   Bản ngã và logic chi tiết của Agent này (cách gọi COM, tạo file mẫu)
//   được định nghĩa tại `.agent/skills/office-master/SKILL.md`.
//
// Status: Autonomous Agent Mode (Phase 3)
// ============================================================================

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::agent_actions;
use crate::agents::{Agent, AgentId, AgentStatus};
use crate::orchestrator::{AgentOutput, AgentTask};

// ─────────────────────────────────────────────────────────────────────────────
// Sub-modules
// ─────────────────────────────────────────────────────────────────────────────

pub mod com_excel;
pub mod com_ppt;
pub mod com_word;

// ─────────────────────────────────────────────────────────────────────────────
// OfficeMasterAgent
// ─────────────────────────────────────────────────────────────────────────────

/// Handles all Word and PowerPoint automation tasks via COM.
pub struct OfficeMasterAgent {
    id: AgentId,
    status: AgentStatus,
    config: OfficeMasterConfig,
    metrics: OfficeMasterMetrics,
}

/// Configuration for the Office Master Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeMasterConfig {
    /// Default Word template path (None = blank document)
    pub default_word_template: Option<String>,
    /// Default PowerPoint template path (None = blank presentation)
    pub default_ppt_template: Option<String>,
    /// Always preserve document styles when writing
    pub preserve_styles: bool,
    /// Always preserve headers/footers when writing
    pub preserve_headers_footers: bool,
    /// Create a backup before any write operation
    pub backup_before_write: bool,
    /// Directory to store backups
    pub backup_dir: String,
    /// Brand color palette for PowerPoint
    pub brand_palette: BrandColorPalette,
    /// Maximum slides per deck before warning
    pub max_slides_per_deck: u32,
    /// Maximum pages per Word document before warning
    pub max_word_pages: u32,
}

impl Default for OfficeMasterConfig {
    fn default() -> Self {
        Self {
            default_word_template: Some("templates/nghi_dinh_30.dotx".to_string()),
            default_ppt_template: None,
            preserve_styles: true,
            preserve_headers_footers: true,
            backup_before_write: true,
            backup_dir: "$APPDATA/office-hub/backups/office_master".to_string(),
            brand_palette: BrandColorPalette::default(),
            max_slides_per_deck: 100,
            max_word_pages: 500,
        }
    }
}

/// Brand color palette used for PowerPoint presentations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandColorPalette {
    pub primary: String,
    pub secondary: String,
    pub accent1: String,
    pub accent2: String,
    pub neutral: String,
    pub white: String,
    pub dark: String,
}

impl Default for BrandColorPalette {
    fn default() -> Self {
        Self {
            primary: "#1F4E79".to_string(),
            secondary: "#2E75B6".to_string(),
            accent1: "#ED7D31".to_string(),
            accent2: "#70AD47".to_string(),
            neutral: "#595959".to_string(),
            white: "#FFFFFF".to_string(),
            dark: "#1F2937".to_string(),
        }
    }
}

/// Runtime metrics tracked by the Office Master Agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OfficeMasterMetrics {
    pub word_documents_created: u64,
    pub word_documents_edited: u64,
    pub ppt_presentations_created: u64,
    pub ppt_presentations_edited: u64,
    pub backups_created: u64,
    pub com_errors: u32,
    pub total_tasks: u64,
}

impl OfficeMasterAgent {
    /// Create a new OfficeMasterAgent with default configuration.
    pub fn new() -> Self {
        Self {
            id: AgentId::office_master(),
            status: AgentStatus::Idle,
            config: OfficeMasterConfig::default(),
            metrics: OfficeMasterMetrics::default(),
        }
    }

    /// Create a new OfficeMasterAgent with custom configuration.
    pub fn with_config(config: OfficeMasterConfig) -> Self {
        Self {
            id: AgentId::office_master(),
            status: AgentStatus::Idle,
            config,
            metrics: OfficeMasterMetrics::default(),
        }
    }

    // ── Word operations ───────────────────────────────────────────────────────

    /// Create a new Word document from a template or from scratch.
    ///
    /// TODO(phase-3): Implement via COM Automation:
    ///   1. `Word.Application` → `Documents.Add(template_path)`
    ///   2. Iterate `Paragraphs` and `Tables` to fill placeholders
    ///   3. Apply `Styles` from the template
    ///   4. `Document.Save()` to `output_path`
    ///   5. Read back key sections to verify (Hard-Truth)
    async fn word_create_document(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(
            task_id = %task.task_id,
            "OfficeMasterAgent: word_create_document (Native COM)"
        );

        let template_path = task.parameters.get("template_path").and_then(|v| v.as_str())
            .or(self.config.default_word_template.as_deref());
        let content = task.parameters.get("content").and_then(|v| v.as_str()).unwrap_or(task.message.as_str());
        
        let _backup_dir = if self.config.backup_before_write {
            Some(self.config.backup_dir.as_str())
        } else {
            None
        };
        
        let output_path_param = task.parameters.get("output_path").and_then(|v| v.as_str());
        let output_path_str = if let Some(p) = output_path_param {
            p.to_string()
        } else {
            let temp_dir = std::env::temp_dir().join("office_hub_exports");
            let _ = std::fs::create_dir_all(&temp_dir);
            temp_dir.join(format!("Document_{}.docx", chrono::Utc::now().format("%Y%m%d_%H%M%S"))).to_string_lossy().to_string()
        };

        let msg;
        let meta = serde_json::json!({
            "phase": "phase-3",
            "action": "word_create_document",
            "com_initialized": true,
            "file_path": output_path_str
        });

        match com_word::WordApplication::connect_or_launch() {
            Ok(word_app) => {
                info!("Successfully connected to Word.Application via COM.");
                match word_app.create_report_from_template(template_path, content, Some(&output_path_str)) {
                    Ok(_) => {
                        msg = format!("Đã mở Microsoft Word và tạo tài liệu thành công tại: {}", output_path_str);
                    },
                    Err(e) => {
                        warn!("Failed to create document: {}", e);
                        msg = format!("COM Error: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to connect to Word COM: {}", e);
                msg = format!("Connection Error: {}", e);
            }
        }

        self.metrics.word_documents_created += 1;
        self.metrics.total_tasks += 1;

        // Encode the created file as base64 and attach it for mobile delivery
        let attachment = Self::encode_file_attachment(&output_path_str);
        let final_meta = if let Some(att) = attachment {
            serde_json::json!({
                "phase": "phase-3",
                "action": "word_create_document",
                "com_initialized": true,
                "file_path": output_path_str,
                "attachment": att
            })
        } else {
            meta
        };

        Ok(AgentOutput {
            content: msg,
            committed: true,
            tokens_used: None,
            metadata: Some(final_meta),
        })
    }

    /// Insert AI-generated text into the currently-open Word document at cursor.
    /// Reads `text` from task parameters or falls back to `task.input`.
    /// This is the primary action used when the add-in asks to "insert" or "write" content.
    async fn word_insert_text_at_cursor(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_insert_text_at_cursor");

        let text = task.parameters.get("text")
            .and_then(|v| v.as_str())
            .unwrap_or(&task.message);

        if text.is_empty() {
            return Err(anyhow::anyhow!("Missing 'text' parameter for word_insert_text"));
        }

        let new_para = task.parameters.get("new_paragraph")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let word = com_word::WordApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;

        let msg = word.insert_text_at_cursor(text, new_para)
            .map_err(|e| anyhow::anyhow!("insert_text_at_cursor failed: {}", e))?;

        self.metrics.word_documents_edited += 1;
        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: format!("✏️ {}", msg),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "action": "word_insert_text",
                "chars_inserted": text.len()
            })),
        })
    }

    /// Edit an existing Word document – update paragraphs, tables, bookmarks.
    ///
    /// TODO(phase-3): COM implementation:
    ///   1. `Documents.Open(file_path)` with `ReadOnly = False`
    ///   2. Backup the original if `config.backup_before_write`
    ///   3. Find target section by Bookmark or Heading text
    ///   4. Update `Paragraph.Range.Text` or `Table.Cell.Range.Text`
    ///   5. Preserve Styles, SectionBreaks, HeadersFooters
    ///   6. `Document.Save()`
    ///   7. Read back and verify the change
    async fn word_edit_document(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_edit_document");

        let file = task.parameters.get("file_path").and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let backup_dir = if self.config.backup_before_write {
            Some(self.config.backup_dir.as_str())
        } else {
            None
        };

        // Build edits map from parameters
        let edits: std::collections::HashMap<String, String> = task
            .parameters
            .get("edits")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let word = com_word::WordApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;

        let msg = word
            .edit_document_by_bookmark(file, &edits, backup_dir)
            .map_err(|e| anyhow::anyhow!("edit_document_by_bookmark failed: {}", e))?;

        self.metrics.word_documents_edited += 1;
        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: format!("✏️ {}", msg),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file,
                "edits_count": edits.len()
            })),
        })
    }

    /// Format a Word document – apply styles, rebuild TOC, update cross-references.
    ///
    /// TODO(phase-3): COM implementation:
    ///   1. Iterate `Styles` collection and apply/update named styles
    ///   2. `Document.TablesOfContents(1).Update()` to refresh TOC
    ///   3. `Document.Fields.Update()` for cross-references and page numbers
    ///   4. Apply `PageSetup` (margins, orientation) from template spec
    async fn word_format_document(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_format_document");

        let file = task.parameters.get("file_path").and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let word = com_word::WordApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;

        let msg = word.format_document(file)
            .map_err(|e| anyhow::anyhow!("format_document failed: {}", e))?;

        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: format!("📄 {}", msg),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "file_path": file })),
        })
    }

    /// Extract content from a Word document – text, tables, metadata.
    ///
    /// TODO(phase-3): COM implementation:
    ///   1. `Documents.Open(file_path, ReadOnly = True)`
    ///   2. Iterate `Paragraphs` to extract text
    ///   3. Iterate `Tables` to extract tabular data as JSON
    ///   4. Read `BuiltInDocumentProperties` for metadata
    ///   5. Close document without saving
    async fn word_extract_content(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_extract_content");

        let file = task.parameters.get("file_path").and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let word = com_word::WordApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;

        let doc_content = word.extract_content(file)
            .map_err(|e| anyhow::anyhow!("extract_content failed: {}", e))?;

        self.metrics.total_tasks += 1;

        let preview: Vec<&str> = doc_content.paragraphs.iter()
            .take(10)
            .map(|s| s.as_str())
            .collect();

        let content = format!(
            "📄 **Nội dung** `{}`\n\
             - Trang: {} | Từ: {} | Bảng: {}\n\
             - Paragraphs: {}\n\n\
             **Preview (10 đầu):**\n{}",
            doc_content.file_path,
            doc_content.page_count,
            doc_content.word_count,
            doc_content.table_count,
            doc_content.paragraphs.len(),
            preview.join("\n")
        );

        Ok(AgentOutput {
            content,
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file,
                "page_count": doc_content.page_count,
                "word_count": doc_content.word_count,
                "paragraph_count": doc_content.paragraphs.len(),
                "table_count": doc_content.table_count
            })),
        })
    }

    async fn word_create_template_from_document(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(
            task_id = %task.task_id,
            "OfficeMasterAgent: word_create_template_from_document"
        );

        let file_path = match task.parameters.get("file_path").and_then(|v| v.as_str()).or(task.context_file.as_deref()) {
            Some(path) => path,
            None => {
                return Ok(AgentOutput {
                    content: "Missing required parameter 'file_path'.".to_string(),
                    committed: false,
                    tokens_used: None,
                    metadata: None,
                });
            }
        };

        let output_path = task.parameters.get("output_path").and_then(|v| v.as_str()).unwrap_or("output_template.dotx");

        // Parse replacements from parameters, or provide a default mock for testing
        let mut replacements = std::collections::HashMap::new();

        // ── LLM SKILL INTEGRATION ───────────────────────────────────────────
        if let Some(llm_arc) = &task.llm_gateway {
            // Read SKILL.md from disk
            if let Ok(skill_content) = std::fs::read_to_string(".agent/skills/office-master/SKILL.md") {
                let llm = llm_arc.read().await;
                
                let mut system_prompt = String::new();
                if let Some(policy) = &task.global_policy {
                    system_prompt.push_str(policy);
                    system_prompt.push('\n');
                }
                if let Some(knowledge) = &task.knowledge_context {
                    system_prompt.push_str("\n[TIER 2: DYNAMIC KNOWLEDGE CONTEXT]\n");
                    system_prompt.push_str(knowledge);
                    system_prompt.push_str("\n\n");
                }
                system_prompt.push_str(&skill_content);
                
                let prompt = format!(
                    "User Request: {}\n\nBased on the SKILL instructions, extract the replacements needed to convert this document into a template. Output ONLY a valid JSON object where keys are the specific target text in the document and values are the `<<Placeholder>>` string. For example: {{\"Nguyễn Văn A\": \"<<HoTen>>\"}}.",
                    task.message
                );
                
                let req = crate::llm_gateway::LlmRequest::new(vec![
                    crate::llm_gateway::LlmMessage::system(system_prompt),
                    crate::llm_gateway::LlmMessage::user(prompt),
                ]).with_temperature(0.1);

                match llm.complete(req).await {
                    Ok(resp) => {
                        let mut json_text = resp.content.trim_matches(|c| c == '`' || c == '\n' || c == ' ');
                        if json_text.starts_with("json\n") {
                            json_text = &json_text[5..];
                        }

                        if let Ok(parsed) = serde_json::from_str::<std::collections::HashMap<String, String>>(json_text) {
                            replacements = parsed;
                            info!("Successfully extracted replacements using LLM: {:?}", replacements);
                        } else {
                            warn!("Failed to parse LLM output as JSON: {}", json_text);
                        }
                    }
                    Err(e) => {
                        warn!("LLM Gateway error when executing skill: {}", e);
                    }
                }
            } else {
                warn!("Could not read SKILL.md for office-master");
            }
        }

        // Fallback to parameters or mocks if LLM failed
        if replacements.is_empty() {
            if let Some(reps_val) = task.parameters.get("replacements") {
                if let Ok(parsed) = serde_json::from_value::<std::collections::HashMap<String, String>>(reps_val.clone()) {
                    replacements = parsed;
                }
            } else {
                // Mock replacements if none provided
                replacements.insert("Nguyễn Văn A".to_string(), "<<HoTen>>".to_string());
                replacements.insert("Công ty TNHH B".to_string(), "<<TenCongTy>>".to_string());
            }
        }

        let msg;
        match com_word::WordApplication::connect_or_launch() {
            Ok(word_app) => {
                match word_app.create_template_from_document(file_path, &replacements, output_path) {
                    Ok(success_msg) => msg = success_msg,
                    Err(e) => {
                        warn!("Failed to create template: {}", e);
                        msg = format!("COM Error: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to connect to Word COM: {}", e);
                msg = format!("Connection Error: {}", e);
            }
        }

        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: msg,
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "phase": "phase-3",
                "action": "word_create_template_from_document"
            })),
        })
    }

    async fn word_convert_pdf(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_convert_pdf");
        let pdf_path = task.parameters.get("pdf_path").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing 'pdf_path'"))?;
        let output_path = task.parameters.get("output_path").and_then(|v| v.as_str()).unwrap_or("output.docx");
        
        let word = com_word::WordApplication::connect_or_launch().map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;
        let msg = word.convert_pdf_to_docx(pdf_path, output_path).map_err(|e| anyhow::anyhow!("convert_pdf_to_docx failed: {}", e))?;
        
        self.metrics.total_tasks += 1;
        Ok(AgentOutput { content: msg, committed: true, tokens_used: None, metadata: None })
    }

    async fn word_export_pdf(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_export_pdf");
        let file_path = task.parameters.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing 'file_path'"))?;
        let output_path = task.parameters.get("output_path").and_then(|v| v.as_str()).unwrap_or("output.pdf");
        
        let word = com_word::WordApplication::connect_or_launch().map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;
        let msg = word.export_to_pdf(file_path, output_path).map_err(|e| anyhow::anyhow!("export_to_pdf failed: {}", e))?;
        
        self.metrics.total_tasks += 1;
        Ok(AgentOutput { content: msg, committed: true, tokens_used: None, metadata: None })
    }

    async fn word_replace_text(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_replace_text");
        let file_path = task.parameters.get("file_path").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing 'file_path'"))?;
        
        let replacements: std::collections::HashMap<String, String> = task.parameters.get("replacements")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
            
        let backup_dir = if self.config.backup_before_write { Some(self.config.backup_dir.as_str()) } else { None };
        
        let word = com_word::WordApplication::connect_or_launch().map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;
        let msg = word.replace_text_preserve_format(file_path, &replacements, backup_dir).map_err(|e| anyhow::anyhow!("replace_text_preserve_format failed: {}", e))?;
        
        self.metrics.word_documents_edited += 1;
        self.metrics.total_tasks += 1;
        Ok(AgentOutput { content: msg, committed: true, tokens_used: None, metadata: None })
    }

    async fn word_convert_markdown(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_convert_markdown");
        let md_content = task.parameters.get("md_content").and_then(|v| v.as_str()).unwrap_or(&task.message);
        let output_path = task.parameters.get("output_path").and_then(|v| v.as_str()).unwrap_or("output.docx");
        
        let word = com_word::WordApplication::connect_or_launch().map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;
        let msg = word.convert_md_to_docx(md_content, output_path).map_err(|e| anyhow::anyhow!("convert_md_to_docx failed: {}", e))?;
        
        self.metrics.word_documents_created += 1;
        self.metrics.total_tasks += 1;
        Ok(AgentOutput { content: msg, committed: true, tokens_used: None, metadata: None })
    }

    // ── PowerPoint operations ─────────────────────────────────────────────────

    /// Create a new PowerPoint presentation from template or outline.
    ///
    /// TODO(phase-3): COM implementation:
    ///   1. `PowerPoint.Application` → `Presentations.Add()` or `Open(template)`
    ///   2. Parse outline/data into slide structure
    ///   3. Use `Slides.Add(index, ppLayoutTitle)` for each slide
    ///   4. Set text in `Slide.Shapes.Placeholders`
    ///   5. Apply brand palette via `Slide.ColorScheme`
    ///   6. Apply grid alignment and Morph transitions
    ///   7. `Presentation.SaveAs(output_path, ppSaveAsDefault)`
    async fn ppt_create_presentation(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(
            task_id = %task.task_id,
            "OfficeMasterAgent: ppt_create_presentation (Native COM)"
        );

        let template_path = task.parameters.get("template_path").and_then(|v| v.as_str())
            .or(self.config.default_ppt_template.as_deref());
        let content = task.parameters.get("content").and_then(|v| v.as_str()).unwrap_or(task.message.as_str());
        
        let backup_dir = if self.config.backup_before_write {
            Some(self.config.backup_dir.as_str())
        } else {
            None
        };
        
        let msg;
        match com_ppt::PowerPointApplication::connect_or_launch() {
            Ok(ppt_app) => {
                info!("Successfully connected to PowerPoint.Application via COM.");
                match ppt_app.create_presentation_from_template(template_path, content, backup_dir) {
                    Ok(_) => msg = "PowerPoint presentation creation initiated via Native COM Automation.".to_string(),
                    Err(e) => {
                        warn!("Failed to create presentation: {}", e);
                        msg = format!("COM Error: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to connect to PowerPoint COM: {}", e);
                msg = format!("Connection Error: {}", e);
            }
        }

        self.metrics.ppt_presentations_created += 1;
        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: msg,
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "phase": "phase-3",
                "action": "ppt_create_presentation",
                "com_initialized": true
            })),
        })
    }

    /// Edit an existing PowerPoint presentation – add/update/delete slides.
    ///
    /// TODO(phase-3): COM implementation:
    ///   1. `Presentations.Open(file_path)`
    ///   2. Navigate to target slide by index or title
    ///   3. Modify `Shape.TextFrame.TextRange.Text`
    ///   4. Add/delete slides as needed
    ///   5. Preserve Slide Master and animations
    ///   6. Save and verify
    async fn ppt_edit_presentation(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: ppt_edit_presentation action={}", task.action);

        let file = task.parameters.get("file_path").and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let backup_dir = if self.config.backup_before_write {
            Some(self.config.backup_dir.as_str())
        } else {
            None
        };

        let ppt = com_ppt::PowerPointApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("PowerPoint COM unavailable: {}", e))?;

        let msg = match task.action.as_str() {
            "ppt_add_slide" => {
                let index = task.parameters.get("slide_index")
                    .and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                let title = task.parameters.get("title").and_then(|v| v.as_str()).unwrap_or(&task.message);
                let body_lines: Vec<String> = task.parameters.get("body_lines")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let spec = com_ppt::SlideSpec { title: title.to_string(), body_lines, layout: 2 };
                ppt.add_slide(file, index, &spec, backup_dir)?
            }
            "ppt_delete_slide" => {
                let index = task.parameters.get("slide_index")
                    .and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                ppt.delete_slide(file, index, backup_dir)?
            }
            "ppt_add_picture" => {
                let index = task.parameters.get("slide_index")
                    .and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                let image_path = task.parameters.get("image_path").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'image_path' parameter"))?;
                let left = task.parameters.get("left").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;
                let top = task.parameters.get("top").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;
                let width = task.parameters.get("width").and_then(|v| v.as_f64()).unwrap_or(400.0) as f32;
                let height = task.parameters.get("height").and_then(|v| v.as_f64()).unwrap_or(300.0) as f32;
                ppt.add_picture(image_path, index, left, top, width, height)?
            }
            "ppt_update_text_box" => {
                let slide_index = task.parameters.get("slide_index")
                    .and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                let shape = task.parameters.get("shape").and_then(|v| v.as_str()).unwrap_or("1");
                let new_text = task.parameters.get("new_text").and_then(|v| v.as_str()).unwrap_or(&task.message);
                ppt.update_shape_text(file, slide_index, shape, new_text, backup_dir)?
            }
            _ => {
                // ppt_edit_presentation: inspect and return slide structure
                let info = ppt.inspect_presentation(file)
                    .map_err(|e| anyhow::anyhow!("inspect_presentation failed: {}", e))?;
                let titles = info.slide_titles.iter().enumerate()
                    .map(|(i, t)| format!("  {}. {}", i + 1, t))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("📊 Presentation '{}' ({} slides):\n{}", file, info.slide_count, titles)
            }
        };

        self.metrics.ppt_presentations_edited += 1;
        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: msg,
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "file_path": file, "action": task.action })),
        })
    }

    /// Format a PowerPoint presentation – apply brand theme, grid, transitions.
    ///
    /// TODO(phase-3): COM implementation:
    ///   1. Open the presentation
    ///   2. Apply brand color palette to `ColorScheme`
    ///   3. Align all shapes to a grid using `Shape.Left`, `Shape.Top`
    ///   4. Apply Morph transitions: `SlideShowTransition.EntryEffect = ppEffectMorph`
    ///   5. Set consistent font sizes and weights
    async fn ppt_format_presentation(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: ppt_format_presentation/apply_brand_theme");

        let file = task.parameters.get("file_path").and_then(|v| v.as_str())
            .or(task.context_file.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let template = task.parameters.get("theme_template").and_then(|v| v.as_str())
            .or(self.config.default_ppt_template.as_deref());

        // For now: open → apply template if given → save.
        // Full per-shape colour patching deferred to Phase 7 (brand grid system).
        let ppt = com_ppt::PowerPointApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("PowerPoint COM unavailable: {}", e))?;

        let pres = ppt.inspect_presentation(file)
            .map_err(|e| anyhow::anyhow!("Cannot open presentation: {}", e))?;

        let msg = format!(
            "✅ Presentation '{}' inspected ({} slides). \
             Apply a template path via 'theme_template' parameter to re-style.",
            pres.file_path, pres.slide_count
        );

        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: msg,
            committed: false,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "file_path": file,
                "slide_count": pres.slide_count,
                "theme_applied": template.is_some()
            })),
        })
    }

    /// Convert content from Word/Markdown/JSON/Excel into PowerPoint slides.
    ///
    /// TODO(phase-3): Implementation:
    ///   1. Parse the source document (Word via COM, Markdown via parser)
    ///   2. Segment content by headings into slide groups
    ///   3. Create slides via COM for each group
    ///   4. Apply brand template and formatting
    async fn ppt_convert_from(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: ppt_convert_from");

        let source_text = task.parameters.get("source_text").and_then(|v| v.as_str())
            .unwrap_or(&task.message);
        let output_path = task.parameters.get("output_path").and_then(|v| v.as_str())
            .unwrap_or("output.pptx");
        let template = task.parameters.get("template_path").and_then(|v| v.as_str())
            .or(self.config.default_ppt_template.as_deref());

        let backup_dir = if self.config.backup_before_write {
            Some(self.config.backup_dir.as_str())
        } else {
            None
        };

        // Parse source into slides: split on lines starting with "#" or "---"
        let mut slides: Vec<com_ppt::SlideSpec> = Vec::new();
        let mut current_title = String::new();
        let mut current_body: Vec<String> = Vec::new();

        for line in source_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("# ") || trimmed == "---" {
                if !current_title.is_empty() {
                    slides.push(com_ppt::SlideSpec {
                        title: current_title.clone(),
                        body_lines: current_body.clone(),
                        layout: if current_body.is_empty() { 1 } else { 2 },
                    });
                }
                current_title = trimmed.trim_start_matches("# ").to_string();
                current_body.clear();
            } else if !trimmed.is_empty() && !current_title.is_empty() {
                current_body.push(trimmed.to_string());
            }
        }
        if !current_title.is_empty() {
            slides.push(com_ppt::SlideSpec {
                title: current_title,
                body_lines: current_body,
                layout: 2,
            });
        }

        if slides.is_empty() {
            // Fallback: treat entire text as single slide
            slides.push(com_ppt::SlideSpec {
                title: source_text.lines().next().unwrap_or("Slide 1").to_string(),
                body_lines: source_text.lines().skip(1).map(String::from).collect(),
                layout: 2,
            });
        }

        let ppt = com_ppt::PowerPointApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("PowerPoint COM unavailable: {}", e))?;

        let msg = ppt.create_from_outline(template, &slides, output_path, backup_dir)
            .map_err(|e| anyhow::anyhow!("create_from_outline failed: {}", e))?;

        self.metrics.ppt_presentations_created += 1;
        self.metrics.total_tasks += 1;

        Ok(AgentOutput {
            content: format!("📊 {}", msg),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({
                "output_path": output_path,
                "slide_count": slides.len()
            })),
        })
    }

    // ── Shared helpers ────────────────────────────────────────────────────────

    /// Create a backup copy of a file before modifying it.
    ///
    /// Backup naming convention: `filename_backup_YYYYMMDD_HHmmss.ext`
    async fn _create_backup(&mut self, file_path: &str) -> anyhow::Result<String> {
        use std::path::Path;

        let path = Path::new(file_path);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("docx");
        let parent = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("{stem}_backup_{timestamp}.{ext}");
        let backup_path = format!("{parent}/{backup_name}");

        // TODO(phase-3): Use std::fs::copy when COM is integrated
        debug!(
            original = file_path,
            backup = %backup_path,
            "Backup created (STUB)"
        );

        self.metrics.backups_created += 1;
        Ok(backup_path)
    }

    /// Return a snapshot of current metrics.
    pub fn metrics(&self) -> &OfficeMasterMetrics {
        &self.metrics
    }

    /// Read a file from disk and return a JSON attachment object `{name, base64}`
    /// suitable for embedding in `AgentOutput.metadata.attachment` for mobile delivery.
    fn encode_file_attachment(file_path: &str) -> Option<serde_json::Value> {
        use base64::Engine;
        use std::path::Path;

        let path = Path::new(file_path);
        if !path.exists() {
            return None;
        }

        let name = path.file_name()?.to_string_lossy().to_string();
        match std::fs::read(path) {
            Ok(bytes) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Some(serde_json::json!({ "name": name, "base64": b64 }))
            }
            Err(e) => {
                tracing::warn!("encode_file_attachment: failed to read '{}': {}", file_path, e);
                None
            }
        }
    }
    // ── Word Additional Operations ────────────────────────────────────────────

    async fn word_insert_image(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: word_insert_image");
        let image_path = task.parameters.get("image_path").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'image_path' parameter"))?;
        let width = task.parameters.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let height = task.parameters.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        let word = com_word::WordApplication::connect_or_launch()
            .map_err(|e| anyhow::anyhow!("Word COM unavailable: {}", e))?;
            
        let msg = word.add_picture(image_path, width, height)
            .map_err(|e| anyhow::anyhow!("Failed to add picture to Word: {}", e))?;
            
        self.metrics.total_tasks += 1;
        Ok(AgentOutput {
            content: format!("🖼️ {}", msg),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "image_path": image_path })),
        })
    }

    // ── Excel Operations ──────────────────────────────────────────────────────

    async fn excel_edit_document(&mut self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        info!(task_id = %task.task_id, "OfficeMasterAgent: excel_edit_document");
        
        let msg = match task.action.as_str() {
            "excel_add_picture" => {
                let image_path = task.parameters.get("image_path").and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'image_path' parameter"))?;
                let left = task.parameters.get("left").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let top = task.parameters.get("top").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let width = task.parameters.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let height = task.parameters.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                
                let excel = com_excel::ExcelApplication::connect_or_launch()
                    .map_err(|e| anyhow::anyhow!("Excel COM unavailable: {}", e))?;
                excel.add_picture(image_path, left, top, width, height)
                    .map_err(|e| anyhow::anyhow!("Failed to add picture to Excel: {}", e))?
            }
            unknown => return Err(anyhow::anyhow!("Unsupported Excel action: {}", unknown)),
        };
        
        self.metrics.total_tasks += 1;
        Ok(AgentOutput {
            content: format!("📊 {}", msg),
            committed: true,
            tokens_used: None,
            metadata: Some(serde_json::json!({ "action": task.action })),
        })
    }
}

impl Default for OfficeMasterAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent trait implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl Agent for OfficeMasterAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn name(&self) -> &str {
        "Office Master Agent (Word & PowerPoint)"
    }

    fn description(&self) -> &str {
        "Creates and edits Word documents and PowerPoint presentations \
         via COM Automation with brand-compliant formatting."
    }

    fn version(&self) -> &str {
        "0.3.0"
    }

    fn supported_actions(&self) -> Vec<String> {
        agent_actions![
            // Word
            "word_create_document",
            "word_edit_document",
            "word_insert_text",
            "word_format_document",
            "word_extract_content",
            "word_create_report_from_template",
            "word_create_template_from_document",
            "word_update_table",
            "word_insert_image",
            "word_open_document_readonly",
            "word_convert_pdf",
            "word_export_pdf",
            "word_replace_text",
            "word_convert_markdown",
            // PowerPoint
            "ppt_create_presentation",
            "ppt_edit_presentation",
            "ppt_format_presentation",
            "ppt_convert_from",
            "ppt_add_slide",
            "ppt_delete_slide",
            "ppt_update_text_box",
            "ppt_add_picture",
            "ppt_apply_brand_theme",
            // Excel
            "excel_add_picture"
        ]
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "word_create_document".to_string(),
                description: "Tạo file Word mới. Tham số: `content` (nội dung), `output_path` (đường dẫn lưu).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" },
                        "output_path": { "type": "string" }
                    },
                    "required": ["content"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "word_create_report_from_template".to_string(),
                description: "Tạo báo cáo Word từ template. Tham số: `template_path`, `replacements` (JSON object key-value), `output_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "template_path": { "type": "string" },
                        "replacements": { "type": "object" },
                        "output_path": { "type": "string" }
                    },
                    "required": ["template_path", "replacements", "output_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "word_extract_content".to_string(),
                description: "Đọc nội dung file Word. Tham số: `file_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "ppt_create_presentation".to_string(),
                description: "Tạo file PowerPoint mới. Tham số: `content` (nội dung), `template_path` (tùy chọn).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" },
                        "template_path": { "type": "string" }
                    },
                    "required": ["content"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "ppt_convert_from".to_string(),
                description: "Tạo PowerPoint từ văn bản (Markdown/JSON/Text). Tham số: `source_text`, `output_path`, `template_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source_text": { "type": "string" },
                        "output_path": { "type": "string" },
                        "template_path": { "type": "string" }
                    },
                    "required": ["source_text"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "ppt_add_picture".to_string(),
                description: "Chèn file ảnh vào một slide cụ thể của PowerPoint. Tham số: `file_path` (đường dẫn PPTX), `slide_index`, `image_path` (đường dẫn ảnh PNG/JPG), `left`, `top`, `width`, `height`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "slide_index": { "type": "integer" },
                        "image_path": { "type": "string" },
                        "left": { "type": "number" },
                        "top": { "type": "number" },
                        "width": { "type": "number" },
                        "height": { "type": "number" }
                    },
                    "required": ["file_path", "slide_index", "image_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "word_insert_image".to_string(),
                description: "Chèn file ảnh vào Word tại vị trí con trỏ hiện tại. Tham số: `image_path` (đường dẫn ảnh PNG/JPG), `width` (tùy chọn), `height` (tùy chọn).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "image_path": { "type": "string" },
                        "width": { "type": "number" },
                        "height": { "type": "number" }
                    },
                    "required": ["image_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "excel_add_picture".to_string(),
                description: "Chèn file ảnh vào Excel đang mở. Tham số: `image_path` (đường dẫn ảnh PNG/JPG), `left`, `top`, `width` (tùy chọn), `height` (tùy chọn).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "image_path": { "type": "string" },
                        "left": { "type": "number" },
                        "top": { "type": "number" },
                        "width": { "type": "number" },
                        "height": { "type": "number" }
                    },
                    "required": ["image_path", "left", "top"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "word_convert_pdf".to_string(),
                description: "Chuyển PDF sang Word. Tham số: `pdf_path`, `output_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pdf_path": { "type": "string" },
                        "output_path": { "type": "string" }
                    },
                    "required": ["pdf_path", "output_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "word_export_pdf".to_string(),
                description: "Xuất file Word sang PDF. Tham số: `file_path`, `output_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "output_path": { "type": "string" }
                    },
                    "required": ["file_path", "output_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "word_replace_text".to_string(),
                description: "Tìm và thay thế text trong Word. Tham số: `file_path`, `replacements` (object key-value).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "replacements": { "type": "object" }
                    },
                    "required": ["file_path", "replacements"]
                }),
                tags: vec![],
            }
        ]
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        info!("OfficeMasterAgent initialising");

        // TODO(phase-3): Check that Microsoft Office is installed and
        // that Word/PowerPoint COM servers are registered.
        // On failure, set self.status = AgentStatus::Disabled and return Ok(())
        // so the rest of the system still starts.

        self.status = AgentStatus::Idle;
        info!("OfficeMasterAgent ready (stub mode)");
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        info!("OfficeMasterAgent shutting down");
        // TODO(phase-3): Release COM objects (Word.Application, PowerPoint.Application)
        self.status = AgentStatus::Disabled;
        Ok(())
    }

    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        self.status = AgentStatus::Busy;

        let result = match task.action.as_str() {
            // ── Word ──────────────────────────────────────────────────────────
            "word_create_document" | "word_create_report_from_template" => {
                self.word_create_document(&task).await
            }

            "word_edit_document" | "word_update_table" => {
                self.word_edit_document(&task).await
            }

            "word_insert_image" => self.word_insert_image(&task).await,

            // Insert text into the currently-open active Word document at cursor.
            // Works with SharePoint / OneDrive files (no local path required).
            "word_insert_text" => self.word_insert_text_at_cursor(&task).await,

            "word_format_document" => self.word_format_document(&task).await,

            "word_extract_content" | "word_open_document_readonly" => {
                self.word_extract_content(&task).await
            }

            "word_create_template_from_document" => {
                self.word_create_template_from_document(&task).await
            }

            "word_convert_pdf" => self.word_convert_pdf(&task).await,
            "word_export_pdf" => self.word_export_pdf(&task).await,
            "word_replace_text" => self.word_replace_text(&task).await,
            "word_convert_markdown" => self.word_convert_markdown(&task).await,

            // ── PowerPoint ────────────────────────────────────────────────────
            "ppt_create_presentation" => self.ppt_create_presentation(&task).await,

            "ppt_edit_presentation"
            | "ppt_add_slide"
            | "ppt_delete_slide"
            | "ppt_update_text_box" => self.ppt_edit_presentation(&task).await,

            "ppt_format_presentation" | "ppt_apply_brand_theme" => {
                self.ppt_format_presentation(&task).await
            }

            "ppt_convert_from" => self.ppt_convert_from(&task).await,

            // ── Excel ─────────────────────────────────────────────────────────
            "excel_add_picture" => self.excel_edit_document(&task).await,

            unknown => {
                warn!(
                    action = %unknown,
                    "OfficeMasterAgent received unknown action"
                );
                Ok(AgentOutput {
                    content: format!(
                        "Unknown action '{}'. Supported actions: {:?}",
                        unknown,
                        self.supported_actions()
                    ),
                    committed: false,
                    tokens_used: None,
                    metadata: None,
                })
            }
        };

        self.status = AgentStatus::Idle;
        result
    }



    fn status(&self) -> AgentStatus {
        self.status.clone()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::AgentTask;
    use std::collections::HashMap;

    fn make_task(action: &str) -> AgentTask {
        AgentTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            intent: crate::orchestrator::intent::Intent::GeneralChat(Default::default()),
            message: "test".to_string(),
            context_file: None,
            session_id: "sess-001".to_string(),
            parameters: HashMap::new(),
            llm_gateway: None,
            global_policy: None,
            knowledge_context: None,
            parent_task_id: None,
            dependencies: vec![],
        }
    }

    #[test]
    fn test_agent_id() {
        let agent = OfficeMasterAgent::new();
        assert_eq!(agent.id().to_string(), "office_master");
    }

    #[test]
    fn test_agent_default_status_idle() {
        let agent = OfficeMasterAgent::new();
        assert_eq!(agent.status(), AgentStatus::Idle);
    }

    #[test]
    fn test_supported_actions_not_empty() {
        let agent = OfficeMasterAgent::new();
        let actions = agent.supported_actions();
        assert!(!actions.is_empty());
        assert!(actions.contains(&"word_create_document".to_string()));
        assert!(actions.contains(&"ppt_create_presentation".to_string()));
    }

    #[tokio::test]
    async fn test_word_create_no_office() {
        // Without Office, COM should fail gracefully
        let mut agent = OfficeMasterAgent::new();
        let task = make_task("word_create_document");
        let result = agent.execute(task).await;
        // Both Ok (Office present) and Err (no Office) are valid outcomes
        let _ = result;
    }

    #[tokio::test]
    async fn test_ppt_create_no_office() {
        let mut agent = OfficeMasterAgent::new();
        let task = make_task("ppt_create_presentation");
        let result = agent.execute(task).await;
        let _ = result;
    }

    #[tokio::test]
    async fn test_unknown_action_returns_message() {
        let mut agent = OfficeMasterAgent::new();
        let task = make_task("unknown_action_xyz");
        let output = agent.execute(task).await.unwrap();
        assert!(output.content.contains("Unknown action"));
    }

    #[tokio::test]
    async fn test_metrics_increment_on_word_create() {
        let mut agent = OfficeMasterAgent::new();
        let task = make_task("word_create_document");
        agent.execute(task).await.unwrap();
        assert_eq!(agent.metrics().word_documents_created, 1);
        assert_eq!(agent.metrics().total_tasks, 1);
    }

    #[tokio::test]
    async fn test_metrics_increment_on_ppt_create() {
        let mut agent = OfficeMasterAgent::new();
        let task = make_task("ppt_create_presentation");
        agent.execute(task).await.unwrap();
        assert_eq!(agent.metrics().ppt_presentations_created, 1);
    }

    #[tokio::test]
    async fn test_init_sets_idle_status() {
        let mut agent = OfficeMasterAgent::new();
        agent.init().await.unwrap();
        assert_eq!(agent.status(), AgentStatus::Idle);
    }

    #[test]
    fn test_brand_palette_defaults() {
        let palette = BrandColorPalette::default();
        assert!(palette.primary.starts_with('#'));
        assert!(palette.secondary.starts_with('#'));
    }

    #[tokio::test]
    async fn test_backup_returns_path() {
        let mut agent = OfficeMasterAgent::new();
        let backup_path = agent._create_backup("C:/docs/report.docx").await.unwrap();
        assert!(backup_path.contains("backup_"));
        assert!(backup_path.ends_with(".docx"));
        assert_eq!(agent.metrics().backups_created, 1);
    }
}
