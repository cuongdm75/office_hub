use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::broker::InternalMcpServer;
use super::{McpTool, ToolCallResult, ToolContent};
use crate::agents::Agent;
use crate::llm_gateway::LlmGateway;
use crate::orchestrator::{intent::Intent, AgentTask};

/// A wrapper that exposes any `Agent` implementation as an `InternalMcpServer`.
/// This enables Agent-to-Agent communication directly via the MCP Broker.
pub struct AgentMcpAdapter {
    agent_id: String,
    agent: Arc<RwLock<Box<dyn Agent>>>,
    llm_gateway: Option<Arc<RwLock<LlmGateway>>>,
}

impl AgentMcpAdapter {
    pub fn new(
        agent_id: String,
        agent: Arc<RwLock<Box<dyn Agent>>>,
        llm_gateway: Option<Arc<RwLock<LlmGateway>>>,
    ) -> Self {
        Self {
            agent_id,
            agent,
            llm_gateway,
        }
    }
}

#[async_trait]
impl InternalMcpServer for AgentMcpAdapter {
    fn name(&self) -> &str {
        &self.agent_id
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let agent_guard = self.agent.read().await;
        let mut tools = agent_guard.tool_schemas();

        // Fallback: If agent hasn't implemented tool_schemas() yet, generate generic schemas
        // from its supported_actions list.
        if tools.is_empty() {
            for action in agent_guard.supported_actions() {
                tools.push(McpTool {
                    name: action.clone(),
                    description: format!(
                        "Execute action '{}' on agent '{}'",
                        action, self.agent_id
                    ),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "session_id": { "type": "string" },
                            "message": { "type": "string" },
                            "parameters": { "type": "object" }
                        }
                    }),
                    tags: vec![self.agent_id.clone()],
                });
            }
        }
        Ok(tools)
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<ToolCallResult> {
        let args_obj = arguments.clone().unwrap_or_else(|| serde_json::json!({}));

        let session_id = args_obj
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("agent-to-agent-session")
            .to_string();

        let message = args_obj
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Extract parameters specific to the task (excluding the top-level MCP arguments like session_id)
        let parameters = match args_obj.get("parameters") {
            Some(Value::Object(map)) => map.clone().into_iter().collect(),
            Some(val) => {
                let mut map = std::collections::HashMap::new();
                map.insert("raw".to_string(), val.clone());
                map
            }
            None => {
                // If no nested 'parameters' object, use the whole argument map minus session_id/message
                if let Value::Object(map) = &args_obj {
                    let mut map = map.clone();
                    map.remove("session_id");
                    map.remove("message");
                    map.into_iter().collect()
                } else {
                    std::collections::HashMap::new()
                }
            }
        };

        let task = AgentTask {
            task_id: uuid::Uuid::new_v4().to_string(),
            action: name.to_string(),
            intent: Intent::Ambiguous(Default::default()), // Agent-to-Agent doesn't always have intent
            message,
            context_file: None,
            session_id,
            parameters,
            llm_gateway: self.llm_gateway.clone(),
            global_policy: None,
            knowledge_context: None,
            parent_task_id: args_obj
                .get("parent_task_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            dependencies: vec![],
        };

        let mut agent_guard = self.agent.write().await;

        // Ensure the action is supported
        if !agent_guard.supported_actions().contains(&name.to_string()) {
            return Err(anyhow!(
                "Action '{}' is not supported by agent '{}'",
                name,
                self.agent_id
            ));
        }

        match agent_guard.execute(task).await {
            Ok(output) => {
                // Serialize any complex metadata into the result string if needed
                let mut full_result = output.content;
                if let Some(metadata) = output.metadata {
                    full_result.push_str("\n\n---\nMetadata:\n");
                    full_result
                        .push_str(&serde_json::to_string_pretty(&metadata).unwrap_or_default());
                }

                Ok(ToolCallResult {
                    content: vec![ToolContent {
                        content_type: "text".to_string(),
                        text: Some(full_result),
                        data: None,
                        mime_type: None,
                    }],
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: Some(format!("Agent Execution Error: {}", e)),
                    data: None,
                    mime_type: None,
                }],
                is_error: true,
            }),
        }
    }
}
