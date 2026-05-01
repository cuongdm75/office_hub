use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::orchestrator::plan::{PlanExecution, PlanExecutionMode, FailurePolicy, PlanStatus};

/// Events that can happen during plan execution.
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    TaskCompleted(String, String), // task_id, output
    TaskFailed(String, String),    // task_id, error_message
    TaskTimeout(String),           // task_id
}

/// Decisions made by the PlanMonitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorDecision {
    /// Proceed normally.
    Continue,
    /// Retry the task.
    RetryTask(String),
    /// Skip the task (only in BestEffort mode).
    SkipTask(String),
    /// Pause the plan for Human-in-the-Loop review.
    PausePlan(String), // reason
    /// Cancel the entire plan immediately.
    CancelPlan(String), // reason
    /// Call the LLM again to re-plan from the current state.
    RequestReplan(String), // reason
}

/// A rule-based monitor that detects deviations during plan execution.
pub struct PlanMonitor {
    consecutive_failures: usize,
    max_consecutive_failures: usize,
}

impl PlanMonitor {
    pub fn new() -> Self {
        Self {
            consecutive_failures: 0,
            max_consecutive_failures: 2, // Arbitrary limit, could be configured
        }
    }

    /// Check the event against the current plan state and decide the next action.
    pub async fn check(&mut self, plan_exec: &PlanExecution, event: MonitorEvent) -> MonitorDecision {
        match event {
            MonitorEvent::TaskCompleted(task_id, _output) => {
                // Reset consecutive failures on success
                self.consecutive_failures = 0;
                info!("PlanMonitor: Task {} completed successfully.", task_id);
                MonitorDecision::Continue
            }
            MonitorEvent::TaskFailed(task_id, error) => {
                self.consecutive_failures += 1;
                warn!("PlanMonitor: Task {} failed: {}", task_id, error);
                
                if let Some(task) = plan_exec.plan.subtasks.iter().find(|t| t.task_id == task_id) {
                    if self.consecutive_failures >= self.max_consecutive_failures {
                        return MonitorDecision::PausePlan(format!(
                            "Too many consecutive failures ({}). Halting for review.", 
                            self.consecutive_failures
                        ));
                    }

                    match task.on_failure {
                        FailurePolicy::Continue => {
                            if plan_exec.plan.mode == PlanExecutionMode::BestEffort {
                                MonitorDecision::SkipTask(task_id.clone())
                            } else {
                                MonitorDecision::CancelPlan(format!(
                                    "Task {} failed and mode is not BestEffort.", task_id
                                ))
                            }
                        }
                        FailurePolicy::Retry(_retries_allowed) => {
                            // Ideally, keep track of how many retries we've done for *this* task.
                            // For MVP, we'll just attempt a retry. The runner must manage the retry count state.
                            MonitorDecision::RetryTask(task_id.clone())
                        }
                        FailurePolicy::StopPlan => {
                            MonitorDecision::CancelPlan(format!("Task {} failed and its policy is StopPlan.", task_id))
                        }
                    }
                } else {
                    MonitorDecision::CancelPlan(format!("Unknown task failed: {}", task_id))
                }
            }
            MonitorEvent::TaskTimeout(task_id) => {
                warn!("PlanMonitor: Task {} timed out.", task_id);
                if let Some(task) = plan_exec.plan.subtasks.iter().find(|t| t.task_id == task_id) {
                    match task.on_failure {
                        FailurePolicy::Retry(_) => MonitorDecision::RetryTask(task_id.clone()),
                        FailurePolicy::Continue if plan_exec.plan.mode == PlanExecutionMode::BestEffort => MonitorDecision::SkipTask(task_id.clone()),
                        _ => MonitorDecision::CancelPlan(format!("Task {} timed out.", task_id)),
                    }
                } else {
                    MonitorDecision::CancelPlan(format!("Unknown task timed out: {}", task_id))
                }
            }
        }
    }
}
