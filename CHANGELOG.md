# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-05-01

### Added
- **Embedded Add-in HTTPS Server**: The Office Web Add-in server (Word/Excel/PowerPoint) is now built into the Rust core and starts automatically with Office Hub — no separate `npm run dev` process required. Served via `axum-server` + `rustls` on `https://localhost:3000`.
- **Native GenAI Pipeline**: Transitioned from legacy context injection to a robust MCP-broker-based orchestration as the primary pipeline (`process_message_native`).
- **Internal MCP Servers (12+)**: PolicyServer, KnowledgeServer, MemoryServer, SkillServer, FileSystemServer, AnalyticServer (Polars SQL), OfficeComServer, Win32AdminServer, ScriptingServer (Rhai), ChartServer, WebSearchServer, WebFetchServer.
- **Native Chart Rendering**: `NativeChartServer` uses Plotters to render charts natively without a browser.
- **Native Office Automation**: COM-based automation for Word, Excel, and PowerPoint (document manipulation, picture insertion).
- **Multi-Agent Registry**: 8 registered agents — Analyst, OfficeMaster, WebResearcher, FolderScanner, Outlook, Converter, System, Win32Admin.
- **LLM Gateway**: 3-tier routing (fast/default/reasoning) with auto-failover across Gemini, OpenAI, Anthropic, Ollama, LMStudio, Z.ai.
- **Mobile App (React Native/Expo)**: 6-screen interface with SSE streaming, QR pairing, HITL approve/reject, and artifacts browsing.
- **DAG-based Agent Monitor**: Real-time orchestrator visualization on the desktop.
- **Workspace Isolation**: Strict per-workspace memory, policies, history, and file access boundaries.
- **CRDT Co-authoring**: Automerge-based real-time document collaboration.
- **SQLite FTS5 Long-term Memory**: Persistent cross-session memory with full-text search.
- **Workflow Engine**: YAML-driven trigger/action workflows with real-time progress events.
- **Tailscale Integration**: Automatic VPN tunnel detection for mobile connectivity.

### Fixed
- Mobile UI hang when LLM errors out — SSE error events now reliably emitted.
- File URI scheme (`office-hub://files/`) resolved to proper HTTP download endpoint on mobile.
- Bounded queues in SSE multiplexer to eliminate backend memory pressure.
- `Start-OfficeHub.ps1` now uses `Start-Process` instead of `Start-Job` for stable add-in server lifetime.
- CI test `test_hybrid_mode_fallback` — bypassed invalid TCP reachability check for mock providers.

### Security
- System prompt isolation based on agent privilege levels.
- Tailscale tunnel communication security for local and mobile clients.
- WebSocket auth token auto-generated and persisted to `config.yaml` on first run.
