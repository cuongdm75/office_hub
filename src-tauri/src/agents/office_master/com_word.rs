// ============================================================================
// Office Hub – agents/office_master/com_word.rs
//
// Word COM Automation – Phase 3 Real Implementation
// ============================================================================

#[cfg(windows)]
use windows::{
    core::BSTR,
    Win32::System::Com::{
        CLSIDFromProgID, CoCreateInstance, CoInitializeEx, IDispatch,
        CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
    },
};

#[cfg(windows)]
use crate::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, var_r4, ComObject};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Text content extracted from a Word document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentContent {
    pub file_path: String,
    pub paragraphs: Vec<String>,
    pub table_count: usize,
    pub word_count: usize,
    pub page_count: usize,
}

/// Result of a Find & Replace operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplaceResult {
    pub replacements_made: usize,
    pub output_path: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// WordApplication
// ─────────────────────────────────────────────────────────────────────────────

pub struct WordApplication {
    #[cfg(windows)]
    pub app: ComObject,
}

impl WordApplication {
    // ── Connect ───────────────────────────────────────────────────────────────

    #[cfg(windows)]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let clsid = CLSIDFromProgID(windows::core::w!("Word.Application"))
                .map_err(|e| anyhow::anyhow!("Word.Application not registered: {}", e))?;
            let dispatch: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| anyhow::anyhow!("CoCreateInstance Word.Application failed: {}", e))?;
            let app = ComObject::new(dispatch);
            // Suppress prompts
            let _ = app.set_property("DisplayAlerts", var_bool(false));
            Ok(Self { app })
        }
    }

    #[cfg(not(windows))]
    pub fn connect_or_launch() -> anyhow::Result<Self> {
        anyhow::bail!("Word COM Automation is only supported on Windows")
    }

    // ── Open / Close ──────────────────────────────────────────────────────────

    /// Open a document. Returns a `ComObject` for the Document.
    /// If already open, returns that instance instead.
    #[cfg(windows)]
    pub fn open_document(&self, file_path: &str, read_only: bool) -> anyhow::Result<ComObject> {
        let docs = self.app.get_property_obj("Documents")?;

        // Check if already open
        let count = i32::try_from(&docs.get_property("Count")?).unwrap_or(0);
        for i in 1..=count {
            if let Ok(dv) = docs.invoke_method("Item", vec![var_i4(i)]) {
                if let Ok(disp) = IDispatch::try_from(&dv) {
                    let doc = ComObject::new(disp);
                    let name = doc.get_property("FullName").ok()
                        .and_then(|v| BSTR::try_from(&v).ok())
                        .map(|b| b.to_string())
                        .unwrap_or_default();
                    if name.to_lowercase() == file_path.to_lowercase() {
                        return Ok(doc);
                    }
                }
            }
        }

        let doc_var = docs.invoke_method("Open", vec![
            var_bstr(file_path),
            var_bool(false),        // ConfirmConversions
            var_bool(read_only),    // ReadOnly
        ]).map_err(|e| anyhow::anyhow!("Documents.Open('{}') failed: {}", file_path, e))?;

        let doc_disp = IDispatch::try_from(&doc_var)
            .map_err(|e| anyhow::anyhow!("Open returned non-object: {}", e))?;
        Ok(ComObject::new(doc_disp))
    }

    #[cfg(not(windows))]
    pub fn open_document(&self, _file_path: &str, _read_only: bool) -> anyhow::Result<ComObject> {
        anyhow::bail!("not available on non-Windows")
    }

    /// Close a document. `save = 0` → don't save, `save = -1` → save.
    #[cfg(windows)]
    pub fn close_document(doc: &ComObject, save: bool) -> anyhow::Result<()> {
        // wdSaveChanges = -1, wdDoNotSaveChanges = 0
        let save_val = if save { -1i32 } else { 0i32 };
        let _ = doc.invoke_method("Close", vec![var_i4(save_val)]);
        Ok(())
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    /// Extract all paragraph texts and basic stats from a document.
    #[cfg(windows)]
    pub fn extract_content(&self, file_path: &str) -> anyhow::Result<DocumentContent> {
        let doc = self.open_document(file_path, true)?;

        let paragraphs_obj = doc.get_property_obj("Paragraphs")?;
        let para_count = i32::try_from(&paragraphs_obj.get_property("Count")?)
            .unwrap_or(0);

        let mut paragraphs = Vec::with_capacity(para_count as usize);
        for i in 1..=para_count {
            if let Ok(pv) = paragraphs_obj.invoke_method("Item", vec![var_i4(i)]) {
                if let Ok(pdisp) = IDispatch::try_from(&pv) {
                    let para = ComObject::new(pdisp);
                    if let Ok(range) = para.get_property_obj("Range") {
                        let text = range.get_property("Text").ok()
                            .and_then(|v| BSTR::try_from(&v).ok())
                            .map(|b| b.to_string().trim_end_matches('\r').to_string())
                            .unwrap_or_default();
                        if !text.is_empty() {
                            paragraphs.push(text);
                        }
                    }
                }
            }
        }

        let table_count = doc.get_property_obj("Tables")
            .ok()
            .and_then(|t| t.get_property("Count").ok())
            .and_then(|v| i32::try_from(&v).ok())
            .unwrap_or(0) as usize;

        let word_count = doc.get_property_obj("Words")
            .ok()
            .and_then(|w| w.get_property("Count").ok())
            .and_then(|v| i32::try_from(&v).ok())
            .unwrap_or(0) as usize;

        let page_count = doc
            .invoke_method("ComputeStatistics", vec![var_i4(2)]) // wdStatisticPages = 2
            .ok()
            .and_then(|v| i32::try_from(&v).ok())
            .unwrap_or(0) as usize;

        Self::close_document(&doc, false)?;

        Ok(DocumentContent {
            file_path: file_path.to_string(),
            paragraphs,
            table_count,
            word_count,
            page_count,
        })
    }

    #[cfg(not(windows))]
    pub fn extract_content(&self, file_path: &str) -> anyhow::Result<DocumentContent> {
        Ok(DocumentContent {
            file_path: file_path.to_string(),
            paragraphs: vec![],
            table_count: 0,
            word_count: 0,
            page_count: 0,
        })
    }

    // ── Write ─────────────────────────────────────────────────────────────────

    /// Create a new document from template, type `content` via Selection, save to `output_path`.
    #[cfg(windows)]
    pub fn create_report_from_template(
        &self,
        template_path: Option<&str>,
        content: &str,
        output_path: Option<&str>,
    ) -> anyhow::Result<String> {
        let docs = self.app.get_property_obj("Documents")?;
        let args = match template_path {
            Some(p) if std::path::Path::new(p).exists() => vec![var_bstr(p)],
            _ => vec![],
        };

        let doc_var = docs.invoke_method("Add", args)?;

        let doc = ComObject::new(
            IDispatch::try_from(&doc_var)
                .map_err(|e| anyhow::anyhow!("Documents.Add non-object: {}", e))?,
        );

        let selection = self.app.get_property_obj("Selection")?;
        selection.invoke_method("TypeText", vec![var_bstr(content)])?;

        if let Some(out_path) = output_path {
            // wdFormatXMLDocument = 12
            doc.invoke_method("SaveAs2", vec![var_bstr(out_path), var_i4(12)])?;
            doc.invoke_method("Close", vec![var_i4(0)])?;
            return Ok(format!("Document saved successfully to {}", out_path));
        }

        Ok("Document created successfully".to_string())
    }

    #[cfg(not(windows))]
    pub fn create_report_from_template(
        &self,
        _template_path: Option<&str>,
        _content: &str,
        _output_path: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Edit bookmark-targeted text in an existing document.
    /// `edits` maps bookmark name → new text to insert at that bookmark.
    #[cfg(windows)]
    pub fn edit_document_by_bookmark(
        &self,
        file_path: &str,
        edits: &std::collections::HashMap<String, String>,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let doc = self.open_document(file_path, false)?;
        self.backup_document(&doc, backup_dir)?;

        let bookmarks = doc.get_property_obj("Bookmarks")?;

        let mut applied = 0usize;
        for (bookmark_name, new_text) in edits {
            // Check if bookmark exists
            let exists_var = bookmarks
                .invoke_method("Exists", vec![var_bstr(bookmark_name)])
                .ok()
                .and_then(|v| bool::try_from(&v).ok())
                .unwrap_or(false);

            if !exists_var {
                tracing::warn!("Bookmark '{}' not found in '{}'", bookmark_name, file_path);
                continue;
            }

            if let Ok(bm_var) = bookmarks.invoke_method("Item", vec![var_bstr(bookmark_name)]) {
                if let Ok(bm_disp) = IDispatch::try_from(&bm_var) {
                    let bm = ComObject::new(bm_disp);
                    if let Ok(range) = bm.get_property_obj("Range") {
                        range.set_property("Text", var_bstr(new_text))?;
                        applied += 1;
                    }
                }
            }
        }

        doc.invoke_method("Save", vec![])?;
        Self::close_document(&doc, false)?;

        Ok(format!(
            "Applied {} bookmark edits to '{}'",
            applied, file_path
        ))
    }

    #[cfg(not(windows))]
    pub fn edit_document_by_bookmark(
        &self,
        _file_path: &str,
        _edits: &std::collections::HashMap<String, String>,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Insert text into the active document at the current cursor position.
    /// Works regardless of whether the file is local or on SharePoint / OneDrive.
    /// Appends a paragraph break before the text so it starts on a new line.
    #[cfg(windows)]
    pub fn insert_text_at_cursor(&self, text: &str, new_paragraph: bool) -> anyhow::Result<String> {
        let selection = self.app.get_property_obj("Selection")
            .map_err(|_| anyhow::anyhow!("No active Word document open"))?;

        if new_paragraph {
            // Move to end, insert paragraph break first
            let _ = selection.invoke_method("EndKey", vec![var_i4(6)]); // wdStory = 6
            selection.invoke_method("TypeParagraph", vec![])?;
        }

        selection.invoke_method("TypeText", vec![var_bstr(text)])?;

        // Retrieve current doc name for reporting
        let doc_name = self.app.get_property_obj("ActiveDocument").ok()
            .and_then(|d| d.get_property("Name").ok())
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "document".to_string());

        Ok(format!("Inserted {} characters into '{}'", text.len(), doc_name))
    }

    #[cfg(not(windows))]
    pub fn insert_text_at_cursor(&self, _text: &str, _new_paragraph: bool) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Replace the entire content of the active document.
    #[cfg(windows)]
    pub fn replace_active_document(&self, text: &str) -> anyhow::Result<String> {
        let active_doc = self.app.get_property_obj("ActiveDocument")
            .map_err(|_| anyhow::anyhow!("No active Word document open"))?;

        let content = active_doc.get_property_obj("Content")
            .map_err(|_| anyhow::anyhow!("Failed to get Content of ActiveDocument"))?;

        content.set_property("Text", var_bstr(text))?;

        let doc_name = active_doc.get_property("Name").ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "document".to_string());

        Ok(format!("Replaced entire content of '{}'", doc_name))
    }

    #[cfg(not(windows))]
    pub fn replace_active_document(&self, _text: &str) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Save the active document.
    #[cfg(windows)]
    pub fn save_active_document(&self) -> anyhow::Result<String> {
        let active_doc = self.app.get_property_obj("ActiveDocument")
            .map_err(|_| anyhow::anyhow!("No active Word document open"))?;

        active_doc.invoke_method("Save", vec![])?;

        let doc_name = active_doc.get_property("Name").ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_else(|| "document".to_string());

        Ok(format!("Saved active document '{}'", doc_name))
    }

    #[cfg(not(windows))]
    pub fn save_active_document(&self) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Extract text from the active document.
    #[cfg(windows)]
    pub fn extract_active_document(&self) -> anyhow::Result<String> {
        let active_doc = self.app.get_property_obj("ActiveDocument")
            .map_err(|_| anyhow::anyhow!("No active Word document open"))?;

        let content = active_doc.get_property_obj("Content")
            .map_err(|_| anyhow::anyhow!("Failed to get Content of ActiveDocument"))?;

        let text = content.get_property("Text").ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string().trim_end_matches('\r').to_string())
            .unwrap_or_default();

        Ok(text)
    }

    #[cfg(not(windows))]
    pub fn extract_active_document(&self) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Find & Replace text across a document, then SaveAs to `output_path`.
    #[cfg(windows)]
    pub fn create_template_from_document(
        &self,
        file_path: &str,
        replacements: &std::collections::HashMap<String, String>,
        output_path: &str,
    ) -> anyhow::Result<String> {
        let docs = self.app.get_property_obj("Documents")?;
        let doc_var = docs.invoke_method("Open", vec![
            var_bstr(file_path),
            var_bool(false),
            var_bool(true), // ReadOnly
        ])?;
        let doc = ComObject::new(
            IDispatch::try_from(&doc_var)
                .map_err(|e| anyhow::anyhow!("Document not object: {}", e))?,
        );

        let content_range = doc.get_property_obj("Content")?;
        let find = content_range.get_property_obj("Find")?;

        for (old_text, new_text) in replacements {
            find.invoke_method("ClearFormatting", vec![])?;
            let replacement = find.get_property_obj("Replacement")?;
            replacement.invoke_method("ClearFormatting", vec![])?;
            find.set_property("Text", var_bstr(old_text))?;
            replacement.set_property("Text", var_bstr(new_text))?;
            find.set_property("Forward", var_bool(true))?;
            find.set_property("Wrap", var_i4(1))?; // wdFindContinue

            find.invoke_method("Execute", vec![
                var_bstr(""), var_bool(false), var_bool(false), var_bool(false),
                var_bool(false), var_bool(false), var_bool(true), var_i4(1),
                var_bool(false), var_bstr(""),
                var_i4(2), // wdReplaceAll
            ])?;
        }

        // wdFormatTemplate = 1
        doc.invoke_method("SaveAs2", vec![var_bstr(output_path), var_i4(1)])?;
        doc.invoke_method("Close", vec![var_i4(0)])?;

        Ok(format!("Template created at: {}", output_path))
    }

    #[cfg(not(windows))]
    pub fn create_template_from_document(
        &self,
        _file_path: &str,
        _replacements: &std::collections::HashMap<String, String>,
        _output_path: &str,
    ) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Format document: update TOC, refresh fields, apply page setup.
    #[cfg(windows)]
    pub fn format_document(&self, file_path: &str) -> anyhow::Result<String> {
        let doc = self.open_document(file_path, false)?;

        // Refresh all fields (cross-refs, page numbers, TOC entries)
        if let Ok(fields) = doc.get_property_obj("Fields") {
            let _ = fields.invoke_method("Update", vec![]);
        }

        // Update first table of contents if present
        if let Ok(tocs) = doc.get_property_obj("TablesOfContents") {
            let toc_count = i32::try_from(&tocs.get_property("Count")?).unwrap_or(0);
            for i in 1..=toc_count {
                if let Ok(tv) = tocs.invoke_method("Item", vec![var_i4(i)]) {
                    if let Ok(tdisp) = IDispatch::try_from(&tv) {
                        let toc = ComObject::new(tdisp);
                        let _ = toc.invoke_method("Update", vec![]);
                    }
                }
            }
        }

        doc.invoke_method("Save", vec![])?;
        Self::close_document(&doc, false)?;
        Ok(format!("Document '{}' formatted and saved.", file_path))
    }

    #[cfg(not(windows))]
    pub fn format_document(&self, _file_path: &str) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Convert PDF to DOCX using Word's native PDF opening capability
    #[cfg(windows)]
    pub fn convert_pdf_to_docx(&self, pdf_path: &str, output_path: &str) -> anyhow::Result<String> {
        let docs = self.app.get_property_obj("Documents")?;
        
        // Open PDF natively (False for ConfirmConversions)
        let doc_var = docs.invoke_method("Open", vec![
            var_bstr(pdf_path),
            var_bool(false),
            var_bool(true), // ReadOnly
        ]).map_err(|e| anyhow::anyhow!("Failed to open PDF natively in Word: {}", e))?;
        
        let doc = ComObject::new(
            IDispatch::try_from(&doc_var)
                .map_err(|e| anyhow::anyhow!("Document not object: {}", e))?,
        );

        // SaveAs2 with wdFormatXMLDocument = 12
        doc.invoke_method("SaveAs2", vec![var_bstr(output_path), var_i4(12)])?;
        Self::close_document(&doc, false)?;

        Ok(format!("Successfully converted PDF to DOCX at: {}", output_path))
    }

    #[cfg(not(windows))]
    pub fn convert_pdf_to_docx(&self, _pdf_path: &str, _output_path: &str) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Export a DOCX file to PDF natively
    #[cfg(windows)]
    pub fn export_to_pdf(&self, file_path: &str, output_path: &str) -> anyhow::Result<String> {
        let doc = self.open_document(file_path, true)?;

        // ExportAsFixedFormat: wdExportFormatPDF = 17
        doc.invoke_method("ExportAsFixedFormat", vec![
            var_bstr(output_path),
            var_i4(17), // ExportFormat
        ]).map_err(|e| anyhow::anyhow!("ExportAsFixedFormat failed: {}", e))?;

        Self::close_document(&doc, false)?;
        Ok(format!("Successfully exported DOCX to PDF at: {}", output_path))
    }

    #[cfg(not(windows))]
    pub fn export_to_pdf(&self, _file_path: &str, _output_path: &str) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Replace text globally while preserving formatting
    #[cfg(windows)]
    pub fn replace_text_preserve_format(
        &self,
        file_path: &str,
        replacements: &std::collections::HashMap<String, String>,
        backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        let doc = self.open_document(file_path, false)?;
        self.backup_document(&doc, backup_dir)?;

        let content_range = doc.get_property_obj("Content")?;
        let find = content_range.get_property_obj("Find")?;
        let mut count = 0;

        for (old_text, new_text) in replacements {
            find.invoke_method("ClearFormatting", vec![])?;
            let replacement = find.get_property_obj("Replacement")?;
            replacement.invoke_method("ClearFormatting", vec![])?;
            
            find.set_property("Text", var_bstr(old_text))?;
            replacement.set_property("Text", var_bstr(new_text))?;
            find.set_property("Forward", var_bool(true))?;
            find.set_property("Wrap", var_i4(1))?; // wdFindContinue

            let success = find.invoke_method("Execute", vec![
                var_bstr(""), var_bool(false), var_bool(false), var_bool(false),
                var_bool(false), var_bool(false), var_bool(true), var_i4(1),
                var_bool(false), var_bstr(""),
                var_i4(2), // wdReplaceAll
            ]).ok().and_then(|v| bool::try_from(&v).ok()).unwrap_or(false);
            
            if success {
                count += 1;
            }
        }

        doc.invoke_method("Save", vec![])?;
        Self::close_document(&doc, false)?;

        Ok(format!("Replaced {} items while preserving format in '{}'", count, file_path))
    }

    #[cfg(not(windows))]
    pub fn replace_text_preserve_format(
        &self,
        _file_path: &str,
        _replacements: &std::collections::HashMap<String, String>,
        _backup_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Convert Markdown string to HTML and paste into Word to create a DOCX natively
    #[cfg(windows)]
    pub fn convert_md_to_docx(&self, md_content: &str, output_path: &str) -> anyhow::Result<String> {
        // Very basic MD to HTML converter for Native Word consumption.
        // Word perfectly parses <h1>, <p>, <ul>, <b>, <i> into Native Styles!
        let mut html = String::from("<html><body>");
        let mut in_list = false;
        
        for line in md_content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            let mut formatted = line.to_string();
            // Basic bold/italic replacements
            formatted = formatted.replace("**", "<b>").replace("__", "<b>");
            // Basic handling of closing <b> is tricky with simple replace, so we use a crude regex-like match
            let mut b_count = 0;
            while let Some(pos) = formatted.find("<b>") {
                if b_count % 2 != 0 {
                    formatted.replace_range(pos..pos+3, "</b>");
                }
                b_count += 1;
            }
            
            if line.starts_with("# ") {
                html.push_str(&format!("<h1>{}</h1>\n", &formatted[2..]));
            } else if line.starts_with("## ") {
                html.push_str(&format!("<h2>{}</h2>\n", &formatted[3..]));
            } else if line.starts_with("### ") {
                html.push_str(&format!("<h3>{}</h3>\n", &formatted[4..]));
            } else if line.starts_with("- ") || line.starts_with("* ") {
                if !in_list {
                    html.push_str("<ul>\n");
                    in_list = true;
                }
                html.push_str(&format!("<li>{}</li>\n", &formatted[2..]));
            } else {
                if in_list {
                    html.push_str("</ul>\n");
                    in_list = false;
                }
                html.push_str(&format!("<p>{}</p>\n", formatted));
            }
        }
        if in_list {
            html.push_str("</ul>\n");
        }
        html.push_str("</body></html>");

        // Write to temp file
        let temp_dir = std::env::temp_dir();
        let temp_html = temp_dir.join(format!("temp_md_{}.html", chrono::Utc::now().timestamp()));
        std::fs::write(&temp_html, html)?;

        // Open temp HTML in Word
        let docs = self.app.get_property_obj("Documents")?;
        let doc_var = docs.invoke_method("Open", vec![
            var_bstr(temp_html.to_string_lossy().as_ref()),
            var_bool(false),
            var_bool(false),
        ]).map_err(|e| anyhow::anyhow!("Failed to open HTML in Word: {}", e))?;
        
        let doc = ComObject::new(
            IDispatch::try_from(&doc_var)
                .map_err(|e| anyhow::anyhow!("Document not object: {}", e))?,
        );

        // SaveAs2 wdFormatXMLDocument = 12
        doc.invoke_method("SaveAs2", vec![var_bstr(output_path), var_i4(12)])?;
        Self::close_document(&doc, false)?;
        
        // Clean up temp file
        let _ = std::fs::remove_file(temp_html);

        Ok(format!("Successfully converted Markdown to DOCX at: {}", output_path))
    }

    #[cfg(not(windows))]
    pub fn convert_md_to_docx(&self, _md_content: &str, _output_path: &str) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    /// Insert an image at the current cursor position.
    #[cfg(windows)]
    pub fn add_picture(&self, image_path: &str, width: f32, height: f32) -> anyhow::Result<String> {
        let selection = self.app.get_property_obj("Selection")
            .map_err(|_| anyhow::anyhow!("No active Word document open"))?;

        let inline_shapes = selection.get_property_obj("InlineShapes")?;
        
        let shape_var = inline_shapes.invoke_method("AddPicture", vec![
            var_bstr(image_path),
            var_bool(false), // LinkToFile
            var_bool(true),  // SaveWithDocument
        ]).map_err(|e| anyhow::anyhow!("Failed to AddPicture: {}", e))?;
        
        let shape = ComObject::new(IDispatch::try_from(&shape_var)?);
        
        if width > 0.0 {
            let _ = shape.set_property("Width", var_r4(width));
        }
        if height > 0.0 {
            let _ = shape.set_property("Height", var_r4(height));
        }
        
        Ok(format!("Chèn ảnh thành công vào Word từ {}", image_path))
    }

    #[cfg(not(windows))]
    pub fn add_picture(&self, _image_path: &str, _width: f32, _height: f32) -> anyhow::Result<String> {
        Ok("Word COM Automation is only supported on Windows".to_string())
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    #[cfg(windows)]
    fn backup_document(&self, doc: &ComObject, backup_dir: Option<&str>) -> anyhow::Result<()> {
        let full_name = doc.get_property("FullName").ok()
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
            .unwrap_or_default();

        if full_name.is_empty() || (!full_name.contains('\\') && !full_name.contains('/')) {
            return Ok(()); // New unsaved doc
        }

        let path = std::path::Path::new(&full_name);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("docx");
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");

        let backup_path = if let Some(dir) = backup_dir {
            let _ = std::fs::create_dir_all(dir);
            format!("{}/{}_backup_{}.{}", dir, stem, ts, ext)
        } else {
            let parent = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
            format!("{}/{}_backup_{}.{}", parent, stem, ts, ext)
        };

        let _ = doc.invoke_method("SaveAs2", vec![var_bstr(&backup_path)]);
        tracing::info!("Backed up Word document → {}", backup_path);
        Ok(())
    }
}
