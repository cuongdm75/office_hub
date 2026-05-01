# Kế hoạch triển khai: Chuyển đổi LlmGateway sang GenAI Crate & Đại tu Agent

Mục tiêu:
1. Thay thế kiến trúc gọi LLM thủ công (dựa trên JSON Schema parsing) bằng Native Tool Calling thông qua crate `genai`.
2. Chuẩn hóa kiến trúc (Clean Architecture): Buộc hệ thống `AgentRegistry` phải tuân thủ chuẩn JSON Schema của `McpBroker` để loại bỏ hoàn toàn tình trạng truyền sai tham số (hallucination).

---

## Phase 1: Nâng cấp LlmGateway & Thiết lập luồng ReAct mới (Hybrid Mode)
*Mục tiêu: Đưa genai vào hoạt động mà không làm gãy các Agent cũ.*

### 1. Thay đổi Cấu hình
- **`Cargo.toml`:** Thêm `genai = "0.1"` vào `[dependencies]`. Giữ nguyên `reqwest`.

### 2. Thay đổi LlmGateway
- **`llm_gateway/mod.rs`:** 
  - Xóa các struct Provider cũ (`GeminiProvider`, `OpenAiCompatProvider`).
  - Khởi tạo và sử dụng `genai::Client`.
  - Hàm `complete_stream` giờ đây trả về luồng `ChatStreamEvent` chuẩn của thư viện.

### 3. Thay đổi Orchestrator
- **`orchestrator/mod.rs`:**
  - Rút gọn `system_prompt` (Xóa đoạn bắt LLM trả về `{ "thought", "agent_calls" }`).
  - Viết Adapter chuyển đổi danh sách Tool từ `McpBroker` thành `genai::chat::Tool`.
  - **Hybrid Bridge:** Tạo ra một Native Tool trung gian tên là `call_legacy_agent` (với tham số tự do `agent_id`, `action`, `parameters`) để LLM tạm thời gọi được các Agent chưa được chuyển đổi.
  - Sửa vòng lặp `max_turns = 5`: Lắng nghe `ChatStreamEvent::Chunk` (cho UI) và `ChatStreamEvent::ToolCall` (để gọi `mcp_broker` hoặc `agent_registry`).

---

## Phase 2: Đại tu kiến trúc Agent (Chuyển đổi sang chuẩn MCP)
*Mục tiêu: Cung cấp JSON Schema Native cho từng Action của tất cả Agent.*

### 1. Cập nhật Agent Trait
- **`agents/mod.rs`:** 
  - Thay thế (hoặc bổ sung) hàm `fn supported_actions(&self) -> Vec<String>` bằng hàm mới:
    `fn tool_schemas(&self) -> Vec<McpTool>`.
  - Yêu cầu mọi Agent phải định nghĩa rõ tham số đầu vào (input_schema) cho từng chức năng của nó thay vì nhận một cục JSON lỏng lẻo.

### 2. Refactor Sub-Agents (Cực kỳ quan trọng)
- Mở từng file Agent (`analyst.rs`, `office_master.rs`, `web_researcher.rs`, `outlook.rs`...) và cập nhật lại chúng:
  - Khai báo rõ ràng các `McpTool` object. Ví dụ: Agent `office_master` sẽ khai báo schema cho `create_word_doc` cần tham số `content`, `heading`...
  - Thay đổi logic nội bộ của hàm `execute` để map tham số chuẩn xác.

### 3. Hợp nhất vào Orchestrator
- Xóa bỏ cái cầu nối trung gian `call_legacy_agent` đã tạo ở Phase 1.
- Khi gửi request cho LLM, Orchestrator sẽ gom (merge) danh sách tools từ `McpBroker` VÀ `AgentRegistry` lại, truyền tất cả vào `ChatOptions` của `genai`. 
- LLM sẽ giao tiếp với tất cả các thành phần hệ thống thông qua chung 1 chuẩn Native Tool Calling.

---

## User Review Required
- [ ] Phê duyệt tiến trình 2 Phase. (Tôi đề xuất chúng ta **hoàn thành và test xong Phase 1**, sau đó mới bắt tay vào **cày cuốc Phase 2** để đảm bảo hệ thống không bị lỗi quá lớn cùng một lúc).

## Verification Plan
### Automated Tests
- Chạy `cargo check` sau mỗi Phase để kiểm tra chặt chẽ type-safety của Rust.
### Manual Verification (Phase 1)
- Chat với App: "Tìm trong memory thông tin dự án Alpha". 
- Quan sát Console log xem LLM có gọi Native Tool `search_memory` thành công không.
### Manual Verification (Phase 2)
- Chat với App: "Tạo file Word báo cáo dự án Alpha".
- Quan sát Console log xem LLM có gọi Native Tool `office_master_create_word` với đầy đủ JSON Parameters hợp lệ không (thay vì gọi `call_legacy_agent`).
