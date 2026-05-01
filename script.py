import json
with open('src-tauri/src/orchestrator/mod.rs', 'r', encoding='utf-8') as f:
    text = f.read()

while '\n\n\n' in text:
    text = text.replace('\n\n\n', '\n\n')

start_str = 'let mut tools_desc = String::new();'
end_str = 'let mut messages = vec![crate::llm_gateway::LlmMessage::system(system_prompt)];'

s = text.find(start_str)
e = text.find(end_str)
if s != -1 and e != -1:
    new_text = text[:s] + '''let mut tools_desc = String::new();
        for agent in statuses {
            tools_desc.push_str(&format!("- Agent ID: {}\\n", agent.id));
            tools_desc.push_str(&format!("  Name: {}\\n", agent.name));
            tools_desc.push_str(&format!("  Capabilities (Actions): {:?}\\n\\n", agent.capabilities));
        }

        let mut mcp_tools_desc = String::new();
        if let Ok(mcp_tools) = self.mcp_broker.list_all_tools().await {
            for tool in mcp_tools {
                mcp_tools_desc.push_str(&format!("- Tool ID: {}\\n", tool.name));
                mcp_tools_desc.push_str(&format!("  Description: {}\\n", tool.description));
                mcp_tools_desc.push_str(&format!("  Schema: {}\\n\\n", serde_json::to_string(&tool.input_schema).unwrap_or_default()));
            }
        }

        let system_prompt = format!(
            "Bạn là Office Hub Orchestrator, một trợ lý điều phối Agent.\\n\\
             \\n[AVAILABLE MCP TOOLS]\\n\\
             Bạn có thể dùng các MCP Tools sau đây để tra cứu Policy, Memory, Knowledge hoặc gọi plugin:\\n\\
             {mcp_tools_desc}\\n\\
             \\n[AVAILABLE SKILLS/AGENTS]\\n\\
             Bạn có thể gọi các Agents sau đây để thực hiện nhiệm vụ:\\n\\
             {tools_desc}\\n\\
             Nếu bạn cần dùng Agent hoặc Tool, hãy trả về danh sách gent_calls với gent_id (với Agent) hoặc 	ool_id (nếu dùng MCP Tool) và ction, kèm parameters dưới dạng JSON.\\n\\
             Lưu ý: Đối với MCP Tools, hãy đặt gent_id = 'mcp_broker' và ction = tên của tool.\\n\\
             Nếu câu hỏi chỉ là trò chuyện thông thường hoặc bạn đã có đủ thông tin, hãy điền vào direct_response.\\n\\
             Luôn đưa ra 'thought' giải thích quá trình suy luận của bạn.\\n\\
             \\n\\
             IMPORTANT: Your ENTIRE output MUST be a valid JSON object matching the requested schema. Do NOT wrap the JSON in Markdown formatting (no `json). Do NOT output bullet points. Output ONLY the raw JSON object."
        );

        ''' + text[e:]
    with open('src-tauri/src/orchestrator/mod.rs', 'w', encoding='utf-8') as f:
        f.write(new_text)
    print('Replaced!')
else:
    print('Not found')
