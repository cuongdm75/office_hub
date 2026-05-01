use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::workflow::{Trigger, TriggerEvent, TriggerType, WorkflowError, WorkflowResult};

pub struct ScheduleTrigger;

#[async_trait]
impl Trigger for ScheduleTrigger {
    fn trigger_type(&self) -> TriggerType {
        TriggerType::Schedule
    }

    async fn start(
        &self,
        workflow_id: String,
        config: serde_json::Value,
        tx: mpsc::Sender<TriggerEvent>,
        cancel_token: CancellationToken,
    ) -> WorkflowResult<()> {
        let cron_str = config
            .get("cron")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                WorkflowError::TriggerError("Schedule trigger requires 'cron' parameter".into())
            })?
            .to_string();

        info!(
            workflow_id = %workflow_id,
            cron = %cron_str,
            "Starting ScheduleTrigger"
        );

        let mut sched = JobScheduler::new().await.map_err(|e| {
            WorkflowError::TriggerError(format!("Failed to create scheduler: {}", e))
        })?;

        let wf_id_clone = workflow_id.clone();
        let job = Job::new_async(cron_str.as_str(), move |_uuid, mut _l| {
            let tx = tx.clone();
            let wf_id = wf_id_clone.clone();
            Box::pin(async move {
                info!(workflow_id = %wf_id, "ScheduleTrigger fired");

                let trigger_event = TriggerEvent {
                    workflow_id: wf_id.clone(),
                    trigger_type: TriggerType::Schedule,
                    data: serde_json::json!({
                        "triggered_at": chrono::Utc::now().to_rfc3339()
                    }),
                    fired_at: chrono::Utc::now(),
                };

                if let Err(e) = tx.send(trigger_event).await {
                    error!("ScheduleTrigger failed to send event: {}", e);
                }
            })
        })
        .map_err(|e| WorkflowError::TriggerError(format!("Invalid cron expression: {}", e)))?;

        sched
            .add(job)
            .await
            .map_err(|e| WorkflowError::TriggerError(format!("Failed to add job: {}", e)))?;

        sched.start().await.map_err(|e| {
            WorkflowError::TriggerError(format!("Failed to start scheduler: {}", e))
        })?;

        // Background task to wait for cancellation and shutdown the scheduler
        tokio::spawn(async move {
            cancel_token.cancelled().await;
            info!(workflow_id = %workflow_id, "ScheduleTrigger cancelled");
            if let Err(e) = sched.shutdown().await {
                error!("Failed to shutdown scheduler: {}", e);
            }
        });

        Ok(())
    }

    async fn stop(&self) -> WorkflowResult<()> {
        Ok(())
    }
}
