// ============================================================================
// Office Hub – workflow/triggers/mod.rs
//
// Trigger implementations for the Event-Driven Workflow Engine.
//
// Each trigger listens for a specific external event and emits a
// `TriggerEvent` on the shared channel when the condition is met.
//
// Trigger taxonomy:
//   EmailTrigger      – Outlook inbox via COM / MAPI (Phase 6)
//   FileWatchTrigger  – FileSystemWatcher for directory changes (Phase 6)
//   ScheduleTrigger   – cron-like timer (Phase 6)
//   VoiceTrigger      – WebSocket message from Mobile App (Phase 5)
//   ManualTrigger     – explicit API/UI invocation (available Phase 1)
// ============================================================================

// TODO(phase-5): pub mod voice;
pub mod email;
pub mod file_watcher;
pub mod schedule;

use crate::workflow::{Trigger, TriggerEvent, TriggerType, WorkflowResult};
use async_trait::async_trait;
use tokio::sync::mpsc;

// ─────────────────────────────────────────────────────────────────────────────
// ManualTrigger – fires only when explicitly called via API or UI
// ─────────────────────────────────────────────────────────────────────────────

pub struct ManualTrigger;

#[async_trait]
impl Trigger for ManualTrigger {
    fn trigger_type(&self) -> TriggerType {
        TriggerType::Manual
    }

    async fn start(
        &self,
        _workflow_id: String,
        _config: serde_json::Value,
        _tx: mpsc::Sender<TriggerEvent>,
        _cancel_token: tokio_util::sync::CancellationToken,
    ) -> WorkflowResult<()> {
        // Manual trigger has no background listener – it is fired directly
        // by WorkflowEngine::trigger() in mod.rs.
        Ok(())
    }

    async fn stop(&self) -> WorkflowResult<()> {
        Ok(())
    }
}

pub use email::EmailTrigger;
pub use file_watcher::FileWatchTrigger;
pub use schedule::ScheduleTrigger;

// ─────────────────────────────────────────────────────────────────────────────
// Stub triggers (Phase 6)
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! stub_trigger {
    ($name:ident, $kind:expr, $phase:expr) => {
        pub struct $name;

        #[async_trait]
        impl Trigger for $name {
            fn trigger_type(&self) -> TriggerType {
                $kind
            }

            async fn start(
                &self,
                workflow_id: String,
                _config: serde_json::Value,
                _tx: mpsc::Sender<TriggerEvent>,
                _cancel_token: tokio_util::sync::CancellationToken,
            ) -> WorkflowResult<()> {
                tracing::warn!(
                    workflow_id = %workflow_id,
                    trigger = ?$kind,
                    "[STUB] {} not yet implemented – scheduled for Phase {}",
                    stringify!($name),
                    $phase
                );
                Ok(())
            }

            async fn stop(&self) -> WorkflowResult<()> {
                Ok(())
            }
        }
    };
}

stub_trigger!(VoiceTrigger, TriggerType::VoiceCommand, 5);
