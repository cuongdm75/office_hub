# Office Hub - Handoff Session 24

## 1. Context & Architectural Decisions
Trong session này, chúng ta đã đi sâu vào việc gỡ lỗi (debug) quá trình tương tác giữa hệ thống `LlmGateway` và tính năng **Marketplace (ConverterAgent)**.
- **Vấn đề đã phân tích:** Tính năng "Import Skill từ Github" đôi khi trả về lỗi `500 Internal Server Error: Unknown API error`. Thông qua phân tích luồng code Rust và log của Ollama, chúng ta xác nhận rằng Backend **đã dùng đúng chung cấu hình LLM** từ Settings (qua `state.llm_gateway`), nhưng model bị crash/timeout do prompt yêu cầu sinh code quá khắt khe.
- **Sửa lỗi Parser:** Đã fix logic parse lỗi JSON trong `OpenAiCompatProvider` để trích xuất đúng message từ Ollama thay vì văng lỗi `Unknown API error`.
- **Quyết định Kiến trúc Mới (QUAN TRỌNG):** Người dùng đã chỉ ra một điểm yếu chí mạng trong thiết kế Phase 7: *Nếu AI sinh ra MCP Server bằng Python, hệ thống sẽ chạy lỗi trên máy tính Windows không có Python*. 
  👉 **Giải pháp chốt:** Sẽ chuyển sang sử dụng **PowerShell (`.ps1`)** làm ngôn ngữ mặc định để sinh ra MCP Server. PowerShell có sẵn trên mọi máy Windows và có thể tương tác chuẩn mực qua `[Console]::In/Out` để xử lý giao thức JSON-RPC 2.0 của MCP.

## 2. Next Steps (Dành cho Session tiếp theo)

### A. Cập nhật ConverterAgent (Rust)
- Sửa đổi Prompt nội bộ trong `src-tauri/src/agents/converter/mod.rs`:
  - Yêu cầu LLM sinh ra script bằng **PowerShell** thay vì Python.
  - Hướng dẫn rõ ràng cách tạo vòng lặp đọc/ghi `stdin/stdout` (`[Console]::In.ReadLine()` và `[Console]::Out.WriteLine()`) để tuân thủ MCP Protocol JSON-RPC 2.0.
- Cập nhật đuôi file khi lưu script sinh ra từ `.py` thành `.ps1`.

### B. Cập nhật luồng cài đặt & thực thi MCP
- Đảm bảo `McpRegistry` (hoặc logic thực thi Sandbox) biết cách khởi chạy các script `.ps1`.
- Lệnh spawn process cần thay đổi thành: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <đường_dẫn_file_ps1>`.
- Đảm bảo thiết lập encoding là UTF-8 `[Console]::OutputEncoding = [System.Text.Encoding]::UTF8` ngay đầu script sinh ra.

### C. Testing & Verification
- Khởi động lại ứng dụng để áp dụng bản fix LLM error parsing.
- Sử dụng UI **Import Wizard** trong Agent Manager để import thử một URL tài liệu.
- Kiểm tra file `.ps1` được sinh ra, thử gửi lệnh MCP Protocol qua stdin để đảm bảo PowerShell MCP Server chạy ổn định, không bị deadlock.
- (Tùy chọn) Bổ sung dòng hiển thị "Đang sử dụng LLM: [Tên Model]" lên giao diện Import Wizard để người dùng yên tâm về tính thống nhất của Settings.

## 3. Files to Focus On
- `e:\Office hub\src-tauri\src\agents\converter\mod.rs`
- `e:\Office hub\src-tauri\src\orchestrator\mcp\mod.rs` (hoặc registry tương ứng xử lý spawn command)
- `e:\Office hub\src\components\AgentManager\AgentManager.tsx` (cập nhật UI hiển thị nếu cần)
