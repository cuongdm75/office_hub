# Handoff Session: MCP-Driven Context Optimization

## Tóm tắt Mục tiêu
Chúng ta đang tiến hành tái cấu trúc kiến trúc của Office Hub Orchestrator để chuyển từ mô hình "Push" (nhồi nhét toàn bộ context vào system prompt) sang mô hình "Pull" (sử dụng Native Tool Calling thông qua một **Internal MCP Broker**). Điều này giúp giảm thiểu lượng token base (Input Tokens), tăng tính linh hoạt và loại bỏ sự phụ thuộc vào classifier cứng nhắc.

## Những gì đã hoàn thành trong Session này
1. **Phase 1: Xây dựng Internal MCP Broker (`src-tauri/src/mcp/broker.rs`)**
   - Định nghĩa trait `InternalMcpServer`.
   - Triển khai `McpBroker` tích hợp chung cho cả Internal Servers (chạy trực tiếp qua trait trong RAM) và External Registry (stdio plugins).
   - Đã thay thế `mcp_registry` bằng `mcp_broker` bên trong struct `Orchestrator`.

2. **Phase 2: Chuyển đổi Context thành MCP Tools (`src-tauri/src/mcp/internal_servers.rs`)**
   - Đã tạo `PolicyServer` (tool `query_policy`).
   - Đã tạo `KnowledgeServer` (tool `list_knowledge`, `read_knowledge`).
   - Đã tạo `MemoryServer` (tool `search_memory`).
   - Đã cập nhật các hàm `set_knowledge_dir`, `set_policy_dir`, `set_memory_store` trong `orchestrator/mod.rs` để tự động đăng ký các internal servers này vào `McpBroker`.
   - Đã fix toàn bộ lỗi compile, hệ thống hiện tại **build thành công** (`cargo check` pass).

## Tình trạng hiện tại (Trạng thái treo)
Tôi đang thực hiện **Phase 3: Cập nhật LLM Orchestrator Context Injection**.
Tại hàm `process_message` trong `src-tauri/src/orchestrator/mod.rs` (khoảng dòng 295), tôi chuẩn bị xóa bỏ logic đọc và inject file trực tiếp vào biến `system_prompt` (`policy_context`, `knowledge_context`, `memory_context`).
Thay vào đó, tôi định truyền vào danh sách cấu trúc schema của các tools thu thập được từ `self.mcp_broker.list_all_tools().await`.
Lệnh edit đã bị dừng do giới hạn context của AI.

## Các bước tiếp theo (Dành cho Agent ở Session mới)
1. **Hoàn thiện Phase 3 trong `src-tauri/src/orchestrator/mod.rs`:**
   - Thay thế việc push text raw vào prompt bằng việc inject schema của các MCP tools (sử dụng `list_all_tools()`).
   - Sửa đổi hướng dẫn trong `system_prompt` để yêu cầu LLM gọi tool qua mảng `agent_calls` với logic: `agent_id = "mcp_broker"` và `action = tool_name`.
2. **Cập nhật Logic Xử lý Output (Phase 4):**
   - Cập nhật hàm `execute_agent_calls` (hoặc block mã vòng lặp phía dưới `process_message`) để nếu nhận diện được `agent_id == "mcp_broker"`, thì tiến hành gọi `self.mcp_broker.call_tool(...)` thay vì đẩy xuống `agent_registry`.
3. **Thử nghiệm & Benchmark:**
   - Chạy thử một lệnh chat cơ bản để xác nhận Agent tự động gọi tool `search_memory` hoặc `query_policy`.
   - So sánh lượng input token khởi tạo trước và sau (kỳ vọng giảm đáng kể).

## Các file quan trọng để tiếp tục:
- `src-tauri/src/orchestrator/mod.rs` (Đang sửa dở ở dòng 295).
- `src-tauri/src/mcp/broker.rs` (Đã hoàn thành).
- `src-tauri/src/mcp/internal_servers.rs` (Đã hoàn thành).
- Kế hoạch tổng quát tại `.gemini/antigravity/brain/.../task.md` và `implementation_plan.md`.

*Chúc Agent tiếp theo code vui vẻ và không dính lỗi lifetime của Rust!*
