use async_trait::async_trait;
use tracing::info;

use crate::agent_actions;
use crate::agents::{Agent, AgentId, AgentStatus};
use crate::orchestrator::{AgentOutput, AgentTask};

pub struct SystemAgent {
    status: AgentStatus,
}

impl Default for SystemAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemAgent {
    pub fn new() -> Self {
        Self {
            status: AgentStatus::Idle,
        }
    }
}

#[async_trait]
impl Agent for SystemAgent {
    fn id(&self) -> &AgentId {
        static ID: std::sync::OnceLock<AgentId> = std::sync::OnceLock::new();
        ID.get_or_init(|| AgentId::custom("system"))
    }

    fn name(&self) -> &str {
        "System Agent"
    }

    fn description(&self) -> &str {
        "Handles internal system tasks like sending files from the local filesystem to the user."
    }

    fn supported_actions(&self) -> Vec<String> {
        agent_actions!["send_file"]
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "send_file".to_string(),
                description: "Gửi một file từ máy tính local đến user (tải xuống). Tham số: `file_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
                tags: vec![],
            }
        ]
    }

    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        self.status = AgentStatus::Busy;

        let result = match task.action.as_str() {
            "send_file" => self.handle_send_file(&task).await,
            _ => Err(anyhow::anyhow!("Unsupported action: {}", task.action)),
        };

        self.status = AgentStatus::Idle;
        result
    }
}

impl SystemAgent {
    async fn handle_send_file(&self, task: &AgentTask) -> anyhow::Result<AgentOutput> {
        let file_path = task.parameters.get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(anyhow::anyhow!("File does not exist: {}", file_path));
        }

        // Keep base64 empty or remove it
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

        let public_dir = std::env::temp_dir().join("office_hub_exports");
        let _ = std::fs::create_dir_all(&public_dir);

        let unique_id = uuid::Uuid::new_v4().to_string();
        let target_filename = format!("{}_{}", unique_id, file_name);
        let target_path = public_dir.join(&target_filename);

        if let Err(e) = std::fs::copy(path, &target_path) {
            return Err(anyhow::anyhow!("Failed to copy file to public directory: {}", e));
        }

        let ip = local_ip_address::local_ip().map(|ip| ip.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());
        // Lấy port của ws server (thường là 9001) cộng 1 thành 9002 cho HTTP server
        // Ở đây ta hardcode 9002 (giả định), tốt nhất là load từ config nhưng để đơn giản ta gán 9002
        let url = format!("http://{}:9002/files/{}", ip, target_filename);

        let meta = serde_json::json!({
            "action": "send_file",
            "file_path": file_path,
            "attachment": {
                "name": file_name,
                "url": url,
                "base64": "" // Fallback/compatibility
            }
        });

        info!("Sending file '{}' to user via HTTP URL: {}", file_name, url);

        Ok(AgentOutput {
            content: format!("Đã đính kèm file: {}", file_name),
            committed: true,
            tokens_used: None,
            metadata: Some(meta),
        })
    }
}
