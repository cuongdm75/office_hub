use std::sync::Arc;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;
use async_trait::async_trait;
use serde_json::Value;

use super::{McpRegistry, McpTool, ToolCallResult};

/// Trait cho một Internal MCP Server (không chạy qua stdio mà giao tiếp trực tiếp trong RAM).
#[async_trait]
pub trait InternalMcpServer: Send + Sync {
    /// Tên của internal server (ví dụ: "analyst_agent", "policy_server")
    fn name(&self) -> &str;
    
    /// Trả về danh sách các công cụ mà server này cung cấp
    async fn list_tools(&self) -> Result<Vec<McpTool>>;
    
    /// Gọi một công cụ cụ thể
    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult>;
}

/// Trạm trung chuyển (Broker) nhận JSON-RPC tool calls từ LLM Gateway (hoặc Agents)
/// và định tuyến tới đúng Internal Server hoặc External Server (qua McpRegistry).
#[derive(Clone)]
pub struct McpBroker {
    /// Registry quản lý các MCP server bên ngoài (stdio)
    pub external_registry: McpRegistry,
    /// Danh sách các Internal MCP server (Policy, Memory, Agents nội bộ)
    internal_servers: Arc<RwLock<HashMap<String, Arc<dyn InternalMcpServer + 'static>>>>,
}

impl McpBroker {
    pub fn new(external_registry: McpRegistry) -> Self {
        Self {
            external_registry,
            internal_servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Đăng ký một Internal MCP Server vào Broker
    pub async fn register_internal_server(&self, server: Arc<dyn InternalMcpServer + 'static>) {
        let name = server.name().to_string();
        tracing::info!(server_name = %name, "Registered Internal MCP Server");
        self.internal_servers.write().await.insert(name, server);
    }

    /// Gọi tool dựa trên tool name. Broker sẽ tự tìm server nào sở hữu tool này.
    pub async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        // 1. Tìm trong Internal Servers trước
        let servers = self.internal_servers.read().await;
        for entry in servers.values() {
            let server = entry;
            // Tạm thời list_tools mỗi lần gọi (có thể cache lại để tối ưu sau)
            if let Ok(tools) = server.list_tools().await {
                if tools.iter().any(|t| t.name == name) {
                    tracing::debug!(tool = %name, server = %server.name(), "Routing tool call to Internal Server");
                    return server.call_tool(name, arguments).await;
                }
            }
        }
        drop(servers);

        // 2. Fallback sang External Servers (McpRegistry đã có sẵn logic map tool -> server)
        tracing::debug!(tool = %name, "Routing tool call to External Registry");
        self.external_registry.call_tool(name, arguments).await
    }

    /// Lấy danh sách toàn bộ tools (bao gồm cả internal và external)
    pub async fn list_all_tools(&self) -> Result<Vec<McpTool>> {
        let mut all_tools = Vec::new();
        
        // Kéo từ Internal
        let servers = self.internal_servers.read().await;
        for entry in servers.values() {
            let server = entry;
            if let Ok(mut tools) = server.list_tools().await {
                all_tools.append(&mut tools);
            }
        }
        drop(servers);
        
        // Kéo từ External (McpRegistry có hàm list_all_tools trả về McpToolWithServer)
        // Chúng ta cần ánh xạ lại thành McpTool
        let external_tools = self.external_registry.list_all_tools();
        for t in external_tools {
            all_tools.push(McpTool {
                name: t.tool.name,
                description: format!("{} (from {})", t.tool.description, t.server_alias),
                input_schema: t.tool.input_schema,
                tags: t.tool.tags,
            });
        }
        Ok(all_tools)
    }

    /// Lấy danh sách các Internal Servers dưới dạng JSON (dùng cho giao diện UI)
    pub async fn list_internal_json(&self) -> Vec<serde_json::Value> {
        let mut result = Vec::new();
        let servers = self.internal_servers.read().await;
        for (name, server) in servers.iter() {
            let tools = server.list_tools().await.unwrap_or_default();
            result.push(serde_json::json!({
                "id": format!("internal-{}", name),
                "alias": name,
                "status": "running",
                "toolCount": tools.len(),
                "tools": tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
                "serverInfo": serde_json::Value::Null,
                "protocolVersion": "internal",
                "totalCalls": 0,
                "errorCount": 0,
                "registeredAt": chrono::Utc::now().to_rfc3339(),
                "lastConnectedAt": chrono::Utc::now().to_rfc3339(),
                "isInternal": true
            }));
        }
        result
    }

    /// Tìm kiếm tools dựa trên từ khóa (Keyword Scoring với tag support)
    pub async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<McpTool>> {
        let all_tools = self.list_all_tools().await?;
        if query.trim().is_empty() {
            return Ok(all_tools.into_iter().take(limit).collect());
        }

        // Tách từ khóa và chuyển thành chữ thường
        let query_lower = query.to_lowercase();
        let tokens: Vec<&str> = query_lower
            .split_whitespace()
            .filter(|s| s.len() > 2 || s.chars().all(|c| c.is_alphanumeric()))
            .collect();

        if tokens.is_empty() {
            return Ok(all_tools.into_iter().take(limit).collect());
        }

        let mut scored_tools: Vec<(usize, McpTool)> = all_tools.into_iter().map(|tool| {
            let mut score = 0;
            let name_lower = tool.name.to_lowercase();
            let desc_lower = tool.description.to_lowercase();

            for token in &tokens {
                // Exact name match = highest priority
                if name_lower == *token {
                    score += 100;
                } else if name_lower.contains(token) {
                    score += 10;
                }
                // Description match
                if desc_lower.contains(token) {
                    score += 3;
                }
                // Tag match – covers alias/synonym keywords (e.g. "excel" → analyze_workbook)
                for tag in &tool.tags {
                    let tag_lower = tag.to_lowercase();
                    if tag_lower == *token {
                        score += 8; // Exact tag match
                    } else if tag_lower.contains(token) || token.contains(tag_lower.as_str()) {
                        score += 5;
                    }
                }
            }
            (score, tool)
        }).collect();

        // Sort by score descending
        scored_tools.sort_by(|a, b| b.0.cmp(&a.0));

        // Lấy top `limit` tool có score > 0 (tối đa 8 để tránh context overflow)
        let effective_limit = limit.min(8);
        let result = scored_tools.into_iter()
            .filter(|(s, _)| *s > 0)
            .take(effective_limit)
            .map(|(_, t)| t)
            .collect();

        Ok(result)
    }
}
