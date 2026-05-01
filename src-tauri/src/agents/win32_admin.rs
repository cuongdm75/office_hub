use async_trait::async_trait;

use crate::agents::{Agent, AgentId, AgentStatus};
use crate::orchestrator::{AgentOutput, AgentTask};

pub struct Win32AdminAgent {
    status: AgentStatus,
}

impl Default for Win32AdminAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Win32AdminAgent {
    pub fn new() -> Self {
        Self {
            status: AgentStatus::Idle,
        }
    }
}

#[async_trait]
impl Agent for Win32AdminAgent {
    fn id(&self) -> &AgentId {
        static ID: std::sync::OnceLock<AgentId> = std::sync::OnceLock::new();
        ID.get_or_init(|| AgentId::custom("win32_admin"))
    }

    fn name(&self) -> &str {
        "Win32 Admin"
    }

    fn description(&self) -> &str {
        "System Administrator for Windows OS. Can manipulate files, processes, registry, and winget packages."
    }

    fn supported_actions(&self) -> Vec<String> {
        // The actual actions are exposed via the internal MCP server.
        // This wrapper just allows it to show up in the marketplace.
        vec![]
    }

    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn execute(&mut self, _task: AgentTask) -> anyhow::Result<AgentOutput> {
        // Orchestrator routes tool calls directly to MCP Broker.
        // This method shouldn't be called directly for tool executions.
        Ok(AgentOutput {
            content: "Thực thi qua MCP Broker".to_string(),
            committed: false,
            tokens_used: None,
            metadata: None,
        })
    }
}
