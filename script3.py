import io
with open('src-tauri/src/orchestrator/mod.rs', 'rb') as f:
    text = f.read().decode('utf-8')

mcp_logic = '''
                    if call.agent_id == "mcp_broker" {
                        match self.mcp_broker.call_tool(&call.action, Some(params)).await {
                            Ok(result) => {
                                let mut result_content = String::new();
                                for res in result.content {
                                    if res.type_ == "text" {
                                        if let Some(text) = res.text {
                                            result_content.push_str(&text);
                                            result_content.push('\\n');
                                        }
                                    }
                                }
                                if result.is_error {
                                    turn_content.push_str(&format!("MCP Tool '{}' error:\\n{}\\n\\n", call.action, result_content));
                                } else {
                                    turn_content.push_str(&format!("MCP Tool '{}' result:\\n{}\\n\\n", call.action, result_content));
                                }
                            }
                            Err(e) => {
                                turn_content.push_str(&format!("Failed to call MCP Tool '{}': {}\\n\\n", call.action, e));
                            }
                        }
                        all_committed = false;
                        continue;
                    }
'''

marker = '                    let agent_arc = self\n                        .agent_registry\n                        .get_mut(&AgentId::custom(&call.agent_id))'
if marker in text:
    text = text.replace(marker, mcp_logic + marker)
    with open('src-tauri/src/orchestrator/mod.rs', 'w', encoding='utf-8') as f:
        f.write(text)
    print("Injected!")
else:
    print("Marker not found")
