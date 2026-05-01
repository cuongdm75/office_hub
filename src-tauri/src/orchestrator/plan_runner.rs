use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use chrono::Utc;
use serde_json::Value;

use super::plan::{PlanExecution, SubTaskStatus, SubTaskResult, PlanStatus};
use super::plan_monitor::{PlanMonitor, MonitorEvent, MonitorDecision};
use crate::agents::{AgentRegistry, AgentId};
use crate::mcp::broker::McpBroker;
use crate::orchestrator::{AgentTask, intent::Intent, HitlManager, HitlRequestBuilder};
use crate::llm_gateway::LlmGateway;

pub struct PlanRunner {
    agent_registry: Arc<AgentRegistry>,
    mcp_broker: Arc<McpBroker>,
    hitl_manager: Arc<HitlManager>,
    llm_gateway: Arc<RwLock<LlmGateway>>,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

impl PlanRunner {
    pub fn new(
        agent_registry: Arc<AgentRegistry>,
        mcp_broker: Arc<McpBroker>,
        hitl_manager: Arc<HitlManager>,
        llm_gateway: Arc<RwLock<LlmGateway>>,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Self {
        Self {
            agent_registry,
            mcp_broker,
            hitl_manager,
            llm_gateway,
            progress_tx,
        }
    }

    /// Execute the plan following the DAG.
    pub async fn run(&self, plan_exec: Arc<PlanExecution>, session_id: &str) {
        let mut monitor = PlanMonitor::new();
        
        *plan_exec.status.write().await = PlanStatus::Running;
        
        // Initialize all results as Pending
        for task in &plan_exec.plan.subtasks {
            plan_exec.results.insert(task.task_id.clone(), SubTaskResult {
                task_id: task.task_id.clone(),
                status: SubTaskStatus::Pending,
                output: None,
                error: None,
                started_at: None,
                finished_at: None,
                tokens_used: 0,
            });
        }

        let mut rx = plan_exec.cancel_tx.subscribe();
        
        loop {
            // Check cancellation
            if *rx.borrow() {
                *plan_exec.status.write().await = PlanStatus::Cancelled;
                self.send_progress("Plan execution cancelled by system or user.");
                break;
            }

            let mut pending_tasks = Vec::new();
            let mut running_count = 0;
            let mut completed_count = 0;
            let mut failed_count = 0;
            let total_tasks = plan_exec.plan.subtasks.len();

            // Analyze current state
            for task in &plan_exec.plan.subtasks {
                let result = plan_exec.results.get(&task.task_id).unwrap();
                match result.status {
                    SubTaskStatus::Pending => pending_tasks.push(task.clone()),
                    SubTaskStatus::Running => running_count += 1,
                    SubTaskStatus::Success | SubTaskStatus::Skipped => completed_count += 1,
                    SubTaskStatus::Failed => failed_count += 1,
                    SubTaskStatus::Cancelled => failed_count += 1,
                }
            }

            // Exit condition
            if completed_count + failed_count == total_tasks {
                let final_status = if failed_count > 0 {
                    PlanStatus::Failed
                } else {
                    PlanStatus::Completed
                };
                *plan_exec.status.write().await = final_status;
                break;
            }

            // Find ready tasks
            let mut ready_tasks = Vec::new();
            for task in pending_tasks {
                let mut deps_met = true;
                for dep in &task.depends_on {
                    if let Some(dep_result) = plan_exec.results.get(dep) {
                        if dep_result.status != SubTaskStatus::Success && dep_result.status != SubTaskStatus::Skipped {
                            deps_met = false;
                            break;
                        }
                    } else {
                        deps_met = false;
                    }
                }
                if deps_met {
                    ready_tasks.push(task);
                }
            }

            // If we have nothing running and nothing ready but we haven't finished,
            // we have a deadlock or unfulfilled dependencies.
            if ready_tasks.is_empty() && running_count == 0 {
                error!("Plan execution deadlocked. Unfulfilled dependencies.");
                *plan_exec.status.write().await = PlanStatus::Failed;
                break;
            }

            // Spawn ready tasks concurrently
            for task in ready_tasks {
                // Update status to Running immediately so the next loop iteration doesn't pick it up again
                if let Some(mut result) = plan_exec.results.get_mut(&task.task_id) {
                    result.status = SubTaskStatus::Running;
                    result.started_at = Some(Utc::now());
                }

                self.send_progress(&format!("▶ Starting task '{}': {}", task.task_id, task.description));

                // Prepare clones for the spawned task
                let agent_registry = Arc::clone(&self.agent_registry);
                let mcp_broker = Arc::clone(&self.mcp_broker);
                let hitl_manager = Arc::clone(&self.hitl_manager);
                let llm_gateway = Arc::clone(&self.llm_gateway);
                let progress_tx = self.progress_tx.clone();
                let plan_exec = Arc::clone(&plan_exec);
                let session_id_clone = session_id.to_string();

                tokio::spawn(async move {
                    let mut monitor = PlanMonitor::new();
                    
                    let send_prog = |msg: &str| {
                        if let Some(tx) = &progress_tx {
                            let _ = tx.send(msg.to_string());
                        }
                    };

                    let handle_decision = |decision: MonitorDecision| {
                        match decision {
                            MonitorDecision::Continue => {}
                            MonitorDecision::CancelPlan(reason) => {
                                send_prog(&format!("🛑 Plan cancelled: {}", reason));
                                let _ = plan_exec.cancel_tx.send(true);
                            }
                            MonitorDecision::PausePlan(reason) => {
                                send_prog(&format!("⏸️ Plan paused: {}", reason));
                                let _ = plan_exec.cancel_tx.send(true);
                            }
                            MonitorDecision::SkipTask(tid) => {
                                if let Some(mut result) = plan_exec.results.get_mut(&tid) {
                                    result.status = SubTaskStatus::Skipped;
                                    result.finished_at = Some(Utc::now());
                                }
                            }
                            MonitorDecision::RetryTask(tid) => {
                                if let Some(mut result) = plan_exec.results.get_mut(&tid) {
                                    result.status = SubTaskStatus::Pending;
                                    result.error = None;
                                }
                            }
                            MonitorDecision::RequestReplan(_) => {
                                let _ = plan_exec.cancel_tx.send(true);
                            }
                        }
                    };

                    // Process HITL if needed
                    if task.risk_level != crate::orchestrator::HitlRiskLevel::Low {
                        send_prog(&format!("⚠️ Task '{}' requires approval.", task.task_id));
                        let (_action_id, rx) = hitl_manager.register(HitlRequestBuilder {
                            description: task.description.clone(),
                            risk_level: task.risk_level.clone(),
                            payload: Some(task.parameters.clone()),
                        });

                        *plan_exec.status.write().await = PlanStatus::PausedForHitl;
                        
                        // Wait for HITL
                        if let Ok(approved) = rx.await {
                            if !approved {
                                send_prog(&format!("❌ Task '{}' was rejected.", task.task_id));
                                if let Some(mut result) = plan_exec.results.get_mut(&task.task_id) {
                                    result.status = SubTaskStatus::Failed;
                                    result.error = Some("Rejected by user".to_string());
                                    result.finished_at = Some(Utc::now());
                                }
                                *plan_exec.status.write().await = PlanStatus::Running;
                                let _ = plan_exec.cancel_tx.send(true); // Stop plan on rejection
                                return;
                            }
                        } else {
                            // Channel closed unexpectedly
                            if let Some(mut result) = plan_exec.results.get_mut(&task.task_id) {
                                result.status = SubTaskStatus::Failed;
                                result.error = Some("HITL approval timeout/error".to_string());
                                result.finished_at = Some(Utc::now());
                            }
                            *plan_exec.status.write().await = PlanStatus::Running;
                            return;
                        }
                        *plan_exec.status.write().await = PlanStatus::Running;
                    }

                    // Execute task
                    let _start_time = std::time::Instant::now();
                    let task_id = task.task_id.clone();
                    let action = task.action.clone();
                    let agent_id_str = task.agent_id.clone();
                    let mut params = task.parameters.clone();

                    let alias = crate::mcp::get_tool_alias(&action);
                    send_prog(&format!("JSON:{{\"type\":\"task_status\",\"agent\":\"{}\",\"status\":\"running\",\"message\":\"{}\"}}", agent_id_str, alias));

                    // Inject outputs from dependencies into parameters
                    if let Some(obj) = params.as_object_mut() {
                        let mut injected_outputs = serde_json::Map::new();
                        for dep in &task.depends_on {
                            if let Some(dep_res) = plan_exec.results.get(dep) {
                                if let Some(out) = &dep_res.output {
                                    injected_outputs.insert(dep.clone(), Value::String(out.clone()));
                                }
                            }
                        }
                        if !injected_outputs.is_empty() {
                            obj.insert("__dependencies".to_string(), Value::Object(injected_outputs));
                        }
                    }

                    let mut error_msg = None;
                    let mut output_msg = None;
                    let mut tokens_used = 0;

                    // 1. Try Agent Registry first
                    let agent_id = AgentId(agent_id_str.clone());
                    if let Some(agent_arc) = agent_registry.get_mut(&agent_id) {
                        let mut agent_guard = agent_arc.write().await;
                        let agent_task = AgentTask {
                            task_id: task_id.clone(),
                            action: action.clone(),
                            intent: Intent::Ambiguous(Default::default()),
                            message: "".to_string(),
                            context_file: None,
                            session_id: session_id_clone.clone(),
                            parameters: params.as_object().cloned().unwrap_or_default().into_iter().collect(),
                            llm_gateway: Some(llm_gateway.clone()),
                            global_policy: None,
                            knowledge_context: None,
                            parent_task_id: Some(plan_exec.plan.plan_id.clone()),
                            dependencies: task.depends_on.clone(),
                        };

                        match agent_guard.execute(agent_task).await {
                            Ok(output) => {
                                output_msg = Some(output.content);
                                tokens_used = output.tokens_used.unwrap_or(0) as usize;
                            }
                            Err(e) => {
                                error_msg = Some(e.to_string());
                            }
                        }
                    } else {
                        // 2. Try MCP Broker
                        let tool_name = action.clone(); // The action is the MCP tool name
                        match mcp_broker.call_tool(&tool_name, Some(params)).await {
                            Ok(result) => {
                                let mut text_buf = String::new();
                                for item in result.content {
                                    if item.content_type == "text" {
                                        if let Some(t) = item.text {
                                            text_buf.push_str(&t);
                                            text_buf.push('\n');
                                        }
                                    }
                                }
                                if result.is_error {
                                    error_msg = Some(text_buf.trim().to_string());
                                } else {
                                    output_msg = Some(text_buf.trim().to_string());
                                }
                            }
                            Err(e) => {
                                error_msg = Some(e.to_string());
                            }
                        }
                    }

                    // Update results
                    send_prog(&format!("JSON:{{\"type\":\"task_status\",\"agent\":\"{}\",\"status\":\"success\",\"message\":\"\"}}", agent_id_str));
                    if let Some(err) = error_msg.clone() {
                        if let Some(mut result) = plan_exec.results.get_mut(&task_id) {
                            result.status = SubTaskStatus::Failed;
                            result.error = Some(err.clone());
                            result.finished_at = Some(Utc::now());
                            result.tokens_used = tokens_used;
                        }
                        send_prog(&format!("❌ Task '{}' failed: {}", task_id, err));
                        
                        let decision = monitor.check(&plan_exec, MonitorEvent::TaskFailed(task_id.clone(), err)).await;
                        handle_decision(decision);
                    } else if let Some(out) = output_msg {
                        if let Some(mut result) = plan_exec.results.get_mut(&task_id) {
                            result.status = SubTaskStatus::Success;
                            result.output = Some(out.clone());
                            result.finished_at = Some(Utc::now());
                            result.tokens_used = tokens_used;
                        }
                        send_prog(&format!("✅ Task '{}' completed.", task_id));
                        
                        let decision = monitor.check(&plan_exec, MonitorEvent::TaskCompleted(task_id.clone(), out)).await;
                        handle_decision(decision);
                    }
                });
            }

            // Small delay to prevent tight loop if we implement concurrent spawns
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    fn send_progress(&self, msg: &str) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(msg.to_string());
        }
    }

    fn handle_decision(&self, decision: MonitorDecision, plan_exec: &PlanExecution) {
        match decision {
            MonitorDecision::Continue => {}
            MonitorDecision::CancelPlan(reason) => {
                self.send_progress(&format!("🛑 Plan cancelled: {}", reason));
                let _ = plan_exec.cancel_tx.send(true);
            }
            MonitorDecision::PausePlan(reason) => {
                self.send_progress(&format!("⏸️ Plan paused: {}", reason));
                // Real HITL pause requires waiting for approval. In MVP we just stop.
                let _ = plan_exec.cancel_tx.send(true);
            }
            MonitorDecision::SkipTask(task_id) => {
                if let Some(mut result) = plan_exec.results.get_mut(&task_id) {
                    result.status = SubTaskStatus::Skipped;
                    result.finished_at = Some(Utc::now());
                }
            }
            MonitorDecision::RetryTask(task_id) => {
                // Set back to Pending
                if let Some(mut result) = plan_exec.results.get_mut(&task_id) {
                    result.status = SubTaskStatus::Pending;
                    result.error = None;
                }
            }
            MonitorDecision::RequestReplan(_) => {
                // MVP: Cancel and let orchestrator handle it.
                let _ = plan_exec.cancel_tx.send(true);
            }
        }
    }
}
