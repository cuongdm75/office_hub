# Office Hub — Session Handoff
**Date**: 2026-04-27 20:50 ICT  
**Task**: Migrate Mobile communication from WebSocket → MCP-Hybrid (SSE + REST)  
**Status**: ~85% complete — `cargo check` PASS, 3 bugs remain unfixed

---

## 1. Tổng quan kiến trúc mới

```
Mobile App (Expo)          Backend (Tauri/Axum)          Desktop/Add-in
─────────────────          ──────────────────────          ──────────────
EventSource ─── SSE ────▶  :9002 /api/v1/stream           WebSocket :9001
fetch POST  ─────────────▶  :9002 /api/v1/command       (giữ nguyên cho
                            :9002 /api/v1/tool_call       Office Add-in)
                            :9002 /api/v1/files/upload
                            :9002 /api/v1/artifacts
```

**Quy tắc cổng**:
- `9001` = WebSocket server (cho Desktop/Office Add-in) — **KHÔNG thay đổi**
- `9002` = SSE+REST server (cho Mobile) — **MỚI**

---

## 2. Files đã hoàn thành

### Backend (Rust)
| File | Trạng thái | Mô tả |
|------|-----------|-------|
| `src-tauri/src/mcp_transport.rs` | ✅ Done | Structs: `SseEvent`, `SseEventType`, `McpToolCall`, `McpResource`, `MobileCommand`, `IncomingMobileCmd` |
| `src-tauri/src/sse_server.rs` | ✅ Done | Axum server port 9002, SSE handler (với `?token=` query param), REST endpoints, file streaming, CORS |
| `src-tauri/src/lib.rs` | ✅ Done | `mobile_cmd_rx` worker loop xử lý commands từ mobile; intercept 3 system commands; bridge Orchestrator → `sse_tx` |

### Mobile (TypeScript)
| File | Trạng thái | Mô tả |
|------|-----------|-------|
| `mobile/src/store/sseStore.ts` | ✅ Done | Drop-in thay `wsStore.ts`; EventSource downlink; fetch POST uplink; auth; reconnect |
| `mobile/src/screens/ChatScreen.tsx` | ✅ Done | Dùng `useSseStore`; upload → `/api/v1/files/upload` |
| `mobile/src/screens/ArtifactsScreen.tsx` | ✅ Done | `/api/v1/artifacts`, Bearer token, download với auth |
| `mobile/src/screens/SettingsScreen.tsx` | ✅ Done | Hiển thị `currentBaseUrl`, connection status banner |
| `mobile/src/screens/HomeScreen.tsx` | ✅ Done | `activeHitlRequest` từ sseStore |
| `mobile/src/screens/ProgressScreen.tsx` | ✅ Done | `activeTasks` từ sseStore |
| `mobile/src/screens/MainTabs.tsx` | ✅ Done | `connect(baseUrls, token)` |
| `mobile/src/components/HitlApprovalSheet.tsx` | ✅ Done | `sendHitlResponse()` qua REST |
| `mobile/src/screens/ConnectionScreen.tsx` | ⚠️ Bug #1 | Xem bên dưới |

---

## 3. Bugs còn lại — Cần fix ngay phiên tới

### 🔴 Bug #1 (CRITICAL — Fix đầu tiên)
**File**: `mobile/src/screens/ConnectionScreen.tsx`  
**Triệu chứng**: Mobile connect vào port 9001 thay vì 9002

**Log thực tế**:
```
LOG  [SSE] Connecting to http://100.102.110.24:9001/api/v1/stream  ← SAI PORT
LOG  [SSE] Backoff retry 3/10 in 8s
```

**Nguyên nhân**: `normaliseUrl()` convert `ws://` → `http://` đúng, nhưng **không đổi port**.  
SecureStore lưu `ws://IP:9001` → convert thành `http://IP:9001` (vẫn 9001).

**Fix cần làm**:
```ts
// Trong normaliseUrl(), sau khi strip ws://:
const normaliseUrl = (raw: string): string => {
  let url = raw.trim();
  if (url.startsWith('ws://')) url = url.replace('ws://', 'http://');
  if (url.startsWith('wss://')) url = url.replace('wss://', 'https://');
  if (!url.startsWith('http://') && !url.startsWith('https://')) {
    url = `http://${url}`;
  }

  // *** FIX: Nếu port là 9001 (WebSocket cũ), đổi thành 9002 (SSE mới)
  url = url.replace(/:9001(\/|$)/, ':9002$1');

  // Add default port 9002 nếu chưa có port nào
  const hostPart = url.replace(/https?:\/\//, '').split('/')[0];
  if (!/:(\d+)$/.test(hostPart)) url = url.replace(hostPart, `${hostPart}:9002`);

  return url;
};
```

**Và thêm migration logic** (xóa cache cũ):
```ts
// Trong useEffect loadAndConnect, thêm sau khi đọc savedUrlsStr:
// Nếu URL cũ chứa :9001, clear cache để user nhập lại URL đúng
// hoặc auto-migrate port
```

---

### 🟡 Bug #2 (MEDIUM — Fix sau Bug #1)
**File**: `src-tauri/src/system/mod.rs` → `qrcode::generate_pairing_qr()`  
**Vấn đề**: QR code tạo ra URL `ws://IP:9001` (WebSocket cũ)

```rust
// HIỆN TẠI (SAI):
let url = format!("ws://{}:{}", ip, net.ws_port);  // ws://192.168.1.10:9001

// CẦN SỬA:
let url = format!("http://{}:{}", ip, net.ws_port + 1);  // http://192.168.1.10:9002
```

Thay đổi ở 3 chỗ trong hàm `generate_pairing_qr()`:
- Line 684: LAN IPs
- Line 692: Tailscale IP
- Line 700: Tailscale hostname

Và cập nhật struct `PairingInfo`:
```rust
pub struct PairingInfo {
  pub ws_url: String,        // → đổi tên thành pub url: String
  pub all_ws_urls: Vec<String>,  // → đổi thành pub urls: Vec<String>
  ...
}
```

---

### 🟢 Bug #3 (LOW — Cleanup)
**File**: `mobile/src/store/wsStore.ts`  
**Vấn đề**: File cũ còn tồn tại, không được import nhưng gây nhầm lẫn  
**Fix**: Xóa file này sau khi xác nhận Bug #1 và #2 đã fix xong và test stable

---

## 4. Kiến trúc SSE Event Dispatcher (mobile)

```
Backend emit SseEvent { event_type, call_id, payload }
     ↓
SSE stream → EventSource.addEventListener(event_type, ...)
     ↓
dispatchSseEvent() trong sseStore.ts
     ↓
switch(event_type):
  "result"           → messages[] (chat bubble)
  "progress"         → llmThought (typing indicator)
  "status"           → activeTasks{} (progress screen)
  "approval_request" → activeHitlRequest (HITL sheet)
  "error"            → messages[] (error bubble)
  "session_list"     → sessions[]
  "session_history"  → messages[] (history load)
  "log"              → (bỏ qua, debug only)
```

---

## 5. System Commands từ Mobile → Backend

Mobile gửi special text qua `POST /api/v1/command`, backend intercept trước Orchestrator:

| Text gửi | Backend xử lý | SSE response |
|----------|--------------|-------------|
| `__LIST_SESSIONS__` | `orchestrator.list_sessions()` | `session_list` event |
| `__GET_SESSION_HISTORY__` | `orchestrator.get_session_store()` | `session_history` event |
| `__DELETE_SESSION__` | `orchestrator.delete_session()` | (silent) |

---

## 6. Auth Flow

```
Mobile ConnectionScreen
  → nhập http://IP:9002 + token
  → POST /api/v1/auth  { token }  →  { ok: true }
  → EventSource /api/v1/stream?token=xxx  (EventSource không hỗ trợ headers)
  → Backend: check_auth_full(headers, query_token)  ← Bearer header OR ?token= param
  → onopen: isConnected = true → navigate Home
```

**Lưu ý quan trọng**: `EventSource` không hỗ trợ custom headers nên token được truyền qua query param. Backend đã xử lý cả 2 cách:
```rust
fn check_auth_full(&self, headers: &HeaderMap, query_token: Option<&str>) -> bool
```

---

## 7. File Upload Flow

```
Mobile ChatScreen chọn file
  → uploadFileToServer(baseUrl, token, { uri, name, type })
  → POST /api/v1/files/upload (multipart/form-data, Bearer token)
  → Backend: lưu file vào public_dir, trả về { resource: McpResource, file_path }
  → sendCommand(text, { name, file_path })
  → Backend: IncomingMobileCmd { context_file_path: Some(path) }
  → Orchestrator xử lý với context file
```

---

## 8. State hiện tại để test

**Để test kết nối**:
```bash
# 1. Build và chạy Tauri app (backend)
cd "e:\Office hub"
cargo tauri dev

# 2. Test SSE endpoint thủ công
curl -N -H "Authorization: Bearer <token>" http://192.168.x.x:9002/api/v1/stream

# 3. Gửi command test
curl -X POST \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"command_id":"test1","text":"xin chào","session_id":null}' \
  http://192.168.x.x:9002/api/v1/command

# 4. Mobile app
cd "e:\Office hub\mobile"
npx expo start --clear
```

**Lưu ý**: Sau khi fix Bug #1, user cần **xóa app khỏi thiết bị** (hoặc clear SecureStore) để xóa URL cũ `ws://IP:9001`.

---

## 9. Cấu trúc thư mục quan trọng

```
e:\Office hub\
├── src-tauri\src\
│   ├── lib.rs              ← Entry point, spawn SSE server, mobile_cmd_rx loop
│   ├── mcp_transport.rs    ← MCP structs (SseEvent, SseEventType, ...)
│   ├── sse_server.rs       ← Axum server :9002 (SSE+REST)
│   ├── websocket\mod.rs    ← WS server :9001 (giữ nguyên cho Add-in)
│   └── system\mod.rs       ← QR pairing (CẦN FIX Bug #2)
│
└── mobile\src\
    ├── store\
    │   ├── sseStore.ts     ← Store chính (SSE+REST) ✅
    │   └── wsStore.ts      ← Legacy, cần xóa sau
    └── screens\
        ├── ConnectionScreen.tsx  ← CẦN FIX Bug #1 (normaliseUrl port)
        ├── ChatScreen.tsx        ✅
        ├── ArtifactsScreen.tsx   ✅
        ├── SettingsScreen.tsx    ✅
        ├── HomeScreen.tsx        ✅
        ├── ProgressScreen.tsx    ✅
        └── MainTabs.tsx          ✅
```

---

## 10. Next Steps (theo thứ tự)

1. **Fix Bug #1** — `normaliseUrl()` thêm `:9001` → `:9002` migration
2. **Test kết nối mobile** — clear SecureStore hoặc gõ lại URL với `:9002`
3. **Fix Bug #2** — QR code generate `http://IP:9002` thay vì `ws://IP:9001`  
4. **End-to-end test** — Chat → SSE → hiện tin nhắn; File upload; HITL
5. **Fix Bug #3** — Xóa `wsStore.ts`
6. **Optional**: Thêm dedicated REST endpoints cho session management thay vì gửi qua command text
