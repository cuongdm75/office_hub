// ============================================================================
// Office Hub – agents/converter/mod.rs
//
// Converter Agent – MCP Skill Learning & Server Packaging
//
// Trách nhiệm:
//   1. Tự động học kỹ năng mới từ GitHub repos, Documentation, Scripts
//   2. Đóng gói kỹ năng thành MCP Server độc lập
//   3. Quản lý vòng đời các MCP Server do agent tạo ra
//   4. Cung cấp giao diện để Orchestrator gọi tools qua MCP protocol
//
// Status: STUB – sẽ được implement đầy đủ trong Phase 7
// ============================================================================

use async_trait::async_trait;

use crate::agents::{Agent, AgentId, AgentStatus};
use crate::orchestrator::{AgentOutput, AgentTask};

// ─────────────────────────────────────────────────────────────────────────────
// ConverterAgent
// ─────────────────────────────────────────────────────────────────────────────

pub struct ConverterAgent {
    id: AgentId,
    status: AgentStatus,
}

impl ConverterAgent {
    pub fn new() -> Self {
        Self {
            id: AgentId::converter(),
            status: AgentStatus::Idle,
        }
    }

    /// Helper để parse các khối code (multi-file) trả về từ LLM
    fn parse_multi_file_output(response: &str) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let mut current_pos = 0;

        while let Some(start_idx) = response[current_pos..].find("```") {
            let abs_start = current_pos + start_idx;
            let line_end = response[abs_start..]
                .find('\n')
                .map(|i| abs_start + i)
                .unwrap_or(abs_start + 3);

            let header = &response[abs_start + 3..line_end];
            let mut path = None;

            if let Some(path_idx) = header.find("path=") {
                path = Some(header[path_idx + 5..].trim().to_string());
            } else if let Some(path_idx) = header.find("path:") {
                path = Some(header[path_idx + 5..].trim().to_string());
            }

            let content_start = line_end + 1;
            if let Some(end_idx) = response[content_start..].find("```") {
                let abs_end = content_start + end_idx;
                let content = &response[content_start..abs_end];

                let mut final_content = content.trim_start();
                let mut final_path = path;

                if final_path.is_none() {
                    if let Some(first_line_end) = final_content.find('\n') {
                        let first_line = &final_content[..first_line_end];
                        if first_line.contains("path:") || first_line.contains("path=") {
                            let path_str = if let Some(idx) = first_line.find("path:") {
                                &first_line[idx + 5..]
                            } else if let Some(idx) = first_line.find("path=") {
                                &first_line[idx + 5..]
                            } else {
                                ""
                            };
                            final_path = Some(
                                path_str
                                    .replace("-->", "")
                                    .replace("*/", "")
                                    .replace("<!--", "")
                                    .trim()
                                    .to_string(),
                            );
                            final_content = &final_content[first_line_end + 1..];
                        }
                    }
                }

                if let Some(p) = final_path {
                    files.push((p, final_content.trim().to_string()));
                } else {
                    files.push((
                        format!("unnamed_{}.md", files.len()),
                        final_content.trim().to_string(),
                    ));
                }

                current_pos = abs_end + 3;
            } else {
                break;
            }
        }

        if files.is_empty() {
            files.push(("unnamed_0.md".to_string(), response.trim().to_string()));
        }

        files
    }
}

impl Default for ConverterAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for ConverterAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn name(&self) -> &str {
        "Converter Agent (MCP Skill Builder & Workflow Editor)"
    }

    fn description(&self) -> &str {
        "Tự động học kỹ năng mới, sinh mã Rhai native, và tạo Workflow definitions. Dùng action 'analyze_and_convert_zip_skill' với tham số 'zip_path' để tự động phân tích, convert và cài đặt một skill từ file nén."
    }

    fn supported_actions(&self) -> Vec<String> {
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
                description:
                    "Phân tích, convert và cài đặt một skill từ file nén ZIP. Tham số: `zip_path`."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "zip_path": { "type": "string" }
                    },
                    "required": ["zip_path"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "learn_skill_from_docs".to_string(),
                description: "Học và sinh skill từ tài liệu API/hướng dẫn. Tham số: `url`."
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"]
                }),
                tags: vec![],
            },
            crate::mcp::McpTool {
                name: "edit".to_string(),
                description:
                    "Tạo hoặc sửa đổi một quy trình làm việc (Workflow). Tham số: `workflow_desc`."
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workflow_desc": { "type": "string" }
                    },
                    "required": ["workflow_desc"]
                }),
                tags: vec![],
            },
        ]
    }

    fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn execute(&mut self, task: AgentTask) -> anyhow::Result<AgentOutput> {
        self.status = AgentStatus::Busy;
        let result = match task.action.as_str() {
            "edit" => {
                let llm_arc = task
                    .llm_gateway
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("LLM Gateway missing"))?;
                let workflow_desc = task.message.as_str();

                let prompt = format!(
                    "Bạn là một chuyên gia phân tích và xây dựng Declarative Workflows & Skills cho hệ thống AI.\n\
                    Người dùng yêu cầu tạo/sửa một Workflow: {}\n\n\
                    [QUAN TRỌNG: WORKFLOW SELF-EVOLUTION & TOOL GENERATION]\n\
                    Khi sinh ra Workflow này, bạn phải phân tích các bước (steps) xem nó có cần tool đặc thù nào không. Nếu hệ thống cơ bản không đủ (ví dụ cần giải nén, ghi đè file đặc thù, lấy thông tin phần cứng...), BẠN BẮT BUỘC PHẢI TỰ ĐỘNG sinh thêm mã Kỹ năng (SKILL.md kèm Rhai script) để hỗ trợ workflow đó.\n\n\
                    Lưu ý về Rhai API:\n\
                    - `cmd(string) -> string`: Chạy lệnh OS.\n\
                    - `read_file(string) -> string`, `write_file(string, string) -> bool`.\n\n\
                    BẠN PHẢI TRẢ VỀ DƯỚI DẠNG MULTI-FILE CODE BLOCKS. Dòng đầu tiên của mỗi khối code (bên trong block hoặc trên dòng ```) phải chỉ định đường dẫn lưu file theo cú pháp `path: <đường_dẫn>`.\n\
                    Ví dụ:\n\
                    ```yaml\n\
                    # path: .agent/workflows/my_workflow.yaml\n\
                    id: my_workflow\n\
                    name: My Workflow\n\
                    steps: []\n\
                    ```\n\
                    ```markdown\n\
                    <!-- path: .agent/skills/my_new_tool/SKILL.md -->\n\
                    ---\n\
                    name: my-new-tool\n\
                    ---\n\
                    # Hướng dẫn\n\
                    ```\n\
                    ",
                    workflow_desc
                );

                let req = crate::llm_gateway::LlmRequest::new(vec![
                    crate::llm_gateway::LlmMessage::user(prompt),
                ]);
                let llm = llm_arc.read().await;
                let resp = llm.complete(req).await?;

                let parsed_files = Self::parse_multi_file_output(&resp.content);

                let mut base_dir = std::env::current_dir().unwrap_or_default();
                if base_dir.ends_with("src-tauri") {
                    base_dir = base_dir.parent().unwrap().to_path_buf();
                }

                let mut saved_paths = Vec::new();
                for (mut file_path, file_content) in parsed_files {
                    if file_path.starts_with("unnamed_") {
                        // Mặc định ném vào workflow nếu LLM quên path
                        file_path =
                            format!(".agent/workflows/{}", file_path.replace(".md", ".yaml"));
                    }
                    let full_path = base_dir.join(&file_path);
                    if let Some(parent) = full_path.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    tokio::fs::write(&full_path, file_content).await?;
                    saved_paths.push(file_path);
                }

                Ok(AgentOutput {
                    content: format!(
                        "Đã tạo thành công {} file cấu hình:\n- {}",
                        saved_paths.len(),
                        saved_paths.join("\n- ")
                    ),
                    committed: true,
                    tokens_used: Some(resp.usage.total_tokens),
                    metadata: Some(serde_json::json!({
                        "generated_files": saved_paths
                    })),
                })
            }
            "analyze_and_convert_zip_skill" => {
                let zip_path = task
                    .parameters
                    .get("zip_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if zip_path.is_empty() {
                    return Ok(AgentOutput {
                        content: "Thiếu tham số 'zip_path' (đường dẫn file zip).".to_string(),
                        committed: false,
                        tokens_used: None,
                        metadata: None,
                    });
                }

                let temp_dir = std::env::temp_dir()
                    .join(format!("office_hub_temp_zip_{}", uuid::Uuid::new_v4()));
                let _ = tokio::fs::create_dir_all(&temp_dir).await;

                let cmd = format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    zip_path,
                    temp_dir.display()
                );
                let output = std::process::Command::new("powershell")
                    .arg("-NoProfile")
                    .arg("-Command")
                    .arg(&cmd)
                    .output();

                match output {
                    Ok(out) if out.status.success() => {
                        let mut aggregated = String::new();
                        // Hàm helper đọc đệ quy
                        async fn read_dir_recursive(
                            dir: &std::path::Path,
                            base: &std::path::Path,
                            out: &mut String,
                        ) {
                            if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
                                while let Ok(Some(entry)) = entries.next_entry().await {
                                    let path = entry.path();
                                    if path.is_file() {
                                        let ext = path
                                            .extension()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        if ["md", "rhai", "yaml", "json", "txt", "js", "ts", "py"]
                                            .contains(&ext.as_str())
                                        {
                                            if let Ok(content) = std::fs::read_to_string(&path) {
                                                let rel_path = path
                                                    .strip_prefix(base)
                                                    .unwrap_or(&path)
                                                    .to_string_lossy();
                                                out.push_str(&format!(
                                                    "=== {} ===\n{}\n\n",
                                                    rel_path, content
                                                ));
                                            }
                                        }
                                    } else if path.is_dir() {
                                        Box::pin(read_dir_recursive(&path, base, out)).await;
                                    }
                                }
                            }
                        }

                        read_dir_recursive(&temp_dir, &temp_dir, &mut aggregated).await;
                        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

                        if aggregated.is_empty() {
                            return Ok(AgentOutput {
                                content:
                                    "Không tìm thấy file text/code nào trong file zip để phân tích."
                                        .to_string(),
                                committed: false,
                                tokens_used: None,
                                metadata: None,
                            });
                        }

                        let llm_arc = task
                            .llm_gateway
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("LLM Gateway missing"))?;
                        let prompt = format!(
                            "Bạn là chuyên gia phân tích và tích hợp Skill cho hệ thống Office Hub.\n\
                            Người dùng vừa cung cấp một mã nguồn Kỹ năng (Skill) được lấy từ nguồn bên ngoài dưới dạng file nén. Dưới đây là nội dung giải nén được:\n\n\
                            {}\n\n\
                            NHIỆM VỤ CỦA BẠN:\n\
                            1. Không được chép y nguyên tài liệu hướng dẫn cũ. Hãy phân tích xem nó làm gì.\n\
                            2. Convert nội dung skill này cho tương thích với chuẩn của Office Hub, sử dụng các công cụ nội bộ (như file system, win32 shell, powershell, rhai script) thay vì các công cụ mông lung của nguồn cũ.\n\
                            3. Phân tích xem skill này khi chạy trên Office Hub có thiếu dependencies nào không (VD: cần Python, cần thư viện X, cần module Y). Hãy liệt kê rõ các khoảng trống (gaps) này trong câu trả lời văn bản thường để người dùng biết.\n\
                            4. ĐỊNH DẠNG ĐẦU RA: Bạn BẮT BUỘC trả về các file cấu hình bằng các khối code (code blocks), có chú thích đường dẫn. Ví dụ:\n\
                            ```markdown\n\
                            <!-- path: .agent/skills/ten_skill/SKILL.md -->\n\
                            ...\n\
                            ```\n\
                            Và nếu có script đi kèm:\n\
                            ```rhai\n\
                            // path: .agent/skills/ten_skill/scripts/script_cua_toi.rhai\n\
                            ...\n\
                            ```\n\
                            Hãy đưa ra báo cáo phân tích dependencies trước, sau đó là các khối mã code được convert.",
                            aggregated
                        );

                        let req = crate::llm_gateway::LlmRequest::new(vec![
                            crate::llm_gateway::LlmMessage::user(prompt),
                        ]);
                        let llm = llm_arc.read().await;
                        let resp = llm.complete(req).await?;

                        let parsed_files = Self::parse_multi_file_output(&resp.content);

                        let mut base_dir = std::env::current_dir().unwrap_or_default();
                        if base_dir.ends_with("src-tauri") {
                            base_dir = base_dir.parent().unwrap().to_path_buf();
                        }

                        let mut saved_paths = Vec::new();
                        for (mut file_path, file_content) in parsed_files {
                            if file_path.starts_with("unnamed_") {
                                file_path = format!(".agent/skills/imported_skill/{}", file_path);
                            }
                            let full_path = base_dir.join(&file_path);
                            if let Some(parent) = full_path.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }
                            if let Ok(_) = tokio::fs::write(&full_path, file_content).await {
                                saved_paths.push(file_path);
                            }
                        }

                        // Lọc bỏ các đoạn code block trong response để trả về report thuần túy
                        let report_text = resp
                            .content
                            .split("```")
                            .step_by(2)
                            .collect::<Vec<&str>>()
                            .join("\n")
                            .trim()
                            .to_string();

                        Ok(AgentOutput {
                            content: format!("Đã phân tích và cài đặt skill mới.\n\n[BÁO CÁO PHÂN TÍCH VÀ DEPENDENCIES]\n{}\n\n[CÁC FILE ĐÃ TẠO]\n- {}", report_text, saved_paths.join("\n- ")),
                            committed: true, // Kết thúc tại đây, Orchestrator sẽ in ra content này
                            tokens_used: Some(resp.usage.total_tokens),
                            metadata: Some(serde_json::json!({
                                "generated_files": saved_paths
                            })),
                        })
                    }
                    Ok(out) => {
                        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                        let err_msg = String::from_utf8_lossy(&out.stderr);
                        Ok(AgentOutput {
                            content: format!("Lỗi khi giải nén:\n{}", err_msg),
                            committed: false,
                            tokens_used: None,
                            metadata: None,
                        })
                    }
                    Err(e) => {
                        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                        Ok(AgentOutput {
                            content: format!("Không thể chạy lệnh giải nén: {}", e),
                            committed: false,
                            tokens_used: None,
                            metadata: None,
                        })
                    }
                }
            }
            "learn_skill_from_docs" | "learn_skill_from_github" => {
                let url = task
                    .parameters
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let llm_arc = task
                    .llm_gateway
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("LLM Gateway missing"))?;

                let docs_content = if !url.is_empty() {
                    match reqwest::get(url).await {
                        Ok(resp) => resp
                            .text()
                            .await
                            .unwrap_or_else(|_| format!("Không thể đọc nội dung từ {}", url)),
                        Err(e) => format!("Lỗi tải trang: {}", e),
                    }
                } else {
                    "Không có tài liệu nào được cung cấp.".to_string()
                };

                let prompt = format!(
                    "Bạn là một chuyên gia phân tích và xây dựng Declarative Skills cho hệ thống AI. Nhiệm vụ của bạn là đọc tài liệu API/hướng dẫn sau và sinh ra một file Markdown (.md) định nghĩa Kỹ năng (Skill). \
                    Hệ thống này xử lý logic bằng YAML Frontmatter và Markdown Prompt. \
                    [QUAN TRỌNG: TỰ ĐỘNG TẠO TOOL BẰNG RHAI SCRIPT]\n\
                    Bạn phải phân tích xem Skill này có cần chức năng đặc thù nào không. Nếu hệ thống cơ bản (gọi HTTP, đọc ghi file) không đủ để giải quyết (VD: cần nén file zip, thao tác Registry, gọi app ngoài), BẠN BẮT BUỘC PHẢI TỰ ĐỘNG VIẾT một đoạn mã Rhai (.rhai) đính kèm bên trong file Markdown và hướng dẫn AI dùng tool `run_rhai_script` để thực thi đoạn mã đó.\n\
                    Lưu ý về Rhai API do hệ thống cung cấp:\n\
                    - `cmd(string) -> string`: Chạy một lệnh hệ điều hành (thông qua PowerShell ẩn) và trả về kết quả.\n\
                    - `read_file(string) -> string`: Đọc file.\n\
                    - `write_file(string, string) -> bool`: Ghi file.\n\
                    Chỉ trả về DUY NHẤT nội dung Markdown, không chứa giải thích thừa. \
                    BẮT BUỘC tuân thủ định dạng sau:\n\
                    ---\n\
                    name: <tên-skill-viet-lien-khong-dau>\n\
                    description: <mô tả ngắn gọn về chức năng>\n\
                    parameters:\n\
                      <tên_tham_số_1>: {{ type: string }}\n\
                      <tên_tham_số_2>: {{ type: number }}\n\
                    ---\n\n\
                    # Hướng dẫn xử lý (Prompt Logic)\n\
                    <Viết các bước suy luận logic để hệ thống AI thực thi kỹ năng này. Bạn có thể sử dụng các biến {{tên_tham_số}} để nội suy dữ liệu đầu vào.>\n\
                    <NẾU THIẾU TOOL NATIVE: Cung cấp khối ```rhai chứa mã script thực thi, và yêu cầu AI chạy nó bằng `run_rhai_script`>\n\n\
                    Tài liệu tham khảo:\n{}",
                    docs_content
                );

                let req = crate::llm_gateway::LlmRequest::new(vec![
                    crate::llm_gateway::LlmMessage::user(prompt),
                ]);
                let llm = llm_arc.read().await;
                let resp = llm.complete(req).await?;

                let parsed_files = Self::parse_multi_file_output(&resp.content);
                let (mut _file_path, mut code) = parsed_files
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| ("unnamed_skill.md".to_string(), resp.content.clone()));

                // Fallback nếu LLM lỗi
                if code.is_empty() {
                    code = "---\nname: fallback-skill\ndescription: Fallback generated skill\n---\n\n# Hướng dẫn\nKỹ năng này chưa được cấu hình đúng.".to_string();
                }

                // Parse tên skill từ frontmatter
                let mut skill_name = uuid::Uuid::new_v4().to_string();
                if let Some(start) = code.find("name: ") {
                    let rest = &code[start + 6..];
                    if let Some(end) = rest.find('\n') {
                        skill_name = rest[..end].trim().to_string();
                    }
                }

                // Lưu vào thư mục .agent/skills (Môi trường Native)
                let mut base_dir = std::env::current_dir().unwrap_or_default();
                if base_dir.ends_with("src-tauri") {
                    base_dir = base_dir.parent().unwrap().to_path_buf();
                }

                // Override path based on parsed skill_name
                let mut path = base_dir.join(".agent").join("skills").join(&skill_name);
                tokio::fs::create_dir_all(&path).await?;

                path.push("SKILL.md");
                tokio::fs::write(&path, &code).await?;

                Ok(AgentOutput {
                    content: format!(
                        "Đã tạo thành công Declarative Skill '{}' tại: {}",
                        skill_name,
                        path.display()
                    ),
                    committed: true,
                    tokens_used: Some(resp.usage.total_tokens),
                    metadata: Some(serde_json::json!({
                        "skill_id": skill_name,
                        "script_path": path.to_string_lossy().to_string(),
                        "script_content": code
                    })),
                })
            }
            _ => Ok(AgentOutput {
                content: format!(
                    "[Converter Agent] Action '{}' received but no native execution defined yet.",
                    task.action
                ),
                committed: false,
                tokens_used: None,
                metadata: None,
            }),
        };
        self.status = AgentStatus::Idle;
        result
    }
}
