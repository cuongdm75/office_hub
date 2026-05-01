# Office Hub – Master Plan Amendment v1.1

**Document version:** 1.1  
**Amends:** MASTER_PLAN.md v1.0  
**Status:** 🔍 DRAFT – For Review  
**Date:** 2025

---

## Nội dung bổ sung (Changes from v1.0)

| # | Thay đổi | Loại |
|---|---------|------|
| A1 | Mobile App – Remote UI đầy đủ (không chỉ HITL) | **Major** |
| A2 | Kết nối LAN + Tailscale + QR Code pairing | **Major** |
| A3 | Folder Scanner Agent | **New Agent** |
| A4 | Outlook Agent (Email + Calendar) | **New Agent** |
| A5 | System Tray + khởi động cùng Windows | **New Feature** |
| A6 | Settings UI mở rộng | **Enhancement** |
| A7 | Sleep Override + Lock-screen awareness | **New Feature** |
| A8 | Intent taxonomy cập nhật | **Enhancement** |

---

## A1. Mobile App – Remote UI (Redesign)

### A1.1 Tầm nhìn (Revised)

Mobile App **không phải chỉ là màn hình phê duyệt** (HITL). Đây là một **Remote UI đầy đủ** để người dùng tương tác với Office Hub từ bất kỳ đâu — như đang ngồi trực tiếp trước máy tính.

```
┌─────────────────────────────────────────────────────┐
│              Mobile App (iOS / Android)              │
│                                                     │
│  ┌─────────────────────────────────────────────┐   │
│  │           CHAT INTERFACE                     │   │
│  │  Giống hệt Chat Pane trên Desktop            │   │
│  │  • Gõ hoặc nói lệnh bằng giọng nói          │   │
│  │  • Nhận phản hồi từ Orchestrator             │   │
│  │  • Xem kết quả agent (text, bảng, link file) │   │
│  └─────────────────────────────────────────────┘   │
│                                                     │
│  ┌──────────────────────┐ ┌──────────────────────┐  │
│  │   TASK PROGRESS       │ │   HITL APPROVALS     │  │
│  │ • Workflow đang chạy  │ │ • Pending actions    │  │
│  │ • % hoàn thành        │ │ • Approve / Reject   │  │
│  │ • Agent đang dùng     │ │ • Xem chi tiết       │  │
│  │ • Log realtime        │ │ • Risk level badge   │  │
│  └──────────────────────┘ └──────────────────────┘  │
│                                                     │
│  ┌─────────────────────────────────────────────┐   │
│  │           AGENT STATUS PANEL                 │   │
│  │  Analyst [idle] | OfficeMaster [busy] | ...  │   │
│  └─────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

### A1.2 Core Mobile Features

| Tab | Chức năng |
|-----|-----------|
| 💬 **Chat** | Gửi tin nhắn text hoặc giọng nói đến Orchestrator; nhận phản hồi markdown; xem file output |
| ⏳ **Progress** | Theo dõi realtime tiến trình từng workflow và từng step; có thể huỷ task đang chạy |
| ✅ **Approvals** | Danh sách HITL requests đang chờ; Approve / Reject với 1 chạm; xem risk level |
| 📊 **Status** | Trạng thái tất cả agents; số workflow đã hoàn thành; token usage |
| ⚙ **Settings** | Cài đặt kết nối; thông báo; ngôn ngữ |

### A1.3 Technical Stack (Mobile)

**Lựa chọn ưu tiên: React Native (Expo)**

| Thành phần | Lựa chọn | Lý do |
|-----------|---------|-------|
| Framework | React Native + Expo | Code sharing với Web frontend; hot reload; OTA update |
| State | Zustand | Đồng nhất với Desktop |
| Networking | `@react-native-community/netinfo` + custom WebSocket hook | |
| Notifications | `expo-notifications` | Push notification cho HITL |
| Voice input | `expo-speech` + `@react-native-voice/voice` | |
| QR scanner | `expo-camera` + `expo-barcode-scanner` | |
| Markdown | `react-native-markdown-display` | |

**Alternative: Flutter** (nếu cần performance cao hơn)

### A1.4 WebSocket Protocol Extensions (Mobile-specific)

**Client → Server (thêm mới):**

```json
// Cập nhật session list (sidebar)
{ "type": "list_sessions" }

// Lấy lịch sử chat của session
{ "type": "get_session_history", "session_id": "..." }

// Hủy workflow đang chạy
{ "type": "cancel_workflow", "run_id": "..." }

// Voice command (audio transcribed on mobile)
{ "type": "voice_command", "text": "...", "language": "vi", "session_id": "..." }

// Subscription (mobile muốn nhận events của workflow cụ thể)
{ "type": "subscribe_workflow", "run_id": "..." }
{ "type": "unsubscribe_workflow", "run_id": "..." }

// Yêu cầu danh sách approvals đang chờ
{ "type": "list_pending_approvals" }
```

**Server → Client (thêm mới):**

```json
// Realtime workflow step progress
{
  "type": "workflow_step_progress",
  "run_id": "...",
  "workflow_name": "Email → Báo cáo",
  "current_step": "Tạo báo cáo Word",
  "step_index": 4,
  "total_steps": 8,
  "percent": 50.0,
  "agent": "office_master",
  "elapsed_seconds": 12
}

// Folder scan progress (FolderScannerAgent events)
{
  "type": "scan_progress",
  "scan_id": "...",
  "event": "file_processing",
  "file_name": "BaoCao_Q1.xlsx",
  "file_index": 5,
  "total_files": 20,
  "percent": 25.0,
  "stage": "summarizing"
}

// Session list update
{
  "type": "session_list",
  "sessions": [
    { "id": "...", "title": "Phân tích báo cáo", "lastActive": "..." },
    ...
  ]
}

// Agent status update
{
  "type": "agent_status_update",
  "agents": [
    { "id": "analyst", "name": "Analyst Agent", "status": "busy", "currentTask": "Đọc file" },
    { "id": "office_master", "name": "Office Master", "status": "idle" }
  ]
}

// Output file ready (link để mở/tải)
{
  "type": "output_ready",
  "session_id": "...",
  "files": [
    { "name": "BaoCao_Tuan.docx", "path": "C:\\...", "size_bytes": 45312, "type": "docx" }
  ]
}
```

### A1.5 Mobile Screens Specification

#### Screen 1: Connection (First Launch / QR Scan)

```
┌────────────────────────────┐
│       OFFICE HUB           │
│   Connect to your PC       │
│                            │
│  [📷 Scan QR Code]         │  ← mở camera để quét QR
│                            │
│  ─── or enter manually ─── │
│                            │
│  Host: [_______________]   │
│  Port: [9001_________]     │
│  Token:[_______________]   │
│                            │
│  [Connect]                 │
│                            │
│  💡 Tip: Scan the QR code  │
│  shown in Office Hub       │
│  Settings on your PC       │
└────────────────────────────┘
```

#### Screen 2: Chat (Main Screen)

```
┌────────────────────────────┐
│ 💬 Chat    ●●● Analyst busy│
│────────────────────────────│
│                            │
│  [User]: Tổng hợp folder   │
│  C:\Reports thành Word     │
│                            │
│  [AI]: 🔄 Đang quét folder │
│  • Phát hiện: 15 file      │
│  • Xử lý: 8/15 (53%)      │
│  [████████░░░░░░░] 53%     │
│                            │
│  [User]: Xong chưa?        │
│                            │
│  [AI]: ✅ Xong! File đã    │
│  tạo: BaoCao_Tong_Hop.docx │
│  [📄 Mở file] [📤 Chia sẻ]│
│                            │
│────────────────────────────│
│  [🎤][___Nhập tin nhắn___] │
└────────────────────────────┘
```

#### Screen 3: Progress Monitor

```
┌────────────────────────────┐
│ ⏳ Tiến trình              │
│────────────────────────────│
│ ▶ Email → Báo cáo  [● 4/8]│
│   Step: Tạo Word report    │
│   [████████████░░] 62%     │
│   Agent: OfficeMaster      │
│   ⏱ 45s                   │
│   [❌ Hủy]                 │
│────────────────────────────│
│ ✅ Folder Scan   [DONE]    │
│   15 files, 23s            │
│   📄 BaoCao_Tong_Hop.docx  │
│────────────────────────────│
│ ✅ Excel Analysis [DONE]   │
│   BaoCao_Q4.xlsx → 5 tab   │
└────────────────────────────┘
```

#### Screen 4: Approvals (HITL)

```
┌────────────────────────────┐
│ ✅ Phê duyệt    (2 pending)│
│────────────────────────────│
│ 🔴 CRITICAL                │
│ Gửi email đến:             │
│  • giamdoc@company.com     │
│ Tiêu đề: "Báo cáo tuần 3" │
│                            │
│ [👁 Xem nội dung]          │
│                            │
│ [✅ Duyệt & Gửi] [❌ Huỷ] │
│                            │
│ ⏰ Hết hạn sau: 4:32       │
│────────────────────────────│
│ 🟡 HIGH                    │
│ Mở trình duyệt:            │
│  petrolimex.com.vn         │
│                            │
│ [✅ Cho phép] [❌ Từ chối] │
└────────────────────────────┘
```

---

## A2. Kết nối: LAN + Tailscale + QR Code

### A2.1 Connection Architecture

```
SCENARIO 1: Same Network (LAN)
  Mobile App ←──── WiFi/LAN ────→ Office Hub PC
  ws://192.168.1.10:9001

SCENARIO 2: Remote (Tailscale)
  Mobile App ←── Tailscale VPN ──→ Office Hub PC
  ws://my-pc.tail1234.ts.net:9001
  OR ws://100.64.1.5:9001

SCENARIO 3: Remote (No Tailscale)
  → Không hỗ trợ trực tiếp (cần VPN hoặc port-forward)
  → Hiển thị hướng dẫn cài Tailscale
```

### A2.2 QR Code Pairing Flow

```
[Office Hub PC – Settings → Mobile Pairing]
    │
    ▼ 1. Detect network IPs
    SystemManager::refresh_network()
        ├── get_lan_ips()         → ["192.168.1.10", "10.0.0.5"]
        └── tailscale::probe()   → { ip: "100.64.1.5", hostname: "my-pc.ts.net" }
    │
    ▼ 2. Generate auth token (if not already set)
    Uuid::new_v4().to_string() → e.g. "a3f8-bc12-..."
    │
    ▼ 3. Build QR payload JSON
    {
      "type":    "office-hub-pairing",
      "version": "1",
      "urls": [
        "ws://my-pc.tail1234.ts.net:9001",  ← Tailscale first (remote-friendly)
        "ws://100.64.1.5:9001",             ← Tailscale IP
        "ws://192.168.1.10:9001"            ← LAN IP
      ],
      "token":   "a3f8-bc12-...",
      "expires": "2025-01-20T10:05:00Z"     ← 5 phút
    }
    │
    ▼ 4. Render as QR SVG (qrcode crate)
    Display in Settings UI → Mobile scans
    │
    ▼ 5. Mobile App receives QR
    Parse JSON → try URLs in order → connect to first responsive server
    → Store connection config in secure storage
    → Mark as "Paired" in both Desktop and Mobile UI
```

### A2.3 Tailscale Configuration UI

**Settings → Connectivity → Tailscale:**

| Setting | Description |
|---------|-------------|
| Enable Tailscale | Toggle: bật/tắt ưu tiên Tailscale address |
| Tailscale Status | Hiển thị: connected/disconnected/not installed |
| Tailscale IP | Hiển thị IP hiện tại (readonly) |
| Tailscale Hostname | Hiển thị DNS name (readonly) |
| Install Tailscale | Button → mở tailscale.com/download |
| Test Connection | Ping test qua cả LAN và Tailscale |

**Trạng thái Tailscale (hiển thị trong UI):**

```
🟢 Tailscale Connected
   IP: 100.64.1.5
   Host: my-pc.tail1234.ts.net
   [Copy Address] [Open Tailscale]

🟡 Tailscale Installed (Not Connected)
   [Connect Tailscale]

🔴 Tailscale Not Installed
   Remote access requires Tailscale.
   [Install Tailscale] (opens tailscale.com)
```

### A2.4 Security: Mobile Authentication

```
Connection request: ws://host:9001?token=<auth_token>

Token lifecycle:
1. Generated once on first QR scan pairing
2. Stored in Windows Credential Manager (PC side)
3. Stored in iOS Keychain / Android Keystore (Mobile side)
4. Rotated when user clicks "Revoke Mobile Access" in Settings
5. QR codes expire in 5 minutes (new QR needed for new devices)
```

**Multiple devices:** Mỗi thiết bị có thể kết nối (tối đa `max_clients = 5`). Tất cả devices dùng cùng một auth token (simplicity over security – enterprise version có thể per-device tokens).

---

## A3. Folder Scanner Agent

### A3.1 Tổng quan

Folder Scanner Agent cho phép người dùng chỉ một folder, tự động:
1. Quét và phát hiện các file được hỗ trợ
2. Đọc nội dung từng file (tùy theo loại)
3. Tóm tắt từng file qua LLM
4. Tổng hợp toàn bộ folder thành output theo yêu cầu

### A3.2 Intent Coverage (thêm vào intent.rs)

```rust
// Thêm vào Intent enum:
FolderScan(FolderScanPayload),
FileSummarize(FileSummarizePayload),
FolderSearch(FolderSearchPayload),

// Routing:
// FolderScan → FolderScannerAgent
// Sensitivity: MEDIUM (creates new output files)
```

**Natural language triggers:**
- *"Tổng hợp tất cả file trong folder Báo cáo thành 1 file Word"*
- *"Đọc và tóm tắt toàn bộ file trong C:\Projects\Q4"*
- *"Tạo slide từ các file Word trong Desktop\Presentations"*
- *"Lấy số liệu từ tất cả Excel trong folder và tổng hợp vào 1 bảng"*

### A3.3 Supported Input/Output Matrix

| Input | Word Report | PPT Slides | Excel Summary |
|-------|-------------|------------|---------------|
| .docx / .doc | ✅ Trích xuất text, tóm tắt | ✅ Mỗi doc = 1-2 slides | ✅ Metadata |
| .xlsx / .xls | ✅ Tóm tắt số liệu | ✅ Chart summary slide | ✅ Merge dữ liệu |
| .pptx / .ppt | ✅ Tóm tắt nội dung slide | ✅ Merge slides | ✅ Metadata |
| .pdf | ✅ Trích xuất text | ✅ Summary slide | ✅ Metadata |
| .txt / .md | ✅ Full content | ✅ Text slides | ❌ N/A |
| .csv | ✅ Tóm tắt data | ✅ Data table slide | ✅ Merge tables |
| .json / .yaml | ✅ Schema + sample | ✅ Structure slide | ✅ Key metrics |
| .eml | ✅ Tóm tắt email | ✅ Email summary slide | ✅ Sender stats |

### A3.4 Progress Broadcasting

Scan progress được broadcast qua hai kênh song song:

```
FolderScannerAgent
    │
    ├── mpsc::Sender<ScanProgressEvent>
    │       │
    │       ├── Tauri Event Bus → Desktop UI (progress bar in Chat Pane)
    │       │         app.emit("scan_progress", event_json)
    │       │
    │       └── WebSocket Server → Mobile App (progress screen)
    │                 ws_server.broadcast(ServerMessage::ScanProgress { ... })
    │
    └── tokio::task::spawn_blocking (for each file read)
            → non-blocking file I/O
```

### A3.5 Output Naming Convention

```
{output_dir}/{folder_name}_{timestamp}_TongHop.docx
{output_dir}/{folder_name}_{timestamp}_TongHop.pptx
{output_dir}/{folder_name}_{timestamp}_TongHop.xlsx

Example:
  Input folder:  C:\Reports\Q4_2024
  Output Word:   C:\Reports\Q4_2024\Q4_2024_20250120_093045_TongHop.docx
```

### A3.6 Word Report Structure (scan_folder_to_word)

```
TRANG BÌA
  • Tiêu đề: "BÁO CÁO TỔNG HỢP – {folder_name}"
  • Ngày tạo, người tạo: "Office Hub AI"
  • Số file đã xử lý

MỤC LỤC (nếu include_toc = true)

TỔNG QUAN
  • Executive summary của toàn bộ folder (LLM-generated)
  • Thống kê: số file, phân loại theo loại, tổng kích thước
  • Các chủ đề / theme chính

NỘI DUNG TỪNG FILE
  Mỗi file = 1 section:
  ─────────────────────────
  [Số thứ tự]. {Tên file} ({Loại}, {Kích thước})
  Ngày sửa đổi: dd/MM/yyyy
  
  Tóm tắt:
  {LLM-generated summary}
  
  Số liệu chính: (nếu là Excel/CSV)
  {Extracted metrics table}
  ─────────────────────────

PHỤ LỤC (nếu include_thumbnails = true)
  • Ảnh chụp màn hình / thumbnail của mỗi file
```

### A3.7 PPT Structure (scan_folder_to_ppt)

```
Slide 1: Title slide
  "TỔNG HỢP: {folder_name}"
  "{count} files | {date}"

Slide 2: Executive Summary
  • Bullet points từ LLM folder summary

Slide 3: File breakdown (chart + table)
  Pie chart: phân loại theo loại file
  Bảng: tên file, loại, kích thước, ngày sửa

Slide 4..N: Mỗi file quan trọng = 1 slide
  • Tiêu đề = tên file
  • Bullet points từ file summary
  • Số liệu chính (nếu có)

Slide cuối: Metadata
  • Generated by Office Hub AI
  • Timestamp
```

---

## A4. Outlook Agent

### A4.1 Tổng quan

Outlook Agent tích hợp với Microsoft Outlook qua COM Automation để quản lý toàn bộ email và calendar workflows.

### A4.2 Intent Coverage (thêm vào intent.rs)

```rust
// Thêm vào Intent enum:
OutlookRead(OutlookReadPayload),
OutlookCompose(OutlookComposePayload),
OutlookSend(OutlookSendPayload),      // sensitivity: HIGH
OutlookCalendar(OutlookCalendarPayload),
OutlookSearch(OutlookSearchPayload),
OutlookSummarize(OutlookSummarizePayload),
```

**Natural language triggers:**
- *"Đọc 10 email chưa đọc trong Inbox"*
- *"Tóm tắt chuỗi email với sếp tuần này"*
- *"Tạo lịch họp ngày mai lúc 9h với team marketing"*
- *"Trả lời email của anh Minh, nói rằng tôi sẽ gửi báo cáo vào thứ 6"*
- *"Tìm email từ client có đính kèm Excel"*
- *"Tạo task từ email yêu cầu của khách hàng"*

### A4.3 HITL Requirement Matrix

| Action | HITL Level | Có thể hủy? |
|--------|-----------|-------------|
| Đọc email | None | N/A |
| Tìm kiếm email | None | N/A |
| Tóm tắt thread | None | N/A |
| Soạn nháp (chưa gửi) | None | Yes |
| Lưu Draft | None | Yes |
| Gửi email | **HIGH** | Yes |
| Trả lời / Forward | **HIGH** | Yes |
| Đánh dấu đọc/chưa đọc | LOW | Yes |
| Chuyển folder | LOW | Yes |
| Xóa email | **CRITICAL** | No (irreversible) |
| Tạo cuộc hẹn | MEDIUM | Yes |
| Tạo meeting (gửi invite) | **HIGH** | Yes |
| Huỷ cuộc hẹn | **HIGH** | Yes |
| Accept/Decline meeting | MEDIUM | Yes |

### A4.4 Email Reply Workflow với HITL

```
User: "Trả lời email của anh Tuấn, xác nhận sẽ giao hàng thứ 6"
    │
    ▼ 1. Outlook Agent đọc email của anh Tuấn (last email in thread)
    │
    ▼ 2. LLM soạn nháp reply:
    │      "Kính gửi anh Tuấn,
    │       Tôi xác nhận sẽ giao hàng vào thứ 6, ngày XX/XX/2025.
    │       Trân trọng,
    │       [Tên người dùng]"
    │       
    │       [AI Disclaimer: Email này được tạo bởi Office Hub AI]
    │
    ▼ 3. Orchestrator gọi HITL Manager → đăng ký approval request
    │
    ▼ 4. Desktop notification + Mobile push:
    │      Title: "📧 Xem xét email trả lời"
    │      Body:  "Gửi đến: tuan@company.com | Chủ đề: Re: Đơn hàng XYZ"
    │      Actions: [👁 Xem nháp] [✅ Gửi] [✏ Chỉnh sửa] [❌ Huỷ]
    │
    ▼ 5a. User click "✏ Chỉnh sửa"
    │       → Mở Outlook draft window để user chỉnh sửa trực tiếp
    │       → User gửi thủ công từ Outlook
    │
    ▼ 5b. User click "✅ Gửi"
    │       → Outlook Agent gọi MailItem.Send() qua COM
    │       → Audit log: { action: send_email, approved_by: user, timestamp }
    │
    ▼ 5c. User click "❌ Huỷ"
           → Draft bị xóa, không gửi gì
           → Thông báo: "Email đã bị hủy"
```

### A4.5 Calendar Integration

```
Outlook Calendar → Office Hub Workflow Integration:

TRIGGER: Sáng sớm hàng ngày (07:00)
  → Outlook Agent đọc lịch hôm nay
  → Gửi summary ra Mobile:
    "📅 Hôm nay bạn có 3 cuộc họp:
     09:00 - Họp team (30 phút)
     14:00 - Review Q4 (1 giờ)
     16:30 - 1-on-1 với sếp (30 phút)"

TRIGGER: 15 phút trước cuộc họp
  → Desktop notification + Mobile push
  → "⏰ Họp team sau 15 phút"
  → [Xem chi tiết] [Tham gia Teams]

INTEGRATION với FolderScanner:
  User: "Chuẩn bị tài liệu cho cuộc họp 14h"
  → Outlook Agent đọc meeting details + attendee list
  → FolderScanner quét folder liên quan
  → OfficeMaster tạo slide briefing từ tài liệu
```

---

## A5. System Tray + Windows Startup

### A5.1 System Tray Design

```
Notification Area (phía bên phải taskbar, khu đồng hồ)

Normal state:   [🏢]  Office Hub icon (32×32 PNG)
Busy state:     [🏢⟳] With spinning indicator overlay
Notification:   [🏢🔔] With badge overlay (HITL pending)
Error state:    [🏢⚠] With warning overlay

Left-click:     Show/Focus main window (toggle)
Right-click:    Context menu (see below)
```

**Tray Context Menu:**

```
✅  Open Office Hub
──────────────────────
📊  Agent Status
     ├── 🟢 Analyst         [idle]
     ├── 🟢 Office Master   [idle]
     ├── 🟡 Web Researcher  [phase 4]
     ├── 🟢 Folder Scanner  [idle]
     ├── 🟢 Outlook Agent   [idle]
     └── 🟢 Converter       [phase 7]
──────────────────────
▶   Workflows running: 2
     ├── Email → Báo cáo  [62% ████░░]
     └── Folder Scan Q4   [100% ✅]
──────────────────────
✅  Pending Approvals: 1
──────────────────────
⚙   Settings
📱  Mobile Pairing QR
──────────────────────
❌  Quit Office Hub
```

**Tooltip (hover trên icon):**

```
Office Hub
2 workflows running | 1 approval pending
Analyst: busy | LLM: Gemini Pro ✓
```

### A5.2 Window Behavior

```
Close button behavior (configurable in Settings):

Option A: "Minimise to tray" (default)
  → Window hides, tray icon remains
  → App continues running in background
  → All agents continue working

Option B: "Exit"
  → Full shutdown (if no active tasks)
  → OR confirm dialog: "Tasks are running. Force quit?"

Startup behavior:
  If startup_with_windows = true:
    → App starts minimised to tray (no window shown)
    → Notification: "Office Hub started"
  If startup_with_windows = false:
    → Normal window launch
```

### A5.3 Windows Registry Startup

```
Registry key: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
Value name:   "OfficeHub"
Value data:   "C:\Program Files\OfficeHub\office-hub.exe" --minimized

Implementation (Phase 1):
  use winreg::RegKey;
  use winreg::enums::HKEY_CURRENT_USER;

  // Register:
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  let run_key = hkcu.open_subkey_with_flags(
      "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
      KEY_SET_VALUE
  )?;
  run_key.set_value("OfficeHub", &exe_path)?;

  // Unregister:
  run_key.delete_value("OfficeHub")?;

  // Check:
  run_key.get_value::<String, _>("OfficeHub").is_ok()
```

### A5.4 Lock-screen Awareness

```
Windows Session Events (registered via WM_WTSSESSION_CHANGE):

WTS_SESSION_LOCK:
  → Emit event: SystemEvent::ScreenLocked
  → If agents_active_on_lockscreen = true:
      → Continue all active workflows
      → Keep COM sessions alive (Excel/Outlook/Word stay open)
      → Keep WebSocket server running (mobile can still connect)
      → Log: "Screen locked – agents continuing in background"
  → If agents_active_on_lockscreen = false:
      → Pause non-critical workflows (save state)
      → Keep critical workflows running (with user approval)

WTS_SESSION_UNLOCK:
  → Emit event: SystemEvent::ScreenUnlocked
  → Resume paused workflows
  → Show desktop notification: "Office Hub: {N} tasks completed"
  → If HITL approvals pending: show notification immediately

PBT_APMSUSPEND (Sleep):
  → Attempt to gracefully pause all workflows
  → Save state to disk (sessions_dir)
  → Log active tasks

PBT_APMRESUMEAUTOMATIC (Wake):
  → Restore state from disk
  → Resume paused workflows
  → Reconnect COM objects
  → Re-check Tailscale connectivity
```

---

## A6. Settings UI – Complete Specification

### A6.1 Settings Page Structure

```
Settings (Tab layout)
│
├── 🤖 LLM Provider
│   ├── Provider selector (Gemini / OpenAI / Ollama / LM Studio)
│   ├── API Key input (masked, Windows Credential Manager)
│   ├── Endpoint URL (for local providers)
│   ├── Model selector (dropdown or text)
│   ├── Max tokens slider
│   ├── Temperature slider
│   ├── Hybrid mode toggle
│   └── [Test Connection] button → shows latency + model info
│
├── 📁 Output & Files
│   ├── Default output folder (path picker)
│   ├── Backup folder (path picker)
│   ├── Grounding screenshots folder (path picker)
│   ├── Attachment download folder (path picker)
│   └── [Open Output Folder] shortcut
│
├── 🚀 Startup & System
│   ├── ☐ Start with Windows
│   ├── ☐ Minimise to tray on close (vs. Exit)
│   ├── ☐ Keep agents active on lock-screen
│   ├── ☐ Override Windows sleep timer during active tasks
│   └── System Status:
│       ├── CPU: 12% | RAM: 180 MB
│       ├── Active tasks: 2
│       └── Uptime: 2h 34m
│
├── 📱 Mobile Connection
│   ├── WebSocket Port: [9001]
│   ├── Auth Token: [**** show/hide/regenerate]
│   ├── Max connected devices: [5]
│   ├── ─── Tailscale ───────────────────
│   ├── ☐ Enable Tailscale for remote access
│   ├── Status: 🟢 Connected / 🔴 Not installed
│   ├── IP: 100.64.1.5
│   ├── Host: my-pc.tail1234.ts.net
│   ├── [Install Tailscale] / [Open Tailscale]
│   ├── ─── QR Code ────────────────────
│   ├── [Generate & Show QR Code]
│   │     → Opens modal with:
│   │       • Large QR code (200×200)
│   │       • Expiry countdown: "Expires in 4:32"
│   │       • Text: "Scan with Office Hub Mobile App"
│   │       • [Refresh QR] button
│   │       • Connection URLs listed below QR
│   └── Connected devices: [list]
│
├── 🤖 Agents
│   ├── Analyst Agent (Excel)
│   │   ├── ☐ Allow VBA execution (with HITL)
│   │   ├── Max rows per operation: [100,000]
│   │   └── Hard-truth tolerance: [0.01]%
│   │
│   ├── Office Master (Word / PPT)
│   │   ├── Default Word template: [Browse...]
│   │   ├── Default PPT template: [Browse...]
│   │   └── ☐ Preserve format on write
│   │
│   ├── Folder Scanner
│   │   ├── Max files per scan: [200]
│   │   ├── Max file size: [50] MB
│   │   ├── Max parallel readers: [4]
│   │   └── Detail level: [Standard ▼]
│   │
│   ├── Outlook Agent
│   │   ├── Primary account: [dropdown / auto-detect]
│   │   ├── Watched folders: [Inbox] [+ Add folder]
│   │   ├── ☐ Always CC me on AI-composed replies
│   │   ├── AI disclaimer text: [editable textarea]
│   │   └── ☐ Teams integration
│   │
│   └── Web Researcher (UIA) [⚠ Phase 4]
│       ├── Preferred browser: [Edge / Chrome]
│       ├── ☐ Screenshot grounding
│       ├── Allowed domains: [editable list]
│       └── [Status: Phase 4 – Not yet available]
│
├── 🔒 Security & Rules
│   ├── Rules file: rules/default.yaml [Edit] [Reload]
│   ├── HITL timeout: [5] minutes
│   ├── Timeout action: [Reject ▼]
│   ├── ☐ Require double-confirm for CRITICAL actions
│   └── [View Audit Log]
│
└── ℹ About
    ├── Version: 0.1.0-alpha
    ├── Build: 2025-01-20
    ├── Tech: Rust 1.80 + Tauri 2 + React 18
    ├── [Check for Updates]
    ├── [Export Diagnostic Report]
    └── [View Logs]
```

### A6.2 LLM Provider Settings – Detailed UX

```
Provider: [Gemini ▼]
           ├── Gemini (Google) ← Cloud
           ├── OpenAI (GPT)   ← Cloud  
           ├── Ollama          ← Local
           └── LM Studio       ← Local

When "Gemini" selected:
  API Key: [••••••••••••••••••••] [👁 Show] [📋 Copy]
  Model:   [gemini-1.5-pro ▼]
           ├── gemini-1.5-pro   (best quality)
           ├── gemini-1.5-flash (faster, cheaper)
           └── gemini-2.0-flash (latest)
  [🔍 Test Connection]
    → ✅ Connected | Model: gemini-1.5-pro | Latency: 342ms

When "Ollama" selected:
  Endpoint: [http://localhost:11434]
  Model:    [llama3.1 ▼] (populated from /api/tags)
  [🔍 Test Connection]
    → ✅ Ollama running | Model: llama3.1:8b | 4.7 GB VRAM

Hybrid Mode toggle:
  ☑ Fallback to local if cloud unavailable
    └── Cloud fails → try Ollama at localhost:11434
```

### A6.3 Mobile Pairing QR – Modal Design

```
┌─────────────────────────────────────────────┐
│  📱 Connect Mobile App                       │
│─────────────────────────────────────────────│
│                                             │
│   ┌─────────────────────────────────────┐   │
│   │                                     │   │
│   │    ████ █ █ ████                   │   │
│   │    █  █   █ █  █   (QR Code)       │   │
│   │    ████ █ █ ████                   │   │
│   │                                     │   │
│   └─────────────────────────────────────┘   │
│                                             │
│   ⏰ Expires in: 04:23                      │
│                                             │
│   Connections available:                    │
│   • 🌐 ws://my-pc.tail1234.ts.net:9001      │
│     (Tailscale – works anywhere)            │
│   • 🏠 ws://192.168.1.10:9001               │
│     (LAN – same network only)               │
│                                             │
│   [🔄 Refresh QR]  [📋 Copy URL]            │
│─────────────────────────────────────────────│
│  How to connect:                            │
│  1. Install "Office Hub" from App Store     │
│  2. Tap "Scan QR Code" in the app           │
│  3. Point camera at this QR                 │
│  ✅ Connected devices: iPhone của Minh (1)  │
└─────────────────────────────────────────────┘
```

---

## A7. Sleep Override + Power Management

### A7.1 Sleep Override Logic

```
Task starts (workflow trigger / agent dispatch)
    │
    ▼ Check: suppress_sleep_during_tasks = true?
    │         Yes
    ▼ SystemManager::suppress_sleep()
      → SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED)
      → Returns non-zero → success
      → tray tooltip: "Office Hub – keeping PC awake during task"
    │
    ▼ Task runs...
    │
    ▼ All tasks complete
      → SystemManager::release_sleep()
      → SetThreadExecutionState(ES_CONTINUOUS)  // clear all flags
      → tray tooltip returns to normal
```

**Edge cases:**

| Scenario | Behavior |
|----------|----------|
| Task crashes | `release_sleep()` in `Drop` impl (RAII guard) |
| Multiple concurrent tasks | Reference counter; only release when ALL tasks done |
| User force-quits | Cleanup in `app.on_exit()` handler |
| Sleep override fails | Log warning; proceed anyway (task continues, PC may sleep) |

### A7.2 RAII Sleep Guard

```rust
// src-tauri/src/system/power.rs

pub struct SleepGuard {
    released: AtomicBool,
}

impl SleepGuard {
    pub fn new() -> Option<Self> {
        if suppress_sleep() {
            Some(Self { released: AtomicBool::new(false) })
        } else {
            None
        }
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        if !self.released.swap(true, Ordering::Relaxed) {
            release_sleep();
        }
    }
}

// Usage in Orchestrator:
pub async fn process_message(...) {
    let _sleep_guard = if config.suppress_sleep_during_tasks {
        SleepGuard::new()
    } else {
        None
    };
    // Guard is automatically released when this function returns
    ...
}
```

### A7.3 Lock-screen Agent Persistence

```
Registry key for session notification:
HKLM\SYSTEM\CurrentControlSet\Control\Terminal Server\Wds\rdpwd\StartupPrograms

Alternative (preferred): WTSRegisterSessionNotification Win32 API
  → Register main window handle
  → Receive WM_WTSSESSION_CHANGE messages
  → Handle: WTS_SESSION_LOCK, WTS_SESSION_UNLOCK

Tauri integration:
  app.run(|app_handle, event| {
      match event {
          RunEvent::WindowEvent { label, event: WindowEvent::Destroyed, .. } => {
              // Window was closed → tray mode only
          }
          _ => {}
      }
  });

  // TODO(phase-1): Register WM_WTSSESSION_CHANGE in Tauri window setup
  // using tauri's raw_window_handle to get HWND, then:
  // WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_ALL_SESSIONS)
```

---

## A8. Updated Intent Taxonomy

### A8.1 New Intents Added

```
Intent (additions to v1.0)
│
├── Folder Scanner Category (3 new intents)
│   ├── FolderScan          – quét + tổng hợp folder (MEDIUM)
│   ├── FileSummarize       – tóm tắt 1 file đơn lẻ (LOW)
│   └── FolderSearch        – tìm nội dung trong folder (LOW)
│
└── Outlook Category (6 new intents)
    ├── OutlookRead         – đọc/tìm email (LOW)
    ├── OutlookCompose      – soạn nháp (MEDIUM)
    ├── OutlookSend         – gửi email (HIGH) ← HITL mandatory
    ├── OutlookCalendar     – đọc/tạo lịch (MEDIUM)
    ├── OutlookSummarize    – tóm tắt thread/inbox (LOW)
    └── OutlookManage       – quản lý email (MEDIUM)
```

### A8.2 Updated Routing Table

| Intent | Primary Agent | HITL Level | Timeout |
|--------|--------------|-----------|---------|
| FolderScan | FolderScanner | None | 600s |
| FileSummarize | FolderScanner | None | 120s |
| FolderSearch | FolderScanner | None | 60s |
| OutlookRead | Outlook | None | 30s |
| OutlookCompose | Outlook | None | 60s |
| OutlookSend | Outlook | **HIGH** | 300s |
| OutlookCalendar (read) | Outlook | None | 30s |
| OutlookCalendar (create) | Outlook | MEDIUM | 60s |
| OutlookSummarize | Outlook | None | 120s |
| OutlookManage | Outlook | LOW-MEDIUM | 30s |

### A8.3 Updated FastClassifier Patterns

```rust
// Thêm vào FastClassifierPatterns:

// Folder Scanner
folder_scan: Regex::new(
    r"(?i)(quét|scan|tổng hợp|tóm tắt|đọc tất cả|đọc toàn bộ)\s.*(folder|thư mục|file trong)"
),
file_summarize: Regex::new(
    r"(?i)(tóm tắt|đọc|phân tích)\s+file\s+[\w\./\\]+"
),
folder_search: Regex::new(
    r"(?i)(tìm kiếm|search|tìm nội dung)\s.*(trong folder|trong thư mục)"
),

// Outlook
outlook_read: Regex::new(
    r"(?i)(đọc|xem|lấy|show|get)\s.*(email|mail|inbox|hộp thư|thư)"
),
outlook_compose: Regex::new(
    r"(?i)(soạn|viết|tạo nháp|draft)\s.*(email|mail|thư trả lời|reply)"
),
outlook_send: Regex::new(
    r"(?i)(gửi|send)\s.*(email|mail|thư)"
),
outlook_calendar: Regex::new(
    r"(?i)(lịch|calendar|cuộc hẹn|họp|meeting|appointment|schedule)"
),
outlook_summarize: Regex::new(
    r"(?i)(tóm tắt|summary)\s.*(email|hộp thư|thread|chuỗi email)"
),
```

---

## A9. Updated System Architecture

### A9.1 Full Component Diagram (v1.1)

```
┌─────────────────────────────────────────────────────────────────┐
│                     APP SHELL (Tauri v2 + React)               │
│                                                                 │
│  FileBrowser | ChatPane | Workflows | Settings | Agent Status  │
│                                                                 │
│   System Tray Icon ← minimises here when window hidden         │
└──────────────────────────────┬──────────────────────────────────┘
                               │ Tauri IPC
┌──────────────────────────────▼──────────────────────────────────┐
│                        ORCHESTRATOR                             │
│   Intent (30+) → Router → Session → RuleEngine → HITL          │
└────┬────────┬────────┬──────────┬──────────┬────────┬──────────┘
     │        │        │          │          │        │
  Analyst  Office  Folder    Outlook   Web      Conv-
  (Excel)  Master  Scanner   (Email/   Resch.   erter
           (Word   (Files/   Calendar) (UIA)    (MCP)
           /PPT)   Folders)
     │        │        │          │          │        │
     └────────┴────────┴──────────┴──────────┴────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
         LLM Gateway     Workflow Engine   WebSocket Server
         (Gemini/GPT/    (YAML triggers,  (Mobile Remote UI
          Ollama)         steps, audit)    + Progress + HITL)
              │                                 │
              │                                 ▼
              │                      Mobile App (iOS/Android)
              │                      • Chat Interface
              │                      • Progress Monitor
              │                      • HITL Approvals
              │
┌─────────────▼──────────────────────────────────────────────────┐
│                    SYSTEM LAYER                                 │
│                                                                 │
│  SystemManager                                                  │
│  ├── Tray Icon (notification area)                             │
│  ├── Windows Startup (Registry Run key)                        │
│  ├── Sleep Override (SetThreadExecutionState)                  │
│  ├── Lock-screen Awareness (WTSRegisterSessionNotification)    │
│  ├── Tailscale Detection (tailscale CLI probe)                 │
│  ├── QR Code Generation (qrcode crate)                        │
│  └── Network Info (LAN IPs + Tailscale IP)                    │
│                                                                 │
│  NATIVE OS                                                      │
│  ├── COM: Excel.Application / Word.Application / PowerPoint    │
│  ├── COM: Outlook.Application (new)                            │
│  ├── UIA: UIAutomationCore.dll (Phase 4)                       │
│  ├── Win32: Power (SetThreadExecutionState)                     │
│  ├── Win32: Registry (winreg)                                   │
│  └── Win32: Session (WTS notifications)                         │
└─────────────────────────────────────────────────────────────────┘
```

### A9.2 New Cargo.toml Dependencies

```toml
# Thêm vào src-tauri/Cargo.toml

# QR Code generation
qrcode = "0.14"

# Windows Registry (startup registration)
winreg = "0.52"

# Local IP detection
local-ip-address = "0.6"

# Power management (thêm Windows features)
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    # ... existing features ...
    "Win32_System_Power",          # SetThreadExecutionState
    "Win32_System_Registry",       # Registry read/write
    "Win32_System_RemoteDesktop",  # WTSRegisterSessionNotification
    "Win32_Security",              # Security context for lockscreen
] }
```

---

## A10. Implementation Phase Updates

### A10.1 Revised Phase Timeline

| Phase | Scope | Weeks | Delta vs v1.0 |
|-------|-------|-------|---------------|
| **0** | Foundation (done) | 1-2 | No change |
| **1** | App Shell + LLM + **Tray + Startup + QR** | 3-6 | +1 week |
| **2** | Orchestrator + MCP + **System Module** | 7-9 | No change |
| **3** | Office Agents + **Outlook Agent** | 10-14 | +1 week |
| **3b** | **Folder Scanner Agent** | 15-16 | +2 weeks (new) |
| **4** | Web Researcher (UIA) | 17-19 | No change |
| **5** | **Mobile App (Remote UI)** + WebSocket | 20-23 | +2 weeks |
| **6** | Event-Driven Workflow Engine | 24-26 | No change |
| **7** | Converter + MCP Marketplace | 27-29 | No change |
| **8** | Testing + Hardening + Release | 30-34 | +1 week |

**Tổng: 34 tuần (~8.5 tháng)**

### A10.2 Phase 1 Additional Tasks (System Layer)

```
Phase 1 additions:
  Backend:
  [ ] src/system/mod.rs – SystemManager::init()
  [ ] src/system/tray.rs – setup_tray() với Tauri v2 TrayIconBuilder
  [ ] src/system/startup.rs – winreg register/unregister/check
  [ ] src/system/power.rs – SetThreadExecutionState wrapper + SleepGuard
  [ ] src/system/qrcode.rs – qrcode crate integration
  [ ] src/system/tailscale.rs – tailscale CLI probe
  [ ] App minimises to tray on window close
  [ ] --minimized CLI flag (cho startup registration)

  Frontend (Settings UI):
  [ ] Settings/SystemTab – Startup / Tray / Sleep controls
  [ ] Settings/MobileTab – WebSocket port + QR code modal
  [ ] Settings/TailscaleSection – Status + install link
  [ ] QR Code modal component (SVG display + countdown)
```

### A10.3 Phase 3b: Folder Scanner + Outlook Agent Tasks

```
Folder Scanner (Phase 3b):
  [ ] calamine crate for Excel reading
  [ ] docx crate for Word reading (fallback to COM)
  [ ] pdf-extract crate for PDF text extraction
  [ ] Real LLM summarization per file
  [ ] Real folder-level summary generation
  [ ] Real Word report creation (via OfficeMasterAgent)
  [ ] Real PPT creation (via OfficeMasterAgent)
  [ ] Real Excel summary (via AnalystAgent)
  [ ] Progress events wired to Tauri + WebSocket
  [ ] Mobile progress screen shows real data

Outlook Agent (Phase 3b):
  [ ] COM: Outlook.Application CoCreateInstance
  [ ] COM: NameSpace.Logon() + GetDefaultFolder()
  [ ] COM: Items.Restrict() for filtering
  [ ] COM: MailItem read (Subject, Body, Sender, Attachments)
  [ ] COM: Attachment.SaveAsFile()
  [ ] COM: CreateItem(olMailItem) for composition
  [ ] COM: MailItem.Send() (gated behind HITL)
  [ ] COM: CalendarItems read/create/update
  [ ] COM: AppointmentItem operations
  [ ] Thread summarization via LLM
  [ ] Action item extraction via LLM
  [ ] Daily calendar briefing workflow
```

### A10.4 Phase 5: Mobile App Tasks (Expanded)

```
Phase 5 Mobile App:
  [ ] Expo project setup (React Native)
  [ ] QR Scanner screen (expo-camera)
  [ ] WebSocket client with auto-reconnect
  [ ] Chat screen (full feature parity with desktop)
  [ ] Voice input (expo-speech + @react-native-voice)
  [ ] Progress monitor screen (real-time updates)
  [ ] HITL approvals screen with countdown timer
  [ ] Agent status panel
  [ ] Push notifications (expo-notifications)
  [ ] Secure storage for connection config (expo-secure-store)
  [ ] Tailscale connection support (no extra config needed)
  [ ] Offline queue (commands buffered when disconnected)
  [ ] Connection status indicator (LAN vs Tailscale vs Disconnected)
  [ ] Dark mode support
  [ ] Vietnamese + English UI
```

---

## A11. Security Updates (v1.1)

### A11.1 Mobile Authentication

```
Token storage:
  PC side:  Windows Credential Manager
            → Target: "OfficeHub/MobileToken"
            → Persists across app restarts

  Mobile:   iOS Keychain / Android Keystore
            → Via expo-secure-store
            → Encrypted at rest

Token rotation:
  → Settings → Mobile → "Revoke All Access" button
  → Generates new random UUID token
  → Old token immediately invalidated
  → All connected clients disconnected
  → New QR code must be scanned

QR Code security:
  → 5-minute expiry (configurable)
  → Single-use? No – simplicity over security for v1.0
  → Rate limit: max 1 new QR per 10 seconds
```

### A11.2 Outlook Agent Security

```
Email composition safety:
  1. AI disclaimer appended to every AI-generated email
  2. User is always CC'd (always_cc_user_on_ai_replies = true by default)
  3. Draft always saved to Outlook Drafts first (never auto-send)
  4. HITL approval required for every Send action
  5. "Reply-to" address validated against sender's domain
  6. Blocked: attaching executable files (.exe, .bat, .ps1, .vbs)
  7. Blocked: emails to distribution lists (require explicit user confirmation)
  8. Audit log for every outbound email with: recipient, subject, timestamp, approved_by

Calendar safety:
  1. HITL for create_meeting (sends invitations to attendees)
  2. HITL for cancel_appointment (sends cancellation emails)
  3. Cannot create recurring meetings without HITL HIGH
  4. Max meeting duration: 8 hours (configurable)
  5. Cannot create meetings in the past
```

### A11.3 Folder Scanner Security

```
File access:
  1. Only reads files (never writes to source folder)
  2. Output always to separate output_dir (never overwrites source)
  3. Does not follow symlinks (prevents traversal attacks)
  4. Respects max_file_size_bytes (prevents memory exhaustion)
  5. Excludes hidden files (starting with .) by default

LLM data handling:
  1. File content sent to cloud LLM only if provider = cloud
  2. Warning shown before scanning sensitive directories
     (e.g., contains keywords: "confidential", "secret", "hr", "payroll")
  3. Hybrid mode: sensitive files use local LLM automatically
     if sensitive content detected (TODO: phase 3b)
```

---

## A12. New Workflow Templates

### A12.1 Daily Morning Briefing (new workflow)

```yaml
id: daily-morning-briefing
name: "Tóm tắt buổi sáng hàng ngày"
trigger:
  type: schedule
  config:
    cron: "0 7 * * 1-5"    # 7:00 AM, Monday–Friday
steps:
  - id: read_calendar
    agent: outlook
    action: read_calendar
    config:
      from_date: "{{ TODAY }}"
      to_date: "{{ TODAY }}"
  - id: read_unread_email
    agent: outlook
    action: read_inbox
    config:
      unread_only: true
      max_results: 10
  - id: summarize_day
    agent: orchestrator
    action: generate_summary
    config:
      prompt: "Tóm tắt lịch hôm nay và email chưa đọc quan trọng"
  - id: notify_mobile
    agent: orchestrator
    action: send_mobile_notification
    config:
      title: "☀️ Chào buổi sáng!"
      body: "{{ steps.summarize_day.result }}"
```

### A12.2 Folder → Report Workflow (new workflow)

```yaml
id: folder-to-report
name: "Tổng hợp folder thành báo cáo"
trigger:
  type: manual
steps:
  - id: scan_folder
    agent: folder_scanner
    action: scan_folder_all_formats
    config:
      recursive: true
      max_files: 100
      detail_level: "standard"
      report_language: "vi"
    input:
      folder_path: "{{ context.folder_path }}"
  - id: notify_completion
    agent: orchestrator
    action: send_mobile_notification
    config:
      title: "✅ Tổng hợp hoàn thành"
      body: "{{ steps.scan_folder.stats.processed }} file → {{ steps.scan_folder.output_files | length }} báo cáo"
```

### A12.3 Email → Task Workflow (new workflow)

```yaml
id: email-to-task
name: "Trích xuất task từ email"
trigger:
  type: email_received
  config:
    filter:
      subject_contains: ["task", "việc cần làm", "yêu cầu"]
steps:
  - id: extract_tasks
    agent: outlook
    action: extract_action_items
    input:
      entry_id: "{{ trigger.email_entry_id }}"
  - id: create_outlook_tasks
    agent: outlook
    action: create_task_from_email
    input:
      entry_id: "{{ trigger.email_entry_id }}"
  - id: notify_mobile
    agent: orchestrator
    action: send_mobile_notification
    config:
      title: "📋 Task mới từ email"
      body: "{{ trigger.email_subject }} → {{ steps.extract_tasks.task_count }} tasks"
```

---

## A13. Tauri Configuration Updates

### A13.1 tauri.conf.json additions

```json
{
  "app": {
    "windows": [{
      "label": "main",
      "title": "Office Hub",
      "width": 1280,
      "height": 800,
      "visible": false,
      "decorations": true,
      "skipTaskbar": false
    }],
    "trayIcon": {
      "iconPath": "icons/tray-32.png",
      "iconAsTemplate": false,
      "menuOnLeftClick": false,
      "tooltip": "Office Hub"
    }
  },
  "plugins": {
    "single-instance": {},
    "autostart": {
      "args": ["--minimized"],
      "desktop": true,
      "linux": false,
      "windows": true
    }
  }
}
```

### A13.2 New Tauri Commands (additions to commands.rs)

| Command | Description |
|---------|-------------|
| `get_system_config` | Get SystemConfig |
| `save_system_config` | Save + apply SystemConfig |
| `toggle_startup` | Enable/disable Windows startup |
| `get_startup_enabled` | Check if startup is registered |
| `suppress_sleep` | Start sleep suppression |
| `release_sleep` | Stop sleep suppression |
| `get_pairing_qr` | Generate QR code payload + SVG |
| `get_network_info` | LAN IPs + Tailscale info |
| `get_tailscale_status` | Tailscale connection state |
| `refresh_network` | Re-probe network + Tailscale |
| `get_system_status` | Full system status JSON |
| `scan_folder` | Trigger Folder Scanner Agent |
| `get_scan_progress` | Get active scan progress |
| `cancel_scan` | Cancel running scan |
| `read_outlook_inbox` | Read Outlook inbox |
| `search_outlook_emails` | Search emails with filter |
| `compose_email_reply` | Compose email reply (draft) |
| `send_email` | Send email (HITL gated) |
| `read_outlook_calendar` | Read calendar items |
| `create_appointment` | Create calendar item (HITL) |

---

## Appendix: Decisions & Rationale

### Decision 1: Mobile App Framework – React Native (Expo)

**Alternatives considered:**
- Flutter → Better performance, but different language (Dart) and no code sharing with Desktop
- Capacitor (Ionic) → Web-based, but worse native feel
- Native Swift/Kotlin → Best UX, but 2x development effort

**Decision:** React Native + Expo
- Code sharing: hooks, stores, types với Desktop (React)
- Expo managed workflow: OTA updates, no native build config
- Strong community: expo-notifications, expo-camera, expo-secure-store
- Cross-platform: iOS + Android from single codebase

### Decision 2: Tailscale vs. ngrok vs. Own TURN server

**Alternatives considered:**
- ngrok → Requires subscription for stable URLs; privacy concerns
- Own TURN/STUN server → Complex to deploy; not suitable for non-technical users
- ZeroTier → Less mainstream than Tailscale; similar complexity

**Decision:** Tailscale
- Free for personal use (up to 3 users, 100 devices)
- Zero-config: just install and connect
- Stable DNS names (hostname.tailnet.ts.net)
- Open source client, privacy-respecting
- Works through NAT, firewalls, CGNAT

### Decision 3: QR Code Expiry – 5 minutes

**Rationale:**
- Long enough: user has time to scan without rushing
- Short enough: prevents old QR codes from being used by accident
- Industry standard: similar to 2FA apps (30s much too short for a QR scan flow)
- Refreshable: user can always generate a new QR instantly

### Decision 4: Always CC User on AI-composed Emails

**Rationale:**
- Transparency: user always sees what was sent in their name
- Safety net: if AI makes an error, user is immediately aware
- Default ON: user can disable in Settings if preferred
- Professional: adds accountability to AI-generated communications

---

*Amendment v1.1 | Reviews incorporates all requirements from user session on 2025. Supersedes no sections of v1.0 – additive only.*

*Next review: After Phase 1 completion (System Tray + QR Code prototype)*