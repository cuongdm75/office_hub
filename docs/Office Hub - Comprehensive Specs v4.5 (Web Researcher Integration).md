# **Office Hub \- Comprehensive Project Specifications v4.5**

**Version:** 4.5 (Extended with Web Researcher & UI Automation)  
**Focus:** Cross-app Automation, Deep Office Integration, Web Data Extraction, Native Performance

## **1\. Hệ thống Sub-Agents & MCP Servers (Updated)**

| Agent / Server | Vai trò và Kỹ năng (Skill Sets) |
| :---- | :---- |
| **Web Researcher Agent** | **Định nghĩa:** Agent chuyên trách truy xuất, trích xuất và phân tích dữ liệu từ các nguồn Web (Trình duyệt Edge, Chrome) hoặc Hidden WebView. **Cơ chế:** Sử dụng thư viện **Windows UI Automation (UIA)** để tương tác trực tiếp với giao diện trình duyệt mà không cần WebDriver. **Kỹ năng:** Trích xuất dữ liệu bảng (Table) và danh sách (List) từ các trang web đang mở. Tự động điều hướng và thu thập thông tin theo yêu cầu của Orchestrator. Chụp ảnh màn hình các vùng dữ liệu quan trọng để làm bằng chứng (Grounding). |
| **Orchestrator** | Điều phối trung tâm. Khi nhận yêu cầu cần dữ liệu thực tế (giá thị trường, tin tức, tài liệu web), Orchestrator sẽ gọi Web Researcher để lấy dữ liệu trước khi chuyển cho Analyst hoặc Office Master. |

## **2\. Cơ chế Cross-app Automation qua UI Automation (UIA)**

Hệ thống sử dụng thư viện tiêu chuẩn UIAutomationCore.dll của Windows để thực hiện kết nối liên ứng dụng:

* **Native Access:** Truy cập trực tiếp vào cây phân cấp giao diện (UI Tree) của các ứng dụng đang chạy (như Microsoft Edge) để lấy giá trị từ các element (Text, Button, List).  
* **Lightweight:** Không yêu cầu cài đặt Selenium, ChromeDriver hay các môi trường giả lập nặng nề. Chạy trực tiếp trên máy cấu hình thấp thông qua Go/Rust wrapper.  
* **Bridge to Office:** Dữ liệu sau khi Web Researcher lấy về sẽ được chuẩn hóa thành JSON để các Office Agents ghi thẳng vào Word/Excel/PPTX qua COM Automation.

## **3\. Workflow Mẫu: Web-to-Office Automation**

1. **Trigger:** Email hoặc lệnh thoại yêu cầu: "Lấy báo giá xăng dầu hôm nay và cập nhật vào file Báo cáo tuần".  
2. **Web Extraction:** **Web Researcher** sử dụng UIA để mở trình duyệt Edge, truy cập trang nguồn, và "đọc" các ô dữ liệu giá.  
3. **Processing:** **Analyst Agent** nhận dữ liệu thô, thực hiện tính toán hoặc so sánh với dữ liệu cũ.  
4. **Finalizing:** **Office Master** mở file Word báo cáo, tìm đến bảng tương ứng và cập nhật số liệu mới, đảm bảo giữ nguyên format hành chính.

## **4\. Quy tắc an toàn và Xác thực (Security)**

* Mọi hành động điều khiển trình duyệt qua UIA phải được ghi log rõ ràng.  
* Các tác vụ nhạy cảm (điền form, thanh toán) bắt buộc phải có sự phê duyệt của người dùng qua Mobile App hoặc xác nhận trên PC (Human-in-the-loop).

---

*Tài liệu v4.5 này bổ sung năng lực Cross-app cho Office Hub, biến nó thành một trung tâm tự động hóa toàn diện từ Web đến Office.*