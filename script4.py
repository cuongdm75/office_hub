import io
import re

with open('src-tauri/src/orchestrator/mod.rs', 'r', encoding='utf-8') as f:
    text = f.read()

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

pattern = r"(continue;\s*}\s*)(let agent_arc = self\s*\.\s*agent_registry\s*\.\s*get_mut)"
match = re.search(pattern, text)
if match:
    text = text[:match.start(2)] + mcp_logic + text[match.start(2):]
    with open('src-tauri/src/orchestrator/mod.rs', 'w', encoding='utf-8') as f:
        f.write(text)
    print("Injected!")
else:
    print("Pattern not found")
