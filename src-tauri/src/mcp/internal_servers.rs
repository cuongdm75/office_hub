use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use super::broker::InternalMcpServer;
use super::{McpTool, ToolCallResult, ToolContent};
use crate::orchestrator::memory::MemoryStore;
use tauri::AppHandle;

/// --- Policy MCP Server ---
pub struct PolicyServer {
    policy_dir: Option<PathBuf>,
}

impl PolicyServer {
    pub fn new(policy_dir: Option<PathBuf>) -> Self {
        Self { policy_dir }
    }
}

#[async_trait]
impl InternalMcpServer for PolicyServer {
    fn name(&self) -> &str {
        "policy_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "list_policies".to_string(),
                description: "Liá»‡t kÃª danh sÃ¡ch táº¥t cáº£ cÃ¡c file policy (quy táº¯c, quy Ä‘á»‹nh) hiá»‡n cÃ³ trong há»‡ thá»‘ng.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    }
                }),
                tags: vec![],
            },
            McpTool {
                name: "query_policy".to_string(),
                description: "Äá»c ná»™i dung cá»§a má»™t file policy cá»¥ thá»ƒ. Tráº£ vá» toÃ n bá»™ policy náº¿u khÃ´ng truyá»n filename.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "TÃªn file policy cáº§n Ä‘á»c (VD: 'security.md')" },
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    }
                }),
                tags: vec![],
            },
            McpTool {
                name: "write_policy".to_string(),
                description: "Táº¡o má»›i hoáº·c cáº\u{AD}p nháº\u{AD}t má»™t file policy (quy táº¯c, quy Ä‘á»‹nh) cá»§a há»‡ thá»‘ng.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "TÃªn file policy (VD: 'code_style.md')" },
                        "content": { "type": "string", "description": "Ná»™i dung markdown cá»§a policy" },
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    },
                    "required": ["filename", "content"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        let dir = match &self.policy_dir {
            Some(d) => {
                if let Some(wid) = arguments
                    .as_ref()
                    .and_then(|a| a.get("workspace_id"))
                    .and_then(|v| v.as_str())
                {
                    if wid != "default" {
                        let w_dir = d
                            .parent()
                            .unwrap_or(d)
                            .join("workspaces")
                            .join(wid)
                            .join("policies");
                        if !w_dir.exists() {
                            let _ = std::fs::create_dir_all(&w_dir);
                        }
                        w_dir
                    } else {
                        d.clone()
                    }
                } else {
                    d.clone()
                }
            }
            None => {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("Policy directory not configured.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                })
            }
        };

        if name == "list_policies" {
            let mut list = String::new();
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                        list.push_str(&format!(
                            "- {}\n",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                }
            }
            if list.is_empty() {
                list = "KhÃ´ng cÃ³ policy nÃ o Ä‘Æ°á»£c thiáº¿t láº\u{AD}p.".to_string();
            }
            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(list),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "query_policy" {
            let args = arguments.unwrap_or_default();
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("");

            let mut content = String::new();
            if filename.is_empty() {
                // Äá»c toÃ n bá»™ file md
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md")
                        {
                            if let Ok(text) = std::fs::read_to_string(&path) {
                                content.push_str(&format!(
                                    "=== {} ===\n{}\n\n",
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    text
                                ));
                            }
                        }
                    }
                }
                if content.is_empty() {
                    content = "KhÃ´ng cÃ³ policy nÃ o Ä‘Æ°á»£c thiáº¿t láº\u{AD}p.".to_string();
                }
            } else {
                let file_path = dir.join(filename);
                content = std::fs::read_to_string(&file_path)
                    .unwrap_or_else(|_| format!("KhÃ´ng tÃ¬m tháº¥y file policy: {}", filename));
            }

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(content),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "write_policy" {
            let args = arguments.unwrap_or_default();
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("");
            let content_str = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

            if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("TÃªn file khÃ´ng há»£p lá»‡.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let final_filename = if !filename.ends_with(".md") {
                format!("{}.md", filename)
            } else {
                filename.to_string()
            };

            let file_path = dir.join(&final_filename);
            match std::fs::write(&file_path, content_str) {
                Ok(_) => Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("ÄÃ£ lÆ°u policy thÃ nh cÃ´ng: {}", final_filename)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                }),
                Err(e) => Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Lá»—i khi lÆ°u policy: {}", e)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                }),
            }
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

/// --- Knowledge MCP Server ---
pub struct KnowledgeServer {
    knowledge_dir: Option<PathBuf>,
}

impl KnowledgeServer {
    pub fn new(knowledge_dir: Option<PathBuf>) -> Self {
        Self { knowledge_dir }
    }

    /// Scan all .md files (excluding index.md) and rebuild index.md automatically.
    /// This ensures index.md is always in sync with the actual files on disk.
    fn rebuild_index(dir: &PathBuf) {
        let mut files: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name != "index.md" {
                        files.push(name);
                    }
                }
            }
        }
        files.sort();

        let mut index = String::from("# Office Hub Knowledge Index\n\n");
        index.push_str("Danh sÃ¡ch tÃ i liá»‡u tri thá»©c ná»™i bá»™. DÃ¹ng `read_knowledge` Ä‘á»ƒ Ä‘á»c ná»™i dung chi tiáº¿t.\n\n");
        index.push_str("## TÃ i liá»‡u cÃ³ sáºµn\n\n");
        for (i, name) in files.iter().enumerate() {
            index.push_str(&format!("{}. `{}`\n", i + 1, name));
        }
        if files.is_empty() {
            index.push_str("_ChÆ°a cÃ³ tÃ i liá»‡u tri thá»©c nÃ o._\n");
        }

        let _ = std::fs::write(dir.join("index.md"), index);
    }
}

#[async_trait]
impl InternalMcpServer for KnowledgeServer {
    fn name(&self) -> &str {
        "knowledge_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "list_knowledge".to_string(),
                description:
                    "Liá»‡t kÃª danh sÃ¡ch cÃ¡c tÃ i liá»‡u tri thá»©c ná»™i bá»™ cÃ³ sáºµn."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    }
                }),
                tags: vec![],
            },
            McpTool {
                name: "read_knowledge".to_string(),
                description: "Äá»c ná»™i dung cá»§a má»™t tÃ i liá»‡u tri thá»©c cá»¥ thá»ƒ."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "TÃªn file tÃ i liá»‡u (VD: 'index.md')" },
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    },
                    "required": ["filename"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "write_knowledge".to_string(),
                description:
                    "Ghi má»›i hoáº·c cáº\u{AD}p nháº\u{AD}t ná»™i dung cá»§a má»™t tÃ i liá»‡u tri thá»©c."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "TÃªn file tÃ i liá»‡u (VD: 'new_rule.md')" },
                        "content": { "type": "string", "description": "Ná»™i dung markdown cáº§n lÆ°u" },
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    },
                    "required": ["filename", "content"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "delete_knowledge".to_string(),
                description: "XÃ³a má»™t tÃ i liá»‡u tri thá»©c cá»¥ thá»ƒ.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filename": { "type": "string", "description": "TÃªn file tÃ i liá»‡u cáº§n xÃ³a" },
                        "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                    },
                    "required": ["filename"]
                }),
                tags: vec![],
            },
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        let dir = match &self.knowledge_dir {
            Some(d) => {
                if let Some(wid) = arguments
                    .as_ref()
                    .and_then(|a| a.get("workspace_id"))
                    .and_then(|v| v.as_str())
                {
                    if wid != "default" {
                        let w_dir = d
                            .parent()
                            .unwrap_or(d)
                            .join("workspaces")
                            .join(wid)
                            .join("knowledge");
                        if !w_dir.exists() {
                            let _ = std::fs::create_dir_all(&w_dir);
                        }
                        w_dir
                    } else {
                        d.clone()
                    }
                } else {
                    d.clone()
                }
            }
            None => {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("Knowledge directory not configured.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                })
            }
        };

        if name == "list_knowledge" {
            // Scan filesystem directly â€” always reflects actual files on disk,
            // never stale even if index.md is out of date.
            let mut files: Vec<String> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        if fname != "index.md" {
                            files.push(fname);
                        }
                    }
                }
            }
            files.sort();

            let listing = if files.is_empty() {
                "ChÆ°a cÃ³ tÃ i liá»‡u tri thá»©c nÃ o.".to_string()
            } else {
                let mut out = "# Danh sÃ¡ch tÃ i liá»‡u tri thá»©c\n\n".to_string();
                for (i, name) in files.iter().enumerate() {
                    out.push_str(&format!("{}. `{}`\n", i + 1, name));
                }
                out.push_str(
                    "\nDÃ¹ng `read_knowledge` vá»›i tÃªn file Ä‘á»ƒ Ä‘á»c ná»™i dung chi tiáº¿t.",
                );
                out
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(listing),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "read_knowledge" {
            let args = arguments.unwrap_or_default();
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("");
            let file_path = dir.join(filename);

            let content = std::fs::read_to_string(&file_path)
                .unwrap_or_else(|_| format!("Lá»—i khi Ä‘á»c file {}", filename));

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(content),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "write_knowledge" {
            let args = arguments.unwrap_or_default();
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("");
            let content_str = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

            if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(
                            "TÃªn file khÃ´ng há»£p lá»‡ (KhÃ´ng Ä‘Æ°á»£c chá»©a thÆ° má»¥c con)."
                                .to_string(),
                        ),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let final_filename = if !filename.ends_with(".md") {
                format!("{}.md", filename)
            } else {
                filename.to_string()
            };

            // Prevent overwriting index.md directly via this tool
            if final_filename == "index.md" {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("KhÃ´ng thá»ƒ ghi trá»±c tiáº¿p vÃ o index.md. Index Ä‘Æ°á»£c tá»± Ä‘á»™ng cáº\u{AD}p nháº\u{AD}t khi thÃªm/xÃ³a file.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let file_path = dir.join(&final_filename);
            match std::fs::write(&file_path, content_str) {
                Ok(_) => {
                    // Auto-rebuild index.md to include the new file
                    Self::rebuild_index(&dir);
                    Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!(
                                "ÄÃ£ lÆ°u '{}' vÃ  cáº­p nháº­t index thÃ nh cÃ´ng.",
                                final_filename
                            )),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: false,
                    })
                }
                Err(e) => Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Lá»—i khi lÆ°u file: {}", e)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                }),
            }
        } else if name == "delete_knowledge" {
            let args = arguments.unwrap_or_default();
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("");

            if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("TÃªn file khÃ´ng há»£p lá»‡.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let file_path = dir.join(filename);
            if !file_path.exists() {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("File khÃ´ng tá»“n táº¡i.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            match std::fs::remove_file(&file_path) {
                Ok(_) => {
                    // Auto-rebuild index.md to remove the deleted entry
                    Self::rebuild_index(&dir);
                    Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!(
                                "ÄÃ£ xÃ³a '{}' vÃ  cáº­p nháº­t index thÃ nh cÃ´ng.",
                                filename
                            )),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: false,
                    })
                }
                Err(e) => Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Lá»—i khi xÃ³a file: {}", e)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                }),
            }
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

/// --- Memory MCP Server ---
pub struct MemoryServer {
    memory_store: Option<Arc<MemoryStore>>,
}

impl MemoryServer {
    pub fn new(memory_store: Option<Arc<MemoryStore>>) -> Self {
        Self { memory_store }
    }
}

#[async_trait]
impl InternalMcpServer for MemoryServer {
    fn name(&self) -> &str {
        "memory_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![McpTool {
            name: "search_memory".to_string(),
            description: "TÃ¬m kiáº¿m cÃ¡c há»™i thoáº¡i trong quÃ¡ khá»© liÃªn quan Ä‘áº¿n truy váº¥n hiá»‡n táº¡i.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Tá»« khÃ³a tÃ¬m kiáº¿m" },
                    "workspace_id": { "type": "string", "description": "ID cá»§a workspace hiá»‡n táº¡i" }
                },
                "required": ["query"]
            }),
                tags: vec![],
        }])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        if name == "search_memory" {
            let args = arguments.unwrap_or_default();
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let workspace_id = args.get("workspace_id").and_then(|v| v.as_str());

            let content = if let Some(mem) = &self.memory_store {
                match mem.search(workspace_id, query, 3) {
                    Ok(results) => {
                        if results.is_empty() {
                            "KhÃ´ng tÃ¬m tháº¥y thÃ´ng tin phÃ¹ há»£p trong bá»™ nhá»›.".to_string()
                        } else {
                            results.join("\n")
                        }
                    }
                    Err(e) => format!("Lá»—i khi truy váº¥n bá»™ nhá»›: {}", e),
                }
            } else {
                "Memory store not initialized.".to_string()
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(content),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

/// --- Skill MCP Server (Declarative Prompt Tools) ---
pub struct SkillServer {
    skills_dir: Option<PathBuf>,
}

impl SkillServer {
    pub fn new(skills_dir: Option<PathBuf>) -> Self {
        Self { skills_dir }
    }

    fn read_skills(&self) -> Vec<(McpTool, String)> {
        let mut tools = Vec::new();
        let dir = match &self.skills_dir {
            Some(d) => d,
            None => return tools,
        };

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Cáº¥u trÃºc Anthropic Agent Skills chuáº©n lÃ  .agent/skills/<skill_name>/SKILL.md
                    let md_path = path.join("SKILL.md");
                    if md_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&md_path) {
                            if let Some((tool, body)) = Self::parse_skill_file(&content) {
                                tools.push((tool, body));
                            }
                        }
                    }
                } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md")
                {
                    // Hoáº·c file .md trá»±c tiáº¿p trong skills_dir
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Some((tool, body)) = Self::parse_skill_file(&content) {
                            tools.push((tool, body));
                        }
                    }
                }
            }
        }
        tools
    }

    fn parse_skill_file(content: &str) -> Option<(McpTool, String)> {
        if !content.starts_with("---") {
            return None;
        }
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return None;
        }

        let yaml_str = parts[1];
        let body = parts[2].trim().to_string();

        #[derive(serde::Deserialize)]
        struct SkillMeta {
            name: String,
            description: String,
            #[serde(default)]
            tags: Option<Vec<String>>,
            #[serde(default)]
            parameters: serde_json::Value,
        }

        if let Ok(meta) = serde_yaml::from_str::<SkillMeta>(yaml_str) {
            let input_schema = if meta.parameters.is_object() {
                serde_json::json!({
                    "type": "object",
                    "properties": meta.parameters,
                })
            } else {
                serde_json::json!({ "type": "object", "properties": {} })
            };

            let tool = McpTool {
                name: meta.name,
                description: meta.description,
                input_schema,
                tags: meta.tags.unwrap_or_default(),
            };
            Some((tool, body))
        } else {
            None
        }
    }
}

#[async_trait]
impl InternalMcpServer for SkillServer {
    fn name(&self) -> &str {
        "skill_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let mut tools: Vec<McpTool> = self
            .read_skills()
            .into_iter()
            .map(|(tool, _)| tool)
            .collect();
        tools.push(McpTool {
            name: "write_skill".to_string(),
            description: "Táº¡o má»›i hoáº·c cáº\u{AD}p nháº\u{AD}t má»™t ká»¹ nÄƒng (skill) cho Agent.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_name": { "type": "string", "description": "TÃªn skill (VD: 'send-email')" },
                    "description": { "type": "string", "description": "MÃ´ táº£ ngáº¯n gá»n chá»©c nÄƒng cá»§a skill" },
                    "parameters": { "type": "object", "description": "Schema JSON Ä‘á»‹nh nghÄ©a cÃ¡c tham sá»‘ Ä‘áº§u vÃ o (nhÆ° JSON Schema properties)" },
                    "instructions": { "type": "string", "description": "Ná»™i dung Markdown hÆ°á»›ng dáº«n LLM cÃ¡ch thá»±c hiá»‡n skill nÃ y" }
                },
                "required": ["skill_name", "description", "instructions"]
            }),
            tags: vec![],
        });
        tools.push(McpTool {
            name: "delete_skill".to_string(),
            description: "XÃ³a má»™t ká»¹ nÄƒng (skill) khá»i há»‡ thá»‘ng.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_name": { "type": "string", "description": "TÃªn skill cáº§n xÃ³a" }
                },
                "required": ["skill_name"]
            }),
            tags: vec![],
        });
        Ok(tools)
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        if name == "write_skill" {
            let args = arguments.unwrap_or_default();
            let skill_name = args
                .get("skill_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let description = args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let parameters = args.get("parameters").unwrap_or(&Value::Null);
            let instructions = args
                .get("instructions")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if skill_name.is_empty()
                || skill_name.contains("..")
                || skill_name.contains("/")
                || skill_name.contains("\\")
            {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("TÃªn skill khÃ´ng há»£p lá»‡.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let dir = match &self.skills_dir {
                Some(d) => d,
                None => {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some("Skill directory not configured.".to_string()),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    })
                }
            };

            let skill_folder = dir.join(skill_name);
            if !skill_folder.exists() {
                if let Err(e) = std::fs::create_dir_all(&skill_folder) {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!("Lá»—i táº¡o thÆ° má»¥c skill: {}", e)),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    });
                }
            }

            let params_yaml = if parameters.is_object() {
                serde_yaml::to_string(parameters).unwrap_or_else(|_| "{}".to_string())
            } else {
                "".to_string()
            };

            let yaml_frontmatter = format!(
                "---\nname: {}\ndescription: {}\nparameters:\n{}\n---\n\n{}",
                skill_name,
                description,
                if params_yaml.is_empty() {
                    "  {}".to_string()
                } else {
                    params_yaml
                        .lines()
                        .map(|l| format!("  {}", l))
                        .collect::<Vec<_>>()
                        .join("\n")
                },
                instructions
            );

            let skill_file = skill_folder.join("SKILL.md");
            match std::fs::write(&skill_file, yaml_frontmatter) {
                Ok(_) => Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!(
                            "ÄÃ£ táº¡o/cáº­p nháº­t skill thÃ nh cÃ´ng: {}",
                            skill_name
                        )),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                }),
                Err(e) => Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Lá»—i khi lÆ°u skill: {}", e)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                }),
            }
        } else if name == "delete_skill" {
            let args = arguments.unwrap_or_default();
            let skill_name = args
                .get("skill_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if skill_name.is_empty()
                || skill_name.contains("..")
                || skill_name.contains("/")
                || skill_name.contains("\\")
            {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("TÃªn skill khÃ´ng há»£p lá»‡.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let dir = match &self.skills_dir {
                Some(d) => d,
                None => {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some("Skill directory not configured.".to_string()),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    })
                }
            };

            let skill_folder = dir.join(skill_name);
            let skill_file = dir.join(format!("{}.md", skill_name));

            let mut deleted = false;
            if skill_folder.exists() {
                let _ = std::fs::remove_dir_all(&skill_folder);
                deleted = true;
            }
            if skill_file.exists() {
                let _ = std::fs::remove_file(&skill_file);
                deleted = true;
            }

            if deleted {
                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("ÄÃ£ xÃ³a skill: {}", skill_name)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            } else {
                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("Skill khÃ´ng tá»“n táº¡i.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                })
            }
        } else {
            let skills = self.read_skills();
            if let Some((_, body)) = skills.into_iter().find(|(tool, _)| tool.name == name) {
                let mut resolved_body = body;

                // Interpolate arguments into the markdown body
                if let Some(Value::Object(args)) = arguments {
                    for (k, v) in args {
                        let val_str = match v {
                            Value::String(s) => s.to_string(),
                            _ => v.to_string(),
                        };
                        resolved_body = resolved_body.replace(&format!("{{{}}}", k), &val_str);
                    }
                }

                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(resolved_body),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            } else {
                Err(anyhow!("Skill not found"))
            }
        }
    }
}

/// --- FileSystem MCP Server ---
pub struct FileSystemServer;

impl Default for FileSystemServer {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystemServer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl InternalMcpServer for FileSystemServer {
    fn name(&self) -> &str {
        "local_fs"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "read_file".to_string(),
                description: "Äá»c ná»™i dung cá»§a má»™t file cá»¥c bá»™ trÃªn mÃ¡y tÃ\u{AD}nh.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "ÄÆ°á»ng dáº«n tuyá»‡t Ä‘á»‘i tá»›i file cáº§n Ä‘á»c" }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "list_directory".to_string(),
                description: "Liá»‡t kÃª cÃ¡c file vÃ  thÆ° má»¥c con trong má»™t Ä‘Æ°á»ng dáº«n.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "ÄÆ°á»ng dáº«n tuyá»‡t Ä‘á»‘i cá»§a thÆ° má»¥c" }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "read_folder_files".to_string(),
                description: "Äá»c vÃ  tá»•ng há»£p ná»™i dung Táº¤T Cáº¢ cÃ¡c file vÄƒn báº£n (text, md, txt, csv) trong má»™t thÆ° má»¥c cá»¥ thá»ƒ. Ráº¥t há»¯u Ã\u{AD}ch khi cáº§n tá»•ng há»£p sá»‘ liá»‡u hoáº·c lÃ m bÃ¡o cÃ¡o tá»« nhiá»u file.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "ÄÆ°á»ng dáº«n tuyá»‡t Ä‘á»‘i cá»§a thÆ° má»¥c" },
                        "max_files": { "type": "number", "description": "Sá»‘ lÆ°á»£ng file tá»‘i Ä‘a Ä‘á»ƒ Ä‘á»c (máº·c Ä‘á»‹nh 20)" }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "fs_move_file".to_string(),
                description: "Di chuyá»ƒn hoáº·c Ä‘á»•i tÃªn file cá»¥c bá»™.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string" },
                        "destination": { "type": "string" }
                    },
                    "required": ["source", "destination"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "fs_read_excel".to_string(),
                description: "Äá»c dá»¯ liá»‡u tá»« file Excel (.xlsx, .xlsb, .ods) vÃ  tráº£ vá» máº£ng JSON.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "ÄÆ°á»ng dáº«n file Excel" },
                        "sheet": { "type": "string", "description": "TÃªn Sheet cáº§n Ä‘á»c. Bá» trá»‘ng Ä‘á»ƒ Ä‘á»c Sheet Ä‘áº§u tiÃªn." }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "fs_read_pdf".to_string(),
                description: "TrÃ\u{AD}ch xuáº¥t vÄƒn báº£n tá»« file PDF.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "ÄÆ°á»ng dáº«n file PDF" }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        let args = arguments.unwrap_or_default();

        if name == "read_file" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|e| format!("Lá»—i khi Ä‘á»c file {}: {}", path, e));

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(content),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "list_directory" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let mut list = String::new();
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let prefix = if is_dir { "[DIR] " } else { "[FILE]" };
                    list.push_str(&format!("{} {}\n", prefix, file_name));
                }
            } else {
                list = format!("KhÃ´ng thá»ƒ Ä‘á»c thÆ° má»¥c: {}", path);
            }
            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(if list.is_empty() {
                        "ThÆ° má»¥c trá»‘ng.".to_string()
                    } else {
                        list
                    }),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "read_folder_files" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let max_files = args.get("max_files").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

            let mut aggregated_content = String::new();
            let mut count = 0;

            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if count >= max_files {
                        aggregated_content.push_str("\n\n[...ÄÃ£ Ä‘áº¡t giá»›i háº¡n sá»‘ lÆ°á»£ng file...] limit reached.");
                        break;
                    }

                    let p = entry.path();
                    if p.is_file() {
                        let ext = p
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        // Only read text-based files for safety and context limits
                        if ["txt", "md", "csv", "json", "xml", "log"].contains(&ext.as_str()) {
                            if let Ok(content) = std::fs::read_to_string(&p) {
                                let file_name = p.file_name().unwrap_or_default().to_string_lossy();
                                aggregated_content.push_str(&format!("=== Báº¯t Ä‘áº§u file: {} ===\n{}\n=== Káº¿t thÃºc file: {} ===\n\n", file_name, content, file_name));
                                count += 1;
                            }
                        }
                    }
                }
            } else {
                aggregated_content = format!("KhÃ´ng thá»ƒ Ä‘á»c thÆ° má»¥c: {}", path);
            }

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(if aggregated_content.is_empty() {
                        "KhÃ´ng tÃ¬m tháº¥y file vÄƒn báº£n há»£p lá»‡ nÃ o trong thÆ° má»¥c."
                            .to_string()
                    } else {
                        aggregated_content
                    }),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "fs_move_file" {
            let src = args.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let dst = args
                .get("destination")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let output = match std::fs::rename(src, dst) {
                Ok(_) => format!(
                    "ThÃ nh cÃ´ng di chuyá»ƒn/Ä‘á»•i tÃªn tá»« {} sang {}",
                    src, dst
                ),
                Err(e) => format!("Lá»—i khi di chuyá»ƒn file: {}", e),
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(output.clone()),
                    data: None,
                    mime_type: None,
                }],
                is_error: output.starts_with("Lá»—i"),
            })
        } else if name == "fs_read_excel" {
            use calamine::Reader;
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let sheet = args.get("sheet").and_then(|v| v.as_str());

            let output = match calamine::open_workbook_auto(path) {
                Ok(mut workbook) => {
                    let sheet_name = match sheet {
                        Some(s) if !s.is_empty() => s.to_string(),
                        _ => workbook.sheet_names().first().cloned().unwrap_or_default(),
                    };

                    if sheet_name.is_empty() {
                        "Lá»—i: KhÃ´ng tÃ¬m tháº¥y sheet nÃ o trong file.".to_string()
                    } else {
                        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                            let mut data = Vec::new();
                            // Limit to 1000 rows to avoid blowing up memory/tokens
                            for row in range.rows().take(1000) {
                                let mut row_data = Vec::new();
                                for cell in row {
                                    let cell_val = match cell {
                                        calamine::Data::String(s) => s.to_string(),
                                        calamine::Data::Float(f) => f.to_string(),
                                        calamine::Data::Int(i) => i.to_string(),
                                        calamine::Data::Bool(b) => b.to_string(),
                                        calamine::Data::DateTime(d) => d.as_f64().to_string(),
                                        calamine::Data::Empty => "".to_string(),
                                        _ => "".to_string(),
                                    };
                                    row_data.push(cell_val);
                                }
                                data.push(row_data);
                            }
                            serde_json::to_string_pretty(&data)
                                .unwrap_or_else(|_| "Lá»—i parse json".to_string())
                        } else {
                            format!("Lá»—i: KhÃ´ng thá»ƒ Ä‘á»c sheet {}", sheet_name)
                        }
                    }
                }
                Err(e) => format!("Lá»—i khi má»Ÿ file Excel: {}", e),
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(output.clone()),
                    data: None,
                    mime_type: None,
                }],
                is_error: output.starts_with("Lá»—i"),
            })
        } else if name == "fs_read_pdf" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

            let output = match pdf_extract::extract_text(path) {
                Ok(mut text) => {
                    if text.len() > 20000 {
                        text.truncate(20000);
                        text.push_str("\n\n[...ÄÃ£ cáº¯t bá»›t do vÄƒn báº£n quÃ¡ dÃ i...]");
                    }
                    if text.trim().is_empty() {
                        "Lá»—i: KhÃ´ng tÃ¬m tháº¥y vÄƒn báº£n nÃ o trong PDF (cÃ³ thá»ƒ lÃ  file scan áº£nh).".to_string()
                    } else {
                        text
                    }
                }
                Err(e) => format!("Lá»—i khi Ä‘á»c file PDF: {}", e),
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(output.clone()),
                    data: None,
                    mime_type: None,
                }],
                is_error: output.starts_with("Lá»—i"),
            })
        } else {
            Err(anyhow::anyhow!("Tool not found: {}", name))
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Rhai Scripting Server
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub struct ScriptingServer {
    app_handle: Option<AppHandle>,
    skills_dir: Option<PathBuf>,
}

impl ScriptingServer {
    pub fn new(app_handle: Option<AppHandle>, skills_dir: Option<PathBuf>) -> Self {
        Self {
            app_handle,
            skills_dir,
        }
    }

    fn get_skill_permissions(&self, skill_name: &str) -> Vec<String> {
        let mut perms = Vec::new();
        if skill_name.is_empty()
            || skill_name.contains("..")
            || skill_name.contains("/")
            || skill_name.contains("\\")
        {
            return perms;
        }

        if let Some(dir) = &self.skills_dir {
            let md_path_dir = dir.join(skill_name).join("SKILL.md");
            let md_path_file = dir.join(format!("{}.md", skill_name));

            let path = if md_path_dir.exists() {
                md_path_dir
            } else if md_path_file.exists() {
                md_path_file
            } else {
                return perms;
            };

            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.starts_with("---") {
                    let parts: Vec<&str> = content.splitn(3, "---").collect();
                    if parts.len() >= 3 {
                        #[derive(serde::Deserialize)]
                        struct SkillMeta {
                            #[serde(default)]
                            permissions: Vec<String>,
                        }
                        if let Ok(meta) = serde_yaml::from_str::<SkillMeta>(parts[1]) {
                            perms = meta.permissions;
                        }
                    }
                }
            }
        }
        perms
    }
}

#[async_trait]
impl InternalMcpServer for ScriptingServer {
    fn name(&self) -> &str {
        "scripting_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "run_rhai_script".to_string(),
                description: "Thá»±c thi mÃ£ ká»‹ch báº£n Rhai (Rust-native scripting). DÃ¹ng Ä‘á»ƒ láº\u{AD}p trÃ¬nh logic, xá»\u{AD} lÃ½ dá»¯ liá»‡u hoáº·c gá»i cÃ¡c lá»‡nh há»‡ thá»‘ng.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "script": { "type": "string", "description": "MÃ£ nguá»“n Rhai cáº§n thá»±c thi." },
                        "skill_name": { "type": "string", "description": "TÃªn skill Ä‘ang thá»±c thi (Ä‘á»ƒ cáº¥p quyá»n). Bá» trá»‘ng náº¿u lÃ  script tá»± do (sandbox)." }
                    },
                    "required": ["script"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        if name == "run_rhai_script" {
            let args = arguments.unwrap_or_default();
            let skill_name = args
                .get("skill_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let script = args.get("script").and_then(|v| v.as_str()).unwrap_or("");
            let permissions = self.get_skill_permissions(skill_name);
            let is_sandbox = skill_name.is_empty();

            if script.trim().is_empty() {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("Script rá»—ng.".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let mut engine = rhai::Engine::new();

            // ÄÄƒng kÃ½ API Bridge: log() (LuÃ´n kháº£ dá»¥ng)
            engine.register_fn("log", |msg: &str| {
                tracing::info!("[Rhai Log]: {}", msg);
            });

            // TiÃªm quyá»n Shell
            if permissions.contains(&"shell".to_string()) || is_sandbox {
                // Táº¡m thá»i cho phÃ©p sandbox Ä‘á»ƒ backward compatible, hoáº·c khÃ³a tÃ¹y cáº¥u hÃ¬nh
                engine.register_fn("cmd", |command: &str| -> String {
                    let os = std::env::consts::OS;
                    let mut cmd_obj = if os == "windows" {
                        let mut c = std::process::Command::new("powershell");
                        c.arg("-NoProfile")
                            .arg("-NonInteractive")
                            .arg("-Command")
                            .arg(command);
                        c
                    } else {
                        let mut c = std::process::Command::new("sh");
                        c.arg("-c").arg(command);
                        c
                    };

                    match cmd_obj.output() {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                            if out.status.success() {
                                stdout
                            } else {
                                format!("Lá»–I:\n{}\n{}", stderr, stdout)
                            }
                        }
                        Err(e) => format!("Lá»—i thá»±c thi lá»‡nh: {}", e),
                    }
                });
            } else {
                engine.register_fn("cmd", |_: &str| -> String {
                    "Lá»—i: Skill khÃ´ng cÃ³ quyá»n 'shell'".to_string()
                });
            }

            // TiÃªm quyá»n Network
            if permissions.contains(&"network".to_string()) || is_sandbox {
                engine.register_fn("http_get", |url: &str| -> String {
                    match reqwest::blocking::get(url) {
                        Ok(resp) => resp
                            .text()
                            .unwrap_or_else(|e| format!("Lá»—i Ä‘á»c ná»™i dung: {}", e)),
                        Err(e) => format!("Lá»—i táº£i URL: {}", e),
                    }
                });
            } else {
                engine.register_fn("http_get", |_: &str| -> String {
                    "Lá»—i: Skill khÃ´ng cÃ³ quyá»n 'network'".to_string()
                });
            }

            // TiÃªm quyá»n FS
            if permissions.contains(&"fs_read".to_string()) || is_sandbox {
                engine.register_fn("read_file", |path: &str| -> String {
                    std::fs::read_to_string(path)
                        .unwrap_or_else(|e| format!("Lá»—i Ä‘á»c file: {}", e))
                });
            } else {
                engine.register_fn("read_file", |_: &str| -> String {
                    "Lá»—i: Skill khÃ´ng cÃ³ quyá»n 'fs_read'".to_string()
                });
            }

            if permissions.contains(&"fs_write".to_string()) || is_sandbox {
                engine.register_fn("write_file", |path: &str, content: &str| -> bool {
                    std::fs::write(path, content).is_ok()
                });
            } else {
                engine.register_fn("write_file", |_: &str, _: &str| -> bool { false });
            }

            // TiÃªm quyá»n DB
            if permissions.contains(&"db".to_string()) || is_sandbox {
                engine.register_fn("db_execute", |sql: &str| -> String {
                    if sql.to_uppercase().contains("DROP") {
                        return "Lá»—i báº£o máº\u{AD}t: KhÃ´ng Ä‘Æ°á»£c phÃ©p dÃ¹ng lá»‡nh DROP"
                            .to_string();
                    }
                    let db_path = std::env::temp_dir().join("office_hub_agent_state.db"); // Dedicated DB
                    match rusqlite::Connection::open(&db_path) {
                        Ok(conn) => match conn.execute(sql, []) {
                            Ok(rows) => format!("ThÃ nh cÃ´ng: {} dÃ²ng bá»‹ áº£nh hÆ°á»Ÿng", rows),
                            Err(e) => format!("Lá»—i SQL: {}", e),
                        },
                        Err(e) => format!("Lá»—i káº¿t ná»‘i DB: {}", e),
                    }
                });
            } else {
                engine.register_fn("db_execute", |_: &str| -> String {
                    "Lá»—i: Skill khÃ´ng cÃ³ quyá»n 'db'".to_string()
                });
            }

            // TiÃªm quyá»n System (Tauri API)
            let app_handle_clone = self.app_handle.clone();
            if permissions.contains(&"system".to_string()) || is_sandbox {
                let app_handle_for_notify = app_handle_clone.clone();
                engine.register_fn("notify", move |title: String, body: String| {
                    if let Some(app) = &app_handle_for_notify {
                        use tauri_plugin_notification::NotificationExt;
                        let _ = app.notification().builder().title(title).body(body).show();
                    }
                });

                let app_handle_for_reload = app_handle_clone.clone();
                engine.register_fn("reload_system", move || {
                    if let Some(app) = &app_handle_for_reload {
                        tracing::info!("System reload requested by Rhai skill");
                        app.restart();
                    }
                });
            } else {
                engine.register_fn("notify", |_: String, _: String| {});
                engine.register_fn("reload_system", || {});
            }

            let result: Result<rhai::Dynamic, Box<rhai::EvalAltResult>> =
                engine.eval::<rhai::Dynamic>(script);

            let output_str = match result {
                Ok(res) => {
                    if res.is_unit() {
                        "Thá»±c thi thÃ nh cÃ´ng (KhÃ´ng cÃ³ giÃ¡ trá»‹ tráº£ vá»).".to_string()
                    } else {
                        res.to_string()
                    }
                }
                Err(e) => format!("Lá»—i thá»±c thi Rhai script: {}", e),
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(output_str),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

pub struct Win32AdminServer {}

impl Default for Win32AdminServer {
    fn default() -> Self {
        Self::new()
    }
}

impl Win32AdminServer {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl InternalMcpServer for Win32AdminServer {
    fn name(&self) -> &str {
        "win32_admin_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "win32_file_create_dir".to_string(),
                description: "Táº¡o má»™t thÆ° má»¥c má»›i táº¡i Ä‘Æ°á»ng dáº«n chá»‰ Ä‘á»‹nh."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_file_move".to_string(),
                description: "Di chuyá»ƒn hoáº·c Ä‘á»•i tÃªn file/thÆ° má»¥c.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "from": { "type": "string" },
                        "to": { "type": "string" }
                    },
                    "required": ["from", "to"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_file_delete".to_string(),
                description: "XÃ³a file hoáº·c thÆ° má»¥c.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_registry_read".to_string(),
                description:
                    "Äá»c giÃ¡ trá»‹ tá»« Windows Registry (vÃ\u{AD} dá»¥: HKLM\\Software\\...)."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "hive": { "type": "string", "description": "HKLM, HKCU, HKCR, HKU, HKCC" },
                        "key": { "type": "string" },
                        "value_name": { "type": "string" }
                    },
                    "required": ["hive", "key", "value_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_registry_write".to_string(),
                description: "Ghi giÃ¡ trá»‹ vÃ o Windows Registry.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "hive": { "type": "string", "description": "HKLM, HKCU, HKCR, HKU, HKCC" },
                        "key": { "type": "string" },
                        "value_name": { "type": "string" },
                        "value_data": { "type": "string" }
                    },
                    "required": ["hive", "key", "value_name", "value_data"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_process_list".to_string(),
                description: "Liet ke cac tien trinh dang chay.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_process_kill".to_string(),
                description: "Cháº¥m dá»©t má»™t tiáº¿n trÃ¬nh theo tÃªn hoáº·c PID.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "process_name": { "type": "string" },
                        "pid": { "type": "number" }
                    }
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_winget_search".to_string(),
                description: "TÃ¬m kiáº¿m pháº§n má»m báº±ng winget.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_winget_install".to_string(),
                description: "CÃ i Ä‘áº·t pháº§n má»m báº±ng winget.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "package_id": { "type": "string" }
                    },
                    "required": ["package_id"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_winget_uninstall".to_string(),
                description: "Gá»¡ cÃ i Ä‘áº·t pháº§n má»m báº±ng winget.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "package_id": { "type": "string" }
                    },
                    "required": ["package_id"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_shell_execute".to_string(),
                description: "Thá»±c thi lá»‡nh PowerShell.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    },
                    "required": ["command"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_inspect".to_string(),
                description: "Kiá»ƒm tra cáº¥u trÃºc UI cá»§a má»™t cá»\u{AD}a sá»•.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" }
                    },
                    "required": ["window_title"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_click".to_string(),
                description:
                    "Click vÃ o má»™t pháº§n tá»\u{AD} trÃªn cá»\u{AD}a sá»• báº±ng AutomationId hoáº·c Name."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" },
                        "element_id_or_name": { "type": "string" }
                    },
                    "required": ["window_title", "element_id_or_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_enter_text".to_string(),
                description: "Nháº\u{AD}p vÄƒn báº£n vÃ o Ã´ nháº\u{AD}p liá»‡u.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" },
                        "element_id_or_name": { "type": "string" },
                        "text": { "type": "string" }
                    },
                    "required": ["window_title", "element_id_or_name", "text"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_get_value".to_string(),
                description: "Láº¥y giÃ¡ trá»‹ (Text) cá»§a má»™t pháº§n tá»\u{AD} trÃªn cá»\u{AD}a sá»•."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" },
                        "element_id_or_name": { "type": "string" }
                    },
                    "required": ["window_title", "element_id_or_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_toggle".to_string(),
                description: "TÃ\u{AD}ch/Bá» tÃ\u{AD}ch (Toggle) má»™t pháº§n tá»\u{AD} (Checkbox, Switch)."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" },
                        "element_id_or_name": { "type": "string" }
                    },
                    "required": ["window_title", "element_id_or_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_select".to_string(),
                description: "Chá»n má»™t má»¥c trong danh sÃ¡ch (Combobox/Dropdown/List)."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" },
                        "element_id_or_name": { "type": "string" }
                    },
                    "required": ["window_title", "element_id_or_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "win32_uia_set_focus".to_string(),
                description: "ÄÆ°a con trá» chuá»™t/bÃ n phÃ\u{AD}m (Focus) vÃ o má»™t pháº§n tá»\u{AD}."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "window_title": { "type": "string" },
                        "element_id_or_name": { "type": "string" }
                    },
                    "required": ["window_title", "element_id_or_name"]
                }),
                tags: vec![],
            },
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        let args = arguments.unwrap_or_default();
        let mut is_error = false;
        let mut output = String::new();

        match name {
            "win32_file_create_dir" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                match std::fs::create_dir_all(path) {
                    Ok(_) => output = format!("ThÃ nh cÃ´ng táº¡o thÆ° má»¥c: {}", path),
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i táº¡o thÆ° má»¥c: {}", e);
                    }
                }
            }
            "win32_file_move" => {
                let from = args.get("from").and_then(|v| v.as_str()).unwrap_or("");
                let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("");
                match std::fs::rename(from, to) {
                    Ok(_) => output = format!("ThÃ nh cÃ´ng di chuyá»ƒn tá»« {} sang {}", from, to),
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i di chuyá»ƒn: {}", e);
                    }
                }
            }
            "win32_file_delete" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let meta = std::fs::metadata(path);
                if let Ok(m) = meta {
                    let res = if m.is_dir() {
                        std::fs::remove_dir_all(path)
                    } else {
                        std::fs::remove_file(path)
                    };
                    match res {
                        Ok(_) => output = format!("ThÃ nh cÃ´ng xÃ³a: {}", path),
                        Err(e) => {
                            is_error = true;
                            output = format!("Lá»—i xÃ³a: {}", e);
                        }
                    }
                } else {
                    is_error = true;
                    output = format!("ÄÆ°á»ng dáº«n khÃ´ng tá»“n táº¡i: {}", path);
                }
            }
            "win32_registry_read" => {
                let hive_str = args.get("hive").and_then(|v| v.as_str()).unwrap_or("");
                let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let val_name = args
                    .get("value_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                use winreg::enums::*;
                use winreg::RegKey;

                let hive = match hive_str {
                    "HKLM" => HKEY_LOCAL_MACHINE,
                    "HKCU" => HKEY_CURRENT_USER,
                    "HKCR" => HKEY_CLASSES_ROOT,
                    "HKU" => HKEY_USERS,
                    "HKCC" => HKEY_CURRENT_CONFIG,
                    _ => HKEY_LOCAL_MACHINE, // Default
                };

                let hk = RegKey::predef(hive);
                match hk.open_subkey(key) {
                    Ok(subkey) => match subkey.get_value::<String, _>(val_name) {
                        Ok(val) => output = format!("GiÃ¡ trá»‹ cá»§a {}: {}", val_name, val),
                        Err(e) => {
                            is_error = true;
                            output = format!("Lá»—i Ä‘á»c giÃ¡ trá»‹: {}", e);
                        }
                    },
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i má»Ÿ key {}: {}", key, e);
                    }
                }
            }
            "win32_registry_write" => {
                let hive_str = args.get("hive").and_then(|v| v.as_str()).unwrap_or("");
                let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let val_name = args
                    .get("value_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let val_data = args
                    .get("value_data")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                use winreg::enums::*;
                use winreg::RegKey;

                let hive = match hive_str {
                    "HKLM" => HKEY_LOCAL_MACHINE,
                    "HKCU" => HKEY_CURRENT_USER,
                    "HKCR" => HKEY_CLASSES_ROOT,
                    "HKU" => HKEY_USERS,
                    "HKCC" => HKEY_CURRENT_CONFIG,
                    _ => HKEY_LOCAL_MACHINE,
                };

                let hk = RegKey::predef(hive);
                match hk.create_subkey(key) {
                    Ok((subkey, _)) => match subkey.set_value(val_name, &val_data.to_string()) {
                        Ok(_) => {
                            output =
                                format!("ThÃ nh cÃ´ng ghi giÃ¡ trá»‹ {} vÃ o key {}", val_name, key)
                        }
                        Err(e) => {
                            is_error = true;
                            output = format!("Lá»—i ghi giÃ¡ trá»‹: {}", e);
                        }
                    },
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i má»Ÿ/táº¡o key {}: {}", key, e);
                    }
                }
            }
            "win32_process_list" => {
                let cmd = std::process::Command::new("tasklist")
                    .arg("/FO")
                    .arg("CSV")
                    .arg("/NH")
                    .output();
                match cmd {
                    Ok(out) => output = String::from_utf8_lossy(&out.stdout).to_string(),
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i láº¥y danh sÃ¡ch tiáº¿n trÃ¬nh: {}", e);
                    }
                }
            }
            "win32_process_kill" => {
                let process_name = args.get("process_name").and_then(|v| v.as_str());
                let pid = args.get("pid").and_then(|v| v.as_i64());
                let mut cmd = std::process::Command::new("taskkill");
                cmd.arg("/F");
                if let Some(p) = pid {
                    cmd.arg("/PID").arg(p.to_string());
                } else if let Some(pn) = process_name {
                    cmd.arg("/IM").arg(pn);
                } else {
                    return Err(anyhow::anyhow!("Cáº§n cung cáº¥p process_name hoáº·c pid"));
                }

                match cmd.output() {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        if out.status.success() {
                            output = stdout;
                        } else {
                            is_error = true;
                            output = format!("Lá»—i taskkill:\n{}\n{}", stderr, stdout);
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y taskkill: {}", e);
                    }
                }
            }
            "win32_winget_search" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let cmd = std::process::Command::new("winget")
                    .arg("search")
                    .arg(query)
                    .arg("--accept-source-agreements")
                    .output();
                match cmd {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        output = format!("{}\n{}", stdout, stderr);
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y winget: {}", e);
                    }
                }
            }
            "win32_winget_install" => {
                let pkg = args
                    .get("package_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cmd = std::process::Command::new("winget")
                    .arg("install")
                    .arg("--id")
                    .arg(pkg)
                    .arg("--exact")
                    .arg("--accept-source-agreements")
                    .arg("--accept-package-agreements")
                    .output();
                match cmd {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        if out.status.success() {
                            output = stdout;
                        } else {
                            is_error = true;
                            output = format!("Lá»—i winget install:\n{}\n{}", stderr, stdout);
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y winget: {}", e);
                    }
                }
            }
            "win32_winget_uninstall" => {
                let pkg = args
                    .get("package_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cmd = std::process::Command::new("winget")
                    .arg("uninstall")
                    .arg("--id")
                    .arg(pkg)
                    .arg("--exact")
                    .arg("--accept-source-agreements")
                    .output();
                match cmd {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        if out.status.success() {
                            output = stdout;
                        } else {
                            is_error = true;
                            output = format!("Lá»—i winget uninstall:\n{}\n{}", stderr, stdout);
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y winget: {}", e);
                    }
                }
            }
            "win32_shell_execute" => {
                let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let cmd = std::process::Command::new("powershell")
                    .arg("-NoProfile")
                    .arg("-NonInteractive")
                    .arg("-Command")
                    .arg(command)
                    .output();
                match cmd {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        if out.status.success() {
                            output = stdout;
                        } else {
                            is_error = true;
                            output = format!("Lá»—i:\n{}\n{}", stderr, stdout);
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y powershell: {}", e);
                    }
                }
            }
            "win32_uia_inspect" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{}")
$ae = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
if ($ae) {{
    $controls = $ae.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $results = @()
    foreach ($c in $controls) {{
        if ($c.Current.AutomationId -or $c.Current.Name) {{
            $results += [PSCustomObject] @{{
                Name = $c.Current.Name
                AutomationId = $c.Current.AutomationId
                ControlType = $c.Current.ControlType.ProgrammaticName
            }}
        }}
    }}
    $results | Select-Object -First 100 | ConvertTo-Json -Depth 2
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    window_title.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        let res = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if res.is_empty() {
                            output = String::from_utf8_lossy(&out.stderr).trim().to_string();
                            is_error = true;
                        } else {
                            output = res;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell UIA: {}", e);
                    }
                }
            }
            "win32_uia_click" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let element_id_or_name = args
                    .get("element_id_or_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$rootCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{w}")
$window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $rootCond)
if ($window) {{
    $condId = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, "{e}")
    $condName = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{e}")
    $condOr = New-Object System.Windows.Automation.OrCondition($condId, $condName)
    $target = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condOr)

    if ($target) {{
        try {{
            $invokePattern = $target.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
            $invokePattern.Invoke()
            Write-Output "Click thÃ nh cÃ´ng"
        }} catch {{
            Write-Output "Lá»—i: KhÃ´ng há»— trá»£ InvokePattern"
        }}
    }} else {{
        Write-Output "KhÃ´ng tÃ¬m tháº¥y element"
    }}
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    w = window_title.replace('"', "`\""),
                    e = element_id_or_name.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if output.starts_with("Lá»—i") || output.starts_with("KhÃ´ng tÃ¬m tháº¥y")
                        {
                            is_error = true;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell: {}", e);
                    }
                }
            }
            "win32_uia_enter_text" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let element_id_or_name = args
                    .get("element_id_or_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");

                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$rootCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{w}")
$window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $rootCond)
if ($window) {{
    $condId = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, "{e}")
    $condName = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{e}")
    $condOr = New-Object System.Windows.Automation.OrCondition($condId, $condName)
    $target = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condOr)

    if ($target) {{
        try {{
            $valuePattern = $target.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
            $valuePattern.SetValue("{t}")
            Write-Output "Nháº­p chá»¯ thÃ nh cÃ´ng"
        }} catch {{
            Write-Output "Lá»—i: Element khÃ´ng há»— trá»£ ValuePattern"
        }}
    }} else {{
        Write-Output "KhÃ´ng tÃ¬m tháº¥y element"
    }}
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    w = window_title.replace('"', "`\""),
                    e = element_id_or_name.replace('"', "`\""),
                    t = text.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if output.starts_with("Lá»—i") || output.starts_with("KhÃ´ng tÃ¬m tháº¥y")
                        {
                            is_error = true;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell: {}", e);
                    }
                }
            }
            "win32_uia_get_value" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let element_id_or_name = args
                    .get("element_id_or_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$rootCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{w}")
$window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $rootCond)
if ($window) {{
    $condId = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, "{e}")
    $condName = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{e}")
    $condOr = New-Object System.Windows.Automation.OrCondition($condId, $condName)
    $target = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condOr)

    if ($target) {{
        try {{
            $valuePattern = $target.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
            Write-Output $valuePattern.Current.Value
        }} catch {{
            try {{
                $textPattern = $target.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
                Write-Output $textPattern.DocumentRange.GetText(-1)
            }} catch {{
                Write-Output $target.Current.Name
            }}
        }}
    }} else {{
        Write-Output "KhÃ´ng tÃ¬m tháº¥y element"
    }}
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    w = window_title.replace('"', "`\""),
                    e = element_id_or_name.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if output.starts_with("KhÃ´ng tÃ¬m tháº¥y") {
                            is_error = true;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell: {}", e);
                    }
                }
            }
            "win32_uia_toggle" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let element_id_or_name = args
                    .get("element_id_or_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$rootCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{w}")
$window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $rootCond)
if ($window) {{
    $condId = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, "{e}")
    $condName = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{e}")
    $condOr = New-Object System.Windows.Automation.OrCondition($condId, $condName)
    $target = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condOr)

    if ($target) {{
        try {{
            $togglePattern = $target.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
            $togglePattern.Toggle()
            Write-Output "Toggle thÃ nh cÃ´ng"
        }} catch {{
            Write-Output "Lá»—i: Element khÃ´ng há»— trá»£ TogglePattern"
        }}
    }} else {{
        Write-Output "KhÃ´ng tÃ¬m tháº¥y element"
    }}
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    w = window_title.replace('"', "`\""),
                    e = element_id_or_name.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if output.starts_with("Lá»—i") || output.starts_with("KhÃ´ng tÃ¬m tháº¥y")
                        {
                            is_error = true;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell: {}", e);
                    }
                }
            }
            "win32_uia_select" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let element_id_or_name = args
                    .get("element_id_or_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$rootCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{w}")
$window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $rootCond)
if ($window) {{
    $condId = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, "{e}")
    $condName = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{e}")
    $condOr = New-Object System.Windows.Automation.OrCondition($condId, $condName)
    $target = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condOr)

    if ($target) {{
        try {{
            $selectPattern = $target.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
            $selectPattern.Select()
            Write-Output "Select thÃ nh cÃ´ng"
        }} catch {{
            Write-Output "Lá»—i: Element khÃ´ng há»— trá»£ SelectionItemPattern"
        }}
    }} else {{
        Write-Output "KhÃ´ng tÃ¬m tháº¥y element"
    }}
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    w = window_title.replace('"', "`\""),
                    e = element_id_or_name.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if output.starts_with("Lá»—i") || output.starts_with("KhÃ´ng tÃ¬m tháº¥y")
                        {
                            is_error = true;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell: {}", e);
                    }
                }
            }
            "win32_uia_set_focus" => {
                let window_title = args
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let element_id_or_name = args
                    .get("element_id_or_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let script = format!(
                    r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$rootCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{w}")
$window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $rootCond)
if ($window) {{
    $condId = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, "{e}")
    $condName = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "{e}")
    $condOr = New-Object System.Windows.Automation.OrCondition($condId, $condName)
    $target = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condOr)

    if ($target) {{
        try {{
            $target.SetFocus()
            Write-Output "Focus thÃ nh cÃ´ng"
        }} catch {{
            Write-Output "Lá»—i: KhÃ´ng thá»ƒ focus element"
        }}
    }} else {{
        Write-Output "KhÃ´ng tÃ¬m tháº¥y element"
    }}
}} else {{
    Write-Output "KhÃ´ng tÃ¬m tháº¥y cá»­a sá»•"
}}
"#,
                    w = window_title.replace('"', "`\""),
                    e = element_id_or_name.replace('"', "`\"")
                );

                match std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .output()
                {
                    Ok(out) => {
                        output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if output.starts_with("Lá»—i") || output.starts_with("KhÃ´ng tÃ¬m tháº¥y")
                        {
                            is_error = true;
                        }
                    }
                    Err(e) => {
                        is_error = true;
                        output = format!("Lá»—i cháº¡y PowerShell: {}", e);
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Tool not found: {}", name)),
        }

        Ok(ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: Some(output),
                data: None,
                mime_type: None,
            }],
            is_error,
        })
    }
}

/// --- Analytic MCP Server (Polars SQL) ---
pub struct AnalyticServer {
    tables: std::sync::Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, polars::prelude::LazyFrame>>,
    >,
}

impl Default for AnalyticServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalyticServer {
    pub fn new() -> Self {
        Self {
            tables: std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl InternalMcpServer for AnalyticServer {
    fn name(&self) -> &str {
        "analytic_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "polars_load_table".to_string(),
                description: "Táº£i má»™t file CSV/JSON vÃ o bá»™ nhá»› RAM dÆ°á»›i dáº¡ng báº£ng dá»¯ liá»‡u Ä‘á»ƒ phÃ¢n tÃ\u{AD}ch SQL.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "ÄÆ°á»ng dáº«n tuyá»‡t Ä‘á»‘i tá»›i file CSV/JSON" },
                        "table_name": { "type": "string", "description": "TÃªn báº£ng Ä‘á»ƒ gá»i trong cÃ¢u lá»‡nh SQL" }
                    },
                    "required": ["file_path", "table_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "polars_get_schema".to_string(),
                description: "Láº¥y danh sÃ¡ch cÃ¡c cá»™t vÃ  kiá»ƒu dá»¯ liá»‡u cá»§a má»™t báº£ng dá»¯ liá»‡u.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "table_name": { "type": "string", "description": "TÃªn báº£ng" }
                    },
                    "required": ["table_name"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "polars_execute_sql".to_string(),
                description: "Thá»±c thi cÃ¢u lá»‡nh SQL trÃªn cÃ¡c báº£ng dá»¯ liá»‡u Ä‘Ã£ táº£i vÃ o bá»™ nhá»›.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "CÃ¢u lá»‡nh SQL chuáº©n" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        use polars::prelude::*;
        use polars_sql::SQLContext;

        if name == "polars_load_table" {
            let args = arguments.unwrap_or_default();
            let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            let table_name = args
                .get("table_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if file_path.is_empty() || table_name.is_empty() {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some("file_path and table_name are required".to_string()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let path = PathBuf::from(file_path);
            if !path.exists() {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("File not found: {}", file_path)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let lf = if file_path.ends_with(".csv") {
                match LazyCsvReader::new(file_path).finish() {
                    Ok(l) => l,
                    Err(e) => {
                        return Ok(ToolCallResult {
                            content: vec![ToolContent {
                                content_type: "text".to_string(),
                                text: Some(format!("Failed to read CSV: {}", e)),
                                data: None,
                                mime_type: None,
                            }],
                            is_error: true,
                        })
                    }
                }
            } else if file_path.ends_with(".json") {
                let file = std::fs::File::open(file_path)?;
                match JsonReader::new(file).finish() {
                    Ok(df) => df.lazy(),
                    Err(e) => {
                        return Ok(ToolCallResult {
                            content: vec![ToolContent {
                                content_type: "text".to_string(),
                                text: Some(format!("Failed to read JSON: {}", e)),
                                data: None,
                                mime_type: None,
                            }],
                            is_error: true,
                        })
                    }
                }
            } else {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(
                            "Only .csv and .json are supported via this tool currently".to_string(),
                        ),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            };

            let mut tables = self.tables.lock().await;
            tables.insert(table_name.to_string(), lf);

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(format!(
                        "ÄÃ£ táº£i thÃ nh cÃ´ng file {} vÃ o báº£ng {}",
                        file_path, table_name
                    )),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "polars_get_schema" {
            let args = arguments.unwrap_or_default();
            let table_name = args
                .get("table_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut tables = self.tables.lock().await;
            if let Some(lf) = tables.get_mut(table_name) {
                let schema = match lf.collect_schema() {
                    Ok(s) => s,
                    Err(e) => {
                        return Ok(ToolCallResult {
                            content: vec![ToolContent {
                                content_type: "text".to_string(),
                                text: Some(format!("Failed to get schema: {}", e)),
                                data: None,
                                mime_type: None,
                            }],
                            is_error: true,
                        })
                    }
                };

                let mut cols = Vec::new();
                for (name, dtype) in schema.iter() {
                    cols.push(serde_json::json!({
                        "column": name.to_string(),
                        "type": format!("{:?}", dtype)
                    }));
                }
                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(serde_json::to_string_pretty(&cols).unwrap_or_default()),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            } else {
                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Table '{}' not found", table_name)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                })
            }
        } else if name == "polars_execute_sql" {
            let args = arguments.unwrap_or_default();
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

            let tables = self.tables.lock().await;
            let mut ctx = SQLContext::new();
            for (name, lf) in tables.iter() {
                ctx.register(name, lf.clone());
            }

            // Execute SQL
            let result_lf = match ctx.execute(query) {
                Ok(l) => l,
                Err(e) => {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!("SQL Execution failed: {}", e)),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    })
                }
            };

            // Collect to DataFrame
            let mut df = match result_lf.collect() {
                Ok(d) => d,
                Err(e) => {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!("Failed to collect DataFrame: {}", e)),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    })
                }
            };

            // Convert DataFrame to JSON
            let mut buf = Vec::new();
            if let Err(e) = JsonWriter::new(&mut buf)
                .with_json_format(JsonFormat::Json)
                .finish(&mut df)
            {
                return Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(format!("Failed to serialize DataFrame to JSON: {}", e)),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: true,
                });
            }

            let json_str = match String::from_utf8(buf) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!("Invalid UTF-8 in JSON output: {}", e)),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    })
                }
            };

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(json_str),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// â”€â”€ Chart Render Server (Hybrid Architecture) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub struct ChartServer {
    app_handle: Option<AppHandle>,
}

impl Default for ChartServer {
    fn default() -> Self {
        Self::new(None)
    }
}

impl ChartServer {
    pub fn new(app_handle: Option<AppHandle>) -> Self {
        Self { app_handle }
    }
}

#[async_trait]
impl InternalMcpServer for ChartServer {
    fn name(&self) -> &str {
        "chart_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "generate_chart_image".to_string(),
                description: "Váº½ biá»ƒu Ä‘á»“ tá»« dá»¯ liá»‡u JSON vÃ  xuáº¥t ra Ä‘Æ°á»ng dáº«n file áº£nh PNG cá»¥c bá»™. Ráº¥t há»¯u Ã\u{AD}ch khi cáº§n chÃ¨n biá»ƒu Ä‘á»“ vÃ o PowerPoint (PPTX) hoáº·c Word.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "chart_type": { "type": "string", "enum": ["bar", "line", "pie", "scatter"], "description": "Loáº¡i biá»ƒu Ä‘á»“" },
                        "title": { "type": "string", "description": "TiÃªu Ä‘á» cá»§a biá»ƒu Ä‘á»“" },
                        "data": { "type": "array", "description": "Máº£ng dá»¯ liá»‡u JSON Ä‘á»ƒ váº½. VD: [{'name': 'Q1', 'value': 100}, {'name': 'Q2', 'value': 200}]", "items": { "type": "object" } },
                        "x_key": { "type": "string", "description": "TÃªn trÆ°á»ng dá»¯ liá»‡u dÃ¹ng cho trá»¥c X (VD: 'name')" },
                        "y_key": { "type": "string", "description": "TÃªn trÆ°á»ng dá»¯ liá»‡u dÃ¹ng cho trá»¥c Y (VD: 'value')" },
                        "theme": { "type": "string", "enum": ["light", "dark"], "description": "Giao diá»‡n (máº·c Ä‘á»‹nh light)" }
                    },
                    "required": ["chart_type", "title", "data", "x_key", "y_key"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        if name == "generate_chart_image" {
            let app = match &self.app_handle {
                Some(a) => a,
                None => return Err(anyhow::anyhow!("AppHandle not initialized for ChartServer")),
            };

            let args = arguments.unwrap_or_default();
            let request_id = uuid::Uuid::new_v4().to_string();

            let (tx, rx) = tokio::sync::oneshot::channel::<String>();

            {
                use crate::AppState;
                use tauri::Manager;
                let state = app.state::<AppState>();
                let mut map = state.chart_render_state.lock().await;
                map.insert(request_id.clone(), tx);
            }

            // Emit event to Frontend
            use tauri::Emitter;
            if let Err(e) = app.emit(
                "mcp_chart_render_request",
                serde_json::json!({
                    "request_id": request_id,
                    "payload": args
                }),
            ) {
                return Err(anyhow::anyhow!(
                    "Failed to emit chart render request: {}",
                    e
                ));
            }

            // Wait for response with 15s timeout
            let base64_result = tokio::time::timeout(std::time::Duration::from_secs(15), rx).await;

            match base64_result {
                Ok(Ok(base64_str)) => {
                    // Extract data from "data:image/png;base64,..."
                    let b64_data = if base64_str.contains(",") {
                        base64_str.split(',').nth(1).unwrap_or(&base64_str)
                    } else {
                        &base64_str
                    };

                    use base64::Engine;
                    let decoded = match base64::engine::general_purpose::STANDARD.decode(b64_data) {
                        Ok(d) => d,
                        Err(e) => return Err(anyhow::anyhow!("Failed to decode base64: {}", e)),
                    };

                    let temp_dir = std::env::temp_dir().join("office_hub_exports");
                    let _ = std::fs::create_dir_all(&temp_dir);
                    let file_name = format!("chart_{}.png", request_id);
                    let file_path = temp_dir.join(&file_name);

                    if let Err(e) = std::fs::write(&file_path, decoded) {
                        return Err(anyhow::anyhow!("Failed to write chart image: {}", e));
                    }

                    Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some(format!(
                                "Táº¡o biá»ƒu Ä‘á»“ thÃ nh cÃ´ng. ÄÃ£ lÆ°u táº¡i: {}",
                                file_path.to_string_lossy()
                            )),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: false,
                    })
                }
                Ok(Err(_)) => Err(anyhow::anyhow!("Sender dropped before returning base64")),
                Err(_) => {
                    // Timeout cleanup
                    use crate::AppState;
                    use tauri::Manager;
                    let state = app.state::<AppState>();
                    let mut map = state.chart_render_state.lock().await;
                    map.remove(&request_id);
                    Err(anyhow::anyhow!(
                        "Timeout waiting for frontend to render chart (15s)"
                    ))
                }
            }
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

/// --- Search MCP Server ---
pub struct WebSearchServer {}

impl Default for WebSearchServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchServer {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl InternalMcpServer for WebSearchServer {
    fn name(&self) -> &str {
        "search_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "search_web".to_string(),
                description: "TÃ¬m kiáº¿m web (qua DuckDuckGo).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Tá»« khÃ³a tÃ¬m kiáº¿m" }
                    },
                    "required": ["query"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "read_url".to_string(),
                description: "Äá»c vÃ  trÃ\u{AD}ch xuáº¥t ná»™i dung tá»« má»™t URL thÃ nh vÄƒn báº£n thuáº§n. Sá»\u{AD} dá»¥ng Obscura headless browser (V8, stealth mode) â€” há»— trá»£ JavaScript-rendered pages, khÃ´ng cáº§n Chrome/Edge.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL cáº§n Ä‘á»c (VD: 'https://vnexpress.net')" }
                    },
                    "required": ["url"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        if name == "search_web" {
            let args = arguments.unwrap_or_default();
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .timeout(std::time::Duration::from_secs(15))
                .build()?;

            let res = client
                .post("https://lite.duckduckgo.com/lite/")
                .form(&[("q", query)])
                .send()
                .await;

            let html = match res {
                Ok(r) => r.text().await.unwrap_or_default(),
                Err(e) => return Err(anyhow::anyhow!("Lá»—i máº¡ng khi tÃ¬m kiáº¿m: {}", e)),
            };

            let re_link =
                regex::Regex::new(r#"<a rel="nofollow" href="([^"]+)"[^>]*>(.+?)</a>"#).unwrap();
            let mut results = String::new();

            for (i, cap) in re_link.captures_iter(&html).enumerate() {
                if i >= 5 {
                    break;
                } // Top 5
                let url = cap.get(1).map_or("", |m| m.as_str()).to_string();
                let title = cap.get(2).map_or("", |m| m.as_str()).to_string();
                if !url.contains("duckduckgo.com") && !url.is_empty() {
                    let clean_title = title
                        .replace("<b>", "")
                        .replace("</b>", "")
                        .replace("&#x27;", "'")
                        .replace("&quot;", "\"")
                        .replace("&amp;", "&");
                    results.push_str(&format!("{}. [{}]({})\n", i + 1, clean_title, url));
                }
            }

            if results.is_empty() {
                results = format!(
                    "KhÃ´ng tÃ¬m tháº¥y káº¿t quáº£ nÃ o cho tá»« khÃ³a '{}'",
                    query
                );
            }

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(results),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else if name == "read_url" {
            let args = arguments.unwrap_or_default();
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");

            if url.is_empty() {
                return Err(anyhow!("Thiáº¿u tham sá»‘ 'url'"));
            }

            // Sá»­ dá»¥ng Obscura headless browser (V8, stealth) â€” khÃ´ng cáº§n Chrome/Edge
            use crate::agents::web_researcher::browser_engine::BrowserEngine;
            let engine = BrowserEngine::new()
                .map_err(|e| anyhow!("KhÃ´ng thá»ƒ khá»Ÿi Ä‘á»™ng Obscura engine: {}", e))?;

            let result = engine
                .fetch_text(url)
                .await
                .map_err(|e| anyhow!("read_url tháº¥t báº¡i cho {}: {}", url, e))?;

            let mut markdown_content = format!(
                "**URL:** {}\n**TiÃªu Ä‘á»:** {}\n\n---\n{}",
                result.url,
                result
                    .title
                    .as_deref()
                    .unwrap_or("(khÃ´ng cÃ³ tiÃªu Ä‘á»)"),
                result.content
            );

            // Truncate to avoid context explosion
            if markdown_content.len() > 50_000 {
                markdown_content.truncate(50_000);
                markdown_content.push_str("\n... (ná»™i dung Ä‘Ã£ bá»‹ cáº¯t bá»›t do quÃ¡ dÃ i)");
            }

            Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(markdown_content),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            })
        } else {
            Err(anyhow!("Tool not found"))
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
/// --- WebFetch MCP Server (Obscura engine) ---
///
/// Exposes `web_fetch` and `web_scrape_parallel` as MCP tools so the LLM
/// orchestrator can directly fetch any URL without going through UIA or
/// requiring a visible browser window.
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub struct WebFetchServer;

impl Default for WebFetchServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchServer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl InternalMcpServer for WebFetchServer {
    fn name(&self) -> &str {
        "web_fetch_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "web_fetch".to_string(),
                description:
                    "Táº£i ná»™i dung trang web báº±ng Obscura headless browser (V8, stealth mode). \
                     Tráº£ vá» ná»™i dung dáº¡ng vÄƒn báº£n thuáº§n, Ä‘Ã£ render JavaScript. \
                     DÃ¹ng khi cáº§n Ä‘á»c trang web, bÃ i bÃ¡o, tÃ i liá»‡u online.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL Ä‘áº§y Ä‘á»§ cáº§n táº£i (VD: 'https://vnexpress.net')"
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["text", "html", "links"],
                            "description": "Cháº¿ Ä‘á»™ xuáº¥t: 'text' (máº·c Ä‘á»‹nh) | 'html' | 'links' (danh sÃ¡ch links)"
                        },
                        "eval": {
                            "type": "string",
                            "description": "JavaScript tÃ¹y chá»n Ä‘á»ƒ cháº¡y trÃªn trang sau khi load (VD: 'document.title')"
                        }
                    },
                    "required": ["url"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "web_scrape_parallel".to_string(),
                description:
                    "Scrape nhiá»u URL cÃ¹ng lÃºc báº±ng Obscura vá»›i nhiá»u worker song song. \
                     Hiá»‡u quáº£ khi cáº§n thu tháº\u{AD}p dá»¯ liá»‡u tá»« nhiá»u trang cÃ¹ng lÃºc.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "urls": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Danh sÃ¡ch URL cáº§n scrape"
                        },
                        "concurrency": {
                            "type": "integer",
                            "description": "Sá»‘ worker song song (máº·c Ä‘á»‹nh: 5, tá»‘i Ä‘a: 25)",
                            "default": 5
                        },
                        "eval": {
                            "type": "string",
                            "description": "JavaScript cháº¡y trÃªn má»—i trang Ä‘á»ƒ trÃ\u{AD}ch xuáº¥t dá»¯ liá»‡u"
                        }
                    },
                    "required": ["urls"]
                }),
                tags: vec![],
            },
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        use crate::agents::web_researcher::browser_engine::BrowserEngine;

        let engine = BrowserEngine::new().map_err(|e| {
            anyhow!(
                "Obscura browser engine khÃ´ng khá»Ÿi Ä‘á»™ng Ä‘Æ°á»£c: {}. \
                 Äáº£m báº£o obscura.exe Ä‘Ã£ Ä‘Æ°á»£c Ä‘áº·t trong thÆ° má»¥c tools/obscura/.",
                e
            )
        })?;

        match name {
            "web_fetch" => {
                let args = arguments.unwrap_or_default();
                let url = args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Thiáº¿u tham sá»‘ 'url'"))?;
                let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("text");
                let eval_js = args.get("eval").and_then(|v| v.as_str());

                let result = if let Some(js) = eval_js {
                    engine.eval_js(url, js).await?
                } else {
                    match mode {
                        "html" => engine.fetch_html(url).await?,
                        "links" => {
                            let links = engine.fetch_links(url).await?;
                            let text = links
                                .iter()
                                .map(|(href, anchor)| format!("{}\t{}", href, anchor))
                                .collect::<Vec<_>>()
                                .join("\n");
                            crate::agents::web_researcher::browser_engine::FetchResult {
                                url: url.to_string(),
                                title: None,
                                content: text,
                            }
                        }
                        _ => {
                            let mut res = engine.fetch_text(url).await?;
                            let trimmed = res
                                .content
                                .trim()
                                .replace('\u{00D7}', "")
                                .trim()
                                .to_string();
                            if trimmed.len() < 50 {
                                tracing::warn!("Obscura fetch_text returned empty/tiny content. Falling back to HTML parsing for {}", url);
                                if let Ok(html_res) = engine.fetch_html(url).await {
                                    if let Ok(re_script) =
                                        regex::Regex::new(r"(?is)<script.*?>.*?</script>")
                                    {
                                        let mut text = html_res.content;
                                        text = re_script.replace_all(&text, "").to_string();
                                        if let Ok(re_style) =
                                            regex::Regex::new(r"(?is)<style.*?>.*?</style>")
                                        {
                                            text = re_style.replace_all(&text, "").to_string();
                                        }
                                        if let Ok(re_tags) = regex::Regex::new(r"(?is)<[^>]+>") {
                                            text = re_tags.replace_all(&text, "\n").to_string();
                                        }
                                        text = text
                                            .replace("&nbsp;", " ")
                                            .replace("&amp;", "&")
                                            .replace("&lt;", "<")
                                            .replace("&gt;", ">")
                                            .replace("&quot;", "\"");
                                        if let Ok(re_spaces) = regex::Regex::new(r"(?m)^[ \t]+") {
                                            text = re_spaces.replace_all(&text, "").to_string();
                                        }
                                        if let Ok(re_lines) = regex::Regex::new(r"\n{3,}") {
                                            text = re_lines.replace_all(&text, "\n\n").to_string();
                                        }

                                        res.content = format!(
                                            "(Fallback HTML Extracted Text)\n{}",
                                            text.trim()
                                        );
                                        if res.title.is_none() && html_res.title.is_some() {
                                            res.title = html_res.title;
                                        }
                                    }
                                }
                            }
                            res
                        }
                    }
                };

                let output = format!(
                    "**URL:** {}\n**TiÃªu Ä‘á»:** {}\n\n---\n{}",
                    result.url,
                    result
                        .title
                        .as_deref()
                        .unwrap_or("(khÃ´ng cÃ³ tiÃªu Ä‘á»)"),
                    result.content
                );

                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(output),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            }

            "web_scrape_parallel" => {
                let args = arguments.unwrap_or_default();
                let urls: Vec<String> = args
                    .get("urls")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                if urls.is_empty() {
                    return Ok(ToolCallResult {
                        content: vec![ToolContent {
                            content_type: "text".to_string(),
                            text: Some("Danh sÃ¡ch URL rá»—ng.".to_string()),
                            data: None,
                            mime_type: None,
                        }],
                        is_error: true,
                    });
                }

                let concurrency = args
                    .get("concurrency")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;
                let eval = args.get("eval").and_then(|v| v.as_str());

                let results = engine.scrape_parallel(&urls, concurrency, eval).await?;

                let output = results
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        if let Some(err) = &r.error {
                            format!("## [{}] {} â€” Lá»–I: {}", i + 1, r.url, err)
                        } else {
                            format!(
                                "## [{}] {}\n{}",
                                i + 1,
                                r.url,
                                &r.content[..r.content.len().min(2000)]
                            )
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");

                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(output),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            }

            _ => Err(anyhow!("Tool not found: {}", name)),
        }
    }
}

/// --- Office COM Server ---
pub struct OfficeComServer {}

impl Default for OfficeComServer {
    fn default() -> Self {
        Self::new()
    }
}

impl OfficeComServer {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl InternalMcpServer for OfficeComServer {
    fn name(&self) -> &str {
        "office_com_server"
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        Ok(vec![
            McpTool {
                name: "com_insert_text_active_doc".to_string(),
                description: "ChÃ¨n vÄƒn báº£n vÃ o tÃ i liá»‡u Ä‘ang má»Ÿ (Word: táº¡i con trá», Excel: táº¡i Ã´ hiá»‡n táº¡i, PowerPoint: táº¡o slide má»›i).".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "app_type": { "type": "string", "enum": ["Word", "Excel", "PowerPoint"], "description": "á»¨ng dá»¥ng Ä‘Ã\u{AD}ch" },
                        "text": { "type": "string", "description": "VÄƒn báº£n cáº§n chÃ¨n" }
                    },
                    "required": ["app_type", "text"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "com_replace_active_doc".to_string(),
                description: "XÃ³a toÃ n bá»™ ná»™i dung hiá»‡n táº¡i vÃ  ghi ná»™i dung má»›i vÃ o tÃ i liá»‡u Ä‘ang má»Ÿ.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "app_type": { "type": "string", "enum": ["Word", "Excel", "PowerPoint"], "description": "á»¨ng dá»¥ng Ä‘Ã\u{AD}ch" },
                        "text": { "type": "string", "description": "VÄƒn báº£n má»›i cho toÃ n bá»™ tÃ i liá»‡u" }
                    },
                    "required": ["app_type", "text"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "com_save_active_doc".to_string(),
                description: "LÆ°u tÃ i liá»‡u Ä‘ang má»Ÿ.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "app_type": { "type": "string", "enum": ["Word", "Excel", "PowerPoint"], "description": "á»¨ng dá»¥ng Ä‘Ã\u{AD}ch" }
                    },
                    "required": ["app_type"]
                }),
                tags: vec![],
            },
            McpTool {
                name: "com_extract_active_doc".to_string(),
                description: "TrÃ\u{AD}ch xuáº¥t toÃ n bá»™ vÄƒn báº£n tá»« tÃ i liá»‡u Ä‘ang má»Ÿ.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "app_type": { "type": "string", "enum": ["Word", "Excel", "PowerPoint"], "description": "á»¨ng dá»¥ng Ä‘Ã\u{AD}ch" }
                    },
                    "required": ["app_type"]
                }),
                tags: vec![],
            }
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        let app_type = arguments
            .as_ref()
            .and_then(|a| a.get("app_type").and_then(|v| v.as_str()).map(String::from))
            .unwrap_or_else(|| "Word".to_string());

        let result = match name {
            "com_insert_text_active_doc" => {
                let text = arguments
                    .and_then(|a| a.get("text").and_then(|v| v.as_str()).map(String::from))
                    .unwrap_or_default();
                match app_type.as_str() {
                    "Word" => crate::agents::office_master::com_word::WordApplication::connect_or_launch()?.insert_text_at_cursor(&text, false),
                    "Excel" => crate::agents::office_master::com_excel::ExcelApplication::connect_or_launch()?.insert_text_at_cursor(&text),
                    "PowerPoint" => crate::agents::office_master::com_ppt::PowerPointApplication::connect_or_launch()?.insert_text_at_cursor(&text),
                    _ => Err(anyhow::anyhow!("Unsupported app_type")),
                }
            }
            "com_replace_active_doc" => {
                let text = arguments
                    .and_then(|a| a.get("text").and_then(|v| v.as_str()).map(String::from))
                    .unwrap_or_default();
                match app_type.as_str() {
                    "Word" => crate::agents::office_master::com_word::WordApplication::connect_or_launch()?.replace_active_document(&text),
                    "Excel" => crate::agents::office_master::com_excel::ExcelApplication::connect_or_launch()?.replace_active_document(&text),
                    "PowerPoint" => crate::agents::office_master::com_ppt::PowerPointApplication::connect_or_launch()?.replace_active_document(&text),
                    _ => Err(anyhow::anyhow!("Unsupported app_type")),
                }
            }
            "com_save_active_doc" => match app_type.as_str() {
                "Word" => {
                    crate::agents::office_master::com_word::WordApplication::connect_or_launch()?
                        .save_active_document()
                }
                "Excel" => {
                    crate::agents::office_master::com_excel::ExcelApplication::connect_or_launch()?
                        .save_active_document()
                }
                "PowerPoint" => {
                    crate::agents::office_master::com_ppt::PowerPointApplication::connect_or_launch(
                    )?
                    .save_active_document()
                }
                _ => Err(anyhow::anyhow!("Unsupported app_type")),
            },
            "com_extract_active_doc" => match app_type.as_str() {
                "Word" => {
                    crate::agents::office_master::com_word::WordApplication::connect_or_launch()?
                        .extract_active_document()
                }
                "Excel" => {
                    crate::agents::office_master::com_excel::ExcelApplication::connect_or_launch()?
                        .extract_active_document()
                }
                "PowerPoint" => {
                    crate::agents::office_master::com_ppt::PowerPointApplication::connect_or_launch(
                    )?
                    .extract_active_document()
                }
                _ => Err(anyhow::anyhow!("Unsupported app_type")),
            },
            _ => return Err(anyhow::anyhow!("Tool not found: {}", name)),
        };

        match result {
            Ok(msg) => Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(msg),
                    data: None,
                    mime_type: None,
                }],
                is_error: false,
            }),
            Err(e) => Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(format!("Error calling COM for {}: {}", app_type, e)),
                    data: None,
                    mime_type: None,
                }],
                is_error: true,
            }),
        }
    }
}
