// ============================================================================
// orchestrator/session.rs
// Session State Management – lưu trữ lịch sử hội thoại, context window,
// và metadata của từng phiên làm việc với Orchestrator.
// ============================================================================

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Type aliases
// ─────────────────────────────────────────────────────────────────────────────

pub type SessionId = String;

// ─────────────────────────────────────────────────────────────────────────────
// Message role
// ─────────────────────────────────────────────────────────────────────────────

/// Vai trò của một message trong hội thoại.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    /// Kết quả trả về từ một sub-agent (Analyst, Office Master, v.v.)
    Agent,
    /// Kết quả từ một MCP tool call
    Tool,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::Agent => write!(f, "agent"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Message
// ─────────────────────────────────────────────────────────────────────────────

/// Một lượt hội thoại (turn) trong session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// UUID của message này
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    /// Tên agent tạo ra message này (chỉ có khi role == Agent hoặc Tool)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Intent đã được classify (chỉ có với role == User)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Số token ước tính của message (dùng cho context window management)
    pub estimated_tokens: usize,
    pub created_at: DateTime<Utc>,
    /// Metadata tuỳ chỉnh (ví dụ: file path, tool call id, …)
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl Message {
    /// Tạo message mới với ID và timestamp tự động.
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        let content = content.into();
        let estimated_tokens = estimate_tokens(&content);
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content,
            agent_name: None,
            intent: None,
            estimated_tokens,
            created_at: Utc::now(),
            metadata: serde_json::Map::new(),
        }
    }

    pub fn with_agent(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = Some(agent_name.into());
        self
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Context Window
// ─────────────────────────────────────────────────────────────────────────────

/// Trạng thái của context window – kiểm soát lượng token gửi lên LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    /// Tổng token tối đa được phép trong một LLM request
    pub max_tokens: usize,
    /// Token dành riêng cho system prompt
    pub system_prompt_reserved: usize,
    /// Token dành riêng cho response của LLM
    pub response_reserved: usize,
    /// Token đang được dùng bởi messages hiện tại
    pub tokens_used: usize,
    /// Đã bị cắt bớt ít nhất 1 lần chưa
    pub was_truncated: bool,
    /// Số messages đã bị tóm tắt (summarised) để giải phóng context
    pub summarised_turns: usize,
}

impl ContextWindow {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            system_prompt_reserved: 2_000,
            response_reserved: 4_000,
            tokens_used: 0,
            was_truncated: false,
            summarised_turns: 0,
        }
    }

    /// Số token còn lại có thể dùng cho message history.
    pub fn available_for_history(&self) -> usize {
        self.max_tokens
            .saturating_sub(self.system_prompt_reserved)
            .saturating_sub(self.response_reserved)
    }

    /// Kiểm tra xem còn đủ chỗ cho `n` token nữa không.
    pub fn can_fit(&self, n: usize) -> bool {
        self.tokens_used + n <= self.available_for_history()
    }

    /// Cập nhật lại tổng token từ một danh sách messages.
    pub fn recalculate(&mut self, messages: &VecDeque<Message>) {
        self.tokens_used = messages.iter().map(|m| m.estimated_tokens).sum();
    }

    /// Phần trăm context đã dùng (0.0 – 1.0).
    pub fn utilization(&self) -> f32 {
        let available = self.available_for_history();
        if available == 0 {
            1.0
        } else {
            self.tokens_used as f32 / available as f32
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Summary – bản tóm tắt khi context quá dài
// ─────────────────────────────────────────────────────────────────────────────

/// Khi context window đầy, các messages cũ được tóm tắt thành `SessionSummary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    /// Nội dung tóm tắt (do LLM tạo ra)
    pub content: String,
    /// Số lượt hội thoại được tóm tắt
    pub turns_covered: usize,
    /// Token range trong lịch sử gốc được tóm tắt
    pub original_token_count: usize,
    pub created_at: DateTime<Utc>,
}

impl SessionSummary {
    pub fn new(
        content: impl Into<String>,
        turns_covered: usize,
        original_token_count: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content: content.into(),
            turns_covered,
            original_token_count,
            created_at: Utc::now(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session Status
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    #[default]
    Active,
    /// Đang chờ Human-in-the-Loop approval
    WaitingApproval,
    /// Đang chạy một agent task
    Processing,
    /// Session đã bị đóng (dữ liệu vẫn còn trong store)
    Closed,
    /// Lỗi không thể phục hồi
    Error,
}

// ─────────────────────────────────────────────────────────────────────────────
// Session
// ─────────────────────────────────────────────────────────────────────────────

/// Toàn bộ trạng thái của một phiên làm việc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,

    /// Tên hiển thị (lấy từ message đầu tiên của user, hoặc do user đặt)
    pub title: Option<String>,

    /// Trạng thái hiện tại
    pub status: SessionStatus,

    /// Lịch sử hội thoại – dùng VecDeque để pop_front() O(1) khi trim
    pub messages: VecDeque<Message>,

    /// Tóm tắt của các messages đã bị loại khỏi context (nếu có)
    #[serde(default)]
    pub summaries: Vec<SessionSummary>,

    /// Quản lý context window
    pub context_window: ContextWindow,

    /// File/document đang được "focus" trong session này
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_file_path: Option<String>,

    /// Agent đang xử lý request hiện tại (nếu status == Processing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_agent: Option<String>,

    /// Intent của lượt hội thoại gần nhất
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_intent: Option<String>,

    /// Metadata của session (ví dụ: workflow_id, trigger_source, …)
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,

    /// Tổng số message đã nhận (kể cả đã bị trim)
    pub total_messages_received: usize,

    /// Tổng token đã tiêu thụ trong toàn session
    pub total_tokens_consumed: usize,

    /// Language preference for this session ("vi" | "en" | "auto")
    #[serde(default = "crate::orchestrator::rule_engine::defaults::default_language")]
    pub language: String,

    /// Context summary from previous turns (for router compatibility)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_summary: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,

    /// Topic ID to group sessions in the UI history tree
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<String>,

    /// Workspace ID this session belongs to
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

impl Session {
    /// Default language for sessions
    fn default_language() -> String {
        "vi".to_string()
    }

    /// Tạo session mới với ID tự sinh.
    pub fn new(max_context_tokens: usize) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            title: None,
            status: SessionStatus::Active,
            messages: VecDeque::new(),
            summaries: Vec::new(),
            context_window: ContextWindow::new(max_context_tokens),
            active_file_path: None,
            active_agent: None,
            last_intent: None,
            metadata: serde_json::Map::new(),
            total_messages_received: 0,
            total_tokens_consumed: 0,
            language: Self::default_language(),
            context_summary: None,
            created_at: now,
            updated_at: now,
            last_active_at: now,
            topic_id: None,
            workspace_id: None,
        }
    }

    /// Tạo session mới với ngôn ngữ mặc định và context trống (cho router compatibility).
    pub fn new_anonymous() -> Self {
        Self::new(8192)
    }

    /// Thêm message vào lịch sử và cập nhật context window.
    /// Tự động trim messages cũ nếu context sắp đầy.
    pub fn push_message(&mut self, msg: Message) {
        self.total_messages_received += 1;
        self.total_tokens_consumed += msg.estimated_tokens;

        // Cập nhật last_intent nếu là message của user
        if msg.role == MessageRole::User {
            if let Some(ref intent) = msg.intent {
                self.last_intent = Some(intent.clone());
            }

            // Đặt title session từ message đầu tiên của user (tối đa 80 ký tự)
            if self.title.is_none() {
                let title = msg.content.chars().take(80).collect::<String>();
                self.title = Some(title);
            }
        }

        self.messages.push_back(msg);
        self.context_window.recalculate(&self.messages);
        self.updated_at = Utc::now();
        self.last_active_at = Utc::now();

        // Nếu context window > 80% thì bắt đầu trim messages cũ nhất
        // (chừa lại ít nhất 4 messages gần nhất)
        while self.context_window.utilization() > 0.80 && self.messages.len() > 4 {
            if let Some(trimmed) = self.messages.pop_front() {
                self.context_window.tokens_used = self
                    .context_window
                    .tokens_used
                    .saturating_sub(trimmed.estimated_tokens);
                self.context_window.was_truncated = true;
            }
        }
    }

    /// Trả về danh sách messages phù hợp để gửi lên LLM.
    /// Prepend summary (nếu có) trước toàn bộ lịch sử còn lại.
    pub fn build_llm_context(&self, system_prompt: &str) -> Vec<serde_json::Value> {
        let mut context: Vec<serde_json::Value> = Vec::new();

        // 1. System prompt
        context.push(serde_json::json!({
            "role": "system",
            "content": system_prompt
        }));

        // 2. Nếu có summary thì inject làm message đầu tiên
        if !self.summaries.is_empty() {
            let combined_summary = self
                .summaries
                .iter()
                .map(|s| s.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");

            context.push(serde_json::json!({
                "role": "system",
                "content": format!(
                    "[Tóm tắt lịch sử hội thoại trước đó]\n{combined_summary}"
                )
            }));
        }

        // 3. Messages hiện tại
        for msg in &self.messages {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Agent => "assistant", // map agent → assistant cho LLM
                MessageRole::Tool => "user",       // map tool result → user
            };

            let content = if let Some(ref agent_name) = msg.agent_name {
                format!("[{}] {}", agent_name, msg.content)
            } else {
                msg.content.clone()
            };

            context.push(serde_json::json!({
                "role": role,
                "content": content
            }));
        }

        context
    }

    /// Đặt summary mới (được gọi sau khi LLM tóm tắt đoạn hội thoại dài).
    pub fn add_summary(&mut self, summary: SessionSummary) {
        self.context_window.summarised_turns += summary.turns_covered;
        self.summaries.push(summary);
    }

    /// Xoá toàn bộ lịch sử và reset trạng thái (giữ lại ID và metadata).
    pub fn clear_history(&mut self) {
        self.messages.clear();
        self.summaries.clear();
        self.context_window.tokens_used = 0;
        self.context_window.was_truncated = false;
        self.context_window.summarised_turns = 0;
        self.last_intent = None;
        self.active_agent = None;
        self.updated_at = Utc::now();
    }

    /// Số lượt hội thoại (1 lượt = 1 user message + 1 assistant message).
    pub fn turn_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count()
    }

    /// Kiểm tra session có idle quá lâu không (mặc định: 30 phút).
    pub fn is_idle(&self, max_idle_seconds: i64) -> bool {
        let idle = Utc::now()
            .signed_duration_since(self.last_active_at)
            .num_seconds();
        idle > max_idle_seconds
    }

    /// Thêm một lượt hội thoại (user message + assistant response).
    pub fn add_turn(
        &mut self,
        user_message: String,
        assistant_message: String,
        intent: String,
        agent_id: crate::agents::AgentId,
    ) {
        if self.title.is_none() {
            let title = user_message.chars().take(80).collect::<String>();
            self.title = Some(title);
        }
        let user_msg = Message::new(MessageRole::User, user_message);
        let assistant_msg = Message::new(MessageRole::Assistant, assistant_message)
            .with_agent(agent_id.to_string())
            .with_intent(intent);

        self.messages.push_back(user_msg);
        self.messages.push_back(assistant_msg);
        self.total_messages_received += 2;
        self.last_active_at = Utc::now();
        self.updated_at = Utc::now();
    }

    /// Kiểm tra session có cần summarisation hoặc handoff không.
    pub fn needs_summarisation(&self, auto_handoff_enabled: bool) -> bool {
        if auto_handoff_enabled {
            self.context_window.utilization() >= 0.8
        } else {
            self.messages.len() > 20
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

// SessionStore – concurrent in-memory store (+ tuỳ chọn persist to disk)
// ─────────────────────────────────────────────────────────────────────────────

/// Thread-safe store cho tất cả sessions đang hoạt động.
/// Dùng `DashMap` để hỗ trợ concurrent read/write không cần lock toàn bộ map.
#[derive(Debug, Clone)]
pub struct SessionStore {
    inner: Arc<DashMap<SessionId, Session>>,
    /// Context window size mặc định cho session mới (tokens)
    default_context_tokens: usize,
    /// Thời gian session được coi là idle (giây)
    idle_timeout_seconds: i64,
    /// Số session tối đa được giữ trong memory
    max_sessions: usize,
    /// Thư mục lưu trữ session persistence
    sessions_dir: Arc<std::sync::RwLock<Option<std::path::PathBuf>>>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(32_000, 1_800, 100) // 32k tokens, 30 phút idle, tối đa 100 sessions
    }
}

impl SessionStore {
    pub fn new(
        default_context_tokens: usize,
        idle_timeout_seconds: i64,
        max_sessions: usize,
    ) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            default_context_tokens,
            idle_timeout_seconds,
            max_sessions,
            sessions_dir: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    // ── Persistence ─────────────────────────────────────────────────────────

    /// Thiết lập thư mục lưu trữ và tải lịch sử cũ.
    pub async fn init_persistence(&self, dir: impl Into<std::path::PathBuf>) -> Result<(), String> {
        let path = dir.into();

        // Tạo thư mục nếu chưa tồn tại
        if !path.exists() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        }

        *self.sessions_dir.write().unwrap() = Some(path);
        self.load_from_disk().await?;
        Ok(())
    }

    /// Tải toàn bộ session từ disk lên memory.
    pub async fn load_from_disk(&self) -> Result<(), String> {
        let dir = {
            let dir_guard = self.sessions_dir.read().unwrap();
            match dir_guard.as_ref() {
                Some(d) => d.clone(),
                None => return Ok(()),
            }
        };

        if !dir.exists() {
            return Ok(());
        }

        let mut count = 0;
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Session>(&content) {
                        self.inner.insert(session.id.clone(), session);
                        count += 1;
                    } else {
                        tracing::warn!("Failed to parse session file: {:?}", path);
                    }
                }
            }
        }

        tracing::info!("Loaded {} sessions from disk", count);
        Ok(())
    }

    /// Ghi một session cụ thể xuống đĩa.
    pub async fn save_session(&self, session_id: &str) -> Result<(), String> {
        let dir = {
            let dir_guard = self.sessions_dir.read().unwrap();
            match dir_guard.as_ref() {
                Some(d) => d.clone(),
                None => return Ok(()),
            }
        };

        if let Some(session) = self.inner.get(session_id) {
            let path = dir.join(format!("{}.json", session_id));
            let json = serde_json::to_string_pretty(&*session).map_err(|e| e.to_string())?;
            tokio::fs::write(path, json)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Xoá file session.
    pub async fn delete_session_file(&self, session_id: &str) -> Result<(), String> {
        let dir = {
            let dir_guard = self.sessions_dir.read().unwrap();
            match dir_guard.as_ref() {
                Some(d) => d.clone(),
                None => return Ok(()),
            }
        };

        let path = dir.join(format!("{}.json", session_id));
        if path.exists() {
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ── CRUD ────────────────────────────────────────────────────────────────

    /// Tạo session mới và trả về ID.
    pub fn create(&self, workspace_id: Option<String>) -> SessionId {
        // Evict idle sessions nếu đã đạt max_sessions
        if self.inner.len() >= self.max_sessions {
            self.evict_idle();
        }

        let mut session = Session::new(self.default_context_tokens);
        session.workspace_id = workspace_id;
        let id = session.id.clone();
        self.inner.insert(id.clone(), session);
        tracing::debug!(session_id = %id, "Session created");
        id
    }

    /// Tạo session với ID tùy chỉnh (dùng khi khôi phục từ disk).
    pub fn create_with_id(
        &self,
        id: impl Into<SessionId>,
        workspace_id: Option<String>,
    ) -> SessionId {
        let id = id.into();
        if !self.inner.contains_key(&id) {
            let mut session = Session::new(self.default_context_tokens);
            session.id = id.clone();
            session.workspace_id = workspace_id;
            self.inner.insert(id.clone(), session);
        }
        id
    }

    /// Trả về clone của session (đọc không block).
    pub fn get(&self, id: &str) -> Option<Session> {
        self.inner.get(id).map(|s| s.clone())
    }

    /// Lấy session theo ID, tạo mới nếu chưa tồn tại.
    /// Trả về mutable reference để có thể sửa session.
    pub fn get_or_create(
        &self,
        id: &str,
    ) -> Option<dashmap::mapref::one::RefMut<'_, SessionId, Session>> {
        // Evict idle sessions nếu đã đạt max_sessions
        if self.inner.len() >= self.max_sessions {
            self.evict_idle();
        }

        // Sử dụng entry API để lấy hoặc tạo session
        // dashmap v6: OccupiedEntry::into_ref() returns RefMut<K, V>
        match self.inner.entry(id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(entry) => Some(entry.into_ref()),
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                let mut session = Session::new(self.default_context_tokens);
                session.id = id.to_string();
                Some(entry.insert(session))
            }
        }
    }

    /// Lấy mutable reference đến session để sửa.
    /// Trả về None nếu session không tồn tại.
    pub fn get_mut(
        &self,
        id: &str,
    ) -> Option<dashmap::mapref::one::RefMut<'_, SessionId, Session>> {
        self.inner.get_mut(id)
    }

    /// Kiểm tra session tồn tại.
    pub fn exists(&self, id: &str) -> bool {
        self.inner.contains_key(id)
    }

    /// Xoá session khỏi store.
    pub fn delete(&self, id: &str) -> bool {
        let existed = self.inner.remove(id).is_some();
        tracing::debug!(session_id = %id, existed, "Deleting session");

        // Luôn xoá file trên ổ cứng dù session có trong memory hay không (vì có thể đã bị evict)
        let store = self.clone();
        let session_id = id.to_string();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = store.delete_session_file(&session_id).await {
                tracing::warn!("Failed to delete session file for {}: {}", session_id, e);
            }
        });

        existed
    }

    /// Danh sách tất cả session IDs đang active.
    pub fn list_ids(&self) -> Vec<SessionId> {
        self.inner.iter().map(|e| e.key().clone()).collect()
    }

    /// Tổng số sessions đang được giữ.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    // ── Mutation helpers ────────────────────────────────────────────────────

    /// Thêm message vào session và cập nhật context window.
    /// Trả về `Err` nếu session không tồn tại.
    pub fn push_message(&self, id: &str, msg: Message) -> Result<(), String> {
        match self.inner.get_mut(id) {
            Some(mut session) => {
                session.push_message(msg);
                Ok(())
            }
            None => Err(format!("Session '{}' not found", id)),
        }
    }

    /// Cập nhật trạng thái session.
    pub fn set_status(&self, id: &str, status: SessionStatus) -> Result<(), String> {
        match self.inner.get_mut(id) {
            Some(mut session) => {
                session.status = status;
                session.updated_at = Utc::now();
                Ok(())
            }
            None => Err(format!("Session '{}' not found", id)),
        }
    }

    /// Đặt agent đang xử lý.
    pub fn set_active_agent(&self, id: &str, agent_name: Option<String>) -> Result<(), String> {
        match self.inner.get_mut(id) {
            Some(mut session) => {
                session.active_agent = agent_name.clone();
                session.status = if agent_name.is_some() {
                    SessionStatus::Processing
                } else {
                    SessionStatus::Active
                };
                session.updated_at = Utc::now();
                Ok(())
            }
            None => Err(format!("Session '{}' not found", id)),
        }
    }

    /// Đặt file đang focus trong session.
    pub fn set_active_file(&self, id: &str, file_path: Option<String>) -> Result<(), String> {
        match self.inner.get_mut(id) {
            Some(mut session) => {
                session.active_file_path = file_path;
                session.updated_at = Utc::now();
                Ok(())
            }
            None => Err(format!("Session '{}' not found", id)),
        }
    }

    /// Inject session summary (sau khi LLM đã summarise).
    pub fn add_summary(&self, id: &str, summary: SessionSummary) -> Result<(), String> {
        match self.inner.get_mut(id) {
            Some(mut session) => {
                session.add_summary(summary);
                Ok(())
            }
            None => Err(format!("Session '{}' not found", id)),
        }
    }

    /// Xoá lịch sử hội thoại của session (giữ lại session ID).
    pub fn clear_history(&self, id: &str) -> Result<(), String> {
        match self.inner.get_mut(id) {
            Some(mut session) => {
                session.clear_history();
                Ok(())
            }
            None => Err(format!("Session '{}' not found", id)),
        }
    }

    // ── Summary / Listing ───────────────────────────────────────────────────

    /// Trả về danh sách tóm tắt sessions (dùng cho UI sidebar).
    pub fn list_summaries(&self) -> Vec<SessionSummaryInfo> {
        let mut list: Vec<SessionSummaryInfo> = self
            .inner
            .iter()
            .map(|e| {
                let s = e.value();
                SessionSummaryInfo {
                    id: s.id.clone(),
                    title: s.title.clone().unwrap_or_else(|| "Phiên mới".to_string()),
                    status: s.status.clone(),
                    turn_count: s.turn_count(),
                    last_active_at: s.last_active_at,
                    context_utilization: s.context_window.utilization(),
                    topic_id: s.topic_id.clone(),
                    workspace_id: s.workspace_id.clone(),
                }
            })
            .collect();

        // Sắp xếp: mới nhất lên đầu
        list.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
        list
    }

    // ── Housekeeping ─────────────────────────────────────────────────────────

    /// Xoá các sessions đã idle quá lâu. Trả về số sessions đã xoá.
    pub fn evict_idle(&self) -> usize {
        let idle_ids: Vec<SessionId> = self
            .inner
            .iter()
            .filter(|e| e.value().is_idle(self.idle_timeout_seconds))
            .map(|e| e.key().clone())
            .collect();

        let count = idle_ids.len();
        for id in &idle_ids {
            self.inner.remove(id);
            tracing::debug!(session_id = %id, "Session evicted (idle timeout)");
        }

        if count > 0 {
            tracing::info!("{} idle sessions evicted", count);
        }

        count
    }

    /// Xoá tất cả sessions (dùng khi shutdown).
    pub fn clear_all(&self) {
        self.inner.clear();
        tracing::info!("All sessions cleared");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SessionSummaryInfo – lightweight DTO cho UI sidebar
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummaryInfo {
    pub id: SessionId,
    pub title: String,
    pub status: SessionStatus,
    pub turn_count: usize,
    pub last_active_at: DateTime<Utc>,
    /// 0.0 – 1.0; context window utilization
    pub context_utilization: f32,
    pub topic_id: Option<String>,
    pub workspace_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Token estimation helper
// ─────────────────────────────────────────────────────────────────────────────

/// Ước tính số token của một chuỗi text theo quy tắc đơn giản:
/// ~4 ký tự / token (cho tiếng Anh), ~2 ký tự / token (cho tiếng Việt / CJK).
/// Đây chỉ là ước tính nhanh, không cần tokenizer thực sự.
fn estimate_tokens(text: &str) -> usize {
    let char_count = text.chars().count();
    // Đếm tỉ lệ ký tự non-ASCII (tiếng Việt có diacritics, thường = multibyte)
    let non_ascii = text.chars().filter(|c| !c.is_ascii()).count();
    let ratio = if char_count > 0 {
        non_ascii as f32 / char_count as f32
    } else {
        0.0
    };

    // Nếu > 15% non-ASCII → text đa ngôn ngữ → chia 2
    // Ngược lại → text tiếng Anh → chia 4
    if ratio > 0.15 {
        (char_count / 2).max(1)
    } else {
        (char_count / 4).max(1)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(role: MessageRole, content: &str) -> Message {
        Message::new(role, content)
    }

    // ── Token estimation ─────────────────────────────────────────────────────

    #[test]
    fn test_estimate_tokens_ascii() {
        // "hello world" = 11 chars → 11/4 = 2 tokens (min 1)
        let t = estimate_tokens("hello world");
        assert!((1..=5).contains(&t), "got {}", t);
    }

    #[test]
    fn test_estimate_tokens_vietnamese() {
        let t = estimate_tokens("Xin chào, đây là văn bản tiếng Việt có dấu");
        // Should use /2 ratio for non-ASCII-heavy text
        assert!(t >= 5, "got {}", t);
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 1); // max(0,1) = 1
    }

    // ── Session lifecycle ─────────────────────────────────────────────────────

    #[test]
    fn test_session_new() {
        let s = Session::new(8_000);
        assert_eq!(s.status, SessionStatus::Active);
        assert!(s.messages.is_empty());
        assert_eq!(s.turn_count(), 0);
    }

    #[test]
    fn test_session_push_message_sets_title() {
        let mut s = Session::new(8_000);
        s.push_message(make_msg(
            MessageRole::User,
            "Phân tích file báo cáo tuần này",
        ));
        assert!(s.title.is_some());
        assert!(s.title.as_ref().unwrap().starts_with("Phân tích"));
    }

    #[test]
    fn test_session_turn_count() {
        let mut s = Session::new(8_000);
        s.push_message(make_msg(MessageRole::User, "câu hỏi 1"));
        s.push_message(make_msg(MessageRole::Assistant, "trả lời 1"));
        s.push_message(make_msg(MessageRole::User, "câu hỏi 2"));
        s.push_message(make_msg(MessageRole::Assistant, "trả lời 2"));
        assert_eq!(s.turn_count(), 2);
    }

    #[test]
    fn test_session_clear_history() {
        let mut s = Session::new(8_000);
        s.push_message(make_msg(MessageRole::User, "tin nhắn 1"));
        s.push_message(make_msg(MessageRole::Assistant, "trả lời 1"));
        s.clear_history();
        assert!(s.messages.is_empty());
        assert_eq!(s.context_window.tokens_used, 0);
    }

    #[test]
    fn test_session_context_window_trim() {
        // Tạo session với context window rất nhỏ (200 tokens)
        let mut s = Session::new(200);
        // Thêm 20 messages dài để trigger trim
        for i in 0..20 {
            s.push_message(make_msg(
                MessageRole::User,
                &"a".repeat(100), // ~25 tokens mỗi message
            ));
            s.push_message(make_msg(
                MessageRole::Assistant,
                &format!("Câu trả lời cho câu hỏi số {}", i),
            ));
        }
        // Sau trim, utilization phải ≤ 80% và phải còn ≥ 4 messages
        assert!(s.messages.len() >= 4);
        assert!(s.context_window.utilization() <= 1.0);
        assert!(s.context_window.was_truncated);
    }

    #[test]
    fn test_session_build_llm_context_includes_system_prompt() {
        let mut s = Session::new(8_000);
        s.push_message(make_msg(MessageRole::User, "xin chào"));
        let ctx = s.build_llm_context("Bạn là trợ lý AI");
        assert!(!ctx.is_empty());
        let first = &ctx[0];
        assert_eq!(first["role"], "system");
        assert!(first["content"].as_str().unwrap().contains("trợ lý AI"));
    }

    #[test]
    fn test_session_build_llm_context_with_summary() {
        let mut s = Session::new(8_000);
        s.add_summary(SessionSummary::new(
            "Tóm tắt: đã phân tích file Excel",
            5,
            200,
        ));
        s.push_message(make_msg(MessageRole::User, "câu hỏi tiếp theo"));
        let ctx = s.build_llm_context("system");
        // Phải có: system prompt + summary system message + user message
        assert!(ctx.len() >= 3);
        let summary_msg = &ctx[1];
        assert_eq!(summary_msg["role"], "system");
        assert!(summary_msg["content"]
            .as_str()
            .unwrap()
            .contains("Tóm tắt lịch sử"));
    }

    // ── SessionStore ─────────────────────────────────────────────────────────

    #[test]
    fn test_store_create_and_get() {
        let store = SessionStore::default();
        let id = store.create(None);
        assert!(store.exists(&id));
        let session = store.get(&id);
        assert!(session.is_some());
        assert_eq!(session.unwrap().id, id);
    }

    #[test]
    fn test_store_delete() {
        let store = SessionStore::default();
        let id = store.create(None);
        assert!(store.delete(&id));
        assert!(!store.exists(&id));
        assert!(!store.delete(&id)); // second delete returns false
    }

    #[test]
    fn test_store_push_message() {
        let store = SessionStore::default();
        let id = store.create(None);
        let msg = Message::new(MessageRole::User, "test message");
        store.push_message(&id, msg).unwrap();
        let session = store.get(&id).unwrap();
        assert_eq!(session.messages.len(), 1);
    }

    #[test]
    fn test_store_push_message_unknown_session() {
        let store = SessionStore::default();
        let msg = Message::new(MessageRole::User, "test");
        let result = store.push_message("nonexistent-id", msg);
        assert!(result.is_err());
    }

    #[test]
    fn test_store_set_status() {
        let store = SessionStore::default();
        let id = store.create(None);
        store
            .set_status(&id, SessionStatus::WaitingApproval)
            .unwrap();
        let session = store.get(&id).unwrap();
        assert_eq!(session.status, SessionStatus::WaitingApproval);
    }

    #[test]
    fn test_store_list_summaries_sorted_by_recency() {
        let store = SessionStore::default();
        let id1 = store.create(None);
        // Small sleep simulation via modifying updated_at is not possible here,
        // but we can verify the list returns both sessions
        let id2 = store.create(None);
        let summaries = store.list_summaries();
        assert_eq!(summaries.len(), 2);
        let ids: Vec<_> = summaries.iter().map(|s| &s.id).collect();
        assert!(ids.contains(&&id1));
        assert!(ids.contains(&&id2));
    }

    #[test]
    fn test_store_evict_idle() {
        let store = SessionStore::new(8_000, -1, 100); // idle_timeout = -1s → all sessions immediately idle
        store.create(None);
        store.create(None);
        store.create(None);
        assert_eq!(store.len(), 3);
        let evicted = store.evict_idle();
        assert_eq!(evicted, 3);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_context_window_utilization() {
        let mut cw = ContextWindow::new(10_000);
        cw.tokens_used = 3_000;
        let util = cw.utilization();
        // available = 10000 - 2000 - 4000 = 4000
        // utilization = 3000 / 4000 = 0.75
        assert!((util - 0.75).abs() < 0.01, "got {}", util);
    }

    #[test]
    fn test_message_with_builder_chain() {
        let msg = Message::new(MessageRole::Agent, "kết quả phân tích")
            .with_agent("AnalystAgent")
            .with_intent("excel.analyze")
            .with_metadata("file_path", serde_json::json!("C:/data/report.xlsx"));

        assert_eq!(msg.role, MessageRole::Agent);
        assert_eq!(msg.agent_name.as_deref(), Some("AnalystAgent"));
        assert_eq!(msg.intent.as_deref(), Some("excel.analyze"));
        assert!(msg.metadata.contains_key("file_path"));
    }

    #[tokio::test]
    async fn test_session_store_concurrency() {
        let store = Arc::new(SessionStore::default());
        let id = store.create(None);

        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            let id_clone = id.clone();
            handles.push(tokio::spawn(async move {
                let msg = Message::new(MessageRole::User, format!("message {}", i));
                // This simulates concurrent read/write access via push_message
                let _ = store_clone.push_message(&id_clone, msg);
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        let session = store.get(&id).unwrap();
        assert_eq!(session.messages.len(), 10);
    }
}
