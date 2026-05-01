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

### ✨ Key Features for Users
- **Office Mastery:** Automatically generates and edits Word documents, Excel spreadsheets, and PowerPoint presentations via native COM and Add-ins.
- **Web Researcher:** Deep web searching and content extraction without lifting a finger.
- **Analytic & Chart Engine:** Blazing-fast data processing using Polars SQL, capable of rendering charts automatically.
- **Mobile Companion:** Real-time remote control of your Desktop agents via the Office Hub Mobile app using SSE + REST.
- **Workspace Isolation:** Your chats, histories, and files are completely isolated by project workspace to ensure data privacy.

### 🚀 Getting Started (End Users)

To use Office Hub immediately without compiling code:

1. **Download the Release:** Go to the [Releases page]() and download `OfficeHub-Setup.exe` (Desktop) and `OfficeHub.apk` (Android).
2. **Install Desktop App:** Run the `.exe` installer. 
3. **Configure LLM:** On the first launch, go to the Settings tab to enter your API Keys (Gemini, OpenAI, or Local Ollama).
4. **Install Office Add-ins:** 
   - Open Word/Excel -> `Insert` -> `Add-ins` -> `My Add-ins` -> `Upload My Add-in`.
   - Select the `manifest.xml` provided in the release package.
5. **Connect Mobile:** Open the Mobile App, enter your Desktop IP address and pair it using the QR code.

---

### 💻 Developer Guide (Open Source)

We welcome contributions! Office Hub is built on a modern stack:
- **Backend:** Rust + Tauri v2 + Axum
- **Frontend (Desktop):** React + TypeScript + Vite + TailwindCSS
- **Frontend (Mobile):** React Native (Expo)
- **Office Add-in:** React + Office.js

#### Prerequisites
- **OS:** Windows 10/11 (64-bit) is required for COM automation and Office Add-in features.
- **Rust:** 1.80+ (`rustup update stable`)
- **Node.js:** 20+
- **Microsoft Office:** 2016 / 2019 / 2021 / 365 (Developer Registry enabled)

#### Local Setup & Development

1. **Clone the repository:**
   ```powershell
   git clone https://github.com/your-org/office-hub.git
   cd office-hub
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
   npm run tauri build

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

---

## 🇻🇳 Tiếng Việt

> **Office Hub AI** — Trợ lý AI siêu nhẹ, tích hợp sâu vào Microsoft Office, tự động hóa quy trình từ Web đến Office thông qua kiến trúc đa Agent điều phối và hệ sinh thái MCP độc lập.

Office Hub là dự án mã nguồn mở (Open Source) bao gồm ứng dụng Desktop và Mobile, mang sức mạnh của AI Agents xuống máy tính cá nhân của bạn. Ứng dụng có thể tự động xử lý bảng tính Excel, soạn thảo Word, phân tích dữ liệu với Polars, và nghiên cứu Web—tất cả diễn ra cục bộ, đảm bảo tính bảo mật dữ liệu tuyệt đối.

### ✨ Tính năng chính (Dành cho Người dùng)
- **Office Mastery:** Tự động tạo, sửa đổi và định dạng Word, Excel, PowerPoint qua chuẩn COM và Office Add-in.
- **Web Researcher:** Nghiên cứu Web, tìm kiếm thông tin và trích xuất dữ liệu hoàn toàn tự động.
- **Analytic & Chart Engine:** Xử lý hàng triệu dòng dữ liệu siêu tốc bằng Polars SQL và tự động vẽ biểu đồ trực quan.
- **Mobile Companion:** Điều khiển Desktop AI từ xa theo thời gian thực qua app Mobile (kiến trúc SSE + REST).
- **Workspace Isolation:** Cách ly không gian làm việc. Ngữ cảnh, lịch sử chat và tài liệu được khoanh vùng độc lập theo từng dự án.

### 🚀 Bắt đầu sử dụng (End Users)

Để sử dụng Office Hub ngay lập tức mà không cần code:

1. **Tải phần mềm:** Truy cập trang [Releases]() và tải file `OfficeHub-Setup.exe` (cho Desktop) và `OfficeHub.apk` (cho Android).
2. **Cài đặt Desktop:** Chạy file `.exe` để cài đặt phần mềm.
3. **Cấu hình LLM:** Trong lần chạy đầu tiên, vào mục Settings để nhập API Key (Gemini, OpenAI, hoặc Ollama).
4. **Cài đặt Office Add-in:** 
   - Mở Word/Excel -> `Insert` (Chèn) -> `Add-ins` -> `My Add-ins` -> `Upload My Add-in`.
   - Chọn file `manifest.xml` đi kèm trong gói cài đặt.
5. **Kết nối Mobile:** Mở app trên điện thoại, nhập địa chỉ IP của máy tính (hoặc quét mã QR) để điều khiển từ xa.

---

### 💻 Hướng dẫn cho Lập trình viên (Developer Guide)

Chúng tôi hoan nghênh mọi sự đóng góp! Office Hub được xây dựng trên công nghệ hiện đại:
- **Backend:** Rust + Tauri v2 + Axum
- **Frontend (Desktop):** React + TypeScript + Vite + TailwindCSS
- **Frontend (Mobile):** React Native (Expo)
- **Office Add-in:** React + Office.js

#### Yêu cầu hệ thống
- **OS:** Bắt buộc dùng Windows 10/11 (64-bit) để hỗ trợ giao thức COM và Office Add-in.
- **Rust:** 1.80+ (`rustup update stable`)
- **Node.js:** 20+
- **Microsoft Office:** 2016 / 2019 / 2021 / 365 (Yêu cầu bật Developer Registry cho Add-in)

#### Cài đặt và Chạy môi trường Dev

1. **Clone mã nguồn:**
   ```powershell
   git clone https://github.com/your-org/office-hub.git
   cd office-hub
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
   npm run tauri build

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

---
*Bản quyền thuộc về những người đóng góp cho dự án Office Hub AI (MIT License).*