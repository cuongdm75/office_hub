// ============================================================================
// Office Hub â€“ llm_gateway/mod.rs
//
// LLM Gateway â€“ Provider Abstraction Layer
//
// TrÃ¡ch nhiá»‡m:
//   1. Cung cáº¥p interface thá»‘ng nháº¥t cho má»i LLM provider
//      (Gemini, OpenAI, Ollama, LM Studio)
//   2. Hybrid Mode: tá»± Ä‘á»™ng fallback Cloud â†’ Local náº¿u Cloud khÃ´ng kháº£ dá»¥ng
//   3. Token Caching: cache prompt/response Ä‘á»ƒ tiáº¿t kiá»‡m chi phÃ­ vÃ  tÄƒng tá»‘c
//   4. Context Window Management: Ä‘áº¿m token, cáº£nh bÃ¡o khi sáº¯p Ä‘áº§y
//   5. Session Summarisation: tÃ³m táº¯t lá»‹ch sá»­ há»™i thoáº¡i dÃ i
//   6. Rate Limiting: tuÃ¢n thá»§ giá»›i háº¡n request/phÃºt cá»§a tá»«ng provider
//   7. Retry Logic: tá»± Ä‘á»™ng thá»­ láº¡i khi gáº·p lá»—i táº¡m thá»i (429, 503â€¦)
//
// Provider support matrix:
//   â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¬â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¬â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”
//   â”‚ Provider    â”‚ Type       â”‚ Endpoint                              â”‚
//   â”œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¼â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¼â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”¤
//   â”‚ Gemini      â”‚ Cloud      â”‚ https://generativelanguage.googleapis â”‚
//   â”‚ OpenAI      â”‚ Cloud      â”‚ https://api.openai.com/v1             â”‚
//   â”‚ Ollama      â”‚ Local      â”‚ http://localhost:11434                â”‚
//   â”‚ LM Studio   â”‚ Local      â”‚ http://localhost:1234/v1              â”‚
//   â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”´â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”´â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜
// ============================================================================

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, info, instrument, warn};

use crate::{AppError, AppResult, LlmConfig};

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Public types re-exported by this module
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub use provider::ProviderKind;
pub use request::{LlmMessage, LlmRequest, MessageRole, TaskComplexity};

pub use response::{LlmResponse, LlmUsage, StopReason};

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Sub-modules (inline definitions below)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 1. REQUEST / RESPONSE DTOs
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub mod request {
    use super::*;

    /// Represents the complexity or intent of the LLM task, used for intelligent routing.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum TaskComplexity {
        /// Use the fastest available model, potentially switching to a local provider.
        Fast,
        /// Use the default configured provider and model.
        #[default]
        Balanced,
        /// Use the most capable reasoning model, potentially switching to a cloud provider.
        Reasoning,
    }

    /// Role of a message in the conversation.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum MessageRole {
        System,
        User,
        Assistant,
    }

    impl std::fmt::Display for MessageRole {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::System => write!(f, "system"),
                Self::User => write!(f, "user"),
                Self::Assistant => write!(f, "assistant"),
            }
        }
    }

    /// A single message in the conversation.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LlmMessage {
        pub role: MessageRole,
        pub content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub image_base64s: Vec<String>,
    }

    impl LlmMessage {
        pub fn system(content: impl Into<String>) -> Self {
            Self {
                role: MessageRole::System,
                content: content.into(),
                image_base64s: vec![],
            }
        }
        pub fn user(content: impl Into<String>) -> Self {
            Self {
                role: MessageRole::User,
                content: content.into(),
                image_base64s: vec![],
            }
        }
        pub fn assistant(content: impl Into<String>) -> Self {
            Self {
                role: MessageRole::Assistant,
                content: content.into(),
                image_base64s: vec![],
            }
        }
        pub fn user_with_images(content: impl Into<String>, images: Vec<String>) -> Self {
            Self {
                role: MessageRole::User,
                content: content.into(),
                image_base64s: images,
            }
        }
    }

    /// A complete LLM request payload.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LlmRequest {
        /// Conversation messages (system + history + new user message).
        pub messages: Vec<LlmMessage>,

        /// Sampling temperature [0.0 â€“ 2.0]. Lower = more deterministic.
        pub temperature: f32,

        /// Maximum tokens to generate in the response.
        pub max_tokens: u32,

        /// Stop sequences (the model stops generating when it hits one).
        #[serde(default)]
        pub stop_sequences: Vec<String>,

        /// Whether to stream the response (not yet implemented â€“ placeholder).
        #[serde(default)]
        pub stream: bool,

        /// Optional JSON schema to constrain the output format.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub response_schema: Option<serde_json::Value>,

        /// Whether to force the provider to return valid JSON (e.g., application/json)
        #[serde(default)]
        pub require_json: bool,

        /// Unique ID for this request (for audit / cache key correlation).
        pub request_id: uuid::Uuid,

        /// The intended complexity of the task, guiding model/provider routing.
        #[serde(default)]
        pub complexity: TaskComplexity,

        /// The agent that initiated this request, used for metrics tracking.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub agent_id: Option<String>,
    }

    impl LlmRequest {
        pub fn new(messages: Vec<LlmMessage>) -> Self {
            Self {
                messages,
                temperature: 0.2,
                max_tokens: 4096,
                stop_sequences: vec![],
                stream: false,
                response_schema: None,
                require_json: false,
                request_id: uuid::Uuid::new_v4(),
                complexity: TaskComplexity::default(),
                agent_id: None,
            }
        }

        pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
            self.agent_id = Some(agent_id.into());
            self
        }

        pub fn with_temperature(mut self, t: f32) -> Self {
            self.temperature = t.clamp(0.0, 2.0);
            self
        }

        pub fn with_max_tokens(mut self, n: u32) -> Self {
            self.max_tokens = n;
            self
        }

        pub fn with_json_schema(mut self, schema: serde_json::Value) -> Self {
            self.response_schema = Some(schema);
            self.require_json = true;
            self
        }

        pub fn with_require_json(mut self, require: bool) -> Self {
            self.require_json = require;
            self
        }

        pub fn with_complexity(mut self, complexity: TaskComplexity) -> Self {
            self.complexity = complexity;
            self
        }

        /// Estimate total input tokens (rough heuristic: ~4 chars / token for
        /// ASCII, ~2 chars / token for Vietnamese / CJK).
        pub fn estimated_input_tokens(&self) -> usize {
            self.messages
                .iter()
                .map(|m| estimate_tokens(&m.content))
                .sum()
        }

        /// Build a stable cache key from the request content.
        pub fn cache_key(&self, model: &str) -> String {
            let mut hasher = Sha256::new();
            hasher.update(model.as_bytes());
            hasher.update(self.temperature.to_le_bytes());
            hasher.update(self.max_tokens.to_le_bytes());
            for msg in &self.messages {
                hasher.update(msg.role.to_string().as_bytes());
                hasher.update(msg.content.as_bytes());
            }
            let hash = hasher.finalize();
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&hash[..16])
        }
    }
}

pub mod response {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum StopReason {
        /// Model finished naturally.
        Stop,
        /// Reached `max_tokens` limit.
        Length,
        /// Triggered a `stop_sequence`.
        StopSequence,
        /// Provider-specific safety filter triggered.
        ContentFilter,
        /// Unknown reason.
        Unknown,
    }

    /// Token usage reported by the provider.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct LlmUsage {
        pub prompt_tokens: u32,
        pub completion_tokens: u32,
        pub total_tokens: u32,
    }

    /// A complete LLM response.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LlmResponse {
        /// ID of the originating request.
        pub request_id: uuid::Uuid,

        /// The generated text content.
        pub content: String,

        /// Why the model stopped generating.
        pub stop_reason: StopReason,

        /// Token usage statistics.
        pub usage: LlmUsage,

        /// Provider that served this response.
        pub provider: String,

        /// Model that served this response.
        pub model: String,

        /// Whether this response came from the cache.
        pub from_cache: bool,

        /// Wall-clock time for this response in milliseconds.
        pub latency_ms: u64,

        /// Timestamp when the response was received.
        pub received_at: DateTime<Utc>,
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 2. PROVIDER CONFIG
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub mod provider {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ProviderKind {
        Gemini,
        OpenAi,
        Ollama,
        LmStudio,
    }

    impl ProviderKind {
        pub fn is_cloud(&self) -> bool {
            matches!(self, Self::Gemini | Self::OpenAi)
        }

        pub fn is_local(&self) -> bool {
            !self.is_cloud()
        }

        pub fn display_name(&self) -> &'static str {
            match self {
                Self::Gemini => "Google Gemini",
                Self::OpenAi => "OpenAI GPT",
                Self::Ollama => "Ollama (Local)",
                Self::LmStudio => "LM Studio (Local)",
            }
        }
    }

    impl std::fmt::Display for ProviderKind {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.display_name())
        }
    }

    impl std::str::FromStr for ProviderKind {
        type Err = String;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s.to_lowercase().as_str() {
                "gemini" => Ok(Self::Gemini),
                "openai" => Ok(Self::OpenAi),
                "ollama" => Ok(Self::Ollama),
                "lmstudio" | "lm_studio" | "lm studio" => Ok(Self::LmStudio),
                _ => Err(format!("Unknown provider: '{s}'")),
            }
        }
    }


}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 3. TOKEN CACHE
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

mod cache {
    use super::*;

    #[derive(Debug, Clone)]
    struct CacheEntry {
        response: LlmResponse,
        cached_at: Instant,
        ttl_secs: u64,
        hit_count: u32,
    }

    impl CacheEntry {
        fn is_expired(&self) -> bool {
            self.cached_at.elapsed().as_secs() >= self.ttl_secs
        }
    }

    /// In-memory LRU-like token cache.
    ///
    /// Keys are the `LlmRequest::cache_key()` hash.
    /// Entries expire after `ttl_secs` and are evicted lazily on the next `get`.
    pub struct TokenCache {
        store: Arc<Mutex<HashMap<String, CacheEntry>>>,
        ttl_secs: u64,
        max_entries: usize,
        /// Rolling statistics
        hits: Arc<Mutex<u64>>,
        misses: Arc<Mutex<u64>>,
    }

    impl TokenCache {
        pub fn new(ttl_secs: u64, max_entries: usize) -> Self {
            Self {
                store: Arc::new(Mutex::new(HashMap::new())),
                ttl_secs,
                max_entries,
                hits: Arc::new(Mutex::new(0)),
                misses: Arc::new(Mutex::new(0)),
            }
        }

        pub async fn get(&self, key: &str) -> Option<LlmResponse> {
            let mut store = self.store.lock().await;
            if let Some(entry) = store.get_mut(key) {
                if entry.is_expired() {
                    store.remove(key);
                    *self.misses.lock().await += 1;
                    return None;
                }
                entry.hit_count += 1;
                let mut resp = entry.response.clone();
                resp.from_cache = true;
                *self.hits.lock().await += 1;
                debug!(cache_key = %key, "Cache HIT");
                return Some(resp);
            }
            *self.misses.lock().await += 1;
            None
        }

        pub async fn put(&self, key: String, response: LlmResponse) {
            let mut store = self.store.lock().await;
            // Evict oldest entries if at capacity
            if store.len() >= self.max_entries {
                // Simple strategy: remove all expired entries first
                store.retain(|_, v| !v.is_expired());
                // If still over capacity, remove the entry with the oldest
                // cached_at timestamp
                if store.len() >= self.max_entries {
                    if let Some(oldest_key) = store
                        .iter()
                        .min_by_key(|(_, v)| v.cached_at)
                        .map(|(k, _)| k.clone())
                    {
                        store.remove(&oldest_key);
                    }
                }
            }
            debug!(cache_key = %key, "Cache STORE");
            store.insert(
                key,
                CacheEntry {
                    response,
                    cached_at: Instant::now(),
                    ttl_secs: self.ttl_secs,
                    hit_count: 0,
                },
            );
        }

        pub async fn clear(&self) {
            self.store.lock().await.clear();
            info!("Token cache cleared");
        }

        pub async fn stats(&self) -> CacheStats {
            let store = self.store.lock().await;
            let hits = *self.hits.lock().await;
            let misses = *self.misses.lock().await;
            let total = hits + misses;
            CacheStats {
                entries: store.len(),
                hits,
                misses,
                hit_rate: if total > 0 {
                    hits as f64 / total as f64
                } else {
                    0.0
                },
                ttl_secs: self.ttl_secs,
                max_entries: self.max_entries,
            }
        }
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct CacheStats {
        pub entries: usize,
        pub hits: u64,
        pub misses: u64,
        pub hit_rate: f64,
        pub ttl_secs: u64,
        pub max_entries: usize,
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 4. PROVIDER TRAIT + IMPLEMENTATIONS
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Internal trait implemented by each provider backend.


#[async_trait]
trait LlmProvider: Send + Sync {
    #[allow(dead_code)]
    fn model(&self) -> &str;

    async fn complete(&self, req: &LlmRequest) -> AppResult<LlmResponse>;

    /// Support partial text streaming (primarily for thoughts/UI feedback).
    async fn complete_stream(&self, req: &LlmRequest) -> AppResult<std::pin::Pin<Box<dyn futures::Stream<Item = AppResult<String>> + Send + 'static>>> {
        // Fallback implementation: just call complete and yield the final string
        let req_clone = req.clone();
        let final_resp = self.complete(&req_clone).await?;
        Ok(Box::pin(futures::stream::once(async move { Ok(final_resp.content) })))
    }

    /// Quick reachability check â€“ should complete within ~3 s.
    async fn health_check(&self) -> bool;
}

// â”€â”€ 4a. Gemini provider â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

struct GeminiProvider {
    api_key: String,
    default_model: String,
    fast_model: Option<String>,
    reasoning_model: Option<String>,
    client: reqwest::Client,
}

impl GeminiProvider {
    fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        fast_model: Option<String>,
        reasoning_model: Option<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build Gemini HTTP client");
        Self {
            api_key: api_key.into(),
            default_model: default_model.into(),
            fast_model,
            reasoning_model,
            client,
        }
    }

    fn resolve_model(&self, complexity: &request::TaskComplexity) -> &str {
        match complexity {
            request::TaskComplexity::Fast => self.fast_model.as_deref().unwrap_or(&self.default_model),
            request::TaskComplexity::Reasoning => self.reasoning_model.as_deref().unwrap_or(&self.default_model),
            request::TaskComplexity::Balanced => &self.default_model,
        }
    }

    fn endpoint(&self, model: &str) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model, self.api_key
        )
    }

    /// Convert our generic messages â†’ Gemini "contents" format.
    fn build_body(&self, req: &LlmRequest) -> serde_json::Value {
        // Gemini uses system_instruction separately + contents array
        let (system_parts, other_messages): (Vec<_>, Vec<_>) = req
            .messages
            .iter()
            .partition(|m| m.role == MessageRole::System);

        let system_instruction = if system_parts.is_empty() {
            None
        } else {
            let combined = system_parts
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            Some(serde_json::json!({ "parts": [{ "text": combined }] }))
        };

        let contents: Vec<serde_json::Value> = other_messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "model",
                    MessageRole::System => "user", // fallback
                };
                let mut parts = vec![serde_json::json!({ "text": m.content })];
                for img in &m.image_base64s {
                    parts.push(serde_json::json!({
                        "inlineData": {
                            "mimeType": "image/png",
                            "data": img
                        }
                    }));
                }
                serde_json::json!({
                    "role": role,
                    "parts": parts
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature":     req.temperature,
                "maxOutputTokens": req.max_tokens,
                "stopSequences":   req.stop_sequences,
            }
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = sys;
        }

        if let Some(schema) = &req.response_schema {
            body["generationConfig"]["responseMimeType"] = "application/json".into();
            body["generationConfig"]["responseSchema"] = schema.clone();
        } else if req.require_json {
            body["generationConfig"]["responseMimeType"] = "application/json".into();
        }

        body
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn model(&self) -> &str {
        &self.default_model
    }

    async fn complete(&self, req: &LlmRequest) -> AppResult<LlmResponse> {
        let model = self.resolve_model(&req.complexity);
        let body = self.build_body(req);
        let start = Instant::now();

        let resp = self
            .client
            .post(self.endpoint(model))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::LlmGateway(format!("Gemini request failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::LlmGateway(format!("Gemini failed to read response: {e}")))?;
            
        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| {
            serde_json::json!({
                "error": { "message": text.trim() }
            })
        });

        if !status.is_success() {
            let msg = json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown Gemini error")
                .to_string();
            return Err(AppError::LlmGateway(format!(
                "Gemini API error {status}: {msg}"
            )));
        }

        let content = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let finish_reason = json["candidates"][0]["finishReason"]
            .as_str()
            .unwrap_or("STOP");

        let stop_reason = match finish_reason {
            "STOP" => StopReason::Stop,
            "MAX_TOKENS" => StopReason::Length,
            "SAFETY" => StopReason::ContentFilter,
            _ => StopReason::Unknown,
        };

        let prompt_tokens = json["usageMetadata"]["promptTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32;
        let completion_tokens = json["usageMetadata"]["candidatesTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32;

        Ok(LlmResponse {
            request_id: req.request_id,
            content,
            stop_reason,
            usage: LlmUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            provider: "gemini".into(),
            model: model.to_string(),
            from_cache: false,
            latency_ms: start.elapsed().as_millis() as u64,
            received_at: Utc::now(),
        })
    }

    async fn complete_stream(&self, req: &LlmRequest) -> AppResult<std::pin::Pin<Box<dyn futures::Stream<Item = AppResult<String>> + Send + 'static>>> {
        let model = self.resolve_model(&req.complexity);
        let body = self.build_body(req);
        
        let endpoint = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            model, self.api_key
        );

        let resp = self
            .client
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::LlmGateway(format!("Gemini stream request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::LlmGateway(format!("Gemini API stream error {status}: {text}")));
        }

        let stream = async_stream::stream! {
            use futures::StreamExt;
            let mut byte_stream = resp.bytes_stream();
            
            while let Some(chunk_res) = byte_stream.next().await {
                match chunk_res {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes[..]).to_string();
                        // Gemini alt=sse streams send data: {"candidates": [...]}
                        for line in text.lines() {
                            if line.starts_with("data: ") {
                                let json_str = line.strip_prefix("data: ").unwrap_or("").trim();
                                if json_str == "[DONE]" || json_str.is_empty() { continue; }
                                
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                                    if let Some(content) = json.get("candidates").and_then(|c| c.get(0)).and_then(|c| c.get("content")).and_then(|c| c.get("parts")).and_then(|p| p.get(0)).and_then(|p| p.get("text")).and_then(|t| t.as_str()) {
                                        yield Ok(content.to_string());
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(AppError::LlmGateway(format!("Stream read error: {}", e)));
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn health_check(&self) -> bool {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models?key={}",
            self.api_key
        );
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                tracing::error!("health_check failed for Gemini: {:?}", e);
                false
            }
        }
    }
}

// â”€â”€ 4b. OpenAI-compatible provider (OpenAI + Ollama + LM Studio) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Ollama and LM Studio both expose an OpenAI-compatible /v1/chat/completions
// endpoint, so we reuse the same implementation.

struct OpenAiCompatProvider {
    kind: ProviderKind,
    default_model: String,
    fast_model: Option<String>,
    reasoning_model: Option<String>,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    fn new(
        kind: ProviderKind,
        default_model: impl Into<String>,
        fast_model: Option<String>,
        reasoning_model: Option<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300)) // local models can be slow
            .build()
            .expect("Failed to build HTTP client");
        Self {
            kind,
            default_model: default_model.into(),
            fast_model,
            reasoning_model,
            base_url: base_url.into(),
            api_key,
            client,
        }
    }

    fn resolve_model(&self, complexity: &request::TaskComplexity) -> &str {
        match complexity {
            request::TaskComplexity::Fast => self.fast_model.as_deref().unwrap_or(&self.default_model),
            request::TaskComplexity::Reasoning => self.reasoning_model.as_deref().unwrap_or(&self.default_model),
            request::TaskComplexity::Balanced => &self.default_model,
        }
    }

    fn chat_endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_body(&self, req: &LlmRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                if m.image_base64s.is_empty() {
                    serde_json::json!({ "role": m.role.to_string(), "content": m.content })
                } else {
                    let mut content_parts = vec![serde_json::json!({ "type": "text", "text": m.content })];
                    for img in &m.image_base64s {
                        content_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": { "url": format!("data:image/png;base64,{}", img) }
                        }));
                    }
                    serde_json::json!({ "role": m.role.to_string(), "content": content_parts })
                }
            })
            .collect();

        let model = self.resolve_model(&req.complexity);
        let mut body = serde_json::json!({
            "model":       model,
            "messages":    messages,
            "temperature": req.temperature,
            "max_tokens":  req.max_tokens,
            "stream":      false,
        });

        if !req.stop_sequences.is_empty() {
            body["stop"] = req.stop_sequences.clone().into();
        }

        if let Some(schema) = &req.response_schema {
            if self.kind == ProviderKind::OpenAi {
                let mut strict_schema = schema.clone();
                fn enforce_strict_schema(val: &mut serde_json::Value) {
                    if let Some(obj) = val.as_object_mut() {
                        if obj.get("type").and_then(|t| t.as_str()) == Some("object") {
                            obj.insert("additionalProperties".to_string(), serde_json::json!(false));
                            if let Some(props) = obj.get_mut("properties").and_then(|p| p.as_object_mut()) {
                                let mut keys = Vec::new();
                                for (k, v) in props.iter_mut() {
                                    keys.push(serde_json::json!(k.clone()));
                                    enforce_strict_schema(v);
                                }
                                obj.insert("required".to_string(), serde_json::Value::Array(keys));
                            }
                        } else if obj.get("type").and_then(|t| t.as_str()) == Some("array") {
                            if let Some(items) = obj.get_mut("items") {
                                enforce_strict_schema(items);
                            }
                        }
                        
                        for key in &["anyOf", "allOf", "oneOf"] {
                            if let Some(arr) = obj.get_mut(*key).and_then(|a| a.as_array_mut()) {
                                for item in arr.iter_mut() {
                                    enforce_strict_schema(item);
                                }
                            }
                        }
                    }
                }
                enforce_strict_schema(&mut strict_schema);

                body["response_format"] = serde_json::json!({ 
                    "type": "json_schema",
                    "json_schema": {
                        "name": "structured_output",
                        "strict": true,
                        "schema": strict_schema
                    }
                });
            } else {
                // Use json_object (widely supported) instead of json_schema (not supported by Ollama proxies)
                body["response_format"] = serde_json::json!({ "type": "json_object" });
            }
        } else if req.require_json {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        body
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn model(&self) -> &str {
        &self.default_model
    }

    async fn complete(&self, req: &LlmRequest) -> AppResult<LlmResponse> {
        let model = self.resolve_model(&req.complexity);
        let body = self.build_body(req);
        let start = Instant::now();

        let mut builder = self.client.post(self.chat_endpoint()).json(&body);

        if let Some(ref key) = self.api_key {
            builder = builder.bearer_auth(key);
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| AppError::LlmGateway(format!("{} request failed: {e}", self.kind)))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::LlmGateway(format!("{} failed to read response: {e}", self.kind)))?;

        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| {
            serde_json::json!({
                "error": { "message": text.trim() }
            })
        });

        if !status.is_success() {
            let msg = if let Some(err_obj) = json.get("error") {
                if let Some(msg_str) = err_obj.get("message").and_then(|v| v.as_str()) {
                    msg_str.to_string()
                } else if let Some(err_str) = err_obj.as_str() {
                    err_str.to_string()
                } else {
                    err_obj.to_string()
                }
            } else {
                "Unknown API error".to_string()
            };
            tracing::error!("{} API error {status}: {msg} | Raw JSON: {json}", self.kind);
            return Err(AppError::LlmGateway(format!(
                "{} API error {status}: {msg}",
                self.kind
            )));
        }

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let finish_reason = json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("stop");

        let stop_reason = match finish_reason {
            "stop" => StopReason::Stop,
            "length" => StopReason::Length,
            "content_filter" => StopReason::ContentFilter,
            _ => StopReason::Unknown,
        };

        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(LlmResponse {
            request_id: req.request_id,
            content,
            stop_reason,
            usage: LlmUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            provider: self.kind.to_string(),
            model: model.to_string(),
            from_cache: false,
            latency_ms: start.elapsed().as_millis() as u64,
            received_at: Utc::now(),
        })
    }


    async fn health_check(&self) -> bool {
        let url = match self.kind {
            ProviderKind::Ollama => {
                let base = self.base_url.trim_end_matches("/v1").trim_end_matches('/');
                format!("{}/api/tags", base)
            },
            ProviderKind::LmStudio | ProviderKind::OpenAi => {
                format!("{}/models", self.base_url.trim_end_matches('/'))
            },
            _ => format!("{}/models", self.base_url.trim_end_matches('/')),
        };

        let mut req = self.client.get(&url);
        if let Some(key) = &self.api_key {
            // Some providers (like Anthropic if we ever use proper headers) might need different ones,
            // but for OpenAI/LMStudio/Z.ai standard Bearer works for models endpoint if supported.
            // Anthropic doesn't have a public models endpoint, but we handle its errors.
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    // Fallback to sending a ping if the models endpoint fails (e.g. Anthropic, Z.ai)
                    let ping = LlmRequest::new(vec![LlmMessage::user("ping")])
                        .with_max_tokens(1)
                        .with_temperature(0.0);
                    match self.complete(&ping).await {
                        Ok(_) => true,
                        Err(e) => {
                            tracing::error!("health_check fallback failed for provider {}: {:?}", self.kind, e);
                            false
                        }
                    }
                } else {
                    true
                }
            },
            Err(e) => {
                tracing::error!("health_check failed for provider {}: {:?}", self.kind, e);
                false
            }
        }
    }
}

// â”€â”€ 4c. Anthropic provider (claude-*) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// Anthropic requires x-api-key + anthropic-version headers (NOT Bearer auth).
// Extended thinking / new claude models do NOT accept temperature.

struct AnthropicProvider {
    api_key: String,
    default_model: String,
    fast_model: Option<String>,
    reasoning_model: Option<String>,
    client: reqwest::Client,
}

impl AnthropicProvider {
    fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        fast_model: Option<String>,
        reasoning_model: Option<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("Failed to build Anthropic HTTP client");
        Self {
            api_key: api_key.into(),
            default_model: default_model.into(),
            fast_model,
            reasoning_model,
            client,
        }
    }

    fn resolve_model(&self, complexity: &request::TaskComplexity) -> &str {
        match complexity {
            request::TaskComplexity::Fast => self.fast_model.as_deref().unwrap_or(&self.default_model),
            request::TaskComplexity::Reasoning => self.reasoning_model.as_deref().unwrap_or(&self.default_model),
            request::TaskComplexity::Balanced => &self.default_model,
        }
    }

    /// Returns true for models that do NOT accept a temperature parameter.
    fn model_rejects_temperature(model: &str) -> bool {
        // claude-3-5 sonnet/haiku and newer claude-* models still accept temperature.
        // Only extended-thinking / o-series variants do not.
        model.contains("claude-3-7") || model.contains("claude-4")
    }

    fn build_body(&self, req: &LlmRequest, model: &str) -> serde_json::Value {
        // Convert messages: system messages become a top-level "system" field.
        let (sys_msgs, other_msgs): (Vec<_>, Vec<_>) = req
            .messages
            .iter()
            .partition(|m| m.role == MessageRole::System);

        let system_text = sys_msgs
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let messages: Vec<serde_json::Value> = other_msgs
            .iter()
            .map(|m| {
                let role = match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "user",
                };
                if m.image_base64s.is_empty() {
                    serde_json::json!({ "role": role, "content": m.content })
                } else {
                    let mut parts = vec![serde_json::json!({ "type": "text", "text": m.content })];
                    for img in &m.image_base64s {
                        parts.push(serde_json::json!({
                            "type": "image",
                            "source": { "type": "base64", "media_type": "image/png", "data": img }
                        }));
                    }
                    serde_json::json!({ "role": role, "content": parts })
                }
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });

        if !system_text.is_empty() {
            body["system"] = system_text.into();
        }

        // Only send temperature for models that accept it.
        if !Self::model_rejects_temperature(model) {
            body["temperature"] = req.temperature.into();
        }

        // Anthropic structured output: force a tool call so Claude returns valid JSON.
        // Claude has no `response_format` field â€“ tool-use is the official pattern.
        if let Some(schema) = &req.response_schema {
            body["tools"] = serde_json::json!([{
                "name": "structured_output",
                "description": "Output a structured JSON object matching the given schema",
                "input_schema": schema
            }]);
            body["tool_choice"] = serde_json::json!({ "type": "tool", "name": "structured_output" });
        }

        body
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn model(&self) -> &str { &self.default_model }

    async fn complete(&self, req: &LlmRequest) -> AppResult<LlmResponse> {
        let model = self.resolve_model(&req.complexity);
        let body = self.build_body(req, model);
        let start = Instant::now();

        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::LlmGateway(format!("Anthropic request failed: {e}")))?;

        let status = resp.status();
        let text = resp.text().await
            .map_err(|e| AppError::LlmGateway(format!("Anthropic failed to read response: {e}")))?;

        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| {
            serde_json::json!({ "error": { "message": text.trim() } })
        });

        if !status.is_success() {
            let msg = json["error"]["message"].as_str()
                .unwrap_or("Unknown Anthropic error").to_string();
            return Err(AppError::LlmGateway(format!("Anthropic API error {status}: {msg}")));
        }

        // Anthropic response: content blocks can be "text" or "tool_use".
        // When using tool-calling for structured output, extract `input` from tool_use block.
        let content = {
            let blocks = json["content"].as_array();
            let tool_block = blocks.as_ref().and_then(|arr| {
                arr.iter().find(|b| b["type"].as_str() == Some("tool_use"))
            });
            if let Some(tool) = tool_block {
                // tool_use.input is already a JSON object â€” serialize back to string
                serde_json::to_string(&tool["input"]).unwrap_or_default()
            } else {
                // Plain text response
                blocks
                    .and_then(|arr| arr.iter().find(|b| b["type"].as_str() == Some("text")))
                    .and_then(|b| b["text"].as_str())
                    .unwrap_or("")
                    .to_string()
            }
        };
        let stop_reason = match json["stop_reason"].as_str().unwrap_or("end_turn") {
            "end_turn" | "stop_sequence" | "tool_use" => StopReason::Stop,
            "max_tokens" => StopReason::Length,
            _ => StopReason::Unknown,
        };
        let prompt_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;


        Ok(LlmResponse {
            request_id: req.request_id,
            content,
            stop_reason,
            usage: LlmUsage { prompt_tokens, completion_tokens, total_tokens: prompt_tokens + completion_tokens },
            provider: "anthropic".into(),
            model: model.to_string(),
            from_cache: false,
            latency_ms: start.elapsed().as_millis() as u64,
            received_at: Utc::now(),
        })
    }


    async fn health_check(&self) -> bool {
        // Anthropic has no public /models endpoint; send a minimal ping.
        let ping = LlmRequest::new(vec![LlmMessage::user("ping")]).with_max_tokens(1);
        self.complete(&ping).await.is_ok()
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 5. GATEWAY METRICS
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GatewayMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub cache_hits: u64,
    pub cloud_requests: u64,
    pub local_requests: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_latency_ms: u64,
    pub fallbacks_triggered: u64,
    pub tokens_per_agent: std::collections::HashMap<String, u64>,
    pub tokens_per_model: std::collections::HashMap<String, u64>,
    pub success_per_model: std::collections::HashMap<String, u64>,
    pub failure_per_model: std::collections::HashMap<String, u64>,
}

impl GatewayMetrics {
    pub fn avg_latency_ms(&self) -> f64 {
        if self.successful_requests == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.successful_requests as f64
        }
    }

    pub fn total_tokens(&self) -> u64 {
        self.total_prompt_tokens + self.total_completion_tokens
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 5b. PROVIDER HEALTH CACHE  (9Router-style health-first routing)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Tracks per-provider health status so we skip known-down providers
/// immediately instead of wasting time on retries.
///
/// - Connection error / 404 â†’ cool-down 60 s
/// - 429 Rate-limited       â†’ cool-down 300 s
/// - 400 Config error       â†’ cool-down 600 s (likely misconfigured)
struct ProviderHealthCache {
    /// provider_key â†’ (marked_at, cooldown_secs, reason)
    inner: HashMap<String, (Instant, u64, String)>,
}

impl ProviderHealthCache {
    fn new() -> Self {
        Self { inner: HashMap::new() }
    }

    /// Returns `true` if the provider is currently in cooldown.
    fn is_down(&self, key: &str) -> Option<&str> {
        if let Some((marked_at, cooldown, reason)) = self.inner.get(key) {
            if marked_at.elapsed().as_secs() < *cooldown {
                return Some(reason.as_str());
            }
        }
        None
    }

    fn mark_down(&mut self, key: &str, cooldown_secs: u64, reason: impl Into<String>) {
        self.inner.insert(key.to_string(), (Instant::now(), cooldown_secs, reason.into()));
    }

    fn mark_up(&mut self, key: &str) {
        self.inner.remove(key);
    }

    /// Classify an error string into a cooldown duration.
    fn cooldown_for_error(err: &str) -> u64 {
        let e = err.to_lowercase();
        if e.contains("429") || e.contains("rate limit") || e.contains("quota") {
            300 // 5 min backoff for rate-limits
        } else if e.contains("400") || e.contains("bad request") {
            600 // 10 min â€“ likely config error, no point retrying fast
        } else {
            60  // 1 min for connection / 404 / 5xx
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 6. LLM GATEWAY (public facade)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Maximum number of concurrent outgoing LLM requests.
const MAX_CONCURRENT_LLM_REQUESTS: usize = 4;

/// Default retry delays for transient errors (exponential back-off).
const RETRY_DELAYS_MS: &[u64] = &[500, 1_500, 4_000];

/// The public LLM Gateway.
///
/// Obtain via `LlmGateway::new(config)`.  All public methods take `&self` and
/// are safe to call concurrently from multiple Tokio tasks.
pub struct LlmGateway {
    /// Active providers mapped by provider name.
    providers: RwLock<std::collections::HashMap<String, Arc<dyn LlmProvider>>>,

    /// Runtime config (mutable via `update_config`).
    config: Arc<RwLock<LlmConfig>>,

    /// Prompt / response token cache.
    cache: Arc<cache::TokenCache>,

    /// Semaphore to bound concurrent outgoing requests.
    concurrency: Arc<Semaphore>,

    /// Rolling metrics.
    metrics: Arc<Mutex<GatewayMetrics>>,

    /// 9Router-style per-provider health cache.
    /// Providers that recently errored are skipped for a cooldown period.
    health: Arc<Mutex<ProviderHealthCache>>,
}

impl std::fmt::Debug for LlmGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmGateway")
            .field("metrics", &self.metrics)
            .finish()
    }
}

impl LlmGateway {
    // â”€â”€ Construction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Build a `LlmGateway` from application config.
    pub fn new(config: LlmConfig) -> Self {
        let providers = Self::build_providers(&config);

        let cache = Arc::new(cache::TokenCache::new(
            60 * 60, // 1 hour TTL by default (overridden by rule engine)
            512,     // max 512 cached responses
        ));

        info!(
            providers = ?providers.keys().collect::<Vec<_>>(),
            "LLM Gateway initialised"
        );

        Self {
            providers: RwLock::new(providers),
            config: Arc::new(RwLock::new(config)),
            cache,
            concurrency: Arc::new(Semaphore::new(MAX_CONCURRENT_LLM_REQUESTS)),
            metrics: Arc::new(Mutex::new(GatewayMetrics::default())),
            health: Arc::new(Mutex::new(ProviderHealthCache::new())),
        }
    }

    fn build_providers(cfg: &LlmConfig) -> std::collections::HashMap<String, Arc<dyn LlmProvider>> {
        let mut map = std::collections::HashMap::<String, Arc<dyn LlmProvider>>::new();

        // Helper to extract tier models for a specific provider
        let get_models = |provider_name: &str, fallback_dm: &str| -> (String, Option<String>, Option<String>) {
            let mut dm = fallback_dm.to_string();
            let mut fm = None;
            let mut rm = None;
            if cfg.default_provider == provider_name { dm = cfg.default_model.clone(); }
            if cfg.fast_provider == provider_name { fm = Some(cfg.fast_model.clone()); }
            if cfg.reasoning_provider == provider_name { rm = Some(cfg.reasoning_model.clone()); }
            (dm, fm, rm)
        };

        // Gemini
        if let Some(key) = cfg.credentials.gemini_api_key.as_deref().filter(|k| !k.is_empty()) {
            let (dm, fm, rm) = get_models("gemini", "gemini-2.0-flash");
            map.insert("gemini".to_string(), Arc::new(GeminiProvider::new(key, dm, fm, rm)));
        }

        // OpenAI
        if let Some(key) = cfg.credentials.openai_api_key.as_deref().filter(|k| !k.is_empty()) {
            let (dm, fm, rm) = get_models("openai", "gpt-4o");
            map.insert("openai".to_string(), Arc::new(OpenAiCompatProvider::new(
                ProviderKind::OpenAi, dm, fm, rm, "https://api.openai.com/v1", Some(key.to_string()),
            )));
        }

        // Anthropic
        if let Some(key) = cfg.credentials.anthropic_api_key.as_deref().filter(|k| !k.is_empty()) {
            let (dm, fm, rm) = get_models("anthropic", "claude-3-5-sonnet-20241022");
            map.insert("anthropic".to_string(), Arc::new(AnthropicProvider::new(key, dm, fm, rm)));
        }

        // Z.ai
        if let Some(key) = cfg.credentials.zai_api_key.as_deref().filter(|k| !k.is_empty()) {
            let (dm, fm, rm) = get_models("z.ai", "zai-model");
            map.insert("z.ai".to_string(), Arc::new(OpenAiCompatProvider::new(
                ProviderKind::OpenAi, dm, fm, rm, "https://api.z.ai/v1", Some(key.to_string()),
            )));
        }

        // Ollama
        let ollama_ep = cfg.credentials.ollama_endpoint.clone().unwrap_or_else(|| "http://localhost:11434/v1".to_string());
        let mut ep = ollama_ep.trim_end_matches('/').to_string();
        if !ep.ends_with("/v1") { ep.push_str("/v1"); }
        let (dm, fm, rm) = get_models("ollama", "__auto__");
        map.insert("ollama".to_string(), Arc::new(OpenAiCompatProvider::new(
            ProviderKind::Ollama, dm, fm, rm, ep, None,
        )));

        // LM Studio
        let lm_ep = cfg.credentials.lmstudio_endpoint.clone().unwrap_or_else(|| "http://localhost:1234/v1".to_string());
        let mut ep2 = lm_ep.trim_end_matches('/').to_string();
        if !ep2.ends_with("/v1") { ep2.push_str("/v1"); }
        let (dm, fm, rm) = get_models("lmstudio", "local-model");
        map.insert("lmstudio".to_string(), Arc::new(OpenAiCompatProvider::new(
            ProviderKind::LmStudio, dm, fm, rm, ep2, None,
        )));

        map
    }


    // â”€â”€ Main API â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Send a chat completion request.
    ///
    /// 9Router-style 3-tier cascading autoroute:
    ///   TIER 1: Active provider (user-configured, retried 3Ã—)
    ///   TIER 2: Cloud fallbacks (other providers with API keys)
    ///   TIER 3: Local fallbacks (Ollama, LM Studio â€“ pre-flight TCP check)
    ///
    /// Known-down providers are skipped immediately via ProviderHealthCache.
    /// 429 â†’ 5-min backoff. 400 â†’ 10-min backoff. Other â†’ 1-min backoff.
    #[instrument(skip(self, req), fields(request_id = %req.request_id))]
    /// Stream the response (if supported by provider)
    pub async fn complete_stream(&self, req: LlmRequest) -> AppResult<std::pin::Pin<Box<dyn futures::Stream<Item = AppResult<String>> + Send + 'static>>> {
        let cfg = self.config.read().await;
        let provider_key = match req.complexity {
            crate::llm_gateway::request::TaskComplexity::Fast => cfg.fast_provider.clone(),
            crate::llm_gateway::request::TaskComplexity::Reasoning => cfg.reasoning_provider.clone(),
            _ => cfg.default_provider.clone(),
        };
        
        // Find the provider without deadlocking
        let mut found_provider = None;
        {
            let providers = self.providers.read().await;
            if let Some(p) = providers.get(&provider_key) {
                found_provider = Some(p.clone());
            } else if let Some(p) = providers.get("gemini") {
                found_provider = Some(p.clone());
            }
        }
        
        if let Some(p) = found_provider {
            p.complete_stream(&req).await
        } else {
            Err(crate::AppError::LlmGateway("No LLM provider available".into()))
        }
    }

    #[instrument(skip(self, req), fields(request_id = %req.request_id))]
    pub async fn complete(&self, req: LlmRequest) -> AppResult<LlmResponse> {
        let cfg = self.config.read().await;

        // 1. Check cache
        if cfg.token_cache_enabled {
            let primary_model = self.primary_model(&cfg, &req.complexity);
            let key = req.cache_key(&primary_model);
            if let Some(cached) = self.cache.get(&key).await {
                debug!("Returning cached LLM response");
                self.metrics.lock().await.cache_hits += 1;
                return Ok(cached);
            }
        }

        // 2. Acquire concurrency slot
        let _permit = self
            .concurrency
            .acquire()
            .await
            .map_err(|e| AppError::LlmGateway(format!("Semaphore error: {e}")))?;

        // 3. Build 9Router-style Tiered Combo Queue
        let target_provider = match req.complexity {
            TaskComplexity::Fast => cfg.fast_provider.clone(),
            TaskComplexity::Balanced => cfg.default_provider.clone(),
            TaskComplexity::Reasoning => cfg.reasoning_provider.clone(),
        };

        // Cloud fallback order (cheapest/fastest first)
        let cloud_order = ["gemini", "openai", "anthropic", "z.ai"];
        // Local fallback order
        let local_order = ["ollama", "lmstudio"];

        let mut routing_queue: Vec<String> = vec![target_provider.clone()];
        // Add cloud fallbacks
        for k in &cloud_order {
            if *k != target_provider.as_str() { routing_queue.push(k.to_string()); }
        }
        // Add local fallbacks
        for k in &local_order {
            if *k != target_provider.as_str() { routing_queue.push(k.to_string()); }
        }
        drop(cfg); // release read lock before network calls

        // 4. 9Router-style Health-First Cascading Autoroute
        let mut error_log = Vec::new();
        let mut skipped_log = Vec::new();

        for (tier, provider_key) in routing_queue.iter().enumerate() {
            // 4a. Check health cache â€“ skip known-down providers
            {
                let hc = self.health.lock().await;
                if let Some(reason) = hc.is_down(provider_key) {
                    skipped_log.push(format!("{provider_key}: in cooldown ({reason})"));
                    debug!(provider = provider_key, "Skipping provider (health cache)");
                    continue;
                }
            }

            let provider = {
                let providers = self.providers.read().await;
                providers.get(provider_key).cloned()
            };

            let p = match provider {
                Some(p) => p,
                None => continue, // provider not configured (no API key)
            };

            // 4b. Pre-flight TCP check for local providers (2s timeout)
            //     Skip immediately if port not open â€“ no point waiting for HTTP timeout.
            let is_local = matches!(provider_key.as_str(), "ollama" | "lmstudio");
            if is_local {
                let endpoint = match provider_key.as_str() {
                    "ollama" => "127.0.0.1:11434",
                    "lmstudio" => "127.0.0.1:1234",
                    _ => "",
                };
                if !endpoint.is_empty() && !Self::tcp_reachable(endpoint).await {
                    let reason = format!("TCP unreachable at {endpoint}");
                    warn!(provider = provider_key, %reason, "Local provider offline, skipping");
                    self.health.lock().await.mark_down(provider_key, 60, &reason);
                    skipped_log.push(format!("{provider_key}: {reason}"));
                    continue;
                }
            }

            // 4b-extra: Ollama auto-model-discovery for fallback routing
            // When model is __auto__ sentinel, query /api/tags and pick the first available model.
            // Updates the provider map in-place so the re-fetched `p` uses the correct model.
            if provider_key == "ollama" && p.model() == "__auto__" {
                let ollama_base_url = {
                    let cfg2 = self.config.read().await;
                    cfg2.credentials.ollama_endpoint
                        .clone()
                        .unwrap_or_else(|| "http://localhost:11434/v1".to_string())
                        .trim_end_matches("/v1")
                        .trim_end_matches('/')
                        .to_string()
                };
                let tags_url = format!("{}/api/tags", ollama_base_url);
                let discovered = match reqwest::get(&tags_url).await {
                    Ok(resp) if resp.status().is_success() => {
                        resp.json::<serde_json::Value>().await.ok()
                            .and_then(|json| {
                                json["models"].as_array()
                                    .and_then(|arr| arr.first())
                                    .and_then(|m| m["name"].as_str())
                                    .map(String::from)
                            })
                    }
                    _ => None,
                };

                match discovered {
                    Some(model_name) => {
                        info!(provider = "ollama", model = %model_name, "[9Router] Auto-discovered Ollama model");
                        // Rebuild the Ollama provider with the real model name
                        let ep = {
                            let c = self.config.read().await;
                            let raw = c.credentials.ollama_endpoint.clone()
                                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
                            let mut e = raw.trim_end_matches('/').to_string();
                            if !e.ends_with("/v1") { e.push_str("/v1"); }
                            e
                        };
                        let new_p: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(
                            ProviderKind::Ollama, model_name, None, None, ep, None,
                        ));
                        // Persist to provider map so future calls also use the discovered model
                        self.providers.write().await.insert("ollama".to_string(), new_p.clone());
                        // Execute immediately with the new provider (no retry for fallback)
                        info!(tier = tier + 1, provider = "ollama", "[9Router] Attempting provider (auto-model)");
                        match new_p.complete(&req).await {
                            Ok(resp) => {
                                self.metrics.lock().await.fallbacks_triggered += 1;
                                self.health.lock().await.mark_up("ollama");
                                self.record_success(&req, &resp).await;
                                let cfg = self.config.read().await;
                                if cfg.token_cache_enabled {
                                    let primary_model = self.primary_model(&cfg, &req.complexity);
                                    let key = req.cache_key(&primary_model);
                                    self.cache.put(key, resp.clone()).await;
                                }
                                return Ok(resp);
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                let cooldown = ProviderHealthCache::cooldown_for_error(&err_str);
                                warn!(provider = "ollama", error = %err_str, cooldown_secs = cooldown, "[9Router] Ollama auto-model failed");
                                self.health.lock().await.mark_down("ollama", cooldown, &err_str);
                                error_log.push(format!("ollama: {err_str}"));
                                continue;
                            }
                        }
                    }
                    None => {
                        let reason = "No models installed in Ollama";
                        warn!(provider = "ollama", reason, "[9Router] Skipping Ollama (no models)");
                        skipped_log.push(format!("ollama: {reason}"));
                        self.health.lock().await.mark_down("ollama", 60, reason);
                        continue;
                    }
                }
            }


            // 4c. Pre-flight HTTP Health Check for fallback providers
            // Skip for primary (tier 0) to avoid 1-RTT latency penalty on happy path.
            if tier > 0 {
                info!(tier = tier + 1, provider = provider_key, "[9Router] Running health check for fallback provider");
                let is_healthy = tokio::time::timeout(Duration::from_secs(3), p.health_check())
                    .await
                    .unwrap_or(false);

                if !is_healthy {
                    let reason = "Health check failed or timed out";
                    warn!(provider = provider_key, reason, "[9Router] Skipping offline fallback provider");
                    skipped_log.push(format!("{provider_key}: {reason}"));
                    self.health.lock().await.mark_down(provider_key, 60, reason);
                    continue;
                }
            }

            info!(tier = tier + 1, provider = provider_key, "[9Router] Attempting provider");

            // 4d. Execute: primary gets retry logic; fallbacks try once
            let result = if tier == 0 {
                self.try_provider_with_retry(&p, &req).await
            } else {
                p.complete(&req).await
            };

            match result {
                Ok(resp) => {
                    if tier > 0 {
                        self.metrics.lock().await.fallbacks_triggered += 1;
                        info!(provider = provider_key, "[9Router] Fallback succeeded");
                    }
                    // Mark provider healthy
                    self.health.lock().await.mark_up(provider_key);
                    self.record_success(&req, &resp).await;

                    // 5. Store in cache
                    let cfg = self.config.read().await;
                    if cfg.token_cache_enabled {
                        let primary_model = self.primary_model(&cfg, &req.complexity);
                        let key = req.cache_key(&primary_model);
                        self.cache.put(key, resp.clone()).await;
                    }

                    return Ok(resp);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let cooldown = ProviderHealthCache::cooldown_for_error(&err_str);
                    warn!(
                        tier = tier + 1,
                        provider = provider_key,
                        error = %err_str,
                        cooldown_secs = cooldown,
                        "[9Router] Provider failed, entering cooldown"
                    );
                    self.health.lock().await.mark_down(provider_key, cooldown, &err_str);
                    error_log.push(format!("{provider_key}: {err_str}"));
                    
                    let model_name = p.model().to_string();
                    let model_key = format!("{}/{}", provider_key, model_name);
                    *self.metrics.lock().await.failure_per_model.entry(model_key).or_insert(0) += 1;
                }
            }
        }

        self.metrics.lock().await.failed_requests += 1;

        // Build a helpful error message
        let mut parts = Vec::new();
        if !error_log.is_empty() {
            parts.push(format!("Failed providers: [{}]", error_log.join(" | ")));
        }
        if !skipped_log.is_empty() {
            parts.push(format!("Skipped (cooldown): [{}]", skipped_log.join(", ")));
        }
        if parts.is_empty() {
            parts.push("No providers configured. Please add an API key in Settings â†’ LLM.".to_string());
        }

        Err(AppError::LlmGateway(format!(
            "Auto-Route exhausted. {}",
            parts.join(" ")
        )))
    }

    /// Convenience: send a single user prompt with an optional system prompt.
    pub async fn prompt(
        &self,
        system_prompt: Option<&str>,
        user_prompt: &str,
    ) -> AppResult<String> {
        let mut messages = Vec::new();
        if let Some(sys) = system_prompt {
            messages.push(LlmMessage::system(sys));
        }
        messages.push(LlmMessage::user(user_prompt));
        let req = LlmRequest::new(messages);
        let resp = self.complete(req).await?;
        Ok(resp.content)
    }

    /// Summarise a conversation history into a compact context string.
    /// Used by the session manager when the context window is getting full.
    pub async fn summarise_history(
        &self,
        history: &[crate::orchestrator::session::Message],
    ) -> AppResult<String> {
        if history.is_empty() {
            return Ok(String::new());
        }

        let history_text: String = history
            .iter()
            .map(|m| format!("[{}]: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let system = "Báº¡n lÃ  trá»£ lÃ½ tÃ³m táº¯t há»™i thoáº¡i. \
                      Nhiá»‡m vá»¥: tÃ³m táº¯t lá»‹ch sá»­ há»™i thoáº¡i sau Ä‘Ã¢y thÃ nh \
                      má»™t Ä‘oáº¡n ngáº¯n gá»n (â‰¤ 300 tá»«), báº±ng ngÃ´n ngá»¯ cá»§a cuá»™c há»™i thoáº¡i, \
                      giá»¯ láº¡i táº¥t cáº£ thÃ´ng tin quan trá»ng (file path, sá»‘ liá»‡u, yÃªu cáº§u \
                      Ä‘Ã£ hoÃ n thÃ nh, quyáº¿t Ä‘á»‹nh Ä‘Ã£ Ä‘Æ°á»£c Ä‘Æ°a ra).";

        let prompt = format!(
            "Lá»‹ch sá»­ há»™i thoáº¡i cáº§n tÃ³m táº¯t:\n\n{history_text}\n\n\
             HÃ£y viáº¿t báº£n tÃ³m táº¯t:"
        );

        let messages = vec![LlmMessage::system(system), LlmMessage::user(prompt)];

        let req = LlmRequest::new(messages)
            .with_max_tokens(512)
            .with_temperature(0.1)
            .with_complexity(request::TaskComplexity::Fast);

        let resp = self.complete(req).await?;
        Ok(resp.content)
    }

    /// Generate a structured JSON response by passing a JSON schema.
    pub async fn complete_json(
        &self,
        messages: Vec<LlmMessage>,
        schema: serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        let req = LlmRequest::new(messages)
            .with_temperature(0.0)
            .with_json_schema(schema);

        let resp = self.complete(req).await?;

        serde_json::from_str(&resp.content).map_err(|e| {
            AppError::LlmGateway(format!(
                "Failed to parse JSON response: {e}\nRaw content: {}",
                &resp.content[..resp.content.len().min(500)]
            ))
        })
    }

    // â”€â”€ Configuration management â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€



    /// Return the current configuration.
    pub async fn config(&self) -> LlmConfig {
        self.config.read().await.clone()
    }

    /// Update the gateway configuration dynamically.
    pub async fn update_config(&mut self, new_cfg: LlmConfig) -> AppResult<()> {
        *self.config.write().await = new_cfg.clone();
        *self.providers.write().await = Self::build_providers(&new_cfg);
        Ok(())
    }

    // â”€â”€ Health checks â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Check if the currently configured primary provider is reachable.
    pub async fn health_check(&self) -> AppResult<bool> {
        let cfg = self.config.read().await;
        let providers = self.providers.read().await;
        if let Some(p) = providers.get(&cfg.default_provider) {
            return Ok(p.health_check().await);
        }
        Ok(false)
    }

    /// Check if a specific provider is reachable.
    pub async fn health_check_provider(&self, provider_id: &str) -> AppResult<bool> {
        let providers = self.providers.read().await;
        match providers.get(provider_id) {
            Some(p) => Ok(p.health_check().await),
            None => Ok(false),
        }
    }

    /// Detect the context window limit of the currently configured provider.
    pub async fn detect_context_limit(&self) -> AppResult<usize> {
        let cfg = self.config.read().await;
        let limit = match cfg.default_provider.as_str() {
            "gemini" => 1_048_576, // 1M tokens for Gemini 1.5 Pro/Flash
            "anthropic" => 200_000,
            "openai" => 128_000,
            "ollama" | "lmstudio" => 128_000, // Safe default for modern local models (e.g. Llama 3)
            _ => 32_768,
        };
        Ok(limit)
    }

    // â”€â”€ Metrics & diagnostics â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Return a snapshot of gateway metrics.
    pub async fn metrics(&self) -> GatewayMetrics {
        self.metrics.lock().await.clone()
    }

    /// Return token cache statistics.
    pub async fn cache_stats(&self) -> cache::CacheStats {
        self.cache.stats().await
    }

    /// Flush the token cache.
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }

    // â”€â”€ Private helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    fn primary_model(&self, cfg: &LlmConfig, complexity: &TaskComplexity) -> String {
        match complexity {
            TaskComplexity::Fast => cfg.fast_model.clone(),
            TaskComplexity::Balanced => cfg.default_model.clone(),
            TaskComplexity::Reasoning => cfg.reasoning_model.clone(),
        }
    }


    /// 9Router-style pre-flight: open a TCP connection within 2 s.
    /// Used to skip local providers (Ollama, LM Studio) that aren't running.
    async fn tcp_reachable(addr: &str) -> bool {
        use tokio::net::TcpStream;
        match tokio::time::timeout(
            Duration::from_secs(2),
            TcpStream::connect(addr),
        )
        .await
        {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }

    /// Attempt the provider with exponential-backoff retry.
    async fn try_provider_with_retry(&self, provider: &Arc<dyn LlmProvider>, req: &LlmRequest) -> AppResult<LlmResponse> {
        for (attempt, &delay_ms) in RETRY_DELAYS_MS.iter().enumerate() {
            match provider.complete(req).await {
                Ok(resp) => {
                    if attempt > 0 {
                        info!(attempt = attempt + 1, "LLM request succeeded on retry");
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    let should_retry = Self::is_retryable_error(&e);
                    warn!(
                        attempt = attempt + 1,
                        error   = %e,
                        retrying = should_retry,
                        delay_ms,
                        "LLM request failed"
                    );
                    if should_retry && attempt < RETRY_DELAYS_MS.len() - 1 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(AppError::LlmGateway("All retry attempts exhausted".into()))
    }



    fn is_retryable_error(err: &AppError) -> bool {
        let msg = err.to_string().to_lowercase();
        msg.contains("429")
            || msg.contains("503")
            || msg.contains("502")
            || msg.contains("connection reset")
            || msg.contains("timeout")
            || msg.contains("temporarily unavailable")
    }

    async fn record_success(&self, req: &LlmRequest, resp: &LlmResponse) {
        let mut m = self.metrics.lock().await;
        m.total_requests += 1;
        m.successful_requests += 1;
        m.total_latency_ms += resp.latency_ms;
        let total_tokens = resp.usage.total_tokens as u64;
        m.total_prompt_tokens += resp.usage.prompt_tokens as u64;
        m.total_completion_tokens += resp.usage.completion_tokens as u64;

        if let Some(agent_id) = &req.agent_id {
            *m.tokens_per_agent.entry(agent_id.clone()).or_insert(0) += total_tokens;
        } else {
            *m.tokens_per_agent.entry("unknown".to_string()).or_insert(0) += total_tokens;
        }
        let model_key = format!("{}/{}", resp.provider, resp.model);
        *m.tokens_per_model.entry(model_key.clone()).or_insert(0) += total_tokens;
        *m.success_per_model.entry(model_key).or_insert(0) += 1;

        match resp.provider.as_str() {
            "gemini" | "openai" | "anthropic" => m.cloud_requests += 1,
            _ => m.local_requests += 1,
        }
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 7. Token estimation helper (shared with session.rs)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Rough token count estimate. See `session.rs` for rationale.
pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let non_ascii = text.chars().filter(|c| !c.is_ascii()).count();
    let ratio = if chars > 0 {
        non_ascii as f32 / chars as f32
    } else {
        0.0
    };
    if ratio > 0.15 {
        (chars / 2).max(1)
    } else {
        (chars / 4).max(1)
    }
}

// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// 8. Unit tests
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppConfig;

    fn default_gateway() -> LlmGateway {
        LlmGateway::new(AppConfig::default().llm)
    }

    // â”€â”€ Token estimation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_estimate_tokens_ascii() {
        let t = estimate_tokens("Hello, how are you?");
        assert!(t >= 3 && t <= 8, "Expected ~5 tokens, got {t}");
    }

    #[test]
    fn test_estimate_tokens_vietnamese() {
        let t = estimate_tokens("Xin chÃ o, Ä‘Ã¢y lÃ  tiáº¿ng Viá»‡t cÃ³ dáº¥u");
        assert!(t >= 10, "Expected more tokens for Vietnamese, got {t}");
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 1);
    }

    // â”€â”€ LlmRequest helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_request_cache_key_deterministic() {
        let msgs = vec![
            LlmMessage::system("You are a helpful assistant."),
            LlmMessage::user("Xin chÃ o!"),
        ];
        let req1 = LlmRequest::new(msgs.clone()).with_temperature(0.2);
        let req2 = LlmRequest::new(msgs).with_temperature(0.2);
        // Different request_id UUIDs but same content â†’ same cache key
        assert_eq!(
            req1.cache_key("gemini-1.5-pro"),
            req2.cache_key("gemini-1.5-pro")
        );
    }

    #[test]
    fn test_request_cache_key_differs_on_model() {
        let msgs = vec![LlmMessage::user("hello")];
        let req = LlmRequest::new(msgs);
        assert_ne!(req.cache_key("gemini-1.5-pro"), req.cache_key("gpt-4o"));
    }

    #[test]
    fn test_request_cache_key_differs_on_temperature() {
        let msgs = vec![LlmMessage::user("hello")];
        let req1 = LlmRequest::new(msgs.clone()).with_temperature(0.0);
        let req2 = LlmRequest::new(msgs).with_temperature(1.0);
        assert_ne!(req1.cache_key("model"), req2.cache_key("model"));
    }

    #[test]
    fn test_request_temperature_clamped() {
        let req = LlmRequest::new(vec![]).with_temperature(5.0);
        assert_eq!(req.temperature, 2.0);
        let req2 = LlmRequest::new(vec![]).with_temperature(-1.0);
        assert_eq!(req2.temperature, 0.0);
    }

    #[test]
    fn test_request_estimated_tokens() {
        let req = LlmRequest::new(vec![
            LlmMessage::system("You are an assistant."),
            LlmMessage::user("Xin chÃ o"),
        ]);
        let t = req.estimated_input_tokens();
        assert!(t >= 5, "Expected at least 5 tokens, got {t}");
    }

    // â”€â”€ Gateway construction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_gateway_constructs_without_panic() {
        let _gw = default_gateway();
    }

    #[test]
    fn test_gateway_no_providers_when_keys_missing() {
        let gw = default_gateway();
        let providers = gw.providers.blocking_read();
        // Default config has no API keys â†’ no cloud providers registered
        assert!(
            !providers.contains_key("gemini") && !providers.contains_key("openai") && !providers.contains_key("anthropic"),
            "Expected no cloud providers without API keys"
        );
        // Local providers (ollama/lmstudio) are always registered
        assert!(providers.contains_key("ollama"));
        assert!(providers.contains_key("lmstudio"));
    }

    // â”€â”€ Token cache â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[tokio::test]
    async fn test_cache_miss_then_hit() {
        let cache = cache::TokenCache::new(3600, 10);
        let key = "test-key-123";

        // Miss
        assert!(cache.get(key).await.is_none());

        // Store
        let resp = LlmResponse {
            request_id: uuid::Uuid::new_v4(),
            content: "cached response".into(),
            stop_reason: StopReason::Stop,
            usage: LlmUsage::default(),
            provider: "test".into(),
            model: "test-model".into(),
            from_cache: false,
            latency_ms: 100,
            received_at: Utc::now(),
        };
        cache.put(key.to_string(), resp.clone()).await;

        // Hit
        let hit = cache.get(key).await;
        assert!(hit.is_some());
        assert!(hit.unwrap().from_cache);
    }

    #[tokio::test]
    async fn test_cache_expired_entry_is_miss() {
        // TTL = 0 â†’ immediately expired
        let cache = cache::TokenCache::new(0, 10);
        let key = "expiry-test";
        let resp = LlmResponse {
            request_id: uuid::Uuid::new_v4(),
            content: "will expire".into(),
            stop_reason: StopReason::Stop,
            usage: LlmUsage::default(),
            provider: "test".into(),
            model: "test-model".into(),
            from_cache: false,
            latency_ms: 10,
            received_at: Utc::now(),
        };
        cache.put(key.to_string(), resp).await;
        // Even immediately after insertion, TTL=0 â†’ expired
        assert!(cache.get(key).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = cache::TokenCache::new(3600, 100);
        let _ = cache.get("nonexistent").await; // 1 miss
        let stats = cache.stats().await;
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    // â”€â”€ Metrics â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[tokio::test]
    async fn test_metrics_default() {
        let gw = default_gateway();
        let m = gw.metrics().await;
        assert_eq!(m.total_requests, 0);
        assert_eq!(m.avg_latency_ms(), 0.0);
    }

    // â”€â”€ Provider kind â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_provider_kind_is_cloud() {
        assert!(ProviderKind::Gemini.is_cloud());
        assert!(ProviderKind::OpenAi.is_cloud());
        assert!(!ProviderKind::Ollama.is_cloud());
        assert!(!ProviderKind::LmStudio.is_cloud());
    }

    #[test]
    fn test_provider_kind_from_str() {
        use std::str::FromStr;
        assert_eq!(
            ProviderKind::from_str("gemini").unwrap(),
            ProviderKind::Gemini
        );
        assert_eq!(
            ProviderKind::from_str("OPENAI").unwrap(),
            ProviderKind::OpenAi
        );
        assert_eq!(
            ProviderKind::from_str("ollama").unwrap(),
            ProviderKind::Ollama
        );
        assert_eq!(
            ProviderKind::from_str("lm studio").unwrap(),
            ProviderKind::LmStudio
        );
        assert!(ProviderKind::from_str("unknown_provider").is_err());
    }

    // â”€â”€ LlmMessage helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_message_constructors() {
        let sys = LlmMessage::system("system text");
        let user = LlmMessage::user("user text");
        let asst = LlmMessage::assistant("assistant text");

        assert_eq!(sys.role, MessageRole::System);
        assert_eq!(user.role, MessageRole::User);
        assert_eq!(asst.role, MessageRole::Assistant);
    }

    #[test]
    fn test_message_role_display() {
        assert_eq!(MessageRole::System.to_string(), "system");
        assert_eq!(MessageRole::User.to_string(), "user");
        assert_eq!(MessageRole::Assistant.to_string(), "assistant");
    }

    // â”€â”€ Retryable error detection â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_retryable_error_detection() {
        let err_429 = AppError::LlmGateway("HTTP 429 Too Many Requests".into());
        let err_503 = AppError::LlmGateway("Service 503 unavailable".into());
        let err_400 = AppError::LlmGateway("400 Bad Request â€“ invalid key".into());

        assert!(LlmGateway::is_retryable_error(&err_429));
        assert!(LlmGateway::is_retryable_error(&err_503));
        assert!(!LlmGateway::is_retryable_error(&err_400));
    }

    // â”€â”€ Provider Body Building â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_gemini_build_body() {
        let req = LlmRequest::new(vec![
            LlmMessage::system("sys instruction"),
            LlmMessage::user("hello"),
        ])
        .with_temperature(0.5)
        .with_max_tokens(100);

        let provider = GeminiProvider::new("api-key", "gemini-2.0-flash", None, None);
        let body = provider.build_body(&req);

        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys instruction");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hello");
        assert_eq!(body["generationConfig"]["temperature"], 0.5);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 100);
    }

    #[test]
    fn test_openai_build_body() {
        let req = LlmRequest::new(vec![
            LlmMessage::system("sys instruction"),
            LlmMessage::user("hello"),
        ])
        .with_temperature(0.5)
        .with_max_tokens(100);

        let provider = OpenAiCompatProvider::new(
            ProviderKind::OpenAi, "gpt-4o", None, None,
            "https://api.openai.com/v1", Some("api-key".into()));
        let body = provider.build_body(&req);

        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "sys instruction");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "hello");
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["max_tokens"], 100);
    }

    // â”€â”€ TokenCache Eviction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[tokio::test]
    async fn test_cache_eviction_max_entries() {
        let cache = cache::TokenCache::new(3600, 2);
        let resp_template = LlmResponse {
            request_id: uuid::Uuid::new_v4(),
            content: "dummy".into(),
            stop_reason: StopReason::Stop,
            usage: LlmUsage::default(),
            provider: "test".into(),
            model: "model".into(),
            from_cache: false,
            latency_ms: 10,
            received_at: Utc::now(),
        };

        cache.put("key1".to_string(), resp_template.clone()).await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        cache.put("key2".to_string(), resp_template.clone()).await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        cache.put("key3".to_string(), resp_template.clone()).await;

        assert!(cache.get("key1").await.is_none());
        assert!(cache.get("key2").await.is_some());
        assert!(cache.get("key3").await.is_some());
    }

    // â”€â”€ Mock Provider & Fallback â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    
    struct MockProvider {
        kind: ProviderKind,
        should_fail: bool,
    }
    
    #[async_trait]
    impl LlmProvider for MockProvider {
        fn model(&self) -> &str { "mock-model" }
        async fn complete(&self, req: &LlmRequest) -> AppResult<LlmResponse> {
            if self.should_fail {
                return Err(AppError::LlmGateway("HTTP 503 Service unavailable".into()));
            }
            Ok(LlmResponse {
                request_id: req.request_id,
                content: "Mock success".into(),
                stop_reason: StopReason::Stop,
                usage: LlmUsage::default(),
                provider: self.kind.to_string(),
                model: "mock-model".into(),
                from_cache: false,
                latency_ms: 5,
                received_at: Utc::now(),
            })
        }

    async fn health_check(&self) -> bool { !self.should_fail }
    }

    #[tokio::test]
    async fn test_hybrid_mode_fallback() {
        // Build a gateway and inject mock providers directly into the HashMap.
        let gw = LlmGateway::new(AppConfig::default().llm);
        {
            let mut p = gw.providers.write().await;
            p.insert("openai".to_string(),
                Arc::new(MockProvider { kind: ProviderKind::OpenAi, should_fail: true }));
            p.insert("ollama".to_string(),
                Arc::new(MockProvider { kind: ProviderKind::Ollama, should_fail: false }));
        }
        // Set active provider to openai so routing tries it first, then falls back to ollama.
        gw.config.write().await.default_provider = "openai".to_string();
        
        let req = LlmRequest::new(vec![LlmMessage::user("test")]).with_max_tokens(10);
        let result = gw.complete(req).await;
        assert!(result.is_ok(), "Should fallback to ollama and succeed: {:?}", result.err());
        assert_eq!(result.unwrap().provider, "Ollama (Local)");
    }
}

// =============================================================================
// 9. GENAI BRIDGE – Native Tool Calling (Phase 1)
// =============================================================================

pub mod genai_bridge {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use anyhow::{Result, Context};
    use tracing::{debug, info};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NativeToolCall {
        pub call_id: String,
        pub tool_name: String,
        pub arguments: Value,
    }

    #[derive(Debug, Clone)]
    pub enum ToolAwareResponse {
        Text(String),
        ToolCalls(Vec<NativeToolCall>),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolResult {
        pub call_id: String,
        pub tool_name: String,
        pub content: String,
    }

    #[derive(Debug, Clone)]
    pub enum ToolChatMessage {
        System(String),
        User(String),
        Assistant(String),
        ToolResults(Vec<ToolResult>),
    }

    pub struct GenAiBridge {
        client: genai::Client,
        model: String,
    }

    impl GenAiBridge {
        pub fn new(api_key: Option<&str>, model: impl Into<String>) -> Self {
            let model = model.into();
            let client = if let Some(key) = api_key {
                use genai::resolver::{AuthData, AuthResolver};
                let key_owned = key.to_string();
                let auth_resolver = AuthResolver::from_resolver_fn(
                    move |_model_iden: genai::ModelIden| {
                        let key_clone = key_owned.clone();
                        Ok(Some(AuthData::from_single(key_clone)))
                    }
                );
                genai::ClientBuilder::default()
                    .with_auth_resolver(auth_resolver)
                    .build()
            } else {
                genai::Client::default()
            };
            Self { client, model }
        }

        fn mcp_to_genai_tools(mcp_tools: &[crate::mcp::McpTool]) -> Vec<genai::chat::Tool> {
            mcp_tools.iter().map(|t| {
                genai::chat::Tool::new(t.name.clone())
                    .with_description(t.description.clone())
                    .with_schema(t.input_schema.clone())
            }).collect()
        }

        fn build_chat_messages(messages: &[ToolChatMessage]) -> (String, Vec<genai::chat::ChatMessage>) {
            let mut chat_messages = Vec::new();
            let mut system_text = String::new();
            for msg in messages {
                match msg {
                    ToolChatMessage::System(text) => {
                        system_text.push_str(text);
                        system_text.push('\n');
                    }
                    ToolChatMessage::User(text) => {
                        chat_messages.push(genai::chat::ChatMessage::user(text.clone()));
                    }
                    ToolChatMessage::Assistant(text) => {
                        chat_messages.push(genai::chat::ChatMessage::assistant(text.clone()));
                    }
                    ToolChatMessage::ToolResults(results) => {
                        for result in results {
                            chat_messages.push(genai::chat::ChatMessage::user(
                                format!("[Tool Result]\nTool: {}\nResult:\n{}",
                                    result.tool_name, result.content)
                            ));
                        }
                    }
                }
            }
            (system_text, chat_messages)
        }

        pub async fn complete_with_tools(
            &self,
            messages: &[ToolChatMessage],
            tools: &[crate::mcp::McpTool],
            temperature: f64,
        ) -> Result<ToolAwareResponse> {
            use genai::chat::{ChatRequest, ChatOptions};

            let (system_text, chat_messages) = Self::build_chat_messages(messages);
            let genai_tools = Self::mcp_to_genai_tools(tools);

            let mut chat_req = ChatRequest::new(chat_messages);
            if !system_text.trim().is_empty() {
                chat_req = chat_req.with_system(system_text.trim());
            }
            if !genai_tools.is_empty() {
                chat_req = chat_req.with_tools(genai_tools);
            }

            let chat_options = ChatOptions::default().with_temperature(temperature);
            info!(model = %self.model, tools = tools.len(), "GenAI: executing chat with tools");

            let response = self.client
                .exec_chat(&self.model, chat_req, Some(&chat_options))
                .await
                .context("GenAI exec_chat failed")?;

            let tool_calls = response.tool_calls();
            if !tool_calls.is_empty() {
                let native_calls: Vec<NativeToolCall> = tool_calls.iter().map(|tc| {
                    NativeToolCall {
                        call_id: tc.call_id.clone(),
                        tool_name: tc.fn_name.clone(),
                        arguments: tc.fn_arguments.clone(),
                    }
                }).collect();
                info!(count = native_calls.len(), "GenAI: LLM requested tool calls");
                return Ok(ToolAwareResponse::ToolCalls(native_calls));
            }

            let text = response.first_text().unwrap_or_default().to_string();
            debug!(len = text.len(), "GenAI: LLM returned text response");
            Ok(ToolAwareResponse::Text(text))
        }
    }
}

// =============================================================================
// LlmGateway – GenAI Bridge factory methods
// =============================================================================

impl LlmGateway {
    pub async fn create_genai_bridge(&self) -> genai_bridge::GenAiBridge {
        let cfg = self.config.read().await;
        let (key, model) = Self::bridge_creds(&cfg, false);
        genai_bridge::GenAiBridge::new(key.as_deref(), model)
    }

    pub async fn create_genai_bridge_reasoning(&self) -> genai_bridge::GenAiBridge {
        let cfg = self.config.read().await;
        let (key, model) = Self::bridge_creds(&cfg, true);
        genai_bridge::GenAiBridge::new(key.as_deref(), model)
    }

    fn bridge_creds(cfg: &LlmConfig, use_reasoning: bool) -> (Option<String>, String) {
        let (provider, model) = if use_reasoning {
            (cfg.reasoning_provider.as_str(), cfg.reasoning_model.clone())
        } else {
            (cfg.default_provider.as_str(), cfg.default_model.clone())
        };
        let key = match provider {
            "gemini"    => cfg.credentials.gemini_api_key.clone(),
            "anthropic" => cfg.credentials.anthropic_api_key.clone(),
            "openai"    => cfg.credentials.openai_api_key.clone(),
            _           => cfg.credentials.gemini_api_key.clone(),
        };
        (key, model)
    }
}
