import re
import sys

file_path = "e:\\Office hub\\src-tauri\\src\\agents\\office_master\\mod.rs"

with open(file_path, "r", encoding="utf-8") as f:
    content = f.read()

# We need to find the start and end of the broken region
start_marker = "    fn encode_file_attachment(file_path: &str) -> Option<serde_json::Value> {"
end_marker = "            crate::mcp::McpTool {\n                name: \"word_extract_content\".to_string(),"

if start_marker not in content or end_marker not in content:
    print("Markers not found!")
    sys.exit(1)

start_idx = content.find(start_marker)
end_idx = content.find(end_marker)

if start_idx >= end_idx:
    print("Start index is after end index!")
    sys.exit(1)

correct_code = """    fn encode_file_attachment(file_path: &str) -> Option<serde_json::Value> {
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
        "Creates and edits Word documents and PowerPoint presentations \\
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
            "ppt_apply_brand_theme"
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
"""

new_content = content[:start_idx] + correct_code + content[end_idx:]

with open(file_path, "w", encoding="utf-8") as f:
    f.write(new_content)

print("Fixed!")
