# Kế Hoạch Triển Khai: Win32 Autonomy (Hệ thống OS-Level Agents)

## Mục Tiêu
Nâng cấp Office Hub từ một trợ lý văn phòng (Productivity) thành một quản trị viên hệ thống (System Administrator) có khả năng tương tác sâu với môi trường Win32 (Quản lý File, Registry, Tiến trình, Cài đặt phần mềm, và chạy PowerShell Script) một cách tự động và bảo mật.

## Phương Pháp Tiếp Cận Kiến Trúc
Thay vì tạo ra vô số các Agent nhỏ lẻ, chúng ta sẽ xây dựng **1 Agent Native** (`Win32AdminAgent`) và tận dụng kiến trúc **MCP Tools** để định nghĩa các chức năng. Điều này giúp LLM dễ dàng hiểu schema và gọi tool chính xác, đồng thời giúp việc kiểm duyệt qua Rule Engine trở nên đồng nhất.

---

## Giai Đoạn 1: Xây Dựng Core Win32 Capabilities

### 1. File System Operations
Bổ sung các thao tác quản lý file cấp thấp (hiện tại `folder_scanner` chỉ đọc):
- `file_create_dir`: Tạo thư mục.
- `file_move`: Di chuyển/Đổi tên file.
- `file_delete`: Xoá file (tuỳ chọn đưa vào Recycle Bin hoặc xoá vĩnh viễn).
- `file_archive`: Nén (Zip) và giải nén (Unzip) thư mục.

### 2. Registry & Process Management
- `registry_read` / `registry_write`: Ghi/Đọc cấu hình hệ thống (HKCU, HKLM).
- `process_list` / `process_kill`: Quản lý tiến trình (như Task Manager).

### 3. Package Management (Quản lý phần mềm)
Tích hợp **Winget** (có sẵn trên Win 10/11) để cài đặt phần mềm im lặng:
- `winget_search`: Tìm kiếm package ID.
- `winget_install`: Cài đặt phần mềm (`winget install --id <ID> --silent --accept-package-agreements`).
- `winget_uninstall`: Gỡ cài đặt phần mềm.

### 4. PowerShell / Shell Executor
- `shell_execute`: Chạy một lệnh hoặc script PowerShell. (Cần bọc trong tiến trình con và bắt stdout/stderr để trả về cho LLM).

---

## Giai Đoạn 2: Lớp Bảo Mật & Human-In-The-Loop (HITL)

Cho phép AI tương tác sâu với HĐH mang lại rủi ro lớn. Lớp bảo mật là **BẮT BUỘC** trước khi phát hành.

### 1. Phân loại Mức độ Rủi ro (Risk Levels)
Gắn nhãn cho các hành động:
- **Low Risk** (Tự động chạy): Đọc file, tìm kiếm phần mềm, xem danh sách tiến trình.
- **Medium Risk** (Cảnh báo): Nén file, ghi file ở thư mục User.
- **High Risk** (Bắt buộc phê duyệt - HITL): Chạy lệnh Shell, sửa Registry, cài đặt/xoá phần mềm, xoá file hệ thống.

### 2. Triển khai HITL (Xác nhận từ người dùng)
- Khi `Win32AdminAgent` chuẩn bị chạy một lệnh High Risk, nó sẽ phát ra sự kiện `SseEvent::ApprovalRequest` xuống UI.
- Tiến trình của Agent sẽ bị treo (await) cho đến khi người dùng bấm **Approve** (Đồng ý) hoặc **Deny** (Từ chối) trên giao diện Desktop/Mobile.
- Nếu người dùng từ chối, Agent sẽ trả về thông báo lỗi cho LLM để nó tự tìm cách khác (hoặc dừng lại).

### 3. Windows UAC Elevation
- Các thao tác ghi HKLM hoặc cài phần mềm yêu cầu quyền Admin.
- Cần sử dụng thư viện Rust (như `runas` hoặc gọi `Start-Process -Verb RunAs` trong PowerShell) để hiển thị bảng UAC của Windows yêu cầu người dùng cấp quyền.

### 4. Hardening Rule Engine
- Bổ sung Regex filter vào `RuleEngine` để chặn ngay lập tức các lệnh độc hại:
  - Cấm can thiệp thư mục `C:\Windows`, `C:\Program Files` (trừ khi là trình cài đặt).
  - Cấm các từ khoá phá hoại: `format`, `Remove-Item -Force -Recurse C:\`, `Set-ExecutionPolicy Bypass`.

---

## Giai Đoạn 3: UI & UX Integration

### 1. Cập nhật Desktop / Mobile UI
- Hiển thị component **Approval Card** ngay trong khung Chat khi nhận được `ApprovalRequest`.
- Card hiển thị: *[Cảnh báo] Trợ lý AI đang muốn chạy lệnh: `winget install vscode`* kèm nút Xanh/Đỏ.

### 2. System Tray & Status
- Cập nhật biểu tượng hoặc trạng thái ở Sidebar để người dùng biết hệ thống đang nắm quyền thay đổi OS.

---

## Verification Plan
1. Viết unit test cho các thao tác File & Winget trong môi trường Sandbox (thư mục temp).
2. Kiểm tra tiến trình HITL: Gửi yêu cầu cài đặt phần mềm -> Bảng Approval hiện ra -> Bấm Deny -> Đảm bảo LLM nhận được thông báo từ chối và phản hồi lại người dùng.
3. Chạy thử trên máy ảo Windows 11 sạch để xác nhận tính năng cài đặt tự động hoạt động hoàn hảo.
