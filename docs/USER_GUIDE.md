# 🏢 Office Hub - Hướng Dẫn Sử Dụng (User Manual)

Chào mừng bạn đến với **Office Hub**! Đây là "trợ lý AI siêu nhẹ" tích hợp sâu vào môi trường Windows và Microsoft Office, giúp bạn tự động hóa các tác vụ lặp đi lặp lại hàng ngày chỉ bằng các câu lệnh trò chuyện (chat) đơn giản.

---

## 📑 Mục lục
1. [Cài đặt & Bắt đầu nhanh](#1-cài-đặt--bắt-đầu-nhanh)
2. [Giao diện chính](#2-giao-diện-chính)
3. [Làm việc với các Trợ lý (Agents)](#3-làm-việc-với-các-trợ-lý-agents)
4. [Tự động hoá quy trình (Visual Workflow)](#4-tự-động-hoá-quy-trình-visual-workflow)
5. [Ứng dụng Mobile Remote & Phê duyệt (HITL)](#5-ứng-dụng-mobile-remote--phê-duyệt-hitl)
6. [Mở rộng kỹ năng (MCP Marketplace)](#6-mở-rộng-kỹ-năng-mcp-marketplace)

---

## 1. Cài đặt & Bắt đầu nhanh

### Cấu hình AI (LLM Gateway)
Khi mở ứng dụng lần đầu, bạn cần thiết lập "Bộ não AI" cho Office Hub tại phần **Settings**:
- **Cloud Models**: Nhập API Key cho Google Gemini hoặc OpenAI GPT nếu bạn muốn dùng AI trên mây (nhanh, thông minh).
- **Local Models (Bảo mật cao)**: Nếu công ty yêu cầu bảo mật dữ liệu tuyệt đối, bạn có thể kết nối với **Ollama** hoặc **LM Studio** chạy trực tiếp trên máy tính của bạn (không cần internet, không rò rỉ dữ liệu).

### Chọn thư mục làm việc
- Trong tab **Folder Explorer**, chọn thư mục chứa các file báo cáo, Excel, Word mà bạn muốn AI làm việc cùng.

---

## 2. Giao diện chính

Giao diện Desktop của Office Hub được chia thành các khu vực chính:

- **Chat Pane (Khung chat)**: Nơi bạn gõ lệnh yêu cầu (Ví dụ: *"Tạo báo cáo doanh thu quý 3 từ file Excel data.xlsx"*). AI sẽ trả lời và hiển thị quá trình làm việc tại đây.
- **History Tree View (Lịch sử theo chủ đề)**: Thanh bên trái giúp bạn nhóm các phiên làm việc theo từng dự án (Ví dụ: "Báo cáo Tài chính Tháng 10"). Bạn có thể dễ dàng xem lại toàn bộ quá trình xử lý công việc cũ.
- **File Browser / Folder Explorer**: Giúp bạn xem trước và chọn các file trong máy tính để AI xử lý mà không cần mở app khác.
- **Settings & Workflow Builder**: Các tab quản lý cấu hình và vẽ sơ đồ tự động hóa.

---

## 3. Làm việc với các Trợ lý (Agents)

Office Hub không chỉ có một AI, mà là một **nhóm các Trợ lý chuyên gia**. Bạn chỉ cần ra lệnh, hệ thống sẽ tự động điều phối đúng chuyên gia để làm việc.

### 📊 Trợ lý Excel (Analyst Agent)
- **Khả năng**: Đọc/ghi dữ liệu, tạo công thức (XLOOKUP, LAMBDA), viết Power Query, và nhận diện bất thường trong số liệu.
- **Cách dùng**: 
  - *"Đọc số liệu tổng doanh thu trong cột B file baocao.xlsx."*
  - *"Tạo công thức XLOOKUP tìm giá sản phẩm và điền vào ô C2."*
- **Lưu ý**: AI có cơ chế "Hard-Truth Verification", sau khi ghi số liệu sẽ tự động đọc lại để đảm bảo không ghi sai số do "ảo giác" của AI.

### 📝 Trợ lý Word & PowerPoint (Office Master)
- **Khả năng**: Điền số liệu vào template có sẵn, tạo slide thuyết trình theo đúng màu sắc thương hiệu (Brand Guidelines).
- **Cách dùng**:
  - *"Dùng file template_hop_dong.dotx, điền tên khách hàng là Nguyễn Văn A và lưu thành file mới."*
  - *"Tạo 5 slide báo cáo doanh thu dựa trên số liệu vừa lấy được, dùng màu chủ đạo của công ty."*

### 🌐 Trợ lý Duyệt Web (Web Researcher)
- **Khả năng**: Tự động mở trình duyệt (Edge/Chrome), trích xuất bảng biểu, tìm kiếm thông tin mà không cần cài extension hay cấu hình phức tạp.
- **Cách dùng**:
  - *"Vào trang web chứng khoán lấy giá vàng hôm nay."*
  - AI sẽ tự điều khiển chuột/phím nền, sau đó **chụp ảnh màn hình (screenshot)** lại để bạn đối chiếu tính xác thực.

### 📁 Trợ lý Quét thư mục & Email (Folder Scanner & Outlook)
- **Khả năng**: Quét hàng loạt file trong thư mục để tổng hợp dữ liệu, tự động đọc và trả lời email qua Outlook.
- **Cách dùng**: *"Đọc các email chưa xem từ sếp hôm nay và tóm tắt lại."*

---

## 4. Tự động hoá quy trình (Visual Workflow)

Thay vì gõ lệnh từng bước, bạn có thể tạo một quy trình chạy tự động qua giao diện **Workflow Builder** (Kéo & Thả).

- Mở tab **Workflows**.
- Kéo các khối (Nodes) ra màn hình để xếp chuỗi hành động.
- **Ví dụ quy trình mẫu**: 
  `Trigger (Khi có email mới tới)` ➡️ `Quét file đính kèm` ➡️ `Trợ lý Web tìm thêm thông tin` ➡️ `Trợ lý Excel tính toán` ➡️ `Gửi tin nhắn duyệt qua Mobile`.

---

## 5. Ứng dụng Mobile Remote & Phê duyệt (HITL)

Để đảm bảo AI không tự ý thực hiện các hành động nguy hiểm (như xoá file, gửi email sai, hoặc bấm nhầm nút trên web), Office Hub có cơ chế **Human-in-the-Loop (HITL) - Chờ người duyệt**.

1. Mở **Settings > Mobile Connection** trên máy tính, hệ thống sẽ hiện mã QR.
2. Mở app Office Hub trên điện thoại, quét mã QR để kết nối an toàn (qua mạng nội bộ/Tailscale).
3. **Phê duyệt rủi ro**: Bất cứ khi nào AI định thực hiện thao tác nhạy cảm (như gửi email đi, hay điền form trên web), thông báo sẽ gửi đến màn hình **Approvals** trên điện thoại của bạn. Bạn bấm "Cho phép" thì AI mới được làm tiếp.
4. **Theo dõi từ xa**: Tab **Progress** trên điện thoại cho phép bạn xem tiến độ làm việc của AI ngay cả khi đang không ngồi ở máy tính.

---

## 6. Mở rộng kỹ năng (MCP Marketplace)

AI của bạn có thể học thêm kỹ năng mới qua **Converter Agent** và chuẩn **Model Context Protocol (MCP)**.

- Bạn muốn AI kết nối với Jira, GitHub, hay Slack? 
- Mở **Agent Manager / Marketplace**.
- Yêu cầu AI: *"Học cách lấy dữ liệu từ hệ thống nội bộ công ty qua tài liệu API này."*
- AI sẽ tự đọc tài liệu, tự động viết code kết nối và bổ sung vào kỹ năng của nó ngay lập tức.

---

> **Lưu ý an toàn:** Hãy luôn kiểm tra lại các tài liệu do AI tạo ra trước khi gửi đi. Mã nguồn của Office Hub được thiết kế để không ghi đè trực tiếp lên file gốc của bạn mà luôn tạo bản sao lưu (backup) trước khi chỉnh sửa.
