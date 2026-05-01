import sys

file_path = 'e:/Office hub/src-tauri/src/agents/folder_scanner/mod.rs'
with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

target = '''    fn supported_actions(&self) -> Vec<String> {
        ACTIONS.iter().map(|s| s.to_string()).collect()
    }'''

replacement = '''    fn supported_actions(&self) -> Vec<String> {
        ACTIONS.iter().map(|s| s.to_string()).collect()
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "scan_folder_to_word".to_string(),
                description: "Quét một thư mục và tạo báo cáo tổng hợp bằng Word. Tham số: `folder_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" }
                    },
                    "required": ["folder_path"]
                }),
            },
            crate::mcp::McpTool {
                name: "list_folder_files".to_string(),
                description: "Liệt kê danh sách file trong thư mục. Tham số: `folder_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" }
                    },
                    "required": ["folder_path"]
                }),
            },
            crate::mcp::McpTool {
                name: "read_and_summarize_file".to_string(),
                description: "Đọc và tóm tắt nội dung một file cụ thể. Tham số: `file_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" }
                    },
                    "required": ["file_path"]
                }),
            },
            crate::mcp::McpTool {
                name: "search_folder_content".to_string(),
                description: "Tìm kiếm nội dung trong thư mục. Tham số: `folder_path`, `query`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "folder_path": { "type": "string" },
                        "query": { "type": "string" }
                    },
                    "required": ["folder_path", "query"]
                }),
            }
        ]
    }'''

if target in content:
    content = content.replace(target, replacement)
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print('Replaced folder_scanner')
else:
    print('Target not found in folder_scanner')

file_path = 'e:/Office hub/src-tauri/src/agents/converter/mod.rs'
with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

target = '''    fn supported_actions(&self) -> Vec<String> {
        crate::agent_actions![
            "learn_skill_from_github",
            "learn_skill_from_docs",
            "analyze_and_convert_zip_skill",
            "package_as_mcp_server",
            "install_mcp_server",
            "list_mcp_servers",
            "call_mcp_tool",
            "edit" // WorkflowEdit action string
        ]
    }'''

replacement = '''    fn supported_actions(&self) -> Vec<String> {
        crate::agent_actions![
            "learn_skill_from_github",
            "learn_skill_from_docs",
            "analyze_and_convert_zip_skill",
            "package_as_mcp_server",
            "install_mcp_server",
            "list_mcp_servers",
            "call_mcp_tool",
            "edit" // WorkflowEdit action string
        ]
    }

    fn tool_schemas(&self) -> Vec<crate::mcp::McpTool> {
        vec![
            crate::mcp::McpTool {
                name: "analyze_and_convert_zip_skill".to_string(),
                description: "Phân tích, convert và cài đặt một skill từ file nén ZIP. Tham số: `zip_path`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "zip_path": { "type": "string" }
                    },
                    "required": ["zip_path"]
                }),
            },
            crate::mcp::McpTool {
                name: "learn_skill_from_docs".to_string(),
                description: "Học và sinh skill từ tài liệu API/hướng dẫn. Tham số: `url`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"]
                }),
            },
            crate::mcp::McpTool {
                name: "edit".to_string(),
                description: "Tạo hoặc sửa đổi một quy trình làm việc (Workflow). Tham số: `workflow_desc`.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workflow_desc": { "type": "string" }
                    },
                    "required": ["workflow_desc"]
                }),
            }
        ]
    }'''

if target in content:
    content = content.replace(target, replacement)
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print('Replaced converter')
else:
    print('Target not found in converter')
