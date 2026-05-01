# 🏢 Office Hub - Sổ tay Ứng dụng Nghiệp vụ Văn phòng

Chào mừng bạn đến với **Office Hub** - Trợ lý AI chuyên dụng được thiết kế để tự động hoá và tối ưu hoá toàn bộ các tác vụ văn phòng của bạn. Thay vì phải làm việc thủ công qua nhiều phần mềm khác nhau, Office Hub đóng vai trò là "Bộ não trung tâm", hiểu ngôn ngữ tự nhiên và tự động điều khiển các ứng dụng như Word, Excel, PowerPoint, Outlook, trình duyệt Web và file PDF.

Tài liệu này sẽ giúp bạn hiểu rõ **hệ thống có thể làm gì** và **áp dụng vào các tình huống thực tế (Use cases)** như thế nào.

---

## 🌟 Các Khả Năng & Tính Năng Cốt Lõi

Hệ thống hoạt động dựa trên các "Trợ lý chuyên gia" (Agents), mỗi trợ lý đảm nhận một nghiệp vụ cụ thể:

1. **📊 Analyst Agent (Trợ lý Dữ liệu & Excel)**
   - **Tính năng**: Xử lý dữ liệu lớn siêu tốc với Polars, điều khiển Excel đang mở (chèn công thức, XLOOKUP, Pivot Table, vẽ biểu đồ), trích xuất SQL.
   - **Sự khác biệt**: Không chỉ đọc file, AI có thể thao tác trực tiếp trên giao diện Excel của bạn thông qua công nghệ Windows COM.

2. **📝 Office Master (Trợ lý Soạn thảo Word/PPT)**
   - **Tính năng**: Tạo mới, chỉnh sửa, định dạng văn bản Word (đáp ứng chuẩn hành chính như Nghị định 30); điền dữ liệu vào Template (Hợp đồng, Quyết định); tự động thiết kế Slide PowerPoint theo Brand Guidelines.
   - **Sự khác biệt**: Hiểu cách căn lề, phối màu, bảo toàn format gốc khi thay nội dung.

3. **📧 Outlook Agent (Trợ lý Email & Lịch họp)**
   - **Tính năng**: Kết nối trực tiếp vào Microsoft Outlook để quét, đọc email, tải file đính kèm, phân loại email, tự động soạn/trả lời thư, và thiết lập lịch họp (Calendar).
   - **Sự khác biệt**: Trích xuất nội dung từ hàng loạt email để làm báo cáo mà không cần bạn phải mở từng thư.

4. **🌐 Web Researcher (Trợ lý Trình duyệt)**
   - **Tính năng**: Tự động mở trình duyệt (Edge/Chrome), điều hướng, nhấp chuột, trích xuất văn bản và bảng biểu từ trang web. Chụp ảnh màn hình làm bằng chứng.
   - **Sự khác biệt**: Mô phỏng thao tác của con người qua UI Automation, không bị chặn bởi các trang web cấm tool tự động.

5. **📑 PDF & File Scanner (Trợ lý Tài liệu)**
   - **Tính năng**: Trích xuất dữ liệu, bảng biểu từ file PDF; gộp/tách trang PDF; OCR nhận diện chữ trong ảnh scan; quét hàng loạt file trong hệ thống thư mục.

---

## 💼 Các User Cases Cụ Thể Trong Thực Tế

Dưới đây là cách bạn có thể kết hợp các tính năng trên để giải quyết các công việc lặp đi lặp lại hàng ngày. Hãy nhập các yêu cầu tương tự vào khung chat của Office Hub.

### 📌 Use Case 1: Tự động làm Báo cáo Doanh thu hàng tuần
- **Vấn đề**: Hàng tuần, bạn nhận được 10 file Excel báo cáo từ các chi nhánh qua email, cần tổng hợp thành 1 file Excel và làm 1 slide PowerPoint báo cáo cho sếp.
- **Cách dùng Office Hub**:
  1. *"Quét email trong Outlook tuần này có tiêu đề 'Báo cáo chi nhánh', tải tất cả file đính kèm lưu vào thư mục 'Báo Cáo'."* (Outlook Agent)
  2. *"Gộp tất cả dữ liệu từ các file Excel trong thư mục 'Báo Cáo' thành một bảng duy nhất, tính tổng doanh thu theo từng khu vực."* (Analyst Agent)
  3. *"Từ số liệu tổng hợp, tạo một file PowerPoint 3 slide báo cáo kết quả doanh thu, sử dụng màu logo công ty."* (Office Master)

### 📌 Use Case 2: Soạn hàng loạt Hợp đồng / Giấy tờ nhân sự
- **Vấn đề**: Bộ phận HR cần soạn 50 hợp đồng lao động mới dựa trên danh sách Excel chứa thông tin nhân viên, sử dụng một mẫu hợp đồng Word có sẵn.
- **Cách dùng Office Hub**:
  - *"Sử dụng danh sách trong file Nhan_vien_moi.xlsx, lấy các cột Họ Tên, CMND, Lương Cơ Bản để điền vào file template_hop_dong.docx. Tạo ra 50 file Word riêng biệt và lưu ở thư mục 'Hop Dong 2026'."* (Sự kết hợp giữa Analyst Agent và Office Master)

### 📌 Use Case 3: Thu thập Dữ liệu Đối thủ & Báo giá
- **Vấn đề**: Bạn cần theo dõi tỷ giá ngoại tệ, giá vàng, hoặc giá sản phẩm của đối thủ trên website của họ mỗi buổi sáng để làm file báo giá gửi khách.
- **Cách dùng Office Hub**:
  1. *"Vào website [địa_chỉ_web], trích xuất bảng giá tỷ giá hôm nay."* (Web Researcher)
  2. *"Cập nhật cột 'Tỷ giá' trong file Bao_gia.xlsx theo số liệu vừa lấy được."* (Analyst Agent)
  3. *"Soạn một email gửi cho danh sách khách hàng VIP trong Outlook, đính kèm file Bao_gia.xlsx vừa cập nhật."* (Outlook Agent)

### 📌 Use Case 4: Xử lý và số hoá Hoá đơn, Hợp đồng PDF
- **Vấn đề**: Nhận được nhiều file PDF hợp đồng scan, cần lấy thông tin số tiền và tên đối tác để nhập vào bảng theo dõi.
- **Cách dùng Office Hub**:
  - *"Đọc 5 file PDF hoá đơn trong thư mục 'Hoa_don_thang_10', trích xuất tên công ty, ngày tháng, và tổng tiền thanh toán, sau đó xuất ra một file Excel."* (PDF Processing + Analyst Agent)

### 📌 Use Case 5: Tự động lọc Email và lên Lịch họp
- **Vấn đề**: Mở máy tính vào buổi sáng, có hàng chục email chờ xử lý, bạn không biết việc nào cần ưu tiên.
- **Cách dùng Office Hub**:
  - *"Hãy đọc các email chưa xem từ Sếp và phòng Kế toán. Tóm tắt nội dung chính. Nếu email nào có yêu cầu họp, hãy tự động tạo lịch họp trong Calendar Outlook và mời các thành viên liên quan."* (Outlook Agent)

---

## 🔒 Cơ Chế An Toàn & Phê Duyệt

Office Hub được thiết kế để giữ quyền kiểm soát tối đa cho bạn:
- **Tạo bản sao lưu**: Mọi thao tác ghi đè lên file Word/Excel gốc đều được hệ thống tự động backup.
- **Human-in-the-Loop (Chờ duyệt)**: Khi AI định thực hiện các thao tác nhạy cảm (như Bấm nút gửi Email, Xoá file, Xác nhận điền form trên web), yêu cầu sẽ được gửi tới Ứng dụng Điện thoại (Mobile App) của bạn. Hệ thống chỉ chạy tiếp khi bạn bấm **"Phê duyệt"**.
- **Chụp ảnh màn hình**: Web Researcher luôn lưu lại ảnh chụp màn hình bước cuối cùng để bạn kiểm chứng số liệu AI lấy về có đúng với mắt thường nhìn thấy không.

---
*Office Hub - Trợ lý thông minh giải phóng bạn khỏi các rắc rối thủ công, để bạn tập trung vào những quyết định quan trọng!*
