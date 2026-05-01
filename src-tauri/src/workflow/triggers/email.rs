use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace, warn};

use crate::workflow::{Trigger, TriggerEvent, TriggerType, WorkflowResult};

pub struct EmailTrigger;

#[async_trait]
impl Trigger for EmailTrigger {
    fn trigger_type(&self) -> TriggerType {
        TriggerType::EmailReceived
    }

    async fn start(
        &self,
        workflow_id: String,
        config: Value,
        tx: mpsc::Sender<TriggerEvent>,
        cancel_token: CancellationToken,
    ) -> WorkflowResult<()> {
        let subject_filter = config
            .get("subject_filter")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        let sender_filter = config
            .get("sender_filter")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        let poll_interval_sec = config
            .get("poll_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        info!(
            workflow_id = %workflow_id,
            poll_interval = %poll_interval_sec,
            "Starting EmailTrigger (Graph API Polling)"
        );

        let client = Client::new();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        info!(workflow_id = %workflow_id, "EmailTrigger cancelled");
                        break;
                    }
                    _ = sleep(Duration::from_secs(poll_interval_sec)) => {
                        if let Err(e) = poll_inbox(&client, &workflow_id, &subject_filter, &sender_filter, &tx).await {
                            error!("EmailTrigger polling error: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(&self) -> WorkflowResult<()> {
        Ok(())
    }
}

async fn poll_inbox(
    client: &Client,
    workflow_id: &str,
    subject_filter: &str,
    sender_filter: &str,
    tx: &mpsc::Sender<TriggerEvent>,
) -> anyhow::Result<()> {
    // 1. Read token from cache
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("office-hub");
    path.push("msgraph_token.json");

    let data = std::fs::read_to_string(&path).unwrap_or_default();
    let cache: Value = serde_json::from_str(&data).unwrap_or_default();

    let token = match cache["access_token"].as_str() {
        Some(t) => t,
        None => {
            trace!("No Graph API token found. Skipping email polling.");
            return Ok(()); // Token doesn't exist, user hasn't authenticated yet
        }
    };

    let expires_at = cache["expires_at"].as_i64().unwrap_or(0);
    let now = Utc::now().timestamp();
    if now > expires_at - 60 {
        trace!("Graph API token expired. Skipping email polling until refreshed by OutlookAgent.");
        return Ok(());
    }

    // 2. Call MS Graph API to get unread messages
    let url = "https://graph.microsoft.com/v1.0/me/messages?$filter=isRead eq false";
    let res = client.get(url).bearer_auth(token).send().await?;

    if !res.status().is_success() {
        warn!("Graph API request failed: {}", res.status());
        return Ok(());
    }

    let data: Value = res.json().await?;
    let emails = data["value"].as_array().cloned().unwrap_or_default();

    for item in emails {
        let subject = item["subject"].as_str().unwrap_or("").to_lowercase();
        let sender = item["sender"]["emailAddress"]["address"]
            .as_str()
            .unwrap_or("")
            .to_lowercase();
        let id = item["id"].as_str().unwrap_or("");

        let match_subject = subject_filter.is_empty() || subject.contains(subject_filter);
        let match_sender = sender_filter.is_empty() || sender.contains(sender_filter);

        if match_subject && match_sender {
            info!("EmailTrigger matched email: {} from {}", subject, sender);

            // Mark as read
            let mark_read_url = format!("https://graph.microsoft.com/v1.0/me/messages/{}", id);
            let payload = serde_json::json!({ "isRead": true });
            let _ = client
                .patch(&mark_read_url)
                .bearer_auth(token)
                .json(&payload)
                .send()
                .await;

            // Send trigger event
            let event = TriggerEvent {
                workflow_id: workflow_id.to_string(),
                trigger_type: TriggerType::EmailReceived,
                data: serde_json::json!({
                    "email_id": id,
                    "subject": item["subject"].as_str().unwrap_or(""),
                    "sender": item["sender"]["emailAddress"]["address"].as_str().unwrap_or(""),
                    "body_preview": item["bodyPreview"].as_str().unwrap_or(""),
                }),
                fired_at: Utc::now(),
            };

            if let Err(e) = tx.send(event).await {
                error!("Failed to send EmailTrigger event: {}", e);
            }
        }
    }

    Ok(())
}
