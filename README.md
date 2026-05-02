# 🏢 Office Hub AI

[![Build Status](https://img.shields.io/badge/Build-Passing-brightgreen.svg)]()
[![Version: v1.0.0](https://img.shields.io/badge/Version-v1.0.0-success.svg)](CHANGELOG.md)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-v2-blue)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.80+-orange)](https://www.rust-lang.org)

*(Tiếng Việt ở bên dưới / Vietnamese below)*

---

## 🇬🇧 English

> **Office Hub AI** — A lightweight, agentic overlay deeply integrated into Microsoft Office. It automates workflows from Web to Office via a multi-agent orchestration architecture and an independent Model Context Protocol (MCP) ecosystem.

Office Hub is an open-source Desktop and Mobile companion application that brings autonomous AI agents to your local machine, allowing you to manipulate Excel spreadsheets, draft Word documents, analyze data with Polars, and conduct web research—all locally without exposing your sensitive documents to untrusted 3rd party clouds.

### 📸 System Screenshots

#### Desktop Interface (Tauri + React)
![Desktop App Interface](docs/assets/desktop_ui.png)
*Modern dark-mode multi-agent chat interface with real-time memory tracking and status metrics.*

#### Mobile Companion App (React Native)
![Mobile App Interface](docs/assets/mobile_ui.png)
*Remote dashboard connecting securely to your desktop agents via local network.*

### ✨ Key Features for Users
- **Multi-Agent Orchestrator:** An advanced AI ecosystem (Web Researcher, Office Master, Analyst) collaborating autonomously via Model Context Protocol (MCP).
- **Office Mastery:** Automatically generates, edits, and extracts data from Word documents, Excel spreadsheets, and PowerPoint presentations via native Win32 COM and Office.js Add-ins.
- **Analytic & Chart Engine:** Blazing-fast data processing using **Polars SQL**, capable of transforming millions of rows and rendering dynamic ECharts automatically.
- **Web Researcher:** Deep web searching and visual layout extraction using an integrated headless browser engine (`obscura`).
- **Mobile Companion:** Real-time remote control of your Desktop agents via the Office Hub Mobile app using secure SSE + REST architecture.
- **Workspace Isolation:** Your chats, context histories, and files are completely isolated by project workspace to ensure absolute data privacy.

### 🛠️ Full Tech Stack

Office Hub utilizes a bleeding-edge technology stack optimized for performance, security, and developer experience:

- **Core Backend:** Rust, Tauri v2, Axum (HTTP/SSE Server), Tokio (Async runtime), Polars (Dataframe), Reqwest, Rusqlite
- **Desktop Frontend:** React 18, TypeScript, Vite, Tailwind CSS v3, Zustand (State Management), ECharts, React Flow
- **Mobile Companion:** React Native, Expo, Zustand, React Navigation
- **Office Integration:** Microsoft Win32 COM APIs, Office.js Web Add-ins (React + Vite)
- **AI Infrastructure:** Model Context Protocol (MCP), GenAI framework (with multi-provider support: Gemini, OpenAI, Anthropic, Local Ollama)

### 🚀 Getting Started (End Users / No Code Required)

With just 5 simple steps, you can install and use Office Hub immediately without any programming knowledge:

1. **Download the Software:**
   - Go to the [Releases page](https://github.com/cuongdm75/office_hub/releases/latest).
   - Download the `OfficeHub-Setup.exe` file (for Windows Desktop) and `OfficeHub.apk` (if you use an Android phone).

2. **Install on Desktop:**
   - Double-click the downloaded `OfficeHub-Setup.exe` file to install it like any regular software.
   - Open the Office Hub AI app from your Desktop.

3. **Configure Artificial Intelligence (AI):**
   - On the first launch, switch to the **Settings** tab (gear icon).
   - Enter the API Key of the AI you want to use (e.g., Google Gemini, OpenAI, or connect Local Ollama if available).

4. **Integrate with Word / Excel (Install Add-in):**
   - Open Microsoft Word or Excel on your computer.
   - Click the **Insert** tab -> **Get Add-ins** -> **My Add-ins**.
   - Select **Upload My Add-in**.
   - Choose the `manifest.xml` file (included in the release package downloaded in Step 1).
   - The Office Hub window will instantly appear on the right side of your Word/Excel screen!

5. **Connect Mobile Companion App:**
   - Install the `OfficeHub.apk` file on your Android phone.
   - Open the Office Hub app on your phone.
   - On the desktop app, click the **QR Code** button to scan the code, or directly enter your computer's **IP Address** into the phone to connect. You can now control the AI remotely!

---

### 💻 Developer Guide (Open Source)

We welcome contributions! Please review the tech stack above before starting.

#### Prerequisites
- **OS:** Windows 10/11 (64-bit) is required for COM automation and Office Add-in features.
- **Rust:** 1.80+ (`rustup update stable`)
- **Node.js:** 20+
- **Microsoft Office:** 2016 / 2019 / 2021 / 365 (Developer Registry enabled)

#### Local Setup & Development

1. **Clone the repository:**
   ```powershell
   git clone https://github.com/cuongdm75/office_hub.git
   cd office_hub
   ```

2. **Install Dependencies:**
   ```powershell
   npm install
   cd office-addin && npm install
   cd ../mobile && npm install
   ```

3. **Run the Application:**
   To start the full stack (Tauri Desktop App + Backend Server + Office Add-in Dev Server):
   ```powershell
   .\Start-OfficeHub.ps1
   ```
   *Note: This script will automatically install local HTTPS certificates required by Office Add-ins.*

4. **Build for Production:**
   ```powershell
   # Build Desktop (.exe / .msi)
   npm run tauri:build

   # Build Mobile (.apk)
   cd mobile
   npx eas-cli build -p android --profile preview
   ```

#### Project Structure
- `src-tauri/`: Rust backend, Agent Orchestrator, LLM Gateway, Axum servers.
- `src/`: Tauri React Frontend (Dashboard, Chat UI).
- `office-addin/`: Office Web Add-in for seamless Word/Excel/PPT extraction.
- `mobile/`: React Native mobile companion app.
- `mcp-servers/`: Independent Python/Rust servers communicating via MCP Protocol.

#### Extensibility & Plugins (MCP Servers)
Office Hub AI is designed to be highly extensible via the **Model Context Protocol (MCP)**. This allows developers to add new capabilities, connect to internal APIs, or write custom automation scripts without modifying the core Rust backend.

**1. Building an MCP Server:**
You can write an MCP server in any language (Python, TypeScript, Rust, Go). The server exposes `tools` (functions) and `resources` (data) over standard input/output (stdio) or HTTP/SSE.
- Example: Create a Python script that provides a tool to fetch data from your company's internal CRM.
- Use the official [MCP SDKs](https://modelcontextprotocol.io) to speed up development.

**2. Registering your Plugin:**
Once your MCP server is ready, register it in the Office Hub configuration (e.g., `config.yaml`):
```yaml
mcp_servers:
  my-crm-plugin:
    command: "python"
    args: ["/path/to/my_crm_server.py"]
```

**3. Custom Workflows & Agents:**
For deeper integration, you can define new Agents inside `src-tauri/src/agents/`. The Orchestrator will automatically route user intents to your custom Agent based on the Rules Engine defined in `rule_engine.rs`.

---
---

## 🇻🇳 Tiếng Việt

> **Office Hub AI** — Trợ lý AI siêu nhẹ, tích hợp sâu vào Microsoft Office, tự động hóa quy trình từ Web đến Office thông qua kiến trúc đa Agent điều phối và hệ sinh thái MCP (Model Context Protocol) độc lập.

Office Hub là dự án mã nguồn mở (Open Source) bao gồm ứng dụng Desktop và Mobile, mang sức mạnh của AI Agents xuống máy tính cá nhân của bạn. Ứng dụng có thể tự động xử lý bảng tính Excel, soạn thảo Word, phân tích dữ liệu với Polars, và nghiên cứu Web—tất cả diễn ra cục bộ, đảm bảo tính bảo mật dữ liệu tuyệt đối.

### 📸 Ảnh chụp màn hình hệ thống

#### Giao diện Ứng dụng Desktop (Tauri + React)
![Desktop App Interface](docs/assets/desktop_ui.png)
*Giao diện Dark Mode hiện đại, tích hợp chat đa luồng Agent cùng trình theo dõi bộ nhớ theo thời gian thực.*

#### Giao diện Ứng dụng Mobile (React Native)
![Mobile App Interface](docs/assets/mobile_ui.png)
*Ứng dụng Mobile đồng hành kết nối bảo mật qua mạng LAN để điều khiển và giám sát tiến trình hệ thống từ xa.*

### ✨ Tính năng nổi bật
- **Hệ thống Multi-Agent:** Kiến trúc điều phối đa đặc vụ (Web Researcher, Office Master, Analyst) phối hợp tự chủ thông qua giao thức MCP.
- **Tự động hoá Office Mastery:** Tự động tạo, sửa đổi, định dạng và trích xuất dữ liệu từ Word, Excel, PowerPoint thông qua công nghệ Win32 COM và Office.js Add-ins.
- **Analytic & Chart Engine:** Công cụ phân tích và biểu diễn dữ liệu bằng **Polars SQL**, tốc độ cực nhanh cho hàng triệu dòng, tự động trích xuất các biểu đồ ECharts.
- **Web Researcher:** Máy dò tìm thông tin và nghiên cứu web chuyên sâu tích hợp engine trình duyệt ẩn danh (`obscura`).
- **Mobile Companion:** Điều khiển AI Agents từ xa theo thời gian thực thông qua ứng dụng Mobile bằng kiến trúc bảo mật SSE + REST.
- **Workspace Isolation:** Cách ly tuyệt đối không gian làm việc. Ngữ cảnh, lịch sử chat và các tài liệu được khoanh vùng độc lập theo từng dự án.

### 🛠️ Công nghệ sử dụng (Tech Stack)

Office Hub được xây dựng dựa trên các công nghệ tiên tiến nhất về hiệu năng và bảo mật:

- **Core Backend:** Rust, Tauri v2, Axum (HTTP/SSE Server), Tokio (Async runtime), Polars (Dataframe), Reqwest, Rusqlite
- **Desktop Frontend:** React 18, TypeScript, Vite, Tailwind CSS v3, Zustand, ECharts, React Flow
- **Mobile Companion:** React Native, Expo, Zustand, React Navigation
- **Office Integration:** Microsoft Win32 COM APIs, Office.js Web Add-ins (React + Vite)
- **AI Infrastructure:** Model Context Protocol (MCP), Framework GenAI (Hỗ trợ đa LLM: Gemini, OpenAI, Anthropic, Ollama cục bộ)

### 🚀 Hướng dẫn cài đặt cho người dùng (Không cần lập trình)

Chỉ với 5 bước đơn giản, bạn có thể cài đặt và sử dụng Office Hub ngay lập tức mà không cần biết viết code:

1. **Tải phần mềm (Download):**
   - Truy cập vào trang [Releases (Phát hành mới nhất)](https://github.com/cuongdm75/office_hub/releases/latest).
   - Tải file `OfficeHub-Setup.exe` (cho máy tính Windows) và `OfficeHub.apk` (nếu bạn dùng điện thoại Android).

2. **Cài đặt trên Máy tính (Desktop App):**
   - Nháy đúp vào file `OfficeHub-Setup.exe` vừa tải về để cài đặt như các phần mềm thông thường.
   - Mở ứng dụng Office Hub AI từ màn hình Desktop của bạn.

3. **Cấu hình Trí tuệ nhân tạo (AI):**
   - Trong lần đầu mở ứng dụng, hãy chuyển sang tab **Cài đặt (Settings)** (biểu tượng bánh răng).
   - Điền API Key của AI mà bạn muốn sử dụng (ví dụ: Google Gemini, OpenAI, hoặc kết nối Local Ollama nếu có).

4. **Tích hợp vào Word / Excel (Cài đặt Add-in):**
   - Mở phần mềm Microsoft Word hoặc Excel trên máy tính của bạn.
   - Bấm vào tab **Insert (Chèn)** -> **Get Add-ins (Tải Add-in)** -> **My Add-ins (Add-in của tôi)**.
   - Chọn **Upload My Add-in (Tải Add-in của tôi lên)**.
   - Chọn file `manifest.xml` (file này đính kèm cùng bộ cài đặt tải về ở Bước 1).
   - Cửa sổ Office Hub sẽ lập tức xuất hiện bên phải màn hình Word/Excel của bạn!

5. **Kết nối Ứng dụng Điện thoại (Mobile Companion):**
   - Cài đặt file `OfficeHub.apk` vào điện thoại Android của bạn.
   - Mở ứng dụng Office Hub trên điện thoại.
   - Trên ứng dụng máy tính, nhấn vào nút **Mã QR** để quét mã, hoặc nhập trực tiếp **Địa chỉ IP** của máy tính vào điện thoại để kết nối. Giờ đây bạn có thể điều khiển AI từ xa!

---

### 💻 Hướng dẫn cho Lập trình viên (Developer Guide)

Chúng tôi hoan nghênh mọi sự đóng góp mã nguồn! 

#### Yêu cầu hệ thống
- **OS:** Bắt buộc dùng Windows 10/11 (64-bit) để hỗ trợ giao thức COM và Office Add-in.
- **Rust:** 1.80+ (`rustup update stable`)
- **Node.js:** 20+
- **Microsoft Office:** Phiên bản 2016 / 2019 / 2021 / 365 (Yêu cầu bật Developer Registry cho Add-in)

#### Cài đặt và Chạy môi trường Dev

1. **Clone mã nguồn:**
   ```powershell
   git clone https://github.com/cuongdm75/office_hub.git
   cd office_hub
   ```

2. **Cài đặt Dependencies:**
   ```powershell
   npm install
   cd office-addin && npm install
   cd ../mobile && npm install
   ```

3. **Chạy ứng dụng (Development):**
   Để khởi động toàn bộ hệ thống (Tauri Desktop App + Backend Server + Office Add-in Dev Server):
   ```powershell
   .\Start-OfficeHub.ps1
   ```
   *Lưu ý: Script này sẽ tự động cài đặt chứng chỉ HTTPS cục bộ (localhost) bắt buộc cho Office Add-in.*

4. **Build bản chính thức (Production):**
   ```powershell
   # Build Desktop (.exe / .msi)
   npm run tauri:build

   # Build Mobile (.apk)
   cd mobile
   npx eas-cli build -p android --profile preview
   ```

#### Cấu trúc dự án
- `src-tauri/`: Rust backend, bộ điều phối Agent Orchestrator, LLM Gateway, Axum servers.
- `src/`: React Frontend cho app Desktop (Dashboard, Chat UI).
- `office-addin/`: Web Add-in chạy ngầm trong Word/Excel/PPT để đọc xuất dữ liệu.
- `mobile/`: Ứng dụng điện thoại React Native điều khiển từ xa.
- `mcp-servers/`: Các server độc lập (Python/Rust) giao tiếp qua chuẩn MCP (Model Context Protocol).

#### Khả năng mở rộng & Phát triển Plugin (MCP Servers)
Office Hub AI được thiết kế với kiến trúc mở thông qua giao thức **MCP (Model Context Protocol)**. Lập trình viên có thể dễ dàng thêm các tính năng mới, kết nối với API nội bộ của công ty, hoặc viết các kịch bản tự động hóa riêng mà không cần can thiệp vào lõi Backend Rust.

**1. Xây dựng một MCP Server (Plugin):**
Bạn có thể viết MCP Server bằng bất kỳ ngôn ngữ nào (Python, TypeScript, Rust, Go). Server này sẽ cung cấp các `tools` (công cụ/hàm) và `resources` (tài nguyên/dữ liệu) thông qua chuẩn giao tiếp stdio hoặc HTTP/SSE.
- Ví dụ: Viết một script Python chứa một `tool` để tự động lấy báo cáo từ hệ thống CRM nội bộ.
- Sử dụng các [MCP SDKs chính thức](https://modelcontextprotocol.io) để phát triển nhanh chóng.

**2. Đăng ký Plugin vào Office Hub:**
Sau khi hoàn thiện MCP Server, bạn chỉ cần khai báo nó vào cấu hình của Office Hub (ví dụ: `config.yaml`):
```yaml
mcp_servers:
  plugin-crm-noi-bo:
    command: "python"
    args: ["/duong/dan/toi/crm_server.py"]
```
Ngay sau khi khởi động lại, các Agent của Office Hub sẽ tự động "hiểu" và biết cách gọi công cụ CRM của bạn khi người dùng yêu cầu.

**3. Tích hợp Workflows & Agents chuyên sâu:**
Nếu bạn muốn xây dựng một đặc vụ (Agent) hoàn toàn mới với logic phức tạp, bạn có thể định nghĩa Agent đó tại thư mục `src-tauri/src/agents/`. Bộ não trung tâm (Orchestrator) sẽ sử dụng `rule_engine.rs` để phân tích ngữ nghĩa câu lệnh của người dùng và tự động định tuyến (route) đến Agent mới của bạn.

---
*Bản quyền thuộc về những người đóng góp cho dự án Office Hub AI (MIT License).*