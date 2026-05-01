# Báo cáo Tổng hợp và Kế hoạch Nâng cấp Mobile App

## 1. Tổng kết những công việc đã hoàn thành (Wrap-up)

Đến thời điểm hiện tại, hệ thống đã trải qua một đợt tái cấu trúc lớn từ phía backend (Tauri) đến giao thức giao tiếp với Mobile:

### Giao thức & Kết nối (Mobile-Desktop)
- **Hoàn tất chuyển đổi WebSocket sang SSE + REST:** Loại bỏ sự phụ thuộc vào WebSocket, thay thế bằng kiến trúc lai (Hybrid) MCP-compliant. Desktop đóng vai trò là server (port 9002) phát SSE stream và cung cấp REST endpoints.
- **Xử lý Timeout & Ổn định mạng:** Đã điều chỉnh cơ chế backoff và timeout (8s) cho `react-native-sse` để thiết bị mobile không bị rơi vào vòng lặp reconnect liên tục khi thay đổi mạng.

### Kiến trúc Orchestrator (Backend)
- **Chuyển đổi Push sang Pull (Internal MCP Broker):** Thay vì nhồi nhét toàn bộ Knowledge, Policy, và Memory vào System Prompt của LLM, Orchestrator hiện sử dụng `McpBroker` nội bộ.
- **Auto-Routing LLM:** Triển khai hệ thống fallback linh hoạt (Subscription → Cheap → Free), hỗ trợ tự động nhận diện Ollama models nội bộ và route lệnh đến LLM khả dụng mà không gây gián đoạn.

### Mobile UI & Artifacts
- **Đồng bộ hóa trạng thái "Thinking":** Ứng dụng mobile đã có thể hiển thị real-time tiến trình suy nghĩ của LLM thông qua các event `ChatProgress`.
- **Chuẩn hóa URI File:** Mobile app hiện nhận được các đường dẫn file chuẩn MCP (`office-hub://files/*`) thay vì đường dẫn vật lý nội bộ của Desktop, bước đầu dọn đường cho việc tải file an toàn.

---

## 2. Kế hoạch nâng cấp (Mục tiêu: Sửa lỗi, Tốc độ, Không delay)

Để đạt được mục tiêu giao tiếp thông suốt, không delay và truyền file siêu tốc, chúng ta cần thực hiện các hạng mục sau:

### Phase 1: Sửa lỗi triệt để Mobile App (Bug Fixes)
- **Vấn đề treo UI (UI Hangs):** Hiện tại nếu Orchestrator gặp lỗi khi gọi Tool hoặc timeout LLM, vòng lặp chat không phát ra event kết thúc. 
  - *Giải pháp:* Bắt buộc bắt mọi `Result::Err` trong `orchestrator.process_message` và phát `SseEvent::result` với nội dung lỗi cụ thể. Mobile app nhận được sẽ tắt loading spinner và hiển thị Error Banner.
- **Khắc phục lỗi URI File (Cross-platform):** File trả về dạng `office-hub://files/...` phải được intercept thành công trên React Native để hiển thị/tải xuống được, tránh lỗi "File not found" trên Android/iOS.
- **Khắc phục Reconnect Loop:** Tối ưu hóa lại thư viện quản lý kết nối (SSE Client) trên Mobile để tự động resume session thay vì start báo lỗi.

### Phase 2: Tăng tốc độ truyền tải file (High-Speed File Transfer)
- **Tách biệt Data Plane và Control Plane:** Hiện tại file có thể đang bị encode base64 qua SSE/WebSocket gây phình to và chậm.
  - *Giải pháp:* Thiết lập 1 endpoint REST riêng biệt, siêu nhanh (`GET /api/v1/files/download`) stream file nhị phân trực tiếp xuống thiết bị. HTTP stream giúp tối đa hóa băng thông của mạng LAN.
- **Chunking / Multipart Upload:** Khi Mobile gửi file lên Desktop (ảnh, tài liệu), sử dụng chuẩn `multipart/form-data` thay vì base64 payload trong body JSON. Tốc độ sẽ tăng gấp nhiều lần.
- **Cache tĩnh:** Cấu hình Cache Control cho các hình ảnh/artifact đã gửi qua lại để không phải tải lại liên tục.

### Phase 3: Giao tiếp thông suốt, Không Delay (Zero-Delay Comm)
- **Non-blocking SSE Dispatch:** Phía Tauri (Rust), các hàm gửi event SSE hiện có thể đang bị chặn (block) trong lúc gọi `tokio::fs` hoặc `LLM Gateway`. 
  - *Giải pháp:* Bọc tất cả quá trình xử lý LLM vào `tokio::spawn`, để channel (mpsc) nhận và phát event ra SSE server không bao giờ bị nghẽn (non-blocking async event delivery).
- **Giảm tải Re-render UI React Native:** Lọc và gộp (batch) các sự kiện `ChatProgress` bắn từ Desktop. Thay vì re-render liên tục mỗi milisecond khi có token mới, ta dùng requestAnimationFrame hoặc throttle (100ms) để UI mobile mượt mà, không giật lag.

---

## 3. Các bước thực thi kế tiếp

1. **Review & Chốt kế hoạch:** Xác nhận với bạn (User) xem hướng đi trên đã đúng ý và bao quát đủ chưa.
2. **Triển khai Phase 1:** Bắt đầu sửa backend `sse_server.rs` và `orchestrator/mod.rs` để xử lý triệt để luồng Error (chống treo UI) và cơ chế Non-blocking channel.
3. **Triển khai Phase 2:** Bổ sung API `GET/POST` cho file upload/download tại `sse_server.rs` (như 1 server HTTP độc lập).
4. **Triển khai Phase 3:** Chỉnh sửa file `ChatScreen.tsx` trên mobile để tích hợp batching render và sửa logic phân giải URI `office-hub://files/*`.
