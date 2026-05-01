# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-05-01

### Added
- **Native GenAI Migration**: Transitioned from legacy context injection to a robust MCP-broker-based orchestration.
- **Plotter Dashboards**: Introduced a React-based interactive dashboard for advanced data visualization.
- **Native Office Automation**: Integrated local COM-based automation for Word, Excel, and PowerPoint (supports picture insertion, document manipulation).
- **Mobile Reconnection**: Persistent LLM sessions on the mobile app via `AsyncStorage`.
- **LLM Gateway Expansion**: Full support for OpenAI, Anthropic, Gemini, Ollama, LMStudio, and Z.ai with 9Router-style fallback routing.
- **Telemetry & Monitoring**: Advanced DAG-based monitoring UI on the Desktop application.
- **Workspace Isolation**: Strict cross-workspace isolation enforcing boundary access and local telemetry DB mapping.

### Fixed
- Addressed mobile UI hang bug related to missing SSE error event parsing.
- Resolved File URI schemes (`office-hub://files/`) to allow smooth file sharing and markdown rendering on mobile.
- Corrected backend payload memory pressure issues by utilizing bounded queues in the SSE multiplexer.

### Security
- Introduced system prompt isolation limits based on agent privileges.
- Upgraded Tailscale logic to handle local and mobile tunnel communication securely.
