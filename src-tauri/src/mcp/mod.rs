// ============================================================================
// Office Hub – mcp/mod.rs
//
// MCP (Model Context Protocol) Host
//
// Trách nhiệm:
//   1. Quản lý vòng đời các MCP Servers (install, start, stop, uninstall)
//   2. Cung cấp JSON-RPC 2.0 transport để giao tiếp với từng server
//   3. Duy trì registry các servers và tools đã đăng ký
//   4. Định tuyến tool-call requests đến đúng server
//   5. Xử lý server discovery (local filesystem + npm packages)
//
// Protocol reference: https://modelcontextprotocol.io/specification
//
// Architecture:
//   McpRegistry
//     ├── McpServerEntry[]     (registered servers with metadata)
//     └── ToolRegistry         (flat map: tool_name → server_id)
//
//   McpHost
//     ├── McpRegistry
//     ├── Transport[]          (one per running server process)
//     └── call_tool()          (public API used by Orchestrator)
// ============================================================================

pub mod broker;
pub mod internal_servers;
pub mod native_chart;
pub mod agent_mcp_adapter;

use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// JSON-RPC 2.0 types
// ─────────────────────────────────────────────────────────────────────────────

/// JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String, // must be "2.0"
    pub id: Value,       // string | number | null
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Value::String(Uuid::new_v4().to_string()),
            method: method.into(),
            params,
        }
    }

    pub fn notification(method: impl Into<String>, params: Option<Value>) -> Self {
        // Notifications have no `id`
        Self {
            jsonrpc: "2.0".to_string(),
            id: Value::Null,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 Response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    // Standard JSON-RPC error codes
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: Self::INTERNAL_ERROR,
            message: msg.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: Self::METHOD_NOT_FOUND,
            message: format!("Method not found: {}", method),
            data: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP Protocol message types
// ─────────────────────────────────────────────────────────────────────────────

/// MCP initialize request params (sent once on startup).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub roots: Option<RootsCapability>,
    #[serde(default)]
    pub sampling: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// MCP initialize response (from server).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// A tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: Value,
    /// Optional keyword aliases to improve search recall.
    /// Example: ["excel", "spreadsheet", "xlsx"] for analyze_workbook.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Helper function to translate technical tool names into user-friendly aliases
pub fn get_tool_alias(tool_name: &str) -> String {
    match tool_name {
        "search_available_tools" => "kiểm tra các công cụ phù hợp".to_string(),
        "search_memory" => "tra cứu bộ nhớ hệ thống".to_string(),
        "list_policies" => "kiểm tra chính sách bảo mật".to_string(),
        "read_file" => "đọc nội dung file".to_string(),
        "write_file" => "ghi nội dung file".to_string(),
        "search_files" => "tìm kiếm file".to_string(),
        "web_fetch" => "tra cứu thông tin web".to_string(),
        "run_python" => "chạy script phân tích".to_string(),
        "office_add_picture" => "chèn hình ảnh vào tài liệu".to_string(),
        "run_script" => "thực thi tập lệnh".to_string(),
        "read_excel_range" => "phân tích dữ liệu Excel".to_string(),
        "read_word_doc" => "đọc tài liệu Word".to_string(),
        _ => {
            let name = tool_name.replace("_", " ");
            format!("gọi tool {}", name)
        }
    }
}


/// Result of a tools/list call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolListResult {
    pub tools: Vec<McpTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// A resource exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Result of a resources/list call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceListResult {
    pub resources: Vec<McpResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Params for a tools/call request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// A single content block returned by a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String, // "text" | "image" | "resource"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>, // base64 for images
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Result of a tools/call request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Server source descriptor
// ─────────────────────────────────────────────────────────────────────────────

/// Describes where an MCP server binary comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpServerSource {
    /// A local executable on the filesystem.
    LocalBinary {
        path: PathBuf,
        #[serde(default)]
        args: Vec<String>,
    },
    /// An npm package (will be run via `npx`).
    NpmPackage {
        package: String,
        version: Option<String>,
        #[serde(default)]
        args: Vec<String>,
    },
    /// A Rust binary managed by cargo.
    CargoBin {
        crate_name: String,
        version: Option<String>,
        #[serde(default)]
        args: Vec<String>,
    },
    /// A Python script run via `uvx` or `python`.
    PythonScript {
        script_path: PathBuf,
        #[serde(default)]
        args: Vec<String>,
    },
    /// A PowerShell script run via `powershell.exe`.
    PowerShellScript {
        script_path: PathBuf,
        #[serde(default)]
        args: Vec<String>,
    },
    /// An MCP Server defined in a Markdown file with a python block.
    MarkdownSkill {
        script_path: PathBuf,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl McpServerSource {
    /// Parse a user-supplied source string into a `McpServerSource`.
    ///
    /// Supported formats:
    /// - `"npm:@modelcontextprotocol/server-filesystem"` → NpmPackage
    /// - `"cargo:mcp-server-excel"` → CargoBin
    /// - `"python:path/to/server.py"` → PythonScript
    /// - Anything else is treated as a local binary path.
    pub fn parse(source: &str) -> Self {
        if let Some(pkg) = source.strip_prefix("npm:") {
            let (package, version) = if let Some(idx) = pkg.rfind('@') {
                if idx == 0 {
                    (pkg.to_string(), None)
                } else {
                    let (p, v) = pkg.split_at(idx);
                    (p.to_string(), Some(v[1..].to_string()))
                }
            } else {
                (pkg.to_string(), None)
            };
            return McpServerSource::NpmPackage {
                package,
                version,
                args: vec![],
            };
        }
        if let Some(crate_name) = source.strip_prefix("cargo:") {
            return McpServerSource::CargoBin {
                crate_name: crate_name.to_string(),
                version: None,
                args: vec![],
            };
        }
        if let Some(script) = source.strip_prefix("python:") {
            return McpServerSource::PythonScript {
                script_path: PathBuf::from(script),
                args: vec![],
            };
        }
        if let Some(script) = source.strip_prefix("powershell:") {
            return McpServerSource::PowerShellScript {
                script_path: PathBuf::from(script),
                args: vec![],
            };
        }

        if let Some(script) = source.strip_prefix("md:") {
            return McpServerSource::MarkdownSkill {
                script_path: PathBuf::from(script),
                args: vec![],
            };
        }

        McpServerSource::LocalBinary {
            path: PathBuf::from(source),
            args: vec![],
        }
    }

    /// Build the command and arguments to launch the server process.
    pub fn to_command(&self) -> (String, Vec<String>) {
        match self {
            McpServerSource::LocalBinary { path, args } => {
                (path.to_string_lossy().to_string(), args.clone())
            }
            McpServerSource::NpmPackage {
                package,
                version,
                args,
            } => {
                let pkg = if let Some(v) = version {
                    format!("{}@{}", package, v)
                } else {
                    package.clone()
                };
                let mut full_args = vec!["--yes".to_string(), pkg];
                full_args.extend(args.clone());
                ("npx".to_string(), full_args)
            }
            McpServerSource::CargoBin {
                crate_name, args, ..
            } => {
                let full_args = args.clone();
                (crate_name.clone(), full_args)
            }
            McpServerSource::PythonScript { script_path, args } => {
                let mut full_args = vec![script_path.to_string_lossy().to_string()];
                full_args.extend(args.clone());
                ("python".to_string(), full_args)
            }
            McpServerSource::PowerShellScript { script_path, args } => {
                let mut full_args = vec![
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    script_path.to_string_lossy().to_string(),
                ];
                full_args.extend(args.clone());
                ("powershell.exe".to_string(), full_args)
            }
            McpServerSource::MarkdownSkill { script_path, args } => {
                let mut full_args = vec![
                    "-c".to_string(),
                    "import sys, re; md=open(sys.argv[1], encoding='utf-8').read(); m=re.search(r'```python\\s*(.*?)```', md, re.DOTALL); exec(m.group(1)) if m else sys.exit(1)".to_string(),
                    script_path.to_string_lossy().to_string(),
                ];
                full_args.extend(args.clone());
                ("python".to_string(), full_args)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// McpServerStatus
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    /// Đã đăng ký nhưng chưa khởi động
    Registered,
    /// Đang khởi động (đang chờ initialize handshake)
    Starting,
    /// Đã kết nối và sẵn sàng nhận requests
    Running,
    /// Đang tắt
    Stopping,
    /// Đã dừng (process đã exit)
    Stopped,
    /// Lỗi không thể phục hồi
    Error(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// McpServerEntry – metadata về một server đã đăng ký
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    /// Unique ID (UUID v4)
    pub id: String,
    /// Human-readable alias
    pub alias: String,
    /// Nguồn của server
    pub source: McpServerSource,
    /// Trạng thái hiện tại
    pub status: McpServerStatus,
    /// Thông tin server (từ initialize response)
    pub server_info: Option<ServerInfo>,
    /// Danh sách tools server này cung cấp
    pub tools: Vec<McpTool>,
    /// Danh sách resources server này cung cấp
    pub resources: Vec<McpResource>,
    /// Protocol version đã negotiate
    pub protocol_version: Option<String>,
    /// Environment variables bổ sung
    pub env: HashMap<String, String>,
    /// Thời điểm đăng ký
    pub registered_at: DateTime<Utc>,
    /// Thời điểm kết nối thành công lần cuối
    pub last_connected_at: Option<DateTime<Utc>>,
    /// Số lần tool call thành công
    pub total_calls: u64,
    /// Số lần lỗi
    pub error_count: u32,
}

impl McpServerEntry {
    pub fn new(alias: impl Into<String>, source: McpServerSource) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            alias: alias.into(),
            source,
            status: McpServerStatus::Registered,
            server_info: None,
            tools: vec![],
            resources: vec![],
            protocol_version: None,
            env: HashMap::new(),
            registered_at: Utc::now(),
            last_connected_at: None,
            total_calls: 0,
            error_count: 0,
        }
    }

    /// Trả về JSON summary để hiển thị trên UI.
    pub fn to_summary_json(&self) -> Value {
        serde_json::json!({
            "id":               self.id,
            "alias":            self.alias,
            "status":           self.status,
            "toolCount":        self.tools.len(),
            "tools":            self.tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
            "resourceCount":    self.resources.len(),
            "resources":        self.resources.clone(),
            "serverInfo":       self.server_info,
            "protocolVersion":  self.protocol_version,
            "totalCalls":       self.total_calls,
            "errorCount":       self.error_count,
            "registeredAt":     self.registered_at.to_rfc3339(),
            "lastConnectedAt":  self.last_connected_at.map(|t| t.to_rfc3339()),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// StdioTransport – JSON-RPC over stdin/stdout
// ─────────────────────────────────────────────────────────────────────────────

/// Manages the IPC connection to a single MCP server process via stdin/stdout.
pub struct StdioTransport {
    server_id: String,
    child_stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    response_map: Arc<DashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>,
    /// Reader task handle (must be kept alive)
    _reader_handle: tokio::task::JoinHandle<()>,
}

impl StdioTransport {
    /// Spawn the server process and set up the stdio transport.
    pub async fn spawn(
        server_id: impl Into<String>,
        source: &McpServerSource,
        env: &HashMap<String, String>,
    ) -> Result<(Self, Child)> {
        let (cmd, args) = source.to_command();
        let server_id = server_id.into();

        let mut command = Command::new(&cmd);
        command
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(env);

        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {cmd} {:?}", args))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to open stdin for MCP server"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to open stdout for MCP server"))?;

        let response_map: Arc<DashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>> =
            Arc::new(DashMap::new());

        let map_clone = Arc::clone(&response_map);
        let sid = server_id.clone();

        // Background reader task: reads newline-delimited JSON from stdout
        let reader_handle = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(resp) => {
                        let id_str = resp
                            .id
                            .as_ref()
                            .and_then(|v| v.as_str().map(str::to_string));

                        if let Some(id) = id_str {
                            if let Some((_, tx)) = map_clone.remove(&id) {
                                let _ = tx.send(resp);
                            } else {
                                debug!(
                                    server_id = %sid,
                                    id = %id,
                                    "Received response for unknown request id"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            server_id = %sid,
                            line = %line,
                            error = %e,
                            "Failed to parse JSON-RPC response from MCP server"
                        );
                    }
                }
            }
            info!(server_id = %sid, "MCP server stdout reader task ended");
        });

        Ok((
            Self {
                server_id,
                child_stdin: Arc::new(Mutex::new(stdin)),
                response_map,
                _reader_handle: reader_handle,
            },
            child,
        ))
    }

    /// Send a JSON-RPC request and wait for the response.
    #[instrument(skip(self), fields(server_id = %self.server_id, method = %request.method))]
    pub async fn send(
        &self,
        request: JsonRpcRequest,
        timeout_secs: u64,
    ) -> Result<JsonRpcResponse> {
        // Only register a receiver if this is a request (has id), not a notification
        let id_str = request.id.as_str().map(str::to_string);

        let rx = if let Some(ref id) = id_str {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.response_map.insert(id.clone(), tx);
            Some(rx)
        } else {
            None
        };

        // Serialize and send
        let mut line =
            serde_json::to_string(&request).context("Failed to serialize JSON-RPC request")?;
        line.push('\n');

        {
            let mut stdin = self.child_stdin.lock().await;
            stdin
                .write_all(line.as_bytes())
                .await
                .context("Failed to write to MCP server stdin")?;
            stdin.flush().await.context("Failed to flush stdin")?;
        }

        // If notification, return a synthetic OK
        let rx = match rx {
            Some(r) => r,
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: Some(Value::Null),
                    error: None,
                })
            }
        };

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(anyhow!("MCP server response channel closed unexpectedly")),
            Err(_) => {
                // Cleanup the response slot
                if let Some(id) = &id_str {
                    self.response_map.remove(id);
                }
                Err(anyhow!(
                    "MCP server '{}' did not respond within {}s",
                    self.server_id,
                    timeout_secs
                ))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// McpRegistry
// ─────────────────────────────────────────────────────────────────────────────

/// Central registry for all MCP servers and their tools.
/// Thread-safe; can be cheaply cloned (Arc-backed).
#[derive(Clone)]
pub struct McpRegistry {
    inner: Arc<McpRegistryInner>,
}

struct McpRegistryInner {
    /// server_id → McpServerEntry
    servers: DashMap<String, McpServerEntry>,
    /// tool_name → server_id  (flat index for fast lookup)
    tool_index: DashMap<String, String>,
    /// server_id → running transport
    transports: DashMap<String, Arc<StdioTransport>>,
    /// server_id → child process handle
    processes: DashMap<String, Arc<Mutex<Child>>>,
    /// Maximum number of allowed servers (from rule config)
    max_servers: usize,
}

impl McpRegistry {
    const DEFAULT_MAX_SERVERS: usize = 50;
    const INIT_TIMEOUT_SECS: u64 = 30;
    const CALL_TIMEOUT_SECS: u64 = 60;
    const MCP_PROTOCOL_VERSION: &'static str = "2024-11-05";

    pub fn new() -> Self {
        Self::with_max(Self::DEFAULT_MAX_SERVERS)
    }

    pub fn with_max(max_servers: usize) -> Self {
        Self {
            inner: Arc::new(McpRegistryInner {
                servers: DashMap::new(),
                tool_index: DashMap::new(),
                transports: DashMap::new(),
                processes: DashMap::new(),
                max_servers,
            }),
        }
    }

    // ── Server lifecycle ─────────────────────────────────────────────────────

    /// Register + start an MCP server from a source string.
    /// Returns the server ID on success.
    #[instrument(skip(self))]
    pub async fn install(&self, source_str: &str) -> Result<String> {
        if self.inner.servers.len() >= self.inner.max_servers {
            bail!(
                "Maximum number of MCP servers reached ({}). \
                 Uninstall an existing server first.",
                self.inner.max_servers
            );
        }

        let source = McpServerSource::parse(source_str);
        let alias = Self::derive_alias(&source, source_str);
        let mut entry = McpServerEntry::new(alias, source.clone());
        let server_id = entry.id.clone();

        entry.status = McpServerStatus::Starting;
        self.inner.servers.insert(server_id.clone(), entry);
        info!(server_id = %server_id, source = %source_str, "Installing MCP server");

        match self.start_server(&server_id).await {
            Ok(()) => {
                info!(server_id = %server_id, "MCP server installed and running");
                Ok(server_id)
            }
            Err(e) => {
                // Mark as error and clean up
                if let Some(mut entry) = self.inner.servers.get_mut(&server_id) {
                    entry.status = McpServerStatus::Error(e.to_string());
                }
                Err(e)
            }
        }
    }

    /// Uninstall a server: stop the process and remove from registry.
    pub async fn uninstall(&self, server_id: &str) -> Result<()> {
        self.stop_server(server_id).await?;

        // Remove all tools belonging to this server from the index
        let tool_names: Vec<String> = self
            .inner
            .tool_index
            .iter()
            .filter(|e| e.value() == server_id)
            .map(|e| e.key().clone())
            .collect();
        for tool_name in tool_names {
            self.inner.tool_index.remove(&tool_name);
        }

        self.inner.servers.remove(server_id);
        self.inner.transports.remove(server_id);
        self.inner.processes.remove(server_id);

        info!(server_id = %server_id, "MCP server uninstalled");
        Ok(())
    }

    /// Start (or restart) a registered server.
    async fn start_server(&self, server_id: &str) -> Result<()> {
        let source = self
            .inner
            .servers
            .get(server_id)
            .map(|e| e.source.clone())
            .ok_or_else(|| anyhow!("Server '{}' not found in registry", server_id))?;

        let env = self
            .inner
            .servers
            .get(server_id)
            .map(|e| e.env.clone())
            .unwrap_or_default();

        let (transport, child) = StdioTransport::spawn(server_id, &source, &env)
            .await
            .with_context(|| format!("Failed to spawn server '{}'", server_id))?;

        let transport = Arc::new(transport);
        self.inner
            .transports
            .insert(server_id.to_string(), Arc::clone(&transport));
        self.inner
            .processes
            .insert(server_id.to_string(), Arc::new(Mutex::new(child)));

        // MCP initialize handshake
        self.initialize_server(server_id, &transport).await?;

        // List tools
        let _ = self.refresh_tools(server_id, &transport).await;
        // List resources
        let _ = self.refresh_resources(server_id, &transport).await;

        if let Some(mut entry) = self.inner.servers.get_mut(server_id) {
            entry.status = McpServerStatus::Running;
            entry.last_connected_at = Some(Utc::now());
        }

        Ok(())
    }

    /// Stop a running server process.
    async fn stop_server(&self, server_id: &str) -> Result<()> {
        if let Some(mut entry) = self.inner.servers.get_mut(server_id) {
            entry.status = McpServerStatus::Stopping;
        }

        // Send shutdown notification (best-effort)
        if let Some(transport) = self.inner.transports.get(server_id) {
            let notification = JsonRpcRequest::notification("notifications/cancelled", None);
            let _ = transport.send(notification, 5).await;
        }

        // Kill the process
        if let Some(process) = self.inner.processes.get(server_id) {
            let mut child = process.lock().await;
            let _ = child.kill().await;
        }

        if let Some(mut entry) = self.inner.servers.get_mut(server_id) {
            entry.status = McpServerStatus::Stopped;
        }

        info!(server_id = %server_id, "MCP server stopped");
        Ok(())
    }

    // ── MCP protocol handshake ───────────────────────────────────────────────

    async fn initialize_server(&self, server_id: &str, transport: &StdioTransport) -> Result<()> {
        let params = InitializeParams {
            protocol_version: Self::MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {
                roots: Some(RootsCapability {
                    list_changed: false,
                }),
                sampling: None,
            },
            client_info: ClientInfo {
                name: "OfficeHub".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let req = JsonRpcRequest::new("initialize", Some(serde_json::to_value(&params).unwrap()));

        let resp = transport
            .send(req, Self::INIT_TIMEOUT_SECS)
            .await
            .context("MCP initialize request failed")?;

        if let Some(err) = resp.error {
            bail!("MCP server initialization error: {}", err.message);
        }

        let init_result: InitializeResult =
            serde_json::from_value(resp.result.unwrap_or(Value::Null))
                .context("Failed to parse MCP initialize result")?;

        // Send initialized notification
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        let _ = transport.send(notif, 5).await;

        // Update registry entry
        if let Some(mut entry) = self.inner.servers.get_mut(server_id) {
            entry.server_info = Some(init_result.server_info);
            entry.protocol_version = Some(init_result.protocol_version);
        }

        debug!(server_id = %server_id, "MCP initialize handshake complete");
        Ok(())
    }

    async fn refresh_tools(&self, server_id: &str, transport: &StdioTransport) -> Result<()> {
        let req = JsonRpcRequest::new("tools/list", None);
        let resp = transport
            .send(req, Self::INIT_TIMEOUT_SECS)
            .await
            .context("tools/list request failed")?;

        if let Some(err) = resp.error {
            bail!("tools/list error: {}", err.message);
        }

        let list: ToolListResult = serde_json::from_value(resp.result.unwrap_or(Value::Null))
            .context("Failed to parse tools/list result")?;

        // Register tools in the flat index
        for tool in &list.tools {
            self.inner
                .tool_index
                .insert(tool.name.clone(), server_id.to_string());
        }

        if let Some(mut entry) = self.inner.servers.get_mut(server_id) {
            entry.tools = list.tools;
        }

        debug!(
            server_id = %server_id,
            tool_count = self.inner
                .servers
                .get(server_id)
                .map_or(0, |e| e.tools.len()),
            "Tool list refreshed"
        );
        Ok(())
    }

    async fn refresh_resources(&self, server_id: &str, transport: &StdioTransport) -> Result<()> {
        let req = JsonRpcRequest::new("resources/list", None);
        let resp = transport
            .send(req, Self::INIT_TIMEOUT_SECS)
            .await
            .context("resources/list request failed")?;

        if let Some(err) = resp.error {
            // Some servers might not support resources/list, just warn
            tracing::warn!("resources/list error: {}", err.message);
            return Ok(());
        }

        let list: ResourceListResult = serde_json::from_value(resp.result.unwrap_or(Value::Null))
            .context("Failed to parse resources/list result")?;

        if let Some(mut entry) = self.inner.servers.get_mut(server_id) {
            entry.resources = list.resources;
        }

        debug!(
            server_id = %server_id,
            "Resource list refreshed"
        );
        Ok(())
    }

    // ── Tool invocation ──────────────────────────────────────────────────────

    /// Call a tool by name (automatically resolves which server owns it).
    #[instrument(skip(self), fields(tool_name = %tool_name))]
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<ToolCallResult> {
        let server_id = self
            .inner
            .tool_index
            .get(tool_name)
            .map(|e| e.clone())
            .ok_or_else(|| {
                anyhow!(
                    "Tool '{}' not found in any registered MCP server. \
                     Available tools: [{}]",
                    tool_name,
                    self.list_all_tool_names().join(", ")
                )
            })?;

        let transport = self
            .inner
            .transports
            .get(&server_id)
            .ok_or_else(|| anyhow!("Transport for server '{}' not available", server_id))?
            .clone();

        // Verify server is running
        let status = self
            .inner
            .servers
            .get(&server_id)
            .map(|e| e.status.clone())
            .unwrap_or(McpServerStatus::Stopped);

        if status != McpServerStatus::Running {
            bail!(
                "MCP server '{}' is not running (status: {:?}). \
                 Please restart it.",
                server_id,
                status
            );
        }

        let params = ToolCallParams {
            name: tool_name.to_string(),
            arguments,
        };

        let req = JsonRpcRequest::new(
            "tools/call",
            Some(serde_json::to_value(&params).context("Failed to serialize tool call params")?),
        );

        info!(
            server_id = %server_id,
            tool = %tool_name,
            "Calling MCP tool"
        );

        let resp = transport
            .send(req, Self::CALL_TIMEOUT_SECS)
            .await
            .with_context(|| format!("tools/call '{}' failed", tool_name))?;

        // Update stats
        if let Some(mut entry) = self.inner.servers.get_mut(&server_id) {
            entry.total_calls += 1;
            if resp.error.is_some() {
                entry.error_count += 1;
            }
        }

        if let Some(err) = resp.error {
            bail!("MCP tool '{}' returned error: {}", tool_name, err.message);
        }

        let result: ToolCallResult = serde_json::from_value(resp.result.unwrap_or(Value::Null))
            .context("Failed to parse tools/call result")?;

        Ok(result)
    }

    // ── Query helpers ─────────────────────────────────────────────────────────

    /// List all servers as JSON summary (for the UI).
    pub fn list_json(&self) -> Vec<Value> {
        self.inner
            .servers
            .iter()
            .map(|e| e.value().to_summary_json())
            .collect()
    }

    /// Get a single server entry by ID.
    pub fn get_server(&self, server_id: &str) -> Option<McpServerEntry> {
        self.inner.servers.get(server_id).map(|e| e.clone())
    }

    /// List all tool names across all running servers.
    pub fn list_all_tool_names(&self) -> Vec<String> {
        self.inner
            .tool_index
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// List all tools across all running servers, with their server alias.
    pub fn list_all_tools(&self) -> Vec<McpToolWithServer> {
        self.inner
            .servers
            .iter()
            .flat_map(|entry| {
                let server_id = entry.id.clone();
                let alias = entry.alias.clone();
                entry
                    .tools
                    .iter()
                    .map(move |tool| McpToolWithServer {
                        server_id: server_id.clone(),
                        server_alias: alias.clone(),
                        tool: tool.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Find tools matching a description (simple keyword match on name/description).
    pub fn find_tools(&self, query: &str) -> Vec<McpToolWithServer> {
        let query_lower = query.to_lowercase();
        self.list_all_tools()
            .into_iter()
            .filter(|t| {
                t.tool.name.to_lowercase().contains(&query_lower)
                    || t.tool.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Returns `true` if the server with the given alias already exists.
    pub fn alias_exists(&self, alias: &str) -> bool {
        self.inner.servers.iter().any(|e| e.alias == alias)
    }

    /// Total number of registered servers.
    pub fn server_count(&self) -> usize {
        self.inner.servers.len()
    }

    /// Total number of tools across all running servers.
    pub fn tool_count(&self) -> usize {
        self.inner.tool_index.len()
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn derive_alias(source: &McpServerSource, source_str: &str) -> String {
        match source {
            McpServerSource::NpmPackage { package, .. } => {
                // "@modelcontextprotocol/server-filesystem" → "server-filesystem"
                package.split('/').next_back().unwrap_or(package).to_string()
            }
            McpServerSource::CargoBin { crate_name, .. } => crate_name.clone(),
            McpServerSource::PythonScript { script_path, .. } => script_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "python-server".to_string()),
            McpServerSource::MarkdownSkill { script_path, .. } => script_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "markdown-skill".to_string()),
            McpServerSource::PowerShellScript { script_path, .. } => script_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "powershell-server".to_string()),
            McpServerSource::LocalBinary { path, .. } => path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| source_str.to_string()),
        }
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// McpToolWithServer – enriched tool info for the UI
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolWithServer {
    pub server_id: String,
    pub server_alias: String,
    pub tool: McpTool,
}

// ─────────────────────────────────────────────────────────────────────────────
// McpHost – high-level facade used by the Orchestrator
// ─────────────────────────────────────────────────────────────────────────────

/// Thin facade over `McpRegistry` that the Orchestrator uses.
///
/// Future extensions:
/// - Health monitoring loop (periodically ping servers)
/// - Automatic restart on crash
/// - Sandboxed execution environments
#[derive(Clone)]
pub struct McpHost {
    pub registry: McpRegistry,
}

impl McpHost {
    pub fn new() -> Self {
        Self {
            registry: McpRegistry::new(),
        }
    }

    /// Install a server and make its tools available.
    pub async fn install(&self, source: &str) -> Result<String> {
        self.registry.install(source).await
    }

    /// Uninstall a server.
    pub async fn uninstall(&self, server_id: &str) -> Result<()> {
        self.registry.uninstall(server_id).await
    }

    /// Call a tool by name with JSON arguments.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<ToolCallResult> {
        self.registry.call_tool(tool_name, arguments).await
    }

    /// Find the best tool for a given natural-language description.
    /// Returns the top N matching tools sorted by relevance (simple keyword match).
    pub fn suggest_tools(&self, description: &str, top_n: usize) -> Vec<McpToolWithServer> {
        let mut tools = self.registry.find_tools(description);
        tools.truncate(top_n);
        tools
    }

    /// List all available tools as a JSON array (for LLM function-calling context).
    pub fn tools_as_llm_context(&self) -> Value {
        let tools: Vec<Value> = self
            .registry
            .list_all_tools()
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name":        format!("{}_{}", t.server_alias, t.tool.name),
                        "description": t.tool.description,
                        "parameters":  t.tool.input_schema,
                    }
                })
            })
            .collect();

        serde_json::json!({ "tools": tools })
    }

    /// Server health summary for the UI dashboard.
    pub fn health_summary(&self) -> Value {
        let servers = self.registry.list_json();
        serde_json::json!({
            "total_servers": self.registry.server_count(),
            "total_tools":   self.registry.tool_count(),
            "servers":       servers,
        })
    }
}

impl Default for McpHost {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── McpServerSource parsing ───────────────────────────────────────────────

    #[test]
    fn test_parse_npm_source() {
        let src = McpServerSource::parse("npm:@modelcontextprotocol/server-filesystem");
        assert!(matches!(src, McpServerSource::NpmPackage { .. }));
        if let McpServerSource::NpmPackage {
            package, version, ..
        } = src
        {
            assert_eq!(package, "@modelcontextprotocol/server-filesystem");
            assert!(version.is_none());
        }
    }

    #[test]
    fn test_parse_npm_with_version() {
        let src = McpServerSource::parse("npm:my-server@1.2.3");
        if let McpServerSource::NpmPackage {
            package, version, ..
        } = src
        {
            assert_eq!(package, "my-server");
            assert_eq!(version, Some("1.2.3".to_string()));
        }
    }

    #[test]
    fn test_parse_cargo_source() {
        let src = McpServerSource::parse("cargo:office-hub-mcp");
        assert!(matches!(src, McpServerSource::CargoBin { .. }));
    }

    #[test]
    fn test_parse_python_source() {
        let src = McpServerSource::parse("python:scripts/mcp_server.py");
        assert!(matches!(src, McpServerSource::PythonScript { .. }));
    }

    #[test]
    fn test_parse_local_binary() {
        let src = McpServerSource::parse("C:\\tools\\my-server.exe");
        assert!(matches!(src, McpServerSource::LocalBinary { .. }));
    }

    // ── Alias derivation ──────────────────────────────────────────────────────

    #[test]
    fn test_derive_alias_npm() {
        let src = McpServerSource::NpmPackage {
            package: "@modelcontextprotocol/server-filesystem".to_string(),
            version: None,
            args: vec![],
        };
        let alias = McpRegistry::derive_alias(&src, "npm:...");
        assert_eq!(alias, "server-filesystem");
    }

    #[test]
    fn test_derive_alias_cargo() {
        let src = McpServerSource::CargoBin {
            crate_name: "my-mcp-tool".to_string(),
            version: None,
            args: vec![],
        };
        let alias = McpRegistry::derive_alias(&src, "cargo:my-mcp-tool");
        assert_eq!(alias, "my-mcp-tool");
    }

    // ── Registry construction ─────────────────────────────────────────────────

    #[test]
    fn test_registry_starts_empty() {
        let reg = McpRegistry::new();
        assert_eq!(reg.server_count(), 0);
        assert_eq!(reg.tool_count(), 0);
        assert!(reg.list_json().is_empty());
    }

    #[test]
    fn test_registry_max_servers_enforced() {
        // We can't easily spawn real processes in tests, but we can verify the
        // capacity check at the registry level by observing the error message.
        let reg = McpRegistry::with_max(0); // 0 max → always at capacity
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(reg.install("npm:test-server"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Maximum number of MCP servers"));
    }

    // ── McpServerEntry ────────────────────────────────────────────────────────

    #[test]
    fn test_server_entry_initial_state() {
        let entry = McpServerEntry::new(
            "test-server",
            McpServerSource::LocalBinary {
                path: PathBuf::from("/usr/bin/test"),
                args: vec![],
            },
        );
        assert_eq!(entry.alias, "test-server");
        assert_eq!(entry.status, McpServerStatus::Registered);
        assert!(entry.tools.is_empty());
        assert_eq!(entry.total_calls, 0);
    }

    #[test]
    fn test_server_entry_summary_json() {
        let entry = McpServerEntry::new(
            "my-server",
            McpServerSource::LocalBinary {
                path: PathBuf::from("server"),
                args: vec![],
            },
        );
        let json = entry.to_summary_json();
        assert_eq!(json["alias"], "my-server");
        assert_eq!(json["toolCount"], 0);
    }

    // ── JSON-RPC types ────────────────────────────────────────────────────────

    #[test]
    fn test_jsonrpc_request_has_id() {
        let req = JsonRpcRequest::new("tools/list", None);
        assert_eq!(req.jsonrpc, "2.0");
        assert!(!req.id.is_null());
    }

    #[test]
    fn test_jsonrpc_notification_null_id() {
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        assert!(notif.id.is_null());
    }

    #[test]
    fn test_jsonrpc_error_codes() {
        let err = JsonRpcError::internal("test error");
        assert_eq!(err.code, JsonRpcError::INTERNAL_ERROR);

        let err2 = JsonRpcError::method_not_found("foo");
        assert_eq!(err2.code, JsonRpcError::METHOD_NOT_FOUND);
        assert!(err2.message.contains("foo"));
    }

    // ── McpHost facade ────────────────────────────────────────────────────────

    #[test]
    fn test_host_health_summary_empty() {
        let host = McpHost::new();
        let summary = host.health_summary();
        assert_eq!(summary["total_servers"], 0);
        assert_eq!(summary["total_tools"], 0);
    }

    #[test]
    fn test_host_tools_as_llm_context_empty() {
        let host = McpHost::new();
        let ctx = host.tools_as_llm_context();
        let tools = ctx["tools"].as_array().unwrap();
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn test_mock_mcp_server_integration() {
        // Only run this if we are in the src-tauri directory during test
        let script_path = PathBuf::from("mock_mcp_server.py");
        if !script_path.exists() {
            // Skip the test if the mock script isn't found
            return;
        }

        let reg = McpRegistry::new();
        
        // 1. Install mock server
        let server_id = reg.install("python:mock_mcp_server.py").await.expect("Should install mock server");
        
        // Check registry state
        let entry = reg.inner.servers.get(&server_id).unwrap();
        println!("Server entry: {:?}", entry.value());
        assert_eq!(entry.status, McpServerStatus::Running);
        assert_eq!(entry.tools.len(), 1);
        assert_eq!(entry.tools[0].name, "echo");
        drop(entry);

        // 2. Test suggest_tools
        let suggestions = reg.find_tools("echo");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].tool.name, "echo");

        // 3. Test tool call
        let result = reg.call_tool(
            "echo",
            Some(serde_json::json!({"message": "Hello, MCP!"}))
        ).await.expect("Tool call should succeed");

        assert_eq!(result.is_error, false);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].text.as_deref(), Some("Mock Server Echo: Hello, MCP!"));

        // 4. Uninstall server
        reg.uninstall(&server_id).await.expect("Should uninstall");
        assert!(reg.inner.servers.get(&server_id).is_none());
    }
}
