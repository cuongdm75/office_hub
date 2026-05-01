use crate::orchestrator::{Orchestrator, OrchestratorResponse};
use anyhow::Result;
use chrono::Utc;
use tracing::{instrument, warn};
use std::sync::Arc;

impl Orchestrator {
    /// [Phase 1] Smart Planning Execution: Generates a DAG plan and executes it via Agent-to-Agent MCP.
    #[instrument(skip(self, message), fields(session = session_id))]
    pub async fn process_message_planned(
        &mut self,
        session_id: &str,
        message: &str,
        context_file: Option<&str>,
        workspace_id: Option<&str>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<OrchestratorResponse> {
        let started_at = Utc::now();
        self.metrics.total_requests += 1;

        if let Some(ref tx) = progress_tx {
            let _ = tx.send("🧠 Phân tích yêu cầu và lập kế hoạch...".to_string());
        }

        // Prepare conversation
        use crate::llm_gateway::genai_bridge::{ToolChatMessage, ToolAwareResponse};
        let mut conv_messages = {
            let session = self.session_store.get_or_create(session_id).ok_or_else(|| anyhow::anyhow!("Session not found"))?;
            
            let mut mcp_tools = self.mcp_broker.list_all_tools().await.unwrap_or_default();
            let agent_tools = self.agent_registry.all_tool_schemas_complete();
            mcp_tools.extend(agent_tools);

            let tools_schema_json = serde_json::to_string_pretty(&mcp_tools).unwrap_or_default();

            let system_prompt = format!(
                "Bạn là Office Hub Orchestrator. Nhiệm vụ của bạn là phân tích yêu cầu người dùng và tạo ra một bản Kế hoạch Thực thi (ExecutionPlan) dạng JSON.
                
Danh sách các công cụ (Agent & MCP Server) hiện có:
{}

Yêu cầu xuất JSON ĐÚNG THEO ĐỊNH DẠNG SAU, KHÔNG CÓ BẤT KỲ VĂN BẢN NÀO KHÁC BÊN NGOÀI:
{{
  \"thought\": \"Suy nghĩ của bạn về cách giải quyết\",
  \"plan\": {{
    \"plan_id\": \"unique_string\",
    \"goal\": \"Mô tả ngắn gọn mục tiêu\",
    \"mode\": \"best_effort\",
    \"timeout_ms\": 120000,
    \"subtasks\": [
      {{
        \"task_id\": \"t1\",
        \"description\": \"Đọc file excel\",
        \"agent_id\": \"analyst\",
        \"action\": \"analyze_workbook\",
        \"parameters\": {{ \"file\": \"data.xlsx\" }},
        \"depends_on\": [],
        \"risk_level\": \"low\",
        \"on_failure\": \"stop_plan\"
      }}
    ]
  }},
  \"direct_response\": null
}}

Lưu ý:
- task_id phải là chuỗi duy nhất.
- agent_id có thể là ID của agent hoặc MCP server.
- action là tên tool cần gọi.
- depends_on là mảng các task_id cần hoàn thành trước.
- risk_level có thể là: low, medium, high, critical.
- direct_response: Chỉ điền nội dung trả lời (chuỗi) vào đây nếu câu hỏi là trò chuyện thông thường và KHÔNG CẦN DÙNG BẤT KỲ TOOL NÀO. Nếu cần dùng tool, hãy để null.
",
                tools_schema_json
            );

            let mut msgs = vec![ToolChatMessage::System(system_prompt)];
            for m in session.messages.iter() {
                if m.role == crate::orchestrator::session::MessageRole::User {
                    msgs.push(ToolChatMessage::User(m.content.clone()));
                } else {
                    msgs.push(ToolChatMessage::Assistant(m.content.clone()));
                }
            }
            msgs.push(ToolChatMessage::User(message.to_string()));
            msgs
        };

        let bridge = {
            let llm = self.llm_gateway.read().await;
            llm.create_genai_bridge_reasoning().await
        };

        let response = bridge.complete_with_tools(&conv_messages, &[], 0.1).await?;
        let text_response = match response {
            ToolAwareResponse::Text(t) => t,
            ToolAwareResponse::ToolCalls(_) => "".to_string(),
        };

        // Parse JSON plan
        let json_start = text_response.find('{').unwrap_or(0);
        let json_end = text_response.rfind('}').unwrap_or(text_response.len() - 1) + 1;
        let json_str = &text_response[json_start..json_end];

        #[derive(serde::Deserialize)]
        struct PlanResponse {
            #[allow(dead_code)]
            thought: Option<String>,
            plan: Option<crate::orchestrator::plan::ExecutionPlan>,
            direct_response: Option<String>,
        }

        let parsed: PlanResponse = match serde_json::from_str(json_str) {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to parse plan JSON: {}. Falling back to native.", e);
                // Fallback to native processing if parsing fails
                return self.process_message_native(session_id, message, context_file, workspace_id, progress_tx).await;
            }
        };

        if let Some(direct) = parsed.direct_response {
            if !direct.is_empty() {
                if let Some(mut session) = self.session_store.get_mut(session_id) {
                    session.add_turn(
                        message.to_string(),
                        direct.clone(),
                        "chat".to_string(),
                        crate::agents::AgentId::custom("orchestrator"),
                    );
                }
                return Ok(OrchestratorResponse {
                    content: direct,
                    intent: Some("chat".to_string()),
                    agent_used: Some("orchestrator".to_string()),
                    tokens_used: None,
                    duration_ms: Utc::now().signed_duration_since(started_at).num_milliseconds() as u64,
                    metadata: None,
                });
            }
        }

        let plan = match parsed.plan {
            Some(p) => p,
            None => {
                return Ok(OrchestratorResponse {
                    content: "Không thể tạo kế hoạch thực thi.".to_string(),
                    intent: Some("error".to_string()),
                    agent_used: None,
                    tokens_used: None,
                    duration_ms: Utc::now().signed_duration_since(started_at).num_milliseconds() as u64,
                    metadata: None,
                });
            }
        };

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(format!("📋 Đã tạo kế hoạch: {}", plan.goal));
        }

        // 2. Execute Plan
        
        let plan_exec = Arc::new(crate::orchestrator::plan::PlanExecution::new(plan));
        let runner = crate::orchestrator::plan_runner::PlanRunner::new(
            Arc::new(self.agent_registry.clone()),
            self.mcp_broker.clone(),
            self.hitl_manager.clone(),
            self.llm_gateway.clone(),
            progress_tx.clone(),
        );

        runner.run(plan_exec.clone(), session_id).await;

        // 3. Synthesize final results
        if let Some(ref tx) = progress_tx {
            let _ = tx.send("📝 Đang tổng hợp kết quả...".to_string());
        }

        let mut results_summary = String::new();
        for entry in plan_exec.results.iter() {
            let res = entry.value();
            results_summary.push_str(&format!("\nTask {}: {:?}\n", res.task_id, res.status));
            if let Some(err) = &res.error {
                results_summary.push_str(&format!("  Lỗi: {}\n", err));
            }
            if let Some(out) = &res.output {
                results_summary.push_str(&format!("  Kết quả: {}\n", out));
            }
        }

        let synthesis_prompt = format!(
            "Dưới đây là kết quả thực thi các tác vụ theo kế hoạch:
{}
            
Dựa trên kết quả này, hãy trả lời câu hỏi/yêu cầu ban đầu của người dùng một cách rõ ràng và ngắn gọn.
Nếu có lỗi, hãy giải thích nguyên nhân.",
            results_summary
        );

        let synth_messages = vec![
            crate::llm_gateway::genai_bridge::ToolChatMessage::User(message.to_string()),
            crate::llm_gateway::genai_bridge::ToolChatMessage::User(synthesis_prompt),
        ];

        let bridge_synth = {
            let llm = self.llm_gateway.read().await;
            llm.create_genai_bridge_reasoning().await
        };

        let synth_resp = bridge_synth.complete_with_tools(&synth_messages, &[], 0.1).await?;
        let final_text = match synth_resp {
            crate::llm_gateway::genai_bridge::ToolAwareResponse::Text(t) => t,
            crate::llm_gateway::genai_bridge::ToolAwareResponse::ToolCalls(_) => "Hoàn tất.".to_string(),
        };

        if let Some(mut session) = self.session_store.get_mut(session_id) {
            session.add_turn(
                message.to_string(),
                final_text.clone(),
                "plan_execute".to_string(),
                crate::agents::AgentId::custom("orchestrator_planned"),
            );
        }

        Ok(OrchestratorResponse {
            content: final_text,
            intent: Some("plan_execute".to_string()),
            agent_used: Some("orchestrator_planned".to_string()),
            tokens_used: None,
            duration_ms: Utc::now().signed_duration_since(started_at).num_milliseconds() as u64,
            metadata: None,
        })
    }
}
