# Office Hub – Kế hoạch Release v1.0

**Tài liệu:** RELEASE_PLAN_v1.0.md  
**Trạng thái:** 📋 UPDATED – Dựa trên audit source code ngày 2026-05-01  
**Phiên bản mục tiêu:** 1.0.0  
**Phiên bản hiện tại:** 0.1.0 (`Cargo.toml`, `package.json`, `App.tsx` sidebar label)

---

## 1. Audit Thực Tế – Những Gì Đã Hoàn Thành

> Nhiều blocker trong plan cũ đã được **GIẢI QUYẾT** mà không được ghi nhận. Audit source code cho thấy:

### ✅ Backend (Rust/Tauri) – Hoàn chỉnh

| Module | File | Trạng thái |
|--------|------|-----------|
| **Native GenAI Pipeline** | `orchestrator/mod.rs` | ✅ `process_message_native()` là **primary pipeline** (confirmed `commands.rs:137`) |
| **Legacy Pipeline** | `orchestrator/mod.rs` | ✅ `process_message()` chỉ là fallback, đã compact (token-efficient) |
| **DAG Planning** | `orchestrator/plan*.rs` | ✅ `process_message_planned()` tồn tại và hoạt động |
| **MCP Broker** | `mcp/broker.rs` | ✅ Hoàn chỉnh – `McpBroker` với internal + external registry |
| **MCP Servers nội bộ** | `mcp/internal_servers.rs` (148KB!) | ✅ PolicyServer, KnowledgeServer, MemoryServer, SkillServer, FileSystemServer, AnalyticServer (Polars), OfficeComServer, Win32AdminServer, ScriptingServer (Rhai), ChartServer, WebSearchServer, WebFetchServer |
| **Native Chart** | `mcp/native_chart.rs` | ✅ `NativeChartServer` dùng **Plotters** render chart natively |
| **Agent Adapters** | `mcp/agent_mcp_adapter.rs` | ✅ `register_agent_adapters()` được gọi trong `lib.rs:655` |
| **Tất cả Agents** | `agents/` | ✅ Registered: analyst, office_master, web_researcher, converter, folder_scanner, **outlook**, system, **win32_admin** |
| **LLM Gateway** | `llm_gateway/mod.rs` (117KB) | ✅ Gemini, OpenAI, Anthropic, Z.AI, Ollama, LMStudio; auto-routing; 3-tier (fast/default/reasoning) |
| **SSE + REST Server** | `sse_server.rs` (38KB) | ✅ Mobile transport port 9002 |
| **WebSocket Server** | `websocket/` | ✅ Office Add-in port 9001 |
| **CRDT** | `crdt.rs` | ✅ Automerge-based co-authoring |
| **System Layer** | `system/` | ✅ Tray, startup registry, QR pairing, Tailscale probe, sleep suppress |
| **Workflow Engine** | `workflow/` | ✅ YAML-driven, broadcast progress |
| **Session Persistence** | `orchestrator/session.rs` | ✅ Handoff docs, auto-summarization |
| **Memory Store** | `orchestrator/memory.rs` | ✅ SQLite FTS5 long-term memory |
| **Knowledge Base** | `knowledge.rs` | ✅ Workspace-aware, CRUD IPC |

### ✅ Desktop UI (React/TypeScript) – Gần hoàn chỉnh

| Component | File | Trạng thái |
|-----------|------|-----------|
| **App Shell** | `src/App.tsx` | ✅ 7 tabs: Chat, Files, Workflows, AI Dashboard, Knowledge, Monitor, Settings |
| **Chat Pane** | `components/ChatPane/ChatPane.tsx` (20KB) | ✅ Streaming, history sidebar, workspace context |
| **Settings – LLM** | `components/Settings/LlmTab.tsx` (19KB) | ✅ Provider config, API keys, model picker, ping |
| **Settings – Mobile** | `components/Settings/MobileTab.tsx` (9.5KB) | ✅ **QR Code tab đã tồn tại** |
| **Settings – System** | `components/Settings/SystemTab.tsx` (8.6KB) | ✅ Sleep override, startup toggles |
| **Settings – Skills** | `components/Settings/SkillManager.tsx` (12KB) | ✅ Skill builder/manager |
| **AI Dashboard** | `components/Settings/AIDashboard.tsx` (10KB) | ✅ Token metrics, model health |
| **Agent Monitor** | `components/AgentMonitor/` | ✅ DAG/realtime visualisation |
| **File Browser** | `components/FileBrowser/` | ✅ Directory navigation |
| **Knowledge Base** | `components/KnowledgeBase/` | ✅ Workspace-scoped docs |
| **Workflow Builder** | `components/WorkflowBuilder/` | ✅ React Flow UI |
| **Workspace Switcher** | `components/WorkspaceSwitcher.tsx` (6.5KB) | ✅ |

### ✅ Mobile App (React Native/Expo) – Đã có đủ screens

| Screen | File | Trạng thái |
|--------|------|-----------|
| **Connection (QR Scan)** | `ConnectionScreen.tsx` (14KB) | ✅ Expo-camera, QR parse, manual IP |
| **Chat** | `ChatScreen.tsx` (27KB) | ✅ SSE streaming, markdown, file attach |
| **Progress** | `ProgressScreen.tsx` (3.8KB) | ✅ Hoạt động, indeterminate progress bar |
| **Approvals (HITL)** | `HomeScreen.tsx` (3.7KB) | ✅ Approve/Reject |
| **Artifacts** | `ArtifactsScreen.tsx` (8.4KB) | ✅ File list from server |
| **Settings** | `SettingsScreen.tsx` (6.4KB) | ✅ Connection info, disconnect |
| **Navigation** | `MainTabs.tsx` (3.5KB) | ✅ Bottom tabs |

---

## 2. Blockers Thực Sự Còn Lại

Sau khi audit, chỉ còn **4 nhóm việc** trước khi release:

### 🔴 R1 – Version Bump (P0, ~1 giờ)

Đây là việc đơn giản nhất nhưng bắt buộc:

```
- [ ] src-tauri/Cargo.toml: version = "0.1.0" → "1.0.0"
- [ ] package.json: "version": "0.1.0" → "1.0.0"
- [ ] src-tauri/tauri.conf.json: kiểm tra và bump version
- [ ] src/App.tsx line 315: "v0.1.0" → "v1.0.0" (sidebar label)
```

---

### 🔴 R2 – E2E Verification & Bug Hunt (P0, 2–3 sessions)

Source code đã đủ nhưng **chưa có bằng chứng pass E2E**. Cần chạy từng test case:

#### TC-01: Desktop Native Pipeline
```
- [ ] Khởi động app (npm run tauri dev hoặc build)
- [ ] Chat: "Tóm tắt file C:\test.docx"
- [ ] Confirm: process_message_native được gọi (log: "Invoking genai native tool call")
- [ ] Confirm: không có UI hang, response hiển thị đầy đủ
```

#### TC-02: MCP Tools gọi đúng
```
- [ ] Chat: "Chính sách bảo mật của hệ thống là gì?"
- [ ] Confirm: LLM gọi query_policy qua McpBroker (không hardcode context)
- [ ] Chat: "Lần trước tôi đã yêu cầu gì?"
- [ ] Confirm: LLM gọi search_memory với workspace prefix
```

#### TC-03: Native Chart (Plotters)
```
- [ ] Chat: "Vẽ biểu đồ doanh thu Q1 2026: Tháng 1=100, Tháng 2=150, Tháng 3=130"
- [ ] Confirm: NativeChartServer render PNG, hiển thị trong chat
```

#### TC-04: Outlook Agent E2E
```
- [ ] Yêu cầu: Outlook phải đang chạy trên máy
- [ ] Chat: "Đọc 3 email chưa đọc mới nhất"
- [ ] Confirm: OutlookAgent trả về list email qua COM
- [ ] Chat: "Gửi email test đến test@example.com"
- [ ] Confirm: HITL popup xuất hiện → User reject → email không gửi
```

#### TC-05: Mobile QR Pairing
```
- [ ] Desktop: Settings → Mobile tab → Generate QR
- [ ] Mobile: ConnectionScreen → Quét QR
- [ ] Confirm: Mobile hiển thị "Connected"
- [ ] Mobile chat: "Xin chào" → Confirm response nhận qua SSE
```

#### TC-06: Mobile Progress Real-time
```
- [ ] Mobile chat: "Quét thư mục C:\Reports và tóm tắt"
- [ ] Confirm: ProgressScreen hiển thị task đang chạy (activeTasks không rỗng)
- [ ] Confirm: Sau khi xong, task biến mất khỏi ProgressScreen
```

#### TC-07: LLM Failover
```
- [ ] Xóa API key của default provider trong config.yaml
- [ ] Chat bất kỳ → Confirm: LLM Gateway tự chuyển sang provider dự phòng
- [ ] Không có error toast hiển thị cho user
```

#### TC-08: Workflow Engine
```
- [ ] Tạo 1 workflow YAML đơn giản trong workflows/
- [ ] Desktop: Workflows tab → Trigger workflow
- [ ] Confirm: progress event được emit, hiển thị trong UI
```

---

### 🟠 R3 – Known Technical Debt (P1, cần fix trước release)

Các vấn đề được ghi nhận trong handoff sessions trước, **chưa có bằng chứng đã fix**:

#### R3.1 – Mobile UI Hang khi Orchestrator lỗi
```
Vấn đề: Nếu LLM timeout hoặc agent lỗi, SSE stream không phát event kết thúc
         → Mobile spinner chạy mãi.

Fix cần làm:
- [ ] src-tauri/src/sse_server.rs: Đảm bảo SseEvent::result được phát khi có Err
- [ ] src-tauri/src/orchestrator/mod.rs: process_message_native bắt buộc phát
      error event thay vì return Err silently khi đang stream
- [ ] Test: Kill LLM server giữa chừng → Mobile phải hiện Error banner trong 5s
```

#### R3.2 – Mobile Reconnect Loop
```
Vấn đề: Khi SSE kết nối bị drop và reconnect, session_id bị reset
         → Chat history bị mất, ProgressScreen reset.

Fix cần làm:
- [ ] mobile/src/store/sseStore: Lưu session_id vào AsyncStorage
- [ ] Khi reconnect, gửi lại session_id cũ qua header X-Session-Id
- [ ] Backend sse_server.rs: Resume session thay vì tạo mới
```

#### R3.3 – File URI Intercept trên Mobile
```
Vấn đề: Server trả về office-hub://files/... nhưng React Native không biết
         cách mở → Lỗi "File not found".

Fix cần làm:
- [ ] mobile/src/screens/ChatScreen.tsx: Detect office-hub:// scheme
- [ ] Map sang: GET http://<host>:9002/api/v1/files/download?path=...
- [ ] Hiển thị: Download button hoặc open-with picker
```

---

### 🟡 R4 – Build & Release Packaging (P0, ~1 session)

```
- [ ] Chạy cargo test --lib → 100% pass (fix failures nếu có)
- [ ] Chạy npm run tauri build → tạo .exe installer
- [ ] Test installer trên máy sạch (không có Rust/Node): app khởi động OK
- [ ] Mobile: expo build hoặc eas build → APK Android
- [ ] Tạo CHANGELOG.md (template bên dưới)
- [ ] Cập nhật README.md: xóa placeholder GitHub URL, đánh dấu Phase 9 pending
```

**Template CHANGELOG.md:**
```markdown
# Changelog

## [1.0.0] – 2026-xx-xx

### Architecture
- Native GenAI Tool Calling pipeline (genai 0.5) làm primary orchestrator
- Internal MCP Broker với 12+ built-in servers (Policy, Memory, Knowledge,
  Analytic/Polars, Office COM, Win32 Admin, Rhai Scripting, Chart, Web Search/Fetch)
- Hybrid SSE+REST mobile transport (port 9002)
- DAG Planning Layer (process_message_planned)

### Agents
- AnalystAgent: Polars SQL, ECharts/Plotters charts, Excel COM
- OfficeMasterAgent: Word/PPT/Excel via COM + Add-in WebSocket
- WebResearcherAgent: Obscura headless engine, UIA browser control
- FolderScannerAgent: Multi-format batch processing
- OutlookAgent: Email/Calendar/Tasks via COM
- ConverterAgent: Skill learning, zip conversion
- SystemAgent + Win32AdminAgent: OS integration

### Desktop UI
- 7-tab interface: Chat, Files, Workflows, AI Dashboard, Knowledge, Monitor, Settings
- Settings: LLM config, Mobile QR pairing, System toggles, Skill Manager
- Theme system: Light/Dark/System with CSS variables
- Resizable sidebar, Workspace isolation

### Mobile
- React Native (Expo) with SSE streaming
- 6 screens: Connection (QR scan), Chat, Progress, Approvals, Artifacts, Settings
- HITL Approve/Reject from mobile

### Infrastructure
- CRDT co-authoring (Automerge)
- Workspace isolation (per-workspace memory, policies, history)
- LLM 3-tier routing: fast/default/reasoning with auto-failover
- SQLite FTS5 long-term memory
- Rule Engine (YAML-based output validation)
- Workflow Engine (YAML triggers + actions)
```

---

## 3. Thứ tự thực thi cho Session tiếp theo

```
START HERE
    │
    ▼
R3.1 Fix Mobile UI Hang  ──────────────────────────────────────┐
    │                                                            │ Có thể
    ▼                                                            │ song song
R3.2 Fix Mobile Reconnect  ────────────────────────────────────┤
    │                                                            │
    ▼                                                            │
R3.3 Fix File URI Intercept ───────────────────────────────────┘
    │
    ▼
R2 E2E Test (TC-01 → TC-08, ghi kết quả vào RELEASE_TEST_RESULTS.md)
    │
    ├─ Nếu có failures → Fix → Re-test
    │
    ▼
R1 Version Bump (Cargo.toml, package.json, tauri.conf.json, App.tsx)
    │
    ▼
R4 Build + Package (.exe installer, Android APK)
    │
    ▼
🎉 RELEASE v1.0.0
```

| Phase | Nội dung | Sessions ước tính |
|-------|---------|-----------------|
| R3 | Mobile stability fixes (3 bugs) | 1 |
| R2 | E2E test all 8 TC | 1–2 |
| R1 | Version bump | < 0.5 |
| R4 | Build + CHANGELOG + README | 1 |
| **Tổng** | | **3–4 sessions** |

---

## 4. Hướng dẫn cho Agent nhận Session tiếp theo

### Bắt đầu từ đây

**Bước 1:** Đọc file này để nắm context  
**Bước 2:** Bắt đầu với **R3.1** (Mobile UI hang) — dễ nhất, impact lớn nhất  
**Bước 3:** Chạy E2E tests theo thứ tự TC-01 → TC-08  
**Bước 4:** Ghi kết quả vào `docs/RELEASE_TEST_RESULTS.md` (tạo mới)  
**Bước 5:** Sau khi tất cả TC pass → version bump → build  

### Files quan trọng nhất

| File | Liên quan đến |
|------|-------------|
| `src-tauri/src/sse_server.rs` | R3.1 – Error event khi lỗi |
| `mobile/src/screens/ChatScreen.tsx` | R3.1 – Error banner UI |
| `mobile/src/store/sseStore.ts` | R3.2 – Session persistence |
| `mobile/src/screens/ChatScreen.tsx` | R3.3 – office-hub:// URI handler |
| `src-tauri/Cargo.toml` | R1 – Version bump |
| `package.json` | R1 – Version bump |
| `src/App.tsx:315` | R1 – Sidebar version label |

### Không cần làm (đã hoàn thành, plan cũ sai)

- ~~MCP Broker Phase 3–4~~ → **ĐÃ XONG** (xem `lib.rs:643-659`)
- ~~Desktop Settings Mobile Tab~~ → **ĐÃ CÓ** (`MobileTab.tsx` 9.5KB)
- ~~Settings System toggles~~ → **ĐÃ CÓ** (`SystemTab.tsx` 8.6KB)
- ~~Outlook Agent chưa register~~ → **ĐÃ REGISTER** (`lib.rs:424`)

---

## 5. Post-Release Roadmap (v1.1)

| Feature | Ghi chú |
|---------|---------|
| Calendar Daily Digest (07:00 trigger) | A4.5 trong Master Plan Amendment |
| Mobile Voice Input | `expo-speech` |
| Binary file stream (thay base64) | `GET /api/v1/files/download` |
| Per-device auth token | Hiện tại shared token |
| iOS build | Cần macOS + Apple Developer |
| Ollama auto-install UI polish | Hiện tại silent background |

---

*Cập nhật: 2026-05-01 — Dựa trên audit trực tiếp source code*  
*Phiên bản này thay thế hoàn toàn plan cũ (tạo ngày 2026-05-01 trước đó)*
