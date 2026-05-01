# **Office Hub \- Comprehensive Project Specifications v4.2**

**Version:** 4.2 (Final Master Edition)  
**Focus:** Orchestrated AI Ecosystem, Deep Office Integration, Event-Driven Workflow, Power User Capability

## **1\. Mục tiêu và Triết lý (Project Philosophy)**

* **Native Performance:** Ưu tiên Go (Wails) hoặc Rust (Tauri) để tạo file thực thi (.exe) siêu nhẹ, chạy trực tiếp không cần Python runtime. Tối ưu cho máy cấu hình thấp.  
* **Native Integration:** Sử dụng thư viện chuẩn (COM Automation) để điều khiển Microsoft Office có sẵn trên máy người dùng.  
* **Modular Intelligence:** Mở rộng qua Model Context Protocol (MCP).

## **2\. Kiến trúc Hệ thống (Core Architecture)**

Hệ thống chia làm 4 tầng logic:

* **App Shell:** Giao diện chính Office Hub, quản lý File Browser và Settings.  
* **Orchestrator:** Bộ não điều phối, quản lý Intent, Session State và MCP Host.  
* **LLM Gateway:** Cấu hình đa nguồn (Cloud API: Gemini/GPT; Local: Ollama/LM Studio). Hỗ trợ Hybrid Mode.  
* **Communication Layer:** WebSocket Server kết nối Mobile Client và Office In-app Chat Pane.

## **3\. Hệ thống Sub-Agents & MCP Servers**

| Agent / Server | Kỹ năng (Skill Sets) |
| :---- | :---- |
| **Orchestrator** | Điều phối đa nhiệm, quản lý Workflow, bảo vệ Quyền riêng tư và Rule Engine. |
| **Converter Agent** | Tự động học kỹ năng từ GitHub/Docs/Scripts và đóng gói thành MCP Servers. |
| **Analyst Agent (Excel)** | **Advanced Power User:** XLOOKUP, Dynamic Arrays, Power Query, VBA/Office Scripts, Audit công thức sống. |
| **Office Master (Word/PPT)** | **Advanced Word:** Styles, Sections, Cross-references. **Advanced PPT:** Grid System, Morph Transitions, Brand Palette. |

## **4\. Event-Driven Workflow Engine**

Hệ thống hỗ trợ tự động hóa quy trình (Trigger \- Action):

* **Triggers:** Email mới, file đính kèm, thay đổi file trong thư mục, hoặc lệnh thoại từ Mobile.  
* **Workflows:** Tự động trích xuất dữ liệu từ Email/Excel để tạo Báo cáo Word hoặc Slide PPT chuyên nghiệp.

## **5\. Cơ chế LLM Engine Configuration**

* **Cấu hình:** Người dùng nhập API Key (Cloud) hoặc trỏ tới Endpoint Local (Ollama).  
* **Tối ưu:** Token Caching và tóm tắt Session để tiết kiệm tài nguyên và chống Drift.

## **6\. Quy trình chống Ảo giác (Grounding & Verification)**

1. **Hybrid RAG:** Global Summary kết hợp Local Vector Search cho file dài.  
2. **Rule Engine:** Đối chiếu kết quả với file Rule (.yaml) trước khi ghi vào Office.  
3. **Hard-Truth Verification:** Đọc số liệu thực tế qua thư viện Native thay vì dựa hoàn toàn vào dự đoán của LLM.

## **7\. Khác biệt với Agentic OS**

Office Hub là một **Agentic Overlay**: Siêu nhẹ, tích hợp sâu vào ứng dụng hiện có, tập trung vào kỹ năng Office chuyên sâu và khả năng điều khiển Mobile từ xa linh hoạt hơn các hệ điều hành AI thuần túy.  
---

*Tài liệu v4.2 này là bản đặc tả tổng hợp cuối cùng cho dự án Office Hub.*