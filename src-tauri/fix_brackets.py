import sys

file_path = 'e:/Office hub/src-tauri/src/orchestrator/mod.rs'
with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

start_idx = content.find('tool_name => {')
end_idx = content.find('tool_results.push(ToolResult {', start_idx)

if start_idx != -1 and end_idx != -1:
    replacement = """tool_name => {
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
                                                if result.is_error {
                                                    format!("⚠️ Tool '{}' lỗi:\\n{}", tool_name, text_buf.trim())
                                                } else {
                                                    text_buf.trim().to_string()
                                                }
                                            }
                                        }
                                        Err(e) => format!("⚠️ Lỗi khi gọi tool '{}': {}", tool_name, e),
                                    }
                                }
                            }
                        };

                        """
    content = content[:start_idx] + replacement + content[end_idx:]
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print("Replaced successfully")
else:
    print("Could not find boundaries")
