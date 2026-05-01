# Handoff Session 21

## Trạng thái hệ thống (System Status)
- **Phase 7 (Converter Agent & MCP Marketplace):** Đã hoàn tất (`✅`).
- **Backend (`src-tauri`):** `ConverterAgent` đã có khả năng tạo ra mã nguồn Python MCP tự động qua `LlmGateway`. Các IPC Commands (`start_skill_learning`, `test_skill_sandbox`, `approve_new_skill`, `call_mcp_tool`) đã được thêm vào và biên dịch thành công mà không có lỗi (`cargo check` pass).
- **Frontend (`src`):** Tab `Marketplace` mới đã được tích hợp (`AgentManager` Component). Hỗ trợ UI Visual Hub và Import Wizard 4 bước (Nhập URL -> Phân tích -> Chạy Sandbox -> Cài đặt).

## Khó khăn đã giải quyết
- Việc LLM trả code có chứa các Markdown blocks (`` ` ``) được xử lý bằng logic cắt gọt (`code.trim_start_matches(...)`) trong `ConverterAgent::execute` để đảm bảo lưu thành file Python hợp lệ.
- Giao diện người dùng được đồng bộ hoá các states giữa `importUrl`, `sandboxId` và các `step` wizard thông suốt và xử lý lỗi hiển thị tốt qua `react-hot-toast`.
- Unused variables cảnh báo ở `commands.rs` (biến `state` dư thừa) đã được fix triệt để.

## Khó khăn đang tồn đọng
- **Bảo mật Sandbox:** Hiện tại script Python được sinh ra và nạp vào bằng cách gọi `python:path/to/script.py`. Mặc dù hệ thống MCP của Office Hub đã có module `McpRegistry::install`, song khả năng cô lập thực thi (OS-level sandbox hoặc WASM) cho code sinh từ LLM vẫn chưa thiết lập đầy đủ (được đánh dấu lại cho Phase sau nếu cần).
- **Kiểm thử giao diện:** Quá trình giao tiếp với LLM chưa có progress stream mà chỉ là một trạng thái spinner quay, nên việc xử lý chậm có thể gây bối rối cho người dùng.
- LLM Provider chưa được test E2E cho luồng sinh code dài (có thể bị limit context window hoặc timeouts nếu Model cấu hình trên `config.yaml` nhỏ).

## Bước tiếp theo (Next Steps - Phase 8 hoặc Phase 9)
Session tiếp theo nên tập trung vào một trong các hướng:
1. **Kiểm thử E2E hệ thống Marketplace (Phase 7 - Follow up):** Chạy `npm run tauri dev`, nhập URL docs và theo dõi logs để quan sát độ chính xác của Python code mà LLM sinh ra và khắc phục các edge cases của MCP script parsing.
2. **Phase 8 (Testing, Hardening & Release):** Bắt đầu rà soát toàn bộ các APIs từ Phase 1->7, tiến hành Unit/Integration tests tự động cho hệ thống Orchestrator, LLM Gateway, Rule Engine.
3. **Phase 9 (Visual Workflow & History):** Tiếp tục phát triển Drag & Drop UI trên React (`WorkflowBuilder`) và lưu log các sessions chia thành từng topic để dễ dàng quản lý.

## Bối cảnh cần lưu ý (Context to Retain)
- Khi gọi LLM cho tác vụ viết code, luôn nhắc nhở model KHÔNG giải thích, chỉ trả về code (điều này đã được hard-code vào prompt của `ConverterAgent`).
- Frontend sử dụng `@tauri-apps/api/core` để gọi `invoke` do hệ thống đang dùng Tauri v2. Mọi plugin/IPC interactions phải tham chiếu docs v2.
- Việc thực thi mã lệnh Python phụ thuộc vào runtime máy Client (người dùng phải có sẵn Python environment và thư viện `mcp` cài trên máy).
