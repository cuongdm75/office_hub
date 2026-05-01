# Office Hub Phase 8: Autonomous Orchestration & Collaboration Roadmap

Tài liệu này vạch ra lộ trình nâng cấp kiến trúc của Office Hub để hướng tới một hệ thống siêu tự trị (fully autonomous) và khả năng làm việc nhóm thời gian thực (collaboration).

---

## 1. Tăng cường Autonomous Capability (Planning Layer)
**Mục tiêu:** Chuyển từ mô hình phản xạ (Reactive) sang mô hình tự chủ (Autonomous).
- **Architecture Update:** Chèn thêm một `Planning Layer` vào vòng lặp Orchestrator (`src-tauri/src/orchestrator/mod.rs`).
- **Luồng hoạt động mới:** 
  1. `Analyzer`: Phân tích User Request (VD: "Làm báo cáo quý").
  2. `Planner`: Trả về một JSON DAG (Directed Acyclic Graph) chia nhỏ công việc.
     - *Step 1:* `FileSystemAgent` -> Scan folder dữ liệu.
     - *Step 2:* `SpreadsheetAgent` -> Tổng hợp Excel.
     - *Step 3:* `WordAgent` -> Ghi nội dung báo cáo.
     - *Step 4:* `PdfAgent` -> Export ra PDF.
  3. `Executor`: Điều phối chạy song song (nếu không phụ thuộc) hoặc tuần tự các sub-task này.
- **Implementations:** 
  - Thêm một `ProjectPlannerAgent` hoặc tích hợp tính năng lập kế hoạch trực tiếp vào LLM Gateway prompt.
  - Sửa đổi `AgentTask` để có thể nhận dạng các `parent_task_id` và `dependencies`.

## 2. Real-time Collaboration (Co-authoring)
**Mục tiêu:** Hỗ trợ nhiều user/agent cùng chỉnh sửa tài liệu mà không conflict.
- **Frontend/Backend:** Tích hợp engine CRDT (Conflict-free Replicated Data Type) như `Yjs` hoặc `Automerge` vào Tauri backend thông qua WebSocket để duy trì Document State.
- **Office Integration (The Hard Part):** 
  - Nếu làm việc với file `.docx`, `.xlsx` offline: Áp dụng cơ chế Check-in/Check-out (File Locking) nội bộ qua `FileSystemServer`.
  - Nếu cần true real-time: Viết các adapter đồng bộ lệnh (Command Syncing) để đẩy thao tác trực tiếp qua COM tới ứng dụng Office đang mở.

## 3. Mở rộng Skill Outlook Master (Calendar & Tasks)
**Mục tiêu:** Quản lý toàn diện lịch trình và nhắc việc.
- **Cập nhật nội dung Skill:** Mở rộng file `.agent/skills/outlook-master/SKILL.md` và các script Python/PowerShell đi kèm.
- **Thêm MCP Tools mới:**
  - `read_calendar`: Đọc lịch họp (`AppointmentItem`) trong N ngày tới.
  - `create_meeting`: Tạo lời mời họp, tự động tìm giờ trống của người tham gia.
  - `manage_tasks`: Quản lý Todo list (`TaskItem`), cài đặt reminder, báo cáo tiến độ.
  - `summarize_meeting`: Đọc nội dung đính kèm hoặc body của lịch họp và tóm tắt thành action items.

## 4. Observability & Analytics Dashboard
**Mục tiêu:** Cung cấp giao diện trực quan theo dõi sức khỏe và chi phí của hệ thống AI.
- **Database Tracking:** Thêm bảng `telemetry_logs` vào cơ sở dữ liệu SQLite (hoặc log file `telemetry.jsonl`) để lưu:
  - Tên Agent được gọi.
  - Thời gian xử lý (Latency).
  - Token đã dùng (Prompt/Completion) và Ước tính chi phí.
  - Tỷ lệ thành công/thất bại (Success rate).
- **React UI (`AnalyticsDashboard.tsx`):**
  - Xây dựng Dashboard hiển thị:
    - Biểu đồ chi phí (Cost breakdown theo Agent/Model).
    - Biểu đồ nhiệt (Heatmap) tần suất sử dụng công cụ.
    - Cảnh báo vòng lặp vô hạn (Infinite loop alerts) hoặc lỗi thất bại liên tục.