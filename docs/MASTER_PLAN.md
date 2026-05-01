# Office Hub – Master Development Plan v1.0

**Document version:** 1.0  
**Last updated:** 2025  
**Status:** 🔍 DRAFT – For Review  
**Tech stack:** Rust + Tauri v2 · React + TypeScript · Windows COM · UIA

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Tech Stack & Rationale](#2-tech-stack--rationale)
3. [System Architecture](#3-system-architecture)
4. [Orchestrator – Deep Dive](#4-orchestrator--deep-dive)
5. [LLM Gateway](#5-llm-gateway)
6. [Agent Specifications](#6-agent-specifications)
7. [MCP Host & Protocol](#7-mcp-host--protocol)
8. [Workflow Engine](#8-workflow-engine)
9. [WebSocket Communication Layer](#9-websocket-communication-layer)
10. [Data Models & Interface Contracts](#10-data-models--interface-contracts)
11. [Implementation Phases](#11-implementation-phases)
12. [Testing Strategy](#12-testing-strategy)
13. [Security & Compliance](#13-security--compliance)
14. [Risk Register](#14-risk-register)
15. [Definition of Done](#15-definition-of-done)

---

## 1. Executive Summary

### 1.1 What is Office Hub?

**Office Hub** là một **Agentic Overlay** siêu nhẹ cho Windows – một lớp AI điều phối được cài đặt ngay trên môi trường làm việc hiện tại (Microsoft Office), không thay thế hay can thiệp vào hệ điều hành. Nó hoạt động như một "bộ não AI" ngồi bên cạnh người dùng, tự động hoá các tác vụ Office phức tạp và lặp đi lặp lại.

### 1.2 Core Vision

> **"Người dùng nói, Office Hub làm."**

Từ một câu lệnh thoại đơn giản như *"Lấy báo giá xăng dầu hôm nay và cập nhật vào báo cáo tuần"*, hệ thống tự động:
1. Trích xuất dữ liệu từ trình duyệt Edge đang mở
2. Cập nhật số liệu vào file Excel
3. Tạo báo cáo Word chuyên nghiệp
4. Gửi notification về điện thoại để xét duyệt

### 1.3 Key Differentiators

| Đặc điểm | Office Hub | Giải pháp thông thường |
|----------|-----------|----------------------|
| **Binary size** | < 20 MB (.exe) | Python/Node runtime: 200-500 MB |
| **Office integration** | COM Automation (native) | Thư viện parse file (không live) |
| **Browser data** | Windows UI Automation (no WebDriver) | Selenium/Playwright (cần ChromeDriver) |
| **AI orchestration** | Multi-agent, MCP extensible | Single LLM call |
| **Privacy** | Local LLM option (Ollama) | Cloud-only |
| **Approval flow** | Mobile HITL (Human-in-the-Loop) | Không có |

### 1.4 Target Users

- **Nhân viên văn phòng** xử lý báo cáo Word/Excel/PPT hàng ngày
- **Kế toán / Phân tích tài chính** cần trích xuất số liệu từ nhiều nguồn
- **Quản lý** cần tổng hợp dữ liệu và tạo báo cáo nhanh
- **Người dùng Power User** muốn tự động hoá workflow phức tạp

### 1.5 Strategic Position (SWOT)

Dựa trên phân tích nội bộ, Office Hub có định vị chiến lược độc đáo:
- **Strengths:** Bảo mật tuyệt đối (100% Local/Ollama), tự động hóa sâu (Native COM), kiến trúc mở rộng (MCP), siêu nhẹ.
- **Weaknesses:** Phụ thuộc hệ sinh thái Windows, rủi ro deadlock từ COM popup, rào cản kỹ thuật khi cài đặt.
- **Opportunities:** Phát triển Voice-to-Action qua mobile, ra mắt Enterprise Marketplace cho MCP skills, mở rộng ra Google Workspace.
- **Threats:** Cạnh tranh trực tiếp từ Microsoft 365 Copilot và thói quen dùng RPA truyền thống (UiPath, Power Automate).

*Chiến lược trọng tâm cho Phase tiếp theo:* Nâng cao độ ổn định của COM (Anti-Deadlock Watchdog), phát triển Voice-to-Action, và đơn giản hóa quá trình cài đặt (One-Click Installer).

---

## 2. Tech Stack & Rationale

### 2.1 Backend: Rust + Tauri v2

| Thành phần | Lựa chọn | Lý do |
|-----------|---------|-------|
| **App framework** | Tauri v2 | Native .exe nhẹ, IPC hiệu quả, tích hợp Windows tốt |
| **Language** | Rust 1.80+ | Memory safety, zero-cost abstractions, FFI với Win32 |
| **Async runtime** | Tokio | Đa luồng, non-blocking I/O cho nhiều agents song song |
| **HTTP client** | Reqwest 0.12 | Async, hỗ trợ streaming cho LLM responses |
| **WebSocket** | tokio-tungstenite 0.23 | Async WebSocket cho Mobile Client |
| **Windows APIs** | `windows` crate 0.58 | COM, UIA, GDI – trực tiếp từ Rust |
| **Serialization** | serde + serde_json + serde_yaml | JSON (IPC, LLM) và YAML (config, rules, workflows) |
| **Concurrency** | DashMap, tokio::sync | Lock-free concurrent maps cho agent registry |
| **Logging** | tracing + tracing-subscriber | Structured logging với JSON output |
| **Error handling** | thiserror + anyhow | Typed errors ở domain layer, flexible ở app layer |

### 2.2 Frontend: React + TypeScript

| Thành phần | Lựa chọn | Lý do |
|-----------|---------|-------|
| **Framework** | React 18 | Hệ sinh thái lớn, component model phù hợp |
| **Build tool** | Vite 5 | Hot reload nhanh, tương thích tốt với Tauri |
| **State** | Zustand 5 + Immer | Đơn giản hơn Redux, immutability built-in |
| **Server state** | TanStack Query v5 | Cache, sync với Rust backend qua IPC |
| **UI** | Tailwind CSS + Lucide Icons | Utility-first, không cần component lib nặng |
| **Routing** | React Router v6 | SPA routing cho các trang Settings, Workflows |
| **Markdown** | react-markdown + remark-gfm | Render LLM responses trong Chat Pane |
| **TypeScript** | Strict mode | Type safety đầy đủ, `noUncheckedIndexedAccess` |

### 2.3 Dependency Versions

```toml
[dependencies]
tauri                  = "2"
tokio                  = { version = "1", features = ["full"] }
reqwest                = { version = "0.12", features = ["json", "rustls-tls"] }
tokio-tungstenite      = "0.23"
windows                = "0.58"
serde                  = { version = "1", features = ["derive"] }
serde_json             = "1"
serde_yaml             = "0.9"
tracing                = "0.1"
tracing-subscriber     = "0.3"
anyhow                 = "1"
thiserror              = "1"
dashmap                = "6"
uuid                   = { version = "1", features = ["v4", "serde"] }
chrono                 = { version = "0.4", features = ["serde"] }
jsonrpc-core           = "18"
sha2                   = "0.10"
base64                 = "0.22"
```

---

## 3. System Architecture

### 3.1 Four-Layer Model

```
┌─────────────────────────────────────────────────────────────────────┐
│                         LAYER 1: APP SHELL                          │
│                    (Tauri Window + React Frontend)                  │
│                                                                     │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐ │
│   │ File Browser  │  │  Chat Pane   │  │  Settings / Workflow UI  │ │
│   │ (Top Preview, │  │  (Topic Tree,│  │  & Agent/Skill Manager   │ │
│   │ Bottom Chat)  │  │  Del. Action)│  │  (Drag-drop + AI Chat)   │ │
│   └──────────────┘  └──────────────┘  └──────────────────────────┘ │
│                                                                     │
│   Tauri IPC Bridge  (invoke / listen)                               │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│                     LAYER 2: ORCHESTRATOR                           │
│                 (Core brain – Rust, single source of truth)         │
│                                                                     │
│  ┌─────────────────┐  ┌────────────────┐  ┌──────────────────────┐ │
│  │ Intent Classifier│  │  Task Router   │  │   Rule Engine        │ │
│  │  (FastRule +    │  │  (routing table│  │   (YAML validation   │ │
│  │   LLM-assisted) │  │   + fallback)  │  │   before writes)     │ │
│  └─────────────────┘  └────────────────┘  └──────────────────────┘ │
│                                                                     │
│  ┌─────────────────┐  ┌────────────────┐  ┌──────────────────────┐ │
│  │  Session Store  │  │  HITL Manager  │  │  MCP Host            │ │
│  │  (DashMap,      │  │  (approval     │  │  (plugin registry)   │ │
│  │   context mgmt) │  │   oneshot ch.) │  │                      │ │
│  └─────────────────┘  └────────────────┘  └──────────────────────┘ │
└──────┬──────────────────┬──────────────────┬──────────────┬────────┘
       │                  │                  │              │
┌──────▼──────┐  ┌────────▼───────┐  ┌───────▼─────┐  ┌───▼──────────┐
│  LAYER 3:   │  │   LAYER 3:     │  │  LAYER 3:   │  │  LAYER 3:   │
│  AGENTS     │  │   LLM GATEWAY  │  │  WORKFLOW   │  │  WEBSOCKET  │
│             │  │                │  │  ENGINE     │  │  SERVER     │
│ • Analyst   │  │ • Gemini API   │  │ • Triggers  │  │ • Mobile    │
│ • OffMaster │  │ • OpenAI API   │  │ • Steps     │  │   Client    │
│ • WebResch  │  │ • Ollama local │  │ • Audit log │  │ • HITL      │
│ • Converter │  │ • LM Studio    │  │             │  │   approval  │
│             │  │ • Token Cache  │  │             │  │             │
└──────┬──────┘  └────────────────┘  └─────────────┘  └─────────────┘
       │
┌──────▼───────────────────────────────────────────────────────────────┐
│                         LAYER 4: NATIVE OS                           │
│                                                                      │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │  COM Automation   │  │  UI Automation    │  │  Win32 APIs      │  │
│  │  (Excel, Word,    │  │  (Edge, Chrome    │  │  (GDI, DWM,      │  │
│  │   PowerPoint)     │  │   browser UIA)    │  │   FileSystem)    │  │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow – Standard Request

```
[User message in Chat Pane]
        │
        ▼ Tauri IPC: invoke("send_chat_message")
[commands.rs: send_chat_message]
        │
        ▼ AppState.orchestrator.process_message()
[Orchestrator::process_message]
        │
        ├─1─▶ SessionStore: get_or_create(session_id)
        │
        ├─2─▶ IntentClassifier::classify_fast()     ← Rule-based (offline, ~0ms)
        │         │ low confidence?
        │         └─▶ IntentClassifier::classify_llm() ← LLM-assisted (~500ms)
        │
        ├─3─▶ Router::resolve(intent, agent_registry) → RouteDecision
        │
        ├─4─▶ [Optional] HitlManager::register()   ← if sensitive action
        │         │
        │         └─▶ Wait for Mobile/Desktop approval (oneshot channel)
        │
        ├─5─▶ AgentRegistry::dispatch(agent_id, task) → AgentOutput
        │         │
        │         └─▶ [Agent executes: COM / UIA / LLM call]
        │
        ├─6─▶ RuleEngine::validate(agent_output)    ← YAML rules check
        │         │ blocked?
        │         └─▶ Return error (never writes to Office)
        │
        ├─7─▶ SessionStore::push_message() + maybe summarise
        │
        └─8─▶ OrchestratorResponse { content, intent, agent_used, tokens }
                │
                ▼ Tauri IPC response
[React Chat Pane: display reply]
```

### 3.3 Project Directory Structure

```
office-hub/
├── src-tauri/                    # Rust backend (Tauri v2)
│   ├── src/
│   │   ├── main.rs               # Windows entry point
│   │   ├── lib.rs                # Module root, AppState, AppConfig, run()
│   │   ├── commands.rs           # All Tauri IPC handlers (#[tauri::command])
│   │   │
│   │   ├── orchestrator/
│   │   │   ├── mod.rs            # Orchestrator struct, process_message pipeline
│   │   │   ├── intent.rs         # Intent enum (30+ variants) + FastClassifier
│   │   │   ├── session.rs        # Session, SessionStore, ContextWindow
│   │   │   ├── router.rs         # Router, routing table, AgentRequest/Response
│   │   │   └── rule_engine.rs    # RuleEngine, YAML parsing, Rule trait + impls
│   │   │
│   │   ├── agents/
│   │   │   ├── mod.rs            # Agent trait, AgentId, AgentRegistry
│   │   │   ├── analyst/          # Excel COM Automation
│   │   │   │   ├── mod.rs
│   │   │   │   └── com_excel.rs  # (Phase 3) Win32 COM wrapper
│   │   │   ├── office_master/    # Word + PowerPoint COM
│   │   │   │   ├── mod.rs
│   │   │   │   ├── com_word.rs
│   │   │   │   └── com_ppt.rs
│   │   │   ├── web_researcher/   # Browser UIA
│   │   │   │   ├── mod.rs
│   │   │   │   └── uia.rs        # (Phase 4) UIAutomationCore.dll wrapper
│   │   │   └── converter/        # MCP skill learning
│   │   │       └── mod.rs
│   │   │
│   │   ├── llm_gateway/
│   │   │   └── mod.rs            # Provider trait, Gemini, OpenAI, Ollama, Cache
│   │   │
│   │   ├── mcp/
│   │   │   └── mod.rs            # McpHost, McpRegistry, StdioTransport
│   │   │
│   │   ├── workflow/
│   │   │   ├── mod.rs            # WorkflowEngine, YAML loader, executor loop
│   │   │   ├── triggers/mod.rs   # Trigger trait + Email/File/Schedule/Voice stubs
│   │   │   └── actions/mod.rs    # Action trait stubs
│   │   │
│   │   └── websocket/
│   │       └── mod.rs            # WebSocketServer, ClientMessage, ServerMessage
│   │
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json
│
├── src/                          # React + TypeScript frontend
│   ├── components/
│   │   ├── FileBrowser/          # Directory tree, file type icons
│   │   ├── HistoryTree/          # Topic => Sub session history view
│   │   ├── ChatPane/             # Chat UI with Markdown rendering
│   │   ├── Settings/             # LLM config, API keys, agent settings
│   │   ├── AgentManager/         # Agent & Skill import, testing, approval
│   │   └── WorkflowBuilder/      # Drag-and-drop workflow editor (input, filter, logic, output)
│   ├── pages/                    # Route-level components
│   ├── hooks/                    # Custom React hooks (useOrchestrator, etc.)
│   ├── store/                    # Zustand stores
│   └── main.tsx
│
├── mcp-servers/                  # Standalone MCP Server plugins
├── mobile/                       # Mobile companion app (Phase 5)
├── rules/
│   └── default.yaml              # Production rule set
├── workflows/
│   └── email-to-report.yaml      # Sample workflow template
├── docs/
│   ├── MASTER_PLAN.md            # This document
│   └── architecture/             # ADRs and diagrams
└── .github/workflows/
    └── ci.yml                    # Lint → Test → Build → Release pipeline
```

---

## 4. Orchestrator – Deep Dive

The Orchestrator is the **single most critical module** in Office Hub. Every user request flows through it. Getting the design right here determines the quality of the entire system.

### 4.1 Intent Taxonomy

The system recognises **30 distinct intent types** organised into 7 categories:

```
Intent
├── Excel Category (7 intents)
│   ├── ExcelRead         – read cell/range values (sensitivity: LOW)
│   ├── ExcelWrite        – write to cells/ranges (MEDIUM)
│   ├── ExcelFormula      – generate/apply formulas (MEDIUM)
│   ├── ExcelPowerQuery   – M-code / query management (MEDIUM)
│   ├── ExcelMacro        – VBA/Office Scripts (HIGH)
│   ├── ExcelAnalyze      – statistics, trends, anomalies (MEDIUM)
│   └── ExcelAudit        – formula audit (LOW)
│
├── Word Category (4 intents)
│   ├── WordCreate        – new document from template (MEDIUM)
│   ├── WordEdit          – update paragraphs/tables (MEDIUM)
│   ├── WordFormat        – styles, TOC, cross-refs (MEDIUM)
│   └── WordExtract       – read content from document (LOW)
│
├── PowerPoint Category (4 intents)
│   ├── PptCreate         – new presentation (MEDIUM)
│   ├── PptEdit           – add/edit/delete slides (MEDIUM)
│   ├── PptFormat         – brand theme, grid, transitions (MEDIUM)
│   └── PptConvertFrom    – convert Word/MD/JSON to slides (MEDIUM)
│
├── Web Research Category (4 intents)
│   ├── WebExtractData    – scrape table/list from browser (CRITICAL)
│   ├── WebNavigate       – navigate browser URL (CRITICAL)
│   ├── WebSearch         – search + summarise results (HIGH)
│   └── WebScreenshot     – capture grounding evidence (HIGH)
│
├── MCP Category (3 intents)
│   ├── McpInstall        – install new MCP server (HIGH)
│   ├── McpCallTool       – invoke registered tool (HIGH)
│   └── McpListServers    – list available servers (LOW)
│
├── Workflow Category (3 intents)
│   ├── WorkflowTrigger   – fire a named workflow (MEDIUM)
│   ├── WorkflowStatus    – query run history (LOW)
│   └── WorkflowEdit      – create/modify workflow YAML (MEDIUM)
│
└── System Category (5 intents)
    ├── GeneralChat       – conversational chat (LOW)
    ├── SystemConfig      – LLM / agent settings (MEDIUM)
    ├── HelpRequest       – usage questions (LOW)
    └── Ambiguous         – unclear intent (LOW)
```

### 4.2 Intent Classification Pipeline

**Two-stage approach** to balance speed vs. accuracy:

```
Stage 1: FastClassifier (Rule-based, ~0ms, offline)
   ├── Compiled regex patterns for each intent category
   ├── Entity extraction (file paths, cell ranges, URLs, sheet names)
   ├── Confidence threshold: 0.75 = "confident", 0.30 = "minimum"
   └── If confidence >= 0.75 → return result, skip Stage 2
           │
           │ if confidence < 0.75
           ▼
Stage 2: LlmClassifier (LLM-assisted, ~300-800ms)
   ├── Build structured JSON prompt:
   │     "User message: '...'
   │      Available intents: [excel_read, word_create, ...]
   │      Return: { intent_type, confidence, entities, clarification_needed }"
   ├── Parse JSON response → IntentClassifyResult
   ├── If confidence < 0.30 → return Ambiguous with clarification_question
   └── Return classified Intent with extracted entities
```

**Intent payload enrichment:** Entity extraction fills in the payload automatically:
- File paths: `C:\Users\...\report.xlsx` → `ExcelReadPayload.file_path`
- Cell ranges: `B2:D50` → `ExcelReadPayload.range`
- Sheet names: `"sheet Sheet1"` → `ExcelReadPayload.sheet_name`
- URLs: `https://...` → `WebExtractPayload.url`

### 4.3 Routing Table

The Router maps each `IntentCategory` to an agent with a `RouteEntry`:

```rust
struct RouteEntry {
    primary_agent:  AgentKind,         // which agent handles this
    fallback_agent: Option<AgentKind>, // backup if primary unavailable
    force_hitl:     bool,              // override rule engine for HITL
    timeout_secs:   u64,               // per-intent timeout
}
```

**Default routing table (condensed):**

| Intent Category | Primary Agent | Fallback | Force HITL | Timeout |
|----------------|--------------|---------|-----------|---------|
| ExcelAnalyze | Analyst | – | No | 120s |
| ExcelWrite | Analyst | – | No | 60s |
| ExcelMacro (VBA) | Analyst | – | **Yes** | 180s |
| WordCreate | OfficeMaster | – | No | 120s |
| WordEdit | OfficeMaster | – | No | 90s |
| PptCreate | OfficeMaster | – | No | 120s |
| WebExtract | WebResearcher | – | No | 60s |
| WebNavigate | WebResearcher | – | **Yes** | 60s |
| WebFormFill | WebResearcher | – | **Yes** | 120s |
| WebToExcel | WebResearcher→Analyst | – | **Yes** | 300s |
| McpToolCall | Converter | – | No | 60s |
| McpInstall | Converter | – | **Yes** | 300s |
| GeneralChat | Orchestrator (direct) | – | No | 30s |

### 4.4 Session State Model

Each conversation is a `Session` stored in `SessionStore` (DashMap-backed, thread-safe):

```
Session {
    id:                String (UUID v4)
    topic_id:          Option<String>   ← used to group sessions by a common overarching topic
    title:             Option<String>   ← auto-set from first user message
    status:            SessionStatus    ← Active | WaitingApproval | Processing | Closed
    messages:          VecDeque<Message> ← sliding window, auto-trims at 80% capacity
    summaries:         Vec<SessionSummary> ← LLM-generated when context is compressed
    context_window:    ContextWindow {
        max_tokens:              usize  ← from config (default: 32,000)
        system_prompt_reserved:  usize  ← 2,000 tokens
        response_reserved:       usize  ← 4,000 tokens
        tokens_used:             usize  ← recalculated on every push_message()
        utilization:             f32    ← 0.0–1.0
    }
    active_file_path:  Option<String>   ← currently focused file in File Browser
    last_intent:       Option<String>
    total_tokens_consumed: usize
    created_at / updated_at / last_active_at: DateTime<Utc>
}
```

**History Tree View (UI):**
To provide a structured view of past work, the chat history is organized in a Tree View component:
- **Topic Level:** Groups related sessions under a single overarching topic (e.g., "Monthly Financial Report"). Displays topic names clearly.
- **Sub-session Level:** Dropdown/collapsible view for individual sessions within that topic to view exchange details and results.
- **Session Management:** Users can delete individual sessions via a dedicated "Delete Session" button.
This allows users to seamlessly navigate and manage complex tasks that span multiple sessions.

**Context window management:**
1. On every `push_message()`, recalculate token count
2. If `utilization > 0.80`, trim oldest messages (keep last 4 minimum)
3. If too much context is lost, trigger `summarise_session()` via LLM Gateway
4. Summary is prepended to the next LLM request as a system message

**Session lifecycle:**
- Sessions are evicted from memory after 30 minutes of inactivity (configurable)
- Max 100 sessions in memory simultaneously
- TODO (Phase 5+): Optional disk persistence for cross-restart recovery

### 4.5 Human-in-the-Loop (HITL) Manager

The HITL Manager suspends task execution until an explicit human decision is received:

```
Orchestrator encounters sensitive action
    │
    ▼
HitlManager::register(HitlRequestBuilder {
    description: "Web Researcher wants to navigate to https://...",
    risk_level:  HitlRiskLevel::Critical,
    payload:     Some(json!({ "url": "..." }))
}) → (action_id: String, rx: oneshot::Receiver<bool>)
    │
    ▼
Notification sent to:
    ├── Desktop toast notification
    └── Mobile WebSocket push: { "type": "approval_request", "action_id": "..." }
    │
    ▼
Task awaits: rx.await → approved: bool
    │
    ├── approved = true  → continue execution
    └── approved = false → return error "User rejected action"
```

**Risk levels and approval requirements:**

| Level | Action types | Approval flow |
|-------|-------------|---------------|
| **Low** | read_file, read_excel_cell | Automatic (no approval) |
| **Medium** | write_excel_cell, write_word_paragraph | Auto-approve after 5s unless rejected |
| **High** | navigate_browser, run_vba, install_mcp | Explicit approval required |
| **Critical** | fill_form, submit_form, delete_file | Approval + second confirmation |

### 4.6 Rule Engine Architecture

The Rule Engine validates every agent output before it reaches Microsoft Office or the browser. It is loaded from `rules/default.yaml` and can be **hot-reloaded** without restarting the app.

**Rule types implemented:**

| Rule | Target | Blocks? |
|------|--------|---------|
| `LengthRule` | Excel cell, Word paragraph, PPT text box, Chat | Yes |
| `BlockedPatternRule` | Any content | Yes (block) / No (flag/redact) |
| `VbaCommandRule` | VBA scripts | Yes (blacklisted commands) |
| `DomainPolicyRule` | Web URLs | Yes (not in whitelist) |
| `PercentageRangeRule` | LLM responses, Excel cells | No (warning) |
| `PlaceholderLeakageRule` | Word, PPT, Excel, Chat | Yes |
| `HardTruthVerificationRule` | Excel cells (post-write verify) | Yes (if > tolerance%) |
| `HitlClassifierRule` | UIA actions | No (annotates approval level) |

**Rule evaluation flow:**
```
ValidationRequest (agent_id, target, content, metadata)
    │
    ▼
Run all rules concurrently (futures::join_all)
    │
    ▼
Collect all Violations
    │
    ├── Any blocking violation? → ValidationResult { passed: false }
    │       ↓
    │   Agent output is DISCARDED, error returned to user
    │
    └── No blocking violations → Sanitizer::sanitize() → ValidationResult { passed: true }
            ↓
        HITL requirements are annotated (if hitl_classifier fired)
            ↓
        Sanitized content proceeds to write-to-Office step
```

---

## 5. LLM Gateway

### 5.1 Provider Support Matrix

| Provider | Type | Endpoint | Authentication |
|---------|------|----------|----------------|
| **Google Gemini** | Cloud | `generativelanguage.googleapis.com/v1beta` | API Key (query param) |
| **OpenAI GPT** | Cloud | `api.openai.com/v1` | Bearer token |
| **Ollama** | Local | `localhost:11434/v1` | None (OpenAI-compat) |
| **LM Studio** | Local | `localhost:1234/v1` | None (OpenAI-compat) |

### 5.2 Hybrid Mode Flow

```
User request arrives
    │
    ├── Token cache hit? → Return cached response immediately
    │
    ▼ Cache miss
Primary provider (configured in config.yaml)
    │
    ├── Success → Record metrics → Store in cache → Return
    │
    └── Failure (429 / 503 / timeout / network)
            │
            ▼ Retry with exponential backoff: [500ms, 1500ms, 4000ms]
            │
            └── Still failing AND hybrid_mode = true?
                    │
                    ▼ Switch to secondary provider
                    ├── Cloud primary → try Local (Ollama)
                    └── Local primary → try Cloud (Gemini/OpenAI)
                            │
                            ├── Success → metrics.fallbacks_triggered++ → Return
                            └── Failure → Error: "All LLM providers failed"
```

### 5.3 Token Caching Strategy

- **Cache key:** `SHA256(model_name + temperature + all message contents)` (URL-safe base64, first 16 bytes)
- **TTL:** 60 minutes (configurable via rules/default.yaml `token_management.cache_ttl_minutes`)
- **Capacity:** 512 entries max (LRU eviction when full)
- **Invalidation:** Automatic on provider config change (via `clear_cache()`)
- **Not cached:** Requests with `stream: true` (future feature)

### 5.4 Context Window Management

Token counting uses a lightweight heuristic (no tokenizer dependency):
- ASCII-heavy text: `chars / 4` tokens
- Vietnamese/CJK-heavy text (> 30% non-ASCII): `chars / 2` tokens

Reserved allocations (from 32K total):
- System prompt: 2,000 tokens
- LLM response: 4,000 tokens
- Message history: remaining ~26,000 tokens

When history utilization > 80%: oldest messages are dropped and the session triggers `LlmGateway::summarise_history()`.

---

## 6. Agent Specifications

### 6.1 Analyst Agent (Excel)

**Phase:** 3  
**Agent ID:** `analyst`  
**COM interface:** `Excel.Application` via `Win32_System_Com`

#### Supported Actions

| Action | Description | HITL required |
|--------|-------------|---------------|
| `analyze_workbook` | Full workbook analysis: sheet structure, key metrics, anomalies | No |
| `read_cell_range` | Read values from a cell range | No |
| `write_cell_range` | Write values with backup + hard-truth verify | No |
| `generate_formula` | Generate Excel formula (XLOOKUP, LAMBDA, etc.) | No |
| `apply_formula` | Apply formula to a range | No |
| `run_power_query` | Refresh or create Power Query | No |
| `generate_power_query` | Generate M-code from description | No |
| `generate_vba` | Generate VBA macro code (no execution) | No |
| `run_vba` | Execute VBA macro | **Yes (High)** |
| `audit_formulas` | Scan all formulas for errors | No |
| `detect_anomalies` | Statistical anomaly detection | No |
| `hard_truth_verify` | Verify value read-back matches intended write | No |

#### Hard-Truth Verification Protocol

```
1. LLM generates value to write (e.g. 1,500,000)
2. Analyst Agent writes to Excel cell via COM
3. Agent reads back the cell value via COM  ← "ground truth"
4. Compare: |intended - actual| / |intended| * 100
5. If deviation > tolerance (0.01%):
   - Log CRITICAL rule violation
   - Discard the write
   - Return error to Orchestrator
   - Never trust LLM-generated numbers without verification
```

#### COM Implementation Notes (Phase 3)

```rust
// Launch or attach to existing Excel instance
CoInitializeEx(None, COINIT_APARTMENTTHREADED)?;
let excel_app: IDispatch = CoCreateInstance(
    &EXCEL_APPLICATION_CLSID,
    None,
    CLSCTX_LOCAL_SERVER
)?;

// Open workbook
let workbooks = excel_app.get_Workbooks()?;
let workbook = workbooks.Open(file_path, ...)?;

// Read a range
let sheet = workbook.ActiveSheet()?;
let range = sheet.Range("B2:D50")?;
let values = range.Value()?;  // Returns VARIANT (2D array)

// Write a range
range.set_Value(new_values)?;

// ALWAYS read back to verify
let actual = range.Value()?;
assert_within_tolerance(intended, actual)?;
```

### 6.2 Office Master Agent (Word + PowerPoint)

**Phase:** 3  
**Agent ID:** `office_master`  
**COM interfaces:** `Word.Application`, `PowerPoint.Application`

#### Key Design Principles

1. **Format preservation first:** Never alter Styles, SectionBreaks, HeadersFooters unless explicitly asked
2. **Template-driven:** Always use a base template (`.dotx` for Word, `.potx` for PPT)
3. **Backup before write:** Copy file to backup directory before any modification
4. **Bookmark navigation:** Use Word Bookmarks as stable anchors for content replacement
5. **Brand palette enforcement:** All PPT elements must use brand colors from config

#### Word Workflow (create_report_from_template)

```
1. word_app.Documents.Add(template_path)
2. For each section in report:
   a. Find bookmark or heading anchor
   b. Replace placeholder text: range.Text = new_content
   c. Preserve style: range.Style = "Body Text" (no override)
3. Update dynamic fields: doc.Fields.Update()
4. Update TOC: doc.TablesOfContents(1).Update()
5. SaveAs(output_path, wdFormatXMLDocument)
6. Read back key sections → Rule Engine validates
7. Return file path + page count + word count
```

#### PowerPoint Brand Grid System

All shapes are snapped to a 12-column, 6-row grid:
- Content area: 1.27cm margin on all sides
- Column width: (slide_width - 2.54cm) / 12
- Shapes aligned via `Shape.Left = col_index * col_width + margin`
- Morph transition: `SlideShowTransition.EntryEffect = ppEffectMorph`

### 6.3 Web Researcher Agent (UIA)

**Phase:** 4  
**Agent ID:** `web_researcher`  
**Windows API:** `UIAutomationCore.dll` via `Win32_UI_Accessibility`

#### UIA Architecture

```
UIAutomation entry point
    CoCreateInstance(&CUIAutomation, ...) → IUIAutomation
    │
    ▼ Find browser window
    uia.GetRootElement() → IUIAutomationElement (desktop root)
    TreeWalker.FindFirst(ConditionEdgeOrChrome) → browser_root_element
    │
    ▼ Navigate to content area
    browser_root → find_by_automation_id("document") → content_element
    │
    ▼ Extract data
    content_element.GetCurrentPattern(UIA_TablePatternId) → IUIAutomationTablePattern
    table_pattern.GetCurrentColumnHeaders() → headers
    table_pattern.GetCurrentRowCount() → row_count
    for row in 0..row_count:
        table_pattern.GetItem(row, col) → cell_element
        cell_element.GetCurrentPropertyValue(UIA_NamePropertyId) → value
    │
    ▼ Normalize to JSON
    { headers: [...], rows: [[...], [...]], source_url: "...", captured_at: "..." }
    │
    ▼ Grounding screenshot (GDI BitBlt or DXGI)
    Save screenshot to $APPDATA/office-hub/grounding/capture_{timestamp}.png
```

#### Security Guarantees

These are **hard limits enforced in code**, not just policy:

| Action | Policy | Enforcement |
|--------|--------|-------------|
| `form_submit` | BLOCKED | `is_action_blocked()` → early return Err |
| `authentication` | BLOCKED | Same |
| `payment` | BLOCKED | Same |
| `navigate_to_url` | HITL required | `approved` param must be `true` |
| Non-whitelisted domain | BLOCKED | `is_domain_allowed()` check |
| Any UIA action | Audit logged | `log_audit_action()` called before execution |

### 6.4 Converter Agent (MCP Skill Builder)

**Phase:** 7  
**Agent ID:** `converter`

#### Skill Learning Process

```
1. User: "Học cách lấy dữ liệu từ API VietStock"
2. Converter Agent:
   a. Fetch GitHub repo / API docs
   b. Parse endpoints, auth requirements, data schemas
   c. Generate Rust/Python MCP Server boilerplate
   d. Package as executable + manifest.json
   e. Install via McpRegistry::install()
3. New tool available: "vietstock_get_stock_price(symbol)"
4. Orchestrator can now route WebExtract intents to this MCP tool
```

### 6.5 Agent & Skill Management Interface

**Goal:** Provide a comprehensive UI to manage, evaluate, and integrate internal agents and external skills.

**Features:**
1. **Visual Hub:** Displays a dashboard of all available agents (Core Agents & Installed) and the specific skills/tools they can access.
2. **Import Process Flow:** When importing a new skill/agent from an external system via the active "New Skill" wizard:
   - **File Reading & Parsing:** Ingests documentation or code files.
   - **Conversion:** Translates the external logic into the Office Hub compatible format (e.g., MCP manifests or native Rust/YAML configs).
   - **Testing:** Runs automated unit and integration tests in a sandbox environment.
   - **Reporting:** Generates an evaluation report outlining strengths, weaknesses, and recommendations for adjusting the agent/skill.
   - **Approval:** Requires explicit user approval to finalize integration into the live registry.
3. **Continuous Adjustment:** Allows users to tweak agent configurations based on the generated reports.

---

## 7. MCP Host & Protocol

### 7.1 Protocol Overview

Office Hub implements **Model Context Protocol (MCP) v2024-11-05** using JSON-RPC 2.0 over stdin/stdout transport.

```
MCP Server process (stdio)
    stdin  ← JSON-RPC requests  from McpHost
    stdout → JSON-RPC responses to McpHost
    stderr → ignored (or logged in debug mode)
```

### 7.2 Server Lifecycle

```
install("npm:@mcp/server-filesystem")
    │
    ▼ 1. Source parsing → McpServerSource::NpmPackage
    ▼ 2. Spawn: npx --yes @mcp/server-filesystem
    ▼ 3. MCP Initialize handshake:
         → { "method": "initialize", "params": { "protocolVersion": "2024-11-05", ... } }
         ← { "result": { "serverInfo": { "name": "...", "version": "..." }, "capabilities": {...} } }
         → { "method": "notifications/initialized" }
    ▼ 4. List tools:
         → { "method": "tools/list" }
         ← { "result": { "tools": [ { "name": "read_file", "description": "...", "inputSchema": {...} } ] } }
    ▼ 5. Register tools in flat index: "read_file" → server_id
    ▼ 6. Status: Running
```

### 7.3 Tool Call Flow

```
Orchestrator → McpHost::call_tool("read_file", { "path": "C:/report.xlsx" })
    │
    ▼ Resolve server: tool_index.get("read_file") → server_id
    ▼ Check server status: Running
    ▼ Send via StdioTransport:
        → { "jsonrpc": "2.0", "id": "uuid", "method": "tools/call",
             "params": { "name": "read_file", "arguments": { "path": "..." } } }
        ← { "jsonrpc": "2.0", "id": "uuid",
             "result": { "content": [ { "type": "text", "text": "..." } ] } }
    ▼ Return ToolCallResult { content: [...], is_error: false }
```

### 7.4 Registry Limits

- Maximum 50 MCP servers (configurable in rules/default.yaml)
- New server installation requires HITL approval (`rules.converter.require_approval_for_new_server`)
- All servers run in separate processes (sandboxed by OS)
- TODO (Phase 7): Optional WASM sandbox for untrusted servers

---

## 8. Workflow Engine

### 8.1 Visual Workflow Builder (Drag-and-Drop)

The frontend features a node-based, drag-and-drop workflow editor to build and manage automation pipelines visually, without writing raw YAML.

**AI-Assisted Flow Generation:**
A dedicated Chat Pane is docked at the bottom right of the builder. Users can input natural language requests (e.g., "Tạo flow đọc email và tóm tắt vào file Word"), which the LLM Gateway parses into a workflow schema and renders directly onto the React Flow canvas for review, editing, and saving.

**Core Nodes/Actions:**
1. **Input:** Defines trigger payloads, variables, and context.
2. **Filter/Condition:** Adds conditional checks (`if/else`, data validation).
3. **Logic Gate:** Controls parallel execution, branching, and merging of data.
4. **Output:** Defines final return values, notification dispatches, or file saves.

The visual builder serializes directly to the system's YAML schema.

### 8.2 YAML Schema

Workflows are defined in `workflows/*.yaml` files. Structure:

```yaml
id: workflow-id              # unique, snake-case
name: "Human-readable name"
version: "1.0.0"
trigger:
  type: email_received | file_changed | schedule | voice_command | manual
  config: { ... }            # trigger-specific config
context:
  variables:
    key: "value or template"
steps:
  - id: step_id
    name: "Step name"
    agent: orchestrator | analyst | office_master | web_researcher | converter
    action: action_string
    condition: "{{ steps.prev.result == 'ok' }}"  # optional
    config: { ... }
    input:
      key: "{{ steps.prev_step.output_field }}"
    output:
      var_name: "{{ result.field }}"
    timeout_seconds: 120
    on_error:
      action: abort | notify_and_pause | skip
    run: on_success | always | on_failure   # default: on_success
error_handlers:
  - type: timeout | com_error | generic
    action: notify_and_abort
```

### 8.2 Template Engine

The `TemplateEngine` resolves `{{ expression }}` in all string fields:

| Expression | Resolves to |
|-----------|------------|
| `{{ context.var_name }}` | Value from workflow context |
| `{{ steps.step_id.field }}` | Output field from a previous step |
| `{{ DATE:yyyy-MM-dd }}` | Current date in given format |
| `{{ NOW }}` | Current UTC datetime (RFC 3339) |
| `{{ missing_var }}` | `[UNRESOLVED: missing_var]` (logged as warning) |

### 8.3 Trigger Implementation Plan (Phase 6)

| Trigger | Implementation | Library/API |
|---------|---------------|-------------|
| `email_received` | Outlook COM: `Application.NewMailEx` event | `Win32_System_Com` |
| `file_changed` | `ReadDirectoryChangesW` | `Win32_Storage_FileSystem` |
| `schedule` | `tokio::time::interval` + cron parser | `tokio-cron-scheduler` |
| `voice_command` | WebSocket message from Mobile | `tokio-tungstenite` |
| `manual` | Direct API call | Already implemented |

### 8.4 Execution Model

```
WorkflowEngine (single Arc<Self>)
    │
    ├── definitions: DashMap<id, WorkflowDefinition>  ← hot-reloadable YAML
    ├── active_runs:  DashMap<run_id, WorkflowRun>    ← in-flight runs
    ├── run_history:  DashMap<workflow_id, Vec<Run>>   ← completed runs (last 100)
    │
    ├── exec_tx: mpsc::Sender<EngineMessage>
    │       └── executor_loop task (single worker, queued execution)
    │
    ├── trigger_tx: mpsc::Sender<TriggerEvent>
    │       └── trigger_dispatcher_loop task (fan-in from all triggers)
    │
    └── status_tx: broadcast::Sender<WorkflowRunStatusUpdate>
            └── consumed by: WebSocket server → Mobile push notifications
```

---

## 9. WebSocket Communication Layer

### 9.1 Protocol

**Transport:** WebSocket (`ws://localhost:9001`)  
**Message format:** Newline-delimited JSON  
**Authentication:** Bearer token in query string `?token=<secret>` (Phase 5)

#### Client → Server messages:

```json
// Voice/text command
{ "type": "command", "session_id": "...", "text": "Tạo báo cáo tuần" }

// HITL approval response
{ "type": "approval_response", "action_id": "...", "approved": true, "responded_by": "user@phone" }

// Workflow status query
{ "type": "workflow_status_request", "workflow_id": "email-to-report" }

// Keep-alive
{ "type": "ping", "timestamp_ms": 1700000000000 }
```

#### Server → Client messages:

```json
// Chat reply from Orchestrator
{ "type": "chat_reply", "session_id": "...", "content": "...", "agent_used": "analyst" }

// Approval request (HITL)
{ "type": "approval_request", "action_id": "...", "description": "...",
  "risk_level": "high", "timeout_seconds": 300,
  "actions": [{"id":"approve","label":"✅ Duyệt","style":"primary"},
               {"id":"reject","label":"❌ Từ chối","style":"danger"}] }

// Workflow status update
{ "type": "workflow_status", "run_id": "...", "status": "success", "message": "..." }

// General notification
{ "type": "notification", "level": "success", "title": "...", "body": "..." }
```

### 9.2 HITL Relay Architecture

```
[Orchestrator HITL Manager]
    │ register() → (action_id, rx: oneshot::Receiver<bool>)
    │
    ▼ notify WebSocket server
[WebSocketServer::send_approval_request(action_id, ...)]
    │
    ▼ broadcast to all connected Mobile clients
[Mobile App shows approval notification]
    │ User taps "✅ Duyệt"
    ▼ sends: { "type": "approval_response", "action_id": "...", "approved": true }
[WebSocketServer receives message]
    │
    ▼ call: Orchestrator::resolve_hitl(action_id, true)
[HitlManager::resolve(action_id, approved=true)]
    │ tx.send(true) on the oneshot channel
    ▼
[Orchestrator rx.await completes with approved=true]
    │
    ▼ Continue execution of the suspended task
```

---

## 10. Data Models & Interface Contracts

### 10.1 Key Rust Types Summary

```rust
// ── Core pipeline ─────────────────────────────────────────────────────
pub struct AgentTask {
    pub task_id:      String,
    pub action:       String,
    pub intent:       Intent,          // classified intent
    pub message:      String,          // original user text
    pub context_file: Option<String>,  // active file in File Browser
    pub session_id:   String,
    pub parameters:   HashMap<String, serde_json::Value>,
}

pub struct AgentOutput {
    pub content:     String,            // human-readable reply
    pub committed:   bool,              // did agent write to Office?
    pub tokens_used: Option<u32>,
    pub metadata:    Option<serde_json::Value>, // file paths, row counts, etc.
}

pub struct OrchestratorResponse {
    pub content:     String,
    pub intent:      Option<String>,
    pub agent_used:  Option<String>,
    pub tokens_used: Option<u32>,
    pub duration_ms: u64,
    pub metadata:    Option<serde_json::Value>,
}

// ── IPC DTOs (sent to React frontend) ────────────────────────────────
#[serde(rename_all = "camelCase")]
pub struct SendChatResponse {
    pub session_id:  String,
    pub reply:       ChatMessage,
    pub intent:      Option<String>,
    pub agent_used:  Option<String>,
    pub tokens_used: Option<u32>,
}

#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name:        String,
    pub path:        String,
    pub is_dir:      bool,
    pub size_bytes:  Option<u64>,
    pub modified_at: Option<String>,
    pub extension:   Option<String>,
}
```

### 10.2 Tauri IPC Command Registry

All `#[tauri::command]` functions are in `commands.rs`:

| Command | Arguments | Returns |
|---------|-----------|---------|
| `send_chat_message` | `SendChatRequest` | `SendChatResponse` |
| `create_session` | – | `String` (session_id) |
| `delete_session` | `session_id: String` | `()` |
| `list_sessions` | – | `Vec<SessionSummaryInfo>` |
| `update_llm_settings` | `LlmProviderSettings` | `()` |
| `get_llm_settings` | – | `LlmProviderSettings` |
| `ping_llm_provider` | – | `bool` |
| `list_directory` | `path: String` | `Vec<FileEntry>` |
| `open_file` | `path: String` | `()` |
| `list_workflows` | – | `Vec<Value>` |
| `trigger_workflow` | `workflow_id, payload` | `WorkflowRunResult` |
| `get_workflow_runs` | `workflow_id` | `Vec<WorkflowRunResult>` |
| `get_agent_statuses` | – | `Vec<AgentInfo>` |
| `list_mcp_servers` | – | `Vec<Value>` |
| `install_mcp_server` | `source: String` | `String` (server_id) |
| `uninstall_mcp_server` | `server_id: String` | `()` |
| `approve_action` | `action_id: String` | `()` |
| `reject_action` | `action_id, reason` | `()` |
| `list_pending_approvals` | – | `Vec<Value>` |
| `get_app_info` | – | `Value` |
| `check_system_requirements` | – | `Value` |
| `export_audit_logs` | `from_date, to_date, output_path` | `String` |

---

## 11. Implementation Phases

### Phase 0 — Foundation (Week 1–2) ✅

**Goal:** Project skeleton, tooling, CI/CD.

| Task | Status |
|------|--------|
| Monorepo structure created | ✅ |
| Cargo.toml with all dependencies | ✅ |
| Tauri v2 configured (tauri.conf.json) | ✅ |
| React + TypeScript + Vite setup | ✅ |
| .gitignore, README.md | ✅ |
| GitHub Actions CI pipeline | ✅ |
| Module stubs for all 6 core modules | ✅ |
| All Rust modules compile (stub mode) | 🔄 Verify |
| Unit tests for stub modules pass | 🔄 Verify |

**Milestone:** `cargo build` and `npm run build` succeed with zero errors.

---

### Phase 1 — App Shell + LLM Gateway (Week 3–5)

**Goal:** Working desktop UI + functional LLM integration.

**Backend tasks:**
- [x] `LlmGateway::complete()` – real Gemini API call working
- [x] `LlmGateway::complete()` – Ollama local fallback working
- [x] Token cache: hit/miss verified in unit tests
- [x] Hybrid mode: fallback tested with mocked failures
- [x] `AppConfig::load()` reads config.yaml correctly
- [x] Tauri IPC: `ping_llm_provider` returns real result

**Frontend tasks:**
- [x] Main layout: sidebar + content area (resizable panels)
- [x] File Browser: directory listing, breadcrumb, file type icons
- [x] Chat Pane: message list, input box, markdown rendering
- [x] Settings page: LLM provider selector, API key input (masked)
- [x] System status indicator (LLM connected / offline)

**Milestone:** User can type a message in Chat Pane and receive a real LLM response. (✅ Hoàn thành)

---

### Phase 2 — Orchestrator + MCP Host (Week 6–8)

**Goal:** Full intent classification and task routing pipeline operational.

**Backend tasks:**
- [x] `IntentClassifier::classify_fast()` – all regex patterns tested
- [x] `IntentClassifier::classify_fast()` → correctly routes 20+ test messages
- [x] `Router::dispatch()` – routes to stub agents, returns `AgentOutput`
- [x] `SessionStore` – push/get/evict tested under concurrent load
- [x] `RuleEngine::validate()` – all 8 rule types tested with edge cases
- [x] `HitlManager::register()` + `resolve()` – oneshot channel flow tested
- [x] `McpRegistry::install()` + `call_tool()` – tested with a simple echo server
- [x] `config.yaml` – rule file hot-reload without restart

**Frontend tasks:**
- [x] Chat shows `intent` and `agent_used` badges on each message
- [x] Pending HITL approvals panel (list + approve/reject buttons)
- [x] Agent status dashboard (idle/busy/error for each agent)
- [x] MCP server manager (list, install from npm, uninstall)

**Milestone:** User can chat, intent is classified, a stub agent responds, HITL approval flow works end-to-end. (✅ Hoàn thành)

---

### Phase 3 — Office Agents: Excel + Word + PowerPoint (Week 9–12)

**Goal:** Real Microsoft Office automation via COM.

**Backend tasks:**
- [ ] COM initialization: `CoInitializeEx` + `CoCreateInstance` for Excel
- [ ] `AnalystAgent::read_range()` – reads real Excel data
- [ ] `AnalystAgent::write_range()` – writes + hard-truth verify
- [ ] `AnalystAgent::generate_formula()` – XLOOKUP, SUMIF via LLM + validate
- [ ] `AnalystAgent::audit_formulas()` – scans for #REF!, #VALUE!
- [x] Agent Skills Standard Integration (`.agent/skills/`)
- [ ] COM for Word: `Documents.Open()` + `Paragraphs` iteration
- [x] `OfficeMasterAgent::word_create_template_from_document()` (LlmGateway integrated)
- [ ] `OfficeMasterAgent::word_edit_document()` with bookmark targeting
- [ ] COM for PowerPoint: `Presentations.Add()` + `Slides.Add()`
- [ ] `OfficeMasterAgent::ppt_create_presentation()` from brand template
- [ ] Backup-before-write implemented and tested
- [ ] `RuleEngine` hard-truth violation blocks incorrect writes

**Test coverage targets:**
- COM operations tested against Office 2016, 2019, 2021, 365
- Fallback to OpenXML SDK if COM unavailable
- All writes verified with read-back

**Milestone:** Full Word report generation from an Excel data source, end-to-end.

---

### Phase 4 — Web Researcher Agent (UIA) (Week 13–15) ✅

**Goal:** Extract real web data from Edge/Chrome via Windows UI Automation.

**Backend tasks:**
- [x] `CoCreateInstance(&CUIAutomation)` – UIA entry point
- [x] Browser process detection (Edge/Chrome via `EnumProcesses`)
- [x] Attach UIA to browser window root element
- [x] `IUIAutomationTablePattern` – extract HTML table data
- [x] `IUIAutomationTextPattern` – extract visible text
- [x] GDI screenshot capture for grounding evidence
- [x] Domain policy enforcement: whitelist check blocks non-listed URLs
- [x] All UIA actions written to structured audit log
- [x] HITL approval required for `navigate_to_url`
- [x] Web-to-Excel end-to-end workflow tested

**Milestone:** "Lấy bảng giá xăng từ Petrolimex → cập nhật vào Excel" works end-to-end. (✅ Hoàn thành)

---

### Phase 5 — Mobile Client + WebSocket (Week 16–18) ⏳ (Backend ✅)

**Goal:** Remote control from smartphone, HITL approval from mobile.

**Backend tasks:**
- [x] `tokio-tungstenite` WebSocket server listening on `:9001`
- [x] Authentication: Bearer token via `ClientMessage::Auth`
- [x] JSON message parser + dispatcher
- [x] HITL relay: `approval_request` pushed to mobile → `approval_response` received
- [x] Workflow status push notifications
- [x] Heartbeat / idle timeout / ping-pong handling

**Mobile App (React Native or Flutter):**
- [ ] Connect to WebSocket with token
- [ ] Voice input → send `{ type: "command", text: "..." }`
- [ ] Approval notification with Approve/Reject buttons
- [ ] Workflow status feed
- [ ] Offline queue (commands buffered when disconnected)

**Milestone:** User approves a browser navigation request from mobile phone. (✅ Backend verified via test scripts)

---

### Phase 6 — Event-Driven Workflow Engine (Week 19–21)

**Goal:** Workflows fire automatically from real triggers.

**Backend tasks:**
- [x] `EmailTrigger` – Outlook COM `NewMailEx` event → filter → emit TriggerEvent
- [x] `FileWatchTrigger` – `ReadDirectoryChangesW` for `.xlsx/.docx` changes
- [x] `ScheduleTrigger` – cron-like scheduling with `tokio-cron-scheduler`
- [ ] `VoiceTrigger` – WebSocket voice command mapped to workflow ID
- [x] Workflow step execution wired to real agents (not stubs)
- [ ] Retry-on-failure with configurable max_retries
- [ ] Workflow YAML hot-reload (watch `workflows/` directory)

**Frontend tasks:**
- [ ] Visual workflow builder (drag-drop steps, configure triggers)
- [ ] Workflow run history with step-by-step timeline
- [ ] Real-time status updates via Tauri event listener

**Milestone:** Email with Excel attachment automatically generates a Word report.

---

### Phase 7 — Converter Agent + MCP Marketplace (Week 22–24) ✅

**Goal:** Self-extending skill system.

**Backend tasks:**
- [x] `ConverterAgent::learn_skill_from_docs()` – parse API doc url, generate MCP server using LLM
- [x] MCP server generation and sandbox persistence (`/tmp/office_hub_skill_<uuid>`)
- [x] Auto-registration: install python scripts as MCP servers into Registry
- [x] Sandboxed execution for new skills via dynamic installation
- [x] Expose `call_mcp_tool` via Orchestrator IPC

**Frontend tasks:**
- [x] MCP Marketplace UI (AgentManager Component)
- [x] Visual Hub displaying core agents and active MCP skills
- [x] 4-step Import Wizard (URL -> Parse -> Test Sandbox -> Approve)
- [x] React routing integration for Marketplace

**Milestone:** User imports a documentation URL → Converter creates working MCP tool → user approves. (✅ Hoàn thành)

---

### Phase 8 — Testing, Hardening & Release (Week 25–28) ✅ [COMPLETED]

**Goal:** Production-ready v1.0 release.

**Key Hardening Tasks (Current Focus):**
- [x] Resolve `AgentTask` instantiation blockers in `analyst` and `folder_scanner` modules.
- [x] Sanitize codebase (resolve compiler warnings, unused imports, dead code).
- [x] Fix all failing unit tests across modules (`orchestrator`, `websocket`, `workflow`, `system`).
- [x] Achieve ≥ 80% line coverage on `Orchestrator`, `LlmGateway`, and `RuleEngine`.
- [x] Validate end-to-end integration of the `RuleEngine` with real Office COM scenarios.

| Task | Detail |
|------|--------|
| Unit test coverage | ≥ 80% line coverage on Orchestrator, LLM Gateway, Rule Engine |
| Integration tests | Full workflow end-to-end on real Office 2016/365 |
| Performance benchmarks | < 100MB RAM idle; < 50ms intent classification |
| Security audit | COM injection, UIA privilege escalation, API key exposure |
| User acceptance testing | 10+ real users in 1 week beta |
| Installer | NSIS `.exe` and `.msi` via Tauri bundler |
| Auto-update | Tauri updater with GitHub Releases |
| Documentation | User guide, developer guide, API reference |

---

### Phase 9 — Advanced UI & Ecosystem Management (Week 29–31) ✅ [COMPLETED]

**Goal:** Enhance user experience with visual workflow building, session history tracking, and agent/skill lifecycle management.

**Frontend tasks:**
- [x] Implement `HistoryTree` component in the App Shell to group sessions by `topic_id`.
- [x] Build the Drag-and-Drop Workflow Editor (nodes: input, filter, logic gate, output) that serializes to YAML.
- [x] Build the Agent & Skill Manager dashboard.
- [x] Implement the external skill import wizard (Read File → Convert to Office Hub format).
- [x] Create the sandbox testing UI and display the generated Evaluation Report (strengths, weaknesses, suggestions).
- [x] Add the Approval workflow UI to finalize the integration of tested agents/skills.

**Backend tasks:**
- [x] Implement Multi-turn Loop (ReAct) in `orchestrator/mod.rs` to allow LLM to process `committed: false` agent outputs and execute follow-up tool calls (e.g., Search -> Read -> Summarize) within a single user request.
- [x] Extend `SessionStore` to support querying and grouping by `topic_id`.
- [x] Build the sandbox execution environment for testing imported external agents/skills.
- [x] Implement the reporting engine that evaluates an imported agent/skill's compatibility and security.

**Milestone:** User can visually drag-and-drop a workflow, view their history by topic, and safely import, test, and approve a 3rd-party skill. (✅ Hoàn thành)

---

### Phase 9.1 — Advanced UI Refinements (Week 32) ✅ [COMPLETED]

**Goal:** Enhance layout usability, workflow generation, and correct UI state issues.

**Frontend tasks:**
- [x] Update `HistoryTree` to show topic names, nested session dropdowns, and a "Delete Session" button.
- [x] Overhaul File Browser layout to include a top `FilePreview` pane and minimize the `ChatPane` to the bottom right for concurrent file interaction.
- [x] Embed an AI chat interface into `WorkflowBuilder` to auto-generate flow schemas from user requests.
- [x] Debug and fix the blank Core Agents display in the `AgentManager`.
- [x] Ensure the "New Skill" functionality is active.
- [x] Relocate the Sidebar Settings button to the bottom using `mt-auto`.

**Milestone:** All requested UI features function seamlessly, enabling easier workflow building and session management. (✅ Hoàn thành)

---

### Phase 10 — System Stability & Advanced Ecosystem (Week 33-35) ⏳ [PLANNED]

**Goal:** Implement strategic upgrades identified from the SWOT analysis to solidify the system's stability, user onboarding, and interaction models.

**Core Objectives:**
- [ ] **COM Anti-Deadlock Watchdog**: Build a Rust-level watchdog to detect and automatically close blocking Office popups (e.g., "Update Links", "Activate Product") to guarantee uninterrupted automation workflows.
- [ ] **Voice-to-Action via Mobile**: Upgrade the React Native Mobile Companion App to capture voice commands, process them via Whisper API (or local equivalent), and send JSON payloads via WebSocket to trigger desktop workflows.
- [ ] **Multi-Modal File Scanner**: Extend the `FolderScanner` capabilities to use Vision APIs to process charts in Excel and complex PowerPoint slides, moving beyond text-only extraction.
- [ ] **One-Click Installer**: Bundle the Tauri build, necessary dependencies, and Ollama default configurations into a single, user-friendly `.exe` to eliminate technical friction during onboarding.

**Milestone:** The system correctly recovers from a blocked Excel popup during an automated task, and a user can trigger a workflow remotely using a voice command from their phone.

---

## 12. Testing Strategy

### 12.1 Test Pyramid

```
                    ┌─────────────┐
                    │  E2E Tests  │  5%
                    │  (Tauri +   │
                    │   Office)   │
                   ┌┴─────────────┴┐
                   │ Integration   │  20%
                   │ Tests         │
                   │ (agents +     │
                   │  workflows)   │
                  ┌┴───────────────┴┐
                  │   Unit Tests    │  75%
                  │  (all modules,  │
                  │   mock I/O)     │
                  └─────────────────┘
```

### 12.2 Critical Test Cases

| Module | Test case | Expected |
|--------|----------|----------|
| IntentClassifier | "Đọc dữ liệu từ Sheet1" | ExcelRead, confidence ≥ 0.78 |
| IntentClassifier | "Tạo slide từ file Word này" | PptConvertFrom, confidence ≥ 0.82 |
| IntentClassifier | "Lấy bảng giá xăng từ Petrolimex" | WebExtractData, URL extracted |
| RuleEngine | CCCD number in LLM output | Blocked (Critical) |
| RuleEngine | `{{PLACEHOLDER}}` in Word content | Blocked |
| RuleEngine | 150% percentage in cell | Warning (non-blocking) |
| HardTruthVerify | Intended=1M, Actual=1.2M | CRITICAL violation, write blocked |
| HardTruthVerify | Intended=1M, Actual=1M+0.001 | PASS |
| SessionStore | Push 200 messages to 32K window | Auto-trim, utilization ≤ 80% |
| TokenCache | Same request twice | Second returns from_cache=true |
| Router | WebNavigate intent | force_hitl=true in route entry |
| HitlManager | register + resolve(true) | oneshot channel delivers true |
| WorkflowEngine | Trigger unknown workflow | WorkflowError::NotFound |
| WorkflowEngine | Dry-run mode | All steps return stub output |
| DomainPolicy | `*.gov.vn` whitelist | mof.gov.vn → allowed; evil.com → blocked |
| ConverterAgent | Import skill from URL | LLM generates valid Python MCP script in /tmp |
| McpRegistry | `call_mcp_tool` command execution | Successfully routes standard MCP protocol tool call to the sandbox server |

### 12.3 Performance Benchmarks

| Metric | Target | Measurement |
|--------|--------|-------------|
| Cold start time | < 2 seconds | From launch to UI visible |
| Intent classification (FastRule) | < 5ms | p99 |
| Intent classification (LLM-assisted) | < 800ms | p95 |
| Excel cell read (COM) | < 100ms | Single cell |
| Excel range read (1000 cells) | < 500ms | |
| Word document create from template | < 3 seconds | 5-page doc |
| Idle memory usage | < 100 MB | No active tasks |
| Peak memory (during workflow) | < 300 MB | Full pipeline |
| Binary size (.exe) | < 20 MB | Release build, stripped |

### 12.4 Office Compatibility Matrix

| Office Version | Target | Priority |
|---------------|--------|---------|
| Microsoft 365 (current) | ✅ Full support | P0 |
| Office 2021 | ✅ Full support | P0 |
| Office 2019 | ✅ Full support | P1 |
| Office 2016 | ✅ Full support | P1 |

---

## 13. Security & Compliance

### 13.1 API Key Management

- API keys stored in **Windows Credential Manager** (not in config.yaml in plaintext)
- Tauri keyring plugin will be used: `tauri-plugin-stronghold` (Phase 2+)
- API keys never appear in log output (`never_log_api_keys: true`)
- Keys masked in UI (show only last 4 characters)
- Rotation reminder after 90 days

### 13.2 COM Security

- All COM calls wrapped in `unsafe` blocks with explicit documentation
- COM pointers never shared across threads without `Send + Sync` wrappers
- `CoUninitialize()` called on all cleanup paths (RAII guard)
- Workbook opened with `ConfirmConversions = false` to prevent macro auto-run

### 13.3 UIA Security

**Non-negotiable hard limits (enforced in code, not just policy):**

```rust
// In WebResearcherAgent::handle_interaction()
if self.is_action_blocked(&task.action) {
    return Err(anyhow!("Action '{}' is permanently blocked", task.action));
}
// This check runs BEFORE any HITL approval, making these truly unbypassable.
```

Permanently blocked (even with HITL approval):
- `form_submit`
- `file_download`  
- `authentication` (typing passwords)
- `payment` (any payment-related interaction)

### 13.4 LLM Output Safety

The Rule Engine enforces these checks on **every LLM response**:

1. **PII detection:** Regex for CCCD/CMND numbers, credit card numbers, email addresses
2. **Placeholder leakage:** `{{VAR}}` or `${VAR}` patterns in content to be written
3. **Percentage sanity:** Values outside 0-100% flagged
4. **Hard-truth verification:** Numeric values verified against real COM read-back

### 13.5 Audit Trail

Every sensitive operation generates a structured log entry:

```json
{
  "timestamp": "2025-01-15T09:30:00Z",
  "event_type": "uia_action",
  "action": "navigate_to_url",
  "url": "https://petrolimex.com.vn",
  "agent": "web_researcher",
  "session_id": "...",
  "run_id": "...",
  "approved_by": "user@mobile",
  "approved_at": "2025-01-15T09:29:55Z",
  "result": "success"
}
```

Audit logs are stored as newline-delimited JSON in `$APPDATA/office-hub/logs/audit/`.

---

## 14. Risk Register

### 14.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| COM API behaviour varies across Office versions | High | High | Test matrix for 2016/2019/2021/365; fallback to OpenXML SDK |
| UIA tree structure changes with browser updates | Medium | High | Multiple selector strategies; auto-heal with fuzzy matching |
| LLM hallucinations in numeric data | High | High | Hard-Truth Verification is mandatory for all writes |
| Tauri v2 API breakage | Low | Medium | Pin exact dependency versions; snapshot test IPC contracts |
| Windows UI Automation unavailable (locked-down enterprise) | Medium | High | Graceful degradation: disable Web Researcher, other agents still work |
| Large Excel files (>50MB) causing COM timeout | Medium | Medium | Chunked reading; configurable timeout; progress feedback |
| Mobile WebSocket connection reliability | Medium | Low | Reconnect logic with exponential backoff; command queue |
| MCP server process crashes | Low | Low | Health check loop; auto-restart with backoff |

### 14.2 Project Risks

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Scope creep from MCP ecosystem | Medium | Medium | Feature freeze after Phase 7; MCP stays opt-in |
| Performance regression from Tauri updates | Low | Low | CI benchmark gate: reject PRs that regress > 10% |
| API key cost overrun during development | Medium | Low | Use Ollama for all dev/test; Gemini only for integration tests |

---

## 15. Definition of Done

### 15.1 Per-Phase DoD

Each phase is "done" when:
- [ ] All planned features implemented and manually verified
- [ ] Unit tests pass with ≥ 80% coverage for new code
- [ ] Zero clippy warnings (`cargo clippy -- -D warnings`)
- [ ] Zero TypeScript errors (`tsc --noEmit`)
- [ ] `cargo build --release` produces a working binary
- [ ] Performance benchmarks within targets
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md roadmap checkbox updated

### 15.2 v1.0 Release DoD

- [ ] All Phases 0–8 complete
- [ ] User documentation written
- [ ] 10+ beta testers confirmed no P0/P1 bugs
- [ ] Security audit completed (internal)
- [ ] Binary signed with code signing certificate
- [ ] Auto-update mechanism tested
- [ ] NSIS installer + MSI available as GitHub Release assets
- [ ] Support email / GitHub Issues triaged and < 3-day response

---

## Appendix A: Config Schema Reference

```yaml
# config.yaml (full schema)
llm:
  provider: "gemini"          # gemini | openai | ollama | lmstudio
  hybrid_mode: true
  cloud:
    gemini_api_key: null       # Store in Windows Credential Manager
    openai_api_key: null
    gemini_model: "gemini-1.5-pro"
    openai_model: "gpt-4o"
  local:
    ollama_endpoint: "http://localhost:11434"
    lmstudio_endpoint: "http://localhost:1234"
    model_name: "llama3.1"
  context_window_limit: 32000
  token_cache_enabled: true

websocket:
  host: "0.0.0.0"
  port: 9001
  require_approval_for_sensitive: true

agents:
  analyst:
    allow_vba_execution: false
    max_rows_per_query: 100000
  office_master:
    default_word_template: null
    default_ppt_template: null
    preserve_format: true
  web_researcher:
    preferred_browser: "edge"
    screenshot_grounding: true
    require_approval_for_navigation: true

paths:
  rules_dir: "rules"
  workflows_dir: "workflows"
  vector_db_dir: "data/vectors"
  sessions_dir: "data/sessions"
  audit_log_dir: "data/audit"
```

---

## Appendix B: Known Limitations (v1.0 Scope)

1. **Windows-only:** COM Automation and UIA are Windows APIs. No macOS/Linux support planned.
2. **Microsoft Office required:** No LibreOffice support. Minimum Office 2016.
3. **Single user:** No multi-user / multi-seat license management.
4. **No cloud sync:** All data stays local. No synchronisation across devices.
5. **English + Vietnamese:** LLM prompts optimised for VI/EN. Other languages best-effort.
6. **Browser support:** Edge and Chrome only for Web Researcher. Firefox not supported (different UIA tree structure).

---

*This document represents the complete technical plan for Office Hub v1.0. All design decisions are intentional and documented. Review comments should reference the section number.*

**Document owner:** Office Hub Core Team  
**Review deadline:** Before Phase 1 kickoff  
**Next revision:** After Phase 2 completion (Orchestrator review)