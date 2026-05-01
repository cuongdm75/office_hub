use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use automerge::{AutoCommit, sync::State as SyncState, sync::Message as SyncMessage, sync::SyncDoc};
use uuid::Uuid;
use tracing::info;

/// Represents a single collaboratively edited document using Automerge.
pub struct CrdtDocument {
    pub doc: AutoCommit,
    /// Tracks the sync state for each connected client (e.g. WebSocket connection ID)
    pub sync_states: HashMap<String, SyncState>,
}

impl Default for CrdtDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl CrdtDocument {
    pub fn new() -> Self {
        Self {
            doc: AutoCommit::new(),
            sync_states: HashMap::new(),
        }
    }

    /// Receives a sync message from a client and applies it to the document.
    pub fn receive_sync_message(
        &mut self,
        client_id: &str,
        message: SyncMessage,
    ) -> Result<(), automerge::AutomergeError> {
        let sync_state = self
            .sync_states
            .entry(client_id.to_string())
            .or_default();
        
        self.doc.sync().receive_sync_message(sync_state, message)?;
        Ok(())
    }

    /// Generates a sync message to send to a client, if necessary.
    pub fn generate_sync_message(&mut self, client_id: &str) -> Option<SyncMessage> {
        let sync_state = self
            .sync_states
            .entry(client_id.to_string())
            .or_default();
        
        self.doc.sync().generate_sync_message(sync_state)
    }
}

/// Manages multiple CRDT documents, typically one per collaborative session/file.
#[derive(Clone)]
pub struct CrdtManager {
    documents: Arc<RwLock<HashMap<String, CrdtDocument>>>,
}

impl Default for CrdtManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CrdtManager {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a new empty document and returns its generated UUID.
    pub async fn create_document(&self) -> String {
        let doc_id = Uuid::new_v4().to_string();
        let doc = CrdtDocument::new();
        self.documents.write().await.insert(doc_id.clone(), doc);
        info!(doc_id = %doc_id, "Created new CRDT document");
        doc_id
    }

    /// Process an incoming sync message from a client.
    /// Returns an optional response message to send back to the client.
    pub async fn process_sync_message(
        &self,
        doc_id: &str,
        client_id: &str,
        message_bytes: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        let mut docs = self.documents.write().await;
        
        let doc = docs.get_mut(doc_id).ok_or_else(|| format!("Document {} not found", doc_id))?;
        
        let message = SyncMessage::decode(message_bytes)
            .map_err(|e| format!("Failed to decode sync message: {}", e))?;
            
        doc.receive_sync_message(client_id, message)
            .map_err(|e| format!("Failed to apply sync message: {}", e))?;
            
        if let Some(response_msg) = doc.generate_sync_message(client_id) {
            Ok(Some(response_msg.encode()))
        } else {
            Ok(None)
        }
    }

    /// Generate a sync message to bring a client up to date.
    pub async fn get_sync_message(&self, doc_id: &str, client_id: &str) -> Result<Option<Vec<u8>>, String> {
        let mut docs = self.documents.write().await;
        
        let doc = docs.get_mut(doc_id).ok_or_else(|| format!("Document {} not found", doc_id))?;
        
        if let Some(msg) = doc.generate_sync_message(client_id) {
            Ok(Some(msg.encode()))
        } else {
            Ok(None)
        }
    }
}
