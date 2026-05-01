use async_trait::async_trait;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace};

use crate::workflow::{Trigger, TriggerEvent, TriggerType, WorkflowError, WorkflowResult};

pub struct FileWatchTrigger;

#[async_trait]
impl Trigger for FileWatchTrigger {
    fn trigger_type(&self) -> TriggerType {
        TriggerType::FileChanged
    }

    async fn start(
        &self,
        workflow_id: String,
        config: serde_json::Value,
        tx: mpsc::Sender<TriggerEvent>,
        cancel_token: CancellationToken,
    ) -> WorkflowResult<()> {
        let dir_str = config
            .get("directory")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();

        let pattern_str = config
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or(".*");

        let regex = Regex::new(pattern_str).map_err(|e| {
            WorkflowError::TriggerError(format!(
                "Invalid file watch regex '{}': {}",
                pattern_str, e
            ))
        })?;

        let dir_path = PathBuf::from(&dir_str);
        if !dir_path.exists() {
            return Err(WorkflowError::TriggerError(format!(
                "Watched directory does not exist: {}",
                dir_str
            )));
        }

        info!(
            workflow_id = %workflow_id,
            directory = %dir_str,
            pattern = %pattern_str,
            "Starting FileWatchTrigger"
        );

        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel();

        // The watcher closure is called from a background thread by `notify`.
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    let _ = notify_tx.send(event);
                }
            },
            Config::default(),
        )
        .map_err(|e| WorkflowError::TriggerError(format!("Failed to create watcher: {}", e)))?;

        watcher
            .watch(&dir_path, RecursiveMode::Recursive)
            .map_err(|e| {
                WorkflowError::TriggerError(format!("Failed to watch directory: {}", e))
            })?;

        tokio::spawn(async move {
            // Keep the watcher alive by moving it into this task
            let _watcher = watcher;

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        info!(workflow_id = %workflow_id, "FileWatchTrigger cancelled");
                        break;
                    }
                    Some(event) = notify_rx.recv() => {
                        // Only care about DataChange, Create, Remove, Rename
                        match event.kind {
                            notify::EventKind::Modify(notify::event::ModifyKind::Data(_))
                            | notify::EventKind::Create(_)
                            | notify::EventKind::Remove(_) => {
                                for path in event.paths {
                                    if let Some(path_str) = path.to_str() {
                                        if regex.is_match(path_str) {
                                            trace!(workflow_id = %workflow_id, path = %path_str, "FileWatchTrigger matched");

                                            let payload = serde_json::json!({
                                                "file_path": path_str,
                                                "event_type": format!("{:?}", event.kind)
                                            });

                                            let trigger_event = TriggerEvent {
                                                workflow_id: workflow_id.clone(),
                                                trigger_type: TriggerType::FileChanged,
                                                data: payload,
                                                fired_at: chrono::Utc::now(),
                                            };

                                            if let Err(e) = tx.send(trigger_event).await {
                                                error!("FileWatchTrigger failed to send event: {}", e);
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
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
