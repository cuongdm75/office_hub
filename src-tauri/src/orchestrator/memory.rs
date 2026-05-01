// ============================================================================
// Office Hub – orchestrator/memory.rs
//
// Long-term Memory utilizing SQLite FTS5 (Full-Text Search).
// Automatically indexes session context and provides rapid semantic-like
// text matching without heavy Vector DB dependencies.
// ============================================================================

use rusqlite::{params, Connection, Result as SqlResult};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use chrono::Utc;
use tracing::{info, debug};

#[derive(Clone)]
pub struct MemoryStore {
    pub conn: Arc<Mutex<Connection>>,
}

impl MemoryStore {
    /// Initialize the Memory Store, creating the SQLite DB and FTS5 table if they don't exist.
    pub fn new(db_path: PathBuf) -> anyhow::Result<Self> {
        let conn = Connection::open(&db_path)
            .map_err(|e| anyhow::anyhow!("Failed to open memory db: {}", e))?;
        
        // Setup FTS5 virtual table
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS memory USING fts5(
                session_id UNINDEXED,
                workspace_id UNINDEXED,
                topic,
                content,
                timestamp UNINDEXED
            )",
            [],
        ).map_err(|e| anyhow::anyhow!("Failed to create FTS5 table: {}", e))?;

        // Setup Telemetry table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS telemetry_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                workspace_id TEXT,
                agent_name TEXT,
                action TEXT,
                latency_ms INTEGER,
                tokens_used INTEGER,
                status TEXT,
                timestamp TEXT
            )",
            [],
        ).map_err(|e| anyhow::anyhow!("Failed to create telemetry_logs table: {}", e))?;

        info!("Long-term Memory & Telemetry initialized at {:?}", db_path);
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Insert a telemetry log for agent execution observability.
    pub fn log_telemetry(&self, session_id: &str, workspace_id: Option<&str>, agent: &str, action: &str, latency_ms: i64, tokens: usize, status: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let timestamp = Utc::now().to_rfc3339();
        let ws_id = workspace_id.unwrap_or("default");
        conn.execute(
            "INSERT INTO telemetry_logs (session_id, workspace_id, agent_name, action, latency_ms, tokens_used, status, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![session_id, ws_id, agent, action, latency_ms, tokens, status, timestamp],
        )?;
        Ok(())
    }

    /// Insert a summarized context or key fact into long-term memory.
    pub fn insert_memory(&self, session_id: &str, workspace_id: Option<&str>, topic: &str, content: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let timestamp = Utc::now().to_rfc3339();
        let ws_id = workspace_id.unwrap_or("default");
        
        conn.execute(
            "INSERT INTO memory (session_id, workspace_id, topic, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, ws_id, topic, content, timestamp],
        )?;
        
        debug!("Memory inserted for session: {}", session_id);
        Ok(())
    }

    /// Search long-term memory using FTS5 `MATCH` syntax.
    pub fn search(&self, workspace_id: Option<&str>, query: &str, limit: usize) -> SqlResult<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let ws_id = workspace_id.unwrap_or("default");
        
        // Basic sanitization: convert words into OR query for broader search
        let tokens: Vec<&str> = query
            .split_whitespace()
            .filter(|s| s.len() > 1 || s.chars().all(|c| c.is_alphanumeric())) // Giữ lại từ tiếng Việt ngắn gọn, bỏ các từ rác 1 ký tự
            .collect();
            
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        
        // Wrap words in quotes to prevent FTS5 syntax errors on special characters (e.g., C++, -)
        let base_fts = tokens.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(" OR ");
        
        // Ensure workspace isolation
        let fts_query = format!("workspace_id:\"{}\" AND ({})", ws_id, base_fts);

        let mut stmt = conn.prepare(
            "SELECT topic, content, timestamp FROM memory WHERE memory MATCH ?1 ORDER BY rank LIMIT ?2"
        )?;

        let results = stmt.query_map(params![fts_query, limit], |row| {
            let topic: String = row.get(0)?;
            let content: String = row.get(1)?;
            let ts: String = row.get(2)?;
            Ok(format!("[{}] {}: {}", ts, topic, content))
        })?;

        let mut memories = Vec::new();
        for r in results {
            memories.push(r?);
        }
        
        if !memories.is_empty() {
            debug!("Found {} memory items for query '{}'", memories.len(), fts_query);
        }
        
        Ok(memories)
    }
}
