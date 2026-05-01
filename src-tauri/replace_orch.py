import sys
import re

# 1. Update agents/mod.rs
agents_mod_path = 'e:/Office hub/src-tauri/src/agents/mod.rs'
with open(agents_mod_path, 'r', encoding='utf-8') as f:
    agents_mod_content = f.read()

find_agent_method = """    /// Return all tool schemas from registered agents.
    pub fn all_tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        self.inner
            .values()
            .filter_map(|arc| arc.try_read().ok().map(|g| g.tool_schemas()))
            .flatten()
            .collect()
    }

    /// Find an agent ID that supports a given action name.
    pub fn find_agent_by_action(&self, action: &str) -> Option<AgentId> {
        let action_string = action.to_string();
        for (id, arc) in self.inner.iter() {
            if let Ok(guard) = arc.try_read() {
                if guard.supported_actions().contains(&action_string) {
                    return Some(id.clone());
                }
            }
        }
        None
    }"""

if 'find_agent_by_action' not in agents_mod_content:
    agents_mod_content = agents_mod_content.replace('''    /// Return all tool schemas from registered agents.
    pub fn all_tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        self.inner
            .values()
            .filter_map(|arc| arc.try_read().ok().map(|g| g.tool_schemas()))
            .flatten()
            .collect()
    }''', find_agent_method)
    with open(agents_mod_path, 'w', encoding='utf-8') as f:
        f.write(agents_mod_content)
    print("Updated agents/mod.rs")


# 2. Update orchestrator/mod.rs
orch_mod_path = 'e:/Office hub/src-tauri/src/orchestrator/mod.rs'
with open(orch_mod_path, 'r', encoding='utf-8') as f:
    orch_mod_content = f.read()

# Remove call_legacy_agent injection and replace with agent_registry schemas
legacy_injection_pattern = r'// 3\. `call_legacy_agent`.*?mcp_tools\.push\(McpTool \{.*?name: "call_legacy_agent"\.to_string\(\).*?\}\);'
replacement_schemas = """        // 3. Đăng ký toàn bộ Native Schemas từ các Agent
        mcp_tools.extend(self.agent_registry.all_tool_schemas());"""
orch_mod_content = re.sub(legacy_injection_pattern, replacement_schemas, orch_mod_content, flags=re.DOTALL)

# Update system prompt to remove call_legacy_agent hint
old_prompt_hint = """             Sau đó dùng `call_legacy_agent` với agent_id tìm được.\n"""
new_prompt_hint = """             Bạn có thể gọi trực tiếp các tool tìm được.\n"""
orch_mod_content = orch_mod_content.replace(old_prompt_hint, new_prompt_hint)

# Remove call_legacy_agent matching block and replace default block
tool_match_pattern = r'// ── Hybrid Bridge: call_legacy_agent ──────────────.*?"call_legacy_agent" => \{.*?\}\s*// ── MCP Tool call ─────────────────────────────────\s*tool_name => \{.*?\}'

replacement_match = """// ── Tool Execution (Agent or MCP Server) ─────────
                            tool_name => {
                                // 1. Thử tìm trong AgentRegistry trước
                                if let Some(agent_id) = self.agent_registry.find_agent_by_action(tool_name) {
                                    if let Some(agent_arc) = self.agent_registry.get_mut(&agent_id) {
                                        let task = AgentTask {
                                            task_id: uuid::Uuid::new_v4().to_string(),
                                            action: tool_name.to_string(),
                                            intent: intent::Intent::Ambiguous(Default::default()),
                                            message: message.to_string(),
                                            context_file: context_file.map(String::from),
                                            session_id: session_id.to_string(),
                                            parameters: call.arguments.clone().as_object().cloned().unwrap_or_default().into_iter().collect(),
                                            llm_gateway: Some(Arc::clone(&llm_gateway)),
                                            global_policy: None,
                                            knowledge_context: None,
                                            parent_task_id: None,
                                            dependencies: vec![],
                                        };

                                        let mut agent_guard = agent_arc.write().await;
                                        match agent_guard.execute(task).await {
                                            Ok(out) => out.content,
                                            Err(e) => format!("⚠️ Agent '{}' lỗi: {}", agent_id, e),
                                        }
                                    } else {
                                        format!("⚠️ Agent '{}' không tồn tại.", agent_id)
                                    }
                                } else {
                                    // 2. Không tìm thấy Agent hỗ trợ -> fallback cho MCP Broker
                                    match mcp_broker.call_tool(tool_name, Some(call.arguments.clone())).await {
                                        Ok(result) => {
                                            let mut text_buf = String::new();
                                            for item in result.content {
                                                if item.content_type == "text" {
                                                    if let Some(t) = item.text {
                                                        text_buf.push_str(&t);
                                                        text_buf.push('\\n');
                                                    }
                                                }
                                            }
                                            if text_buf.trim().is_empty() {
                                                "Tool executed successfully but returned empty result.".to_string()
                                            } else {
                                                text_buf.trim().to_string()
                                            }
                                        }
                                        Err(e) => format!("⚠️ Lỗi khi gọi tool '{}': {}", tool_name, e),
                                    }
                                }
                            }"""

orch_mod_content = re.sub(tool_match_pattern, replacement_match, orch_mod_content, flags=re.DOTALL)

with open(orch_mod_path, 'w', encoding='utf-8') as f:
    f.write(orch_mod_content)
print("Updated orchestrator/mod.rs")
