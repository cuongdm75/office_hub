# Office Hub – Session Handoff Protocol & Final Decisions

**Document version:** 2.0  
**Status:** ✅ APPROVED – Ready for Implementation  
**Applies to:** All development sessions across the entire project lifecycle

---

## Part 1: Review Decisions (Final)

### Decisions Applied

| # | Decision | Value | Applied To |
|---|---------|-------|-----------|
| D1 | Mobile app framework | **Flutter** (Dart) | `MASTER_PLAN_AMENDMENT_v1.1.md` A1 |
| D2 | QR code expiry | **5 minutes** ✅ confirmed | `system/qrcode.rs` |
| D3 | Outlook send HITL level | **HIGH** ✅ confirmed | `agents/outlook/mod.rs` |
| D4 | Lock-screen default | **`true`** (agents stay active) | `system/mod.rs` `SystemConfig` |
| D5 | Tailscale | **Optional** ✅ confirmed | `system/tailscale.rs` |
| D6 | Folder Scanner max files | **200** ✅ confirmed | `agents/folder_scanner/mod.rs` |
| D7 | Daily briefing time | **User-configurable** (not hardcoded 7AM) | workflow YAML `context.variables.briefing_time` |
| D8 | Multi-session handoff | **Session Handoff Protocol** (this document) | All sessions |

### D1 Detail: Flutter Mobile App

**Changed from:** React Native + Expo  
**Changed to:** Flutter (Dart)

Rationale accepted:
- Single codebase → iOS + Android
- Superior rendering performance (Skia engine)
- Strong typing (Dart) matches Rust discipline
- Better offline/local-first support
- Material Design 3 out of the box

Tech stack:
```
mobile/
├── lib/
│   ├── main.dart
│   ├── core/
│   │   ├── websocket_client.dart    # WebSocket with auto-reconnect
│   │   ├── auth_service.dart        # QR scan → token → secure storage
│   │   └── models/                  # Message types matching Rust DTOs
│   ├── features/
│   │   ├── chat/                    # Chat screen (mirrors Desktop ChatPane)
│   │   ├── progress/                # Task/workflow progress monitor
│   │   ├── approvals/               # HITL approval cards
│   │   ├── status/                  # Agent status dashboard
│   │   └── settings/                # Connection + notification settings
│   └── widgets/                     # Shared components
├── pubspec.yaml
└── README.md

Key packages:
  web_socket_channel: ^3.0.0        # WebSocket client
  mobile_scanner: ^5.0.0            # QR code scanning
  flutter_secure_storage: ^9.0.0    # Credential storage
  flutter_local_notifications: ^17  # Push notifications
  speech_to_text: ^6.6.0            # Voice input
  flutter_markdown: ^0.7.0          # Render LLM responses
  riverpod: ^2.5.0                  # State management
  go_router: ^14.0.0                # Navigation
  freezed: ^2.5.0                   # Immutable data classes
```

### D4 Detail: Lock-screen Default Changed

```rust
// src-tauri/src/system/mod.rs – SystemConfig::default()
pub fn default() -> Self {
    Self {
        agents_active_on_lockscreen: true,  // CHANGED from false → true
        // ... rest unchanged
    }
}
```

### D7 Detail: Configurable Briefing Time

```yaml
# workflows/daily-morning-briefing.yaml
context:
  variables:
    # User-configurable time (24h format HH:MM)
    # Can be changed in Settings UI → Workflows → Daily Briefing
    briefing_time: "07:00"
    # Timezone (auto-detected from Windows, overridable)
    timezone: "Asia/Ho_Chi_Minh"
    # Days to run (1=Mon, 7=Sun)
    active_days: [1, 2, 3, 4, 5]  # Mon-Fri default

trigger:
  type: schedule
  config:
    # Dynamic cron built from context variables
    time: "{{ context.briefing_time }}"
    days: "{{ context.active_days }}"
    timezone: "{{ context.timezone }}"
```

---

## Part 2: Session Handoff Protocol

### 2.1 Purpose

Office Hub is a large system (~40 source files, 30,000+ lines of Rust + TypeScript + Dart) developed across **many AI chat sessions**. Without a structured handoff mechanism, each new session risks:

1. **Context Drift** — the AI forgets earlier design decisions and invents contradictory ones
2. **Hallucination** — the AI confidently describes code that doesn't exist or works differently
3. **Duplicated Work** — re-implementing things that already exist
4. **Inconsistency** — naming conventions, patterns, or architecture changing mid-project
5. **Regression** — undoing previously tested and approved work

The Session Handoff Protocol solves this by enforcing a **checkpoint-resume** discipline.

### 2.2 Core Principle

> **Every session BEGINS by reading the handoff file.**  
> **Every session ENDS by updating the handoff file.**

The handoff file is the **single source of truth** for project state.

### 2.3 The Handoff File

**Location:** `docs/HANDOFF.md`

This file is **machine-readable** and **human-readable**. It is updated at the END of every development session and read at the START of the next session.

#### Handoff File Template

```markdown
# Office Hub – Session Handoff State

## Last Session
- **Date:** YYYY-MM-DD
- **Session ID:** (unique identifier for tracking)
- **Focus:** (what was worked on)
- **Duration:** ~Xh
- **Developer:** (human or AI model name)

## Current Phase
- **Phase:** X — Phase Name
- **Phase Status:** IN_PROGRESS | COMPLETED | BLOCKED
- **Overall Progress:** XX% (of total project)

## Completed Work (cumulative)

### Phase 0 — Foundation ✅
- [x] Repo structure created
- [x] Cargo.toml with all dependencies
- [x] Tauri v2 config
- [x] React + TypeScript + Vite frontend scaffold
- [x] CI/CD pipeline (GitHub Actions)
- [x] All 8 Rust module stubs compile
- [x] Module stubs: orchestrator, agents (7), llm_gateway, mcp, workflow, websocket, system

### Phase 1 — App Shell + LLM Gateway + System Layer
- [ ] LLM Gateway: Gemini API integration
- [ ] LLM Gateway: Ollama local fallback
- [ ] Token cache: hit/miss working
- [ ] Hybrid mode: cloud → local fallback
- [ ] System Tray icon + context menu
- [ ] Windows Startup registration (winreg)
- [ ] QR code generation (qrcode crate)
- [ ] Tailscale probe (CLI)
- [ ] Sleep override (SetThreadExecutionState)
- [ ] Frontend: Main layout (sidebar + content)
- [ ] Frontend: File Browser
- [ ] Frontend: Chat Pane (basic)
- [ ] Frontend: Settings page
- ... (all tasks listed)

### Phase 2 — Orchestrator + MCP Host
- [ ] ...

(Continue for all phases)

## In-Progress Work
- **Currently implementing:** (exact file + function)
- **Blocked on:** (if anything)
- **Next immediate step:** (what to do when session resumes)

## Key Architecture Decisions (immutable)
These decisions are FINAL and must NOT be changed without explicit user approval:

1. **Tech stack:** Rust + Tauri v2 + React/TS (Desktop) + Flutter/Dart (Mobile)
2. **LLM providers:** Gemini, OpenAI (cloud) + Ollama, LM Studio (local)
3. **Hybrid mode:** cloud primary → local fallback (configurable)
4. **COM Automation:** Excel, Word, PowerPoint, Outlook via windows crate 0.58
5. **UIA:** Browser control via UIAutomationCore.dll (Phase 4)
6. **MCP:** JSON-RPC 2.0 over stdio transport
7. **Mobile connection:** WebSocket + QR pairing + optional Tailscale
8. **HITL:** oneshot channel pattern, 4 risk levels (Low/Medium/High/Critical)
9. **Rule Engine:** YAML-driven, hot-reloadable, 8+ rule types
10. **Agents (7):** Analyst, OfficeMaster, WebResearcher, Converter, FolderScanner, Outlook, System
11. **Intent taxonomy:** 39+ intents across 9 categories
12. **Session management:** DashMap store, auto-trim at 80% context, LLM summarisation
13. **Lock-screen:** agents_active_on_lockscreen = true by default
14. **Outlook send:** HITL HIGH (not CRITICAL)
15. **QR expiry:** 5 minutes
16. **Mobile framework:** Flutter (Dart)

## File Inventory
(Auto-generated list of all project files with line counts and last-modified)

### Rust Backend (src-tauri/src/)
| File | Lines | Status | Last Modified |
|------|-------|--------|---------------|
| main.rs | 6 | ✅ Done | Session 0 |
| lib.rs | 400 | ✅ Done | Session 0 |
| commands.rs | 599 | ✅ Done | Session 0 |
| orchestrator/mod.rs | 770 | ✅ Done | Session 0 |
| orchestrator/intent.rs | 1617 | ✅ Done | Session 0 |
| orchestrator/session.rs | 909 | ✅ Done | Session 0 |
| orchestrator/router.rs | 963 | ⚠️ Needs update | Session 0 |
| orchestrator/rule_engine.rs | 1658 | ✅ Done | Session 0 |
| agents/mod.rs | 598 | ✅ Done | Session 0 |
| agents/analyst/mod.rs | 897 | ✅ Stub | Session 0 |
| agents/office_master/mod.rs | 681 | ✅ Stub | Session 0 |
| agents/office_master/com_word.rs | 35 | ✅ Stub | Session 0 |
| agents/office_master/com_ppt.rs | 18 | ✅ Stub | Session 0 |
| agents/web_researcher/mod.rs | 1135 | ✅ Stub | Session 0 |
| agents/converter/mod.rs | 86 | ✅ Stub | Session 0 |
| agents/folder_scanner/mod.rs | 2055 | ✅ Stub | Session 1 |
| agents/outlook/mod.rs | 1833 | ✅ Stub | Session 1 |
| llm_gateway/mod.rs | 1584 | ✅ Done | Session 0 |
| mcp/mod.rs | 1337 | ✅ Done | Session 0 |
| workflow/mod.rs | 1763 | ✅ Done | Session 0 |
| workflow/triggers/mod.rs | 95 | ✅ Stub | Session 0 |
| workflow/actions/mod.rs | 17 | ✅ Stub | Session 0 |
| websocket/mod.rs | ~750 | ✅ Done | Session 0 |
| system/mod.rs | 1096 | ✅ Stub | Session 1 |

### Frontend (src/)
| File | Lines | Status |
|------|-------|--------|
| main.tsx | 10 | ✅ Scaffold | 

### Config & Data
| File | Lines | Status |
|------|-------|--------|
| Cargo.toml | 135 | ✅ Done |
| tauri.conf.json | 83 | ✅ Done |
| package.json | 70 | ✅ Done |
| vite.config.ts | 66 | ✅ Done |
| tsconfig.json | 94 | ✅ Done |
| config.example.yaml | 232 | ✅ Done |
| rules/default.yaml | 448 | ✅ Done |
| workflows/email-to-report.yaml | 452 | ✅ Done |
| .github/workflows/ci.yml | 253 | ✅ Done |
| .gitignore | 108 | ✅ Done |
| README.md | 163 | ✅ Done |

### Documentation
| File | Lines | Status |
|------|-------|--------|
| docs/MASTER_PLAN.md | 1485 | ✅ Done |
| docs/MASTER_PLAN_AMENDMENT_v1.1.md | 1522 | ✅ Done |
| docs/SESSION_HANDOFF_PROTOCOL.md | THIS FILE | ✅ Done |

## Known Issues & Tech Debt
(List of things that need fixing but aren't blocking current work)

1. `router.rs` references types (`IntentCategory`, `IntentPriority`) not defined in `intent.rs` — needs alignment
2. `commands.rs` references module paths that don't match `lib.rs` module structure — needs refactor
3. `lib.rs` `run()` function references command names inconsistent with `commands.rs` — needs sync
4. `mcp/mod.rs` has a typo: `Deserialize"` should be `Deserialize` on `ToolCallResult` derive
5. `agents/mod.rs` references `orchestrator::AgentOutput` and `AgentTask` but orchestrator re-exports may not match
6. No `index.html` for frontend yet
7. No `src/App.tsx` for frontend yet
8. No `tailwind.config.js` for frontend yet
9. `system/mod.rs` uses `tauri::Manager` and `AppHandle` which may need Tauri plugin imports
10. Cargo workspace not verified to compile end-to-end (need `cargo check`)

## Naming Conventions (enforced)
- Rust modules: `snake_case`
- Rust structs/enums: `PascalCase`
- Rust functions: `snake_case`
- Tauri commands: `snake_case` (e.g. `send_chat_message`)
- IPC DTOs: `#[serde(rename_all = "camelCase")]`
- TypeScript: `camelCase` functions, `PascalCase` components
- YAML keys: `snake_case`
- File naming: `snake_case.rs`, `PascalCase.tsx`
- Agent IDs: `snake_case` strings (e.g. `"folder_scanner"`)
- Intent variants: `PascalCase` enum (e.g. `Intent::ExcelRead`)

## Anti-Hallucination Checklist
Before writing any code in a new session, verify:
- [ ] Read HANDOFF.md completely
- [ ] Read MASTER_PLAN.md Section 3 (Architecture) 
- [ ] Read MASTER_PLAN.md Section 4 (Orchestrator) if touching orchestrator
- [ ] `grep` for any type/function name before declaring it "missing"
- [ ] Check Cargo.toml before adding dependencies (may already exist)
- [ ] Check existing test patterns before writing new tests
- [ ] Never rename an existing public API without updating all callers
- [ ] Never change a Decision from the Key Architecture Decisions list
```

### 2.4 Session Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                    SESSION START PROTOCOL                        │
│                                                                 │
│  1. Human provides context:                                     │
│     "Continue Office Hub development. Read handoff file."       │
│                                                                 │
│  2. AI reads these files (in order):                            │
│     a) docs/HANDOFF.md                    ← MANDATORY           │
│     b) docs/SESSION_HANDOFF_PROTOCOL.md   ← This file          │
│     c) docs/MASTER_PLAN.md (Section 3-4)  ← If touching core   │
│     d) The specific file(s) being modified ← Before editing     │
│                                                                 │
│  3. AI confirms understanding:                                  │
│     "I've read the handoff. Current state:                      │
│      Phase X, Y% complete. Last session worked on Z.            │
│      Next step: [specific task]. Proceeding."                   │
│                                                                 │
│  4. AI asks clarifying questions if handoff is ambiguous         │
│     (NEVER assume or hallucinate missing context)               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ACTIVE DEVELOPMENT                            │
│                                                                 │
│  Rules during session:                                          │
│                                                                 │
│  ✅ DO:                                                         │
│    • Read the target file before editing it                     │
│    • grep for symbols before declaring them missing             │
│    • Follow existing patterns (naming, error handling, tests)   │
│    • Write tests for new code                                   │
│    • Keep changes focused (one concern per session ideally)     │
│    • Log what you changed and why                               │
│                                                                 │
│  ❌ DON'T:                                                      │
│    • Change architecture decisions without user approval        │
│    • Rename public APIs without updating all callers            │
│    • Add dependencies not in Cargo.toml without explaining why  │
│    • Assume a function exists — grep first                      │
│    • Rewrite a file from scratch if editing will suffice        │
│    • Ignore compiler errors — fix or document them              │
│    • Skip the handoff update at session end                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    SESSION END PROTOCOL                          │
│                                                                 │
│  Before ending, AI MUST:                                        │
│                                                                 │
│  1. Update docs/HANDOFF.md with:                                │
│     • What was completed this session                           │
│     • What is in-progress (exact file + function)               │
│     • What the next session should do first                     │
│     • Any new Known Issues discovered                           │
│     • Updated File Inventory (if new files created)             │
│                                                                 │
│  2. Run diagnostics (if possible):                              │
│     • cargo check (Rust compilation)                            │
│     • npm run type-check (TypeScript)                           │
│     • Note any errors that remain                               │
│                                                                 │
│  3. Provide human-readable summary:                             │
│     "Session complete. Updated HANDOFF.md.                      │
│      Completed: [list]. Next: [specific task].                  │
│      Known issues: [list]. Ready for next session."             │
└─────────────────────────────────────────────────────────────────┘
```

### 2.5 Handoff Prompt Templates

#### Starting a New Session (paste this to the AI)

```
Continue developing Office Hub.

Read these files first:
1. docs/HANDOFF.md
2. docs/SESSION_HANDOFF_PROTOCOL.md

Then confirm:
- Current phase and progress
- What was done last session
- What to work on next

Do NOT start coding until you've confirmed the above.
```

#### Continuing Mid-Phase Work

```
Continue Office Hub. Read docs/HANDOFF.md.
Last session was working on [specific area].
Continue from where we left off.
Focus on: [specific task or file].
```

#### Starting a New Phase

```
Continue Office Hub. Read docs/HANDOFF.md.
Phase [N-1] is complete. Begin Phase [N].
Read the Phase [N] tasks from docs/MASTER_PLAN.md Section 11.
List all tasks for this phase, then start with the first one.
```

#### Debugging / Fixing Issues

```
Continue Office Hub. Read docs/HANDOFF.md.
There's an issue: [describe problem].
Read the relevant source file(s) before proposing changes.
Do NOT rewrite the file — make targeted edits only.
```

### 2.6 Anti-Drift Mechanisms

#### Mechanism 1: Decision Lock

All 16 architecture decisions listed in the handoff file are **LOCKED**. The AI must refuse to change them without explicit human instruction:

```
Human: "Change the mobile framework to React Native"
AI: "This contradicts Decision D1 (Flutter) which was explicitly approved.
     Are you sure you want to change this? If yes, I'll update:
     - HANDOFF.md Decision D1
     - MASTER_PLAN_AMENDMENT_v1.1.md Section A1
     - docs/SESSION_HANDOFF_PROTOCOL.md
     Please confirm."
```

#### Mechanism 2: File-Before-Edit Rule

The AI must **read a file before editing it**. This prevents hallucinating file contents:

```
# CORRECT workflow:
1. read_file("src-tauri/src/agents/mod.rs")
2. Understand current structure
3. edit_file(...) with targeted changes

# WRONG workflow:
1. edit_file("src-tauri/src/agents/mod.rs") with "overwrite" mode
   ← DANGEROUS: may destroy existing code the AI doesn't remember
```

#### Mechanism 3: Grep-Before-Create Rule

Before declaring that a type, function, or module doesn't exist, the AI must search:

```
# CORRECT:
1. grep("struct SessionStore")  → found in orchestrator/session.rs
2. Use the existing SessionStore

# WRONG:
1. Assume SessionStore doesn't exist
2. Create a new one in a different file
   ← CREATES DUPLICATE, BREAKS COMPILATION
```

#### Mechanism 4: Compilation Gate

At the end of each session, attempt:
```bash
cd src-tauri && cargo check 2>&1 | tail -20
```

If there are errors:
- Fix critical ones (missing imports, type mismatches)
- Document remaining ones in HANDOFF.md "Known Issues"
- Do NOT delete working code just to fix a compilation error

#### Mechanism 5: Test Continuity

When modifying a module that has tests:
1. Read the existing tests first
2. Run existing tests: `cargo test --lib module_name`
3. If tests fail after changes → fix the tests or revert the change
4. Add new tests for new functionality
5. Never delete a passing test

### 2.7 Phase Completion Checklist

Before marking a phase as COMPLETED in HANDOFF.md:

```
Phase X Completion Checklist:
[ ] All planned tasks from MASTER_PLAN.md Section 11 are done
[ ] All new code has unit tests
[ ] cargo check passes (zero errors)
[ ] cargo test passes (zero failures)
[ ] npm run type-check passes (zero errors, if frontend work was done)
[ ] HANDOFF.md updated with all completed items
[ ] No unresolved "Known Issues" that block the next phase
[ ] README.md roadmap updated (checkbox marked)
[ ] CHANGELOG.md entry added for this phase
[ ] Human has reviewed and approved the phase
```

### 2.8 Emergency Recovery

If a session produces broken code or contradictory changes:

```
Recovery Protocol:
1. Human: "Stop. Read docs/HANDOFF.md. Do not change any files."
2. AI reads handoff, identifies current state
3. Human: "Revert [specific file] to its state from Session [N]"
4. AI uses restore_file_from_disk or git checkout
5. AI re-reads the restored file
6. Resume from the last known-good state described in HANDOFF.md
```

### 2.9 Session Size Guidelines

To prevent context overload and maintain quality:

| Session Type | Recommended Scope |
|-------------|-------------------|
| **Small** (1-2h equivalent) | 1-3 files, focused feature or bugfix |
| **Medium** (2-4h equivalent) | 3-8 files, one sub-module implementation |
| **Large** (4-8h equivalent) | Full sub-module or cross-cutting change |
| **Architecture** | Design docs only, no code changes |
| **Review** | Read + audit existing code, update HANDOFF.md |

**Rule of thumb:** If you need to edit more than 10 files, split into 2+ sessions.

---

## Part 3: Initial Handoff State (Session 0 → Session 1)

This section captures the state at the end of the initial setup sessions (Session 0 and Session 1), to bootstrap the HANDOFF.md file.

### What Has Been Completed (Sessions 0–1)

```
Phase 0 — Foundation: COMPLETED ✅

Deliverables:
  ✅ Monorepo structure: 30+ directories created
  ✅ Cargo.toml: 40+ dependencies configured (Tauri 2, windows 0.58, tokio, etc.)
  ✅ tauri.conf.json: Tauri v2 app config with security CSP
  ✅ package.json: React 18 + TypeScript 5 + Vite 5 + Tailwind
  ✅ vite.config.ts: Tauri-optimized with path aliases
  ✅ tsconfig.json: Strict mode, all checks enabled
  ✅ CI/CD: GitHub Actions (lint → test → audit → build → release)
  ✅ .gitignore: Comprehensive (Rust + Node + Office + secrets)
  ✅ README.md: Architecture diagram, quickstart, roadmap
  ✅ config.example.yaml: Full template with comments (232 lines)

  Rust Backend (all modules with stubs + tests):
  ✅ orchestrator/mod.rs: Core pipeline, HITL Manager, metrics (770 lines)
  ✅ orchestrator/intent.rs: 30 intent variants, FastClassifier, 16+ tests (1617 lines)
  ✅ orchestrator/session.rs: Session, SessionStore, ContextWindow (909 lines)
  ✅ orchestrator/router.rs: Router, routing table, dispatch patterns (963 lines)
  ✅ orchestrator/rule_engine.rs: 8 rule types, YAML parsing, hot-reload (1658 lines)
  ✅ agents/mod.rs: Agent trait, AgentRegistry, AgentId (598 lines)
  ✅ agents/analyst/mod.rs: Excel COM stub, Hard-Truth Verify (897 lines)
  ✅ agents/office_master/mod.rs: Word + PPT COM stubs (681 lines)
  ✅ agents/web_researcher/mod.rs: UIA stubs, domain policy, audit (1135 lines)
  ✅ agents/converter/mod.rs: MCP skill builder stub (86 lines)
  ✅ agents/folder_scanner/mod.rs: Full scan pipeline, progress events (2055 lines)
  ✅ agents/outlook/mod.rs: 31 actions, email/calendar stubs (1833 lines)
  ✅ llm_gateway/mod.rs: 4 providers, cache, hybrid mode (1584 lines)
  ✅ mcp/mod.rs: JSON-RPC 2.0, StdioTransport, registry (1337 lines)
  ✅ workflow/mod.rs: YAML loader, template engine, executor (1763 lines)
  ✅ websocket/mod.rs: Server, message types, HITL relay (~750 lines)
  ✅ system/mod.rs: Tray, startup, sleep, Tailscale, QR (1096 lines)
  ✅ commands.rs: IPC bridge, all DTOs (599 lines)
  ✅ lib.rs: Module root, AppState, AppConfig, run() (400 lines)

  Data Files:
  ✅ rules/default.yaml: Production rule set (448 lines)
  ✅ workflows/email-to-report.yaml: Full 10-step workflow (452 lines)

  Documentation:
  ✅ docs/MASTER_PLAN.md: Complete development plan (1485 lines)
  ✅ docs/MASTER_PLAN_AMENDMENT_v1.1.md: All additions (1522 lines)
  ✅ docs/SESSION_HANDOFF_PROTOCOL.md: This file
```

### Known Issues at Handoff

```
PRIORITY 1 (must fix in Phase 1):
  1. router.rs references IntentCategory/IntentPriority types not in intent.rs
     → Need to either add these types or refactor router to use Intent enum directly
  2. commands.rs module path references don't match lib.rs structure
     → commands::config::get_config vs flat commands::get_config
  3. lib.rs run() lists command names that don't exist in commands.rs
     → Need to sync the invoke_handler list
  4. mcp/mod.rs line ~270: typo Deserialize" (extra quote in derive)
  5. No index.html, App.tsx, or tailwind.config for frontend
  6. agents/mod.rs Agent trait execute() signature might conflict with
     orchestrator/mod.rs Agent trait — need to verify single trait definition

PRIORITY 2 (fix during Phase 1-2):
  7. system/mod.rs uses tauri::Manager but system module isn't declared in lib.rs
  8. agents/folder_scanner and agents/outlook aren't declared in agents/mod.rs
  9. Entire project has not been cargo check'd yet — may have many compile errors
  10. Frontend has no components — just main.tsx scaffold

PRIORITY 3 (fix during respective phases):
  11. All COM implementations are stubs (Phase 3)
  12. UIA implementation is stub (Phase 4)
  13. WebSocket server is stub (Phase 5)
  14. All workflow triggers except Manual are stubs (Phase 6)
```

### Next Session Instructions

```
NEXT SESSION SHOULD:

1. Create docs/HANDOFF.md from the template in Section 2.3 of this protocol
   → Populate with the state captured in Part 3 above

2. Fix Priority 1 known issues (items 1-6):
   → Make the project pass cargo check (even if with warnings)
   → This is the gate for Phase 0 → Phase 1 transition

3. Begin Phase 1:
   → Start with LLM Gateway real integration (Gemini API)
   → Then System Tray + Startup
   → Then Frontend scaffold (index.html, App.tsx, basic layout)

FOCUS ORDER:
  a) cargo check passes → GATE
  b) LLM Gateway Gemini call works → MILESTONE
  c) System Tray visible → MILESTONE
  d) Frontend renders in Tauri window → MILESTONE
```

---

## Part 4: Quick Reference Card

### File Reading Priority (for any session)

```
ALWAYS read (every session):
  1. docs/HANDOFF.md

Read if touching that area:
  2. docs/MASTER_PLAN.md               → Architecture, Phases
  3. docs/MASTER_PLAN_AMENDMENT_v1.1.md → Mobile, new agents, system
  4. docs/SESSION_HANDOFF_PROTOCOL.md   → This file (process)
  5. src-tauri/Cargo.toml               → Dependencies
  6. src-tauri/src/lib.rs               → Module declarations, AppState
  7. The specific file being modified    → ALWAYS before editing
```

### Key File Locations

```
Orchestrator core:    src-tauri/src/orchestrator/
  Intent schema:      orchestrator/intent.rs
  Session state:      orchestrator/session.rs
  Task routing:       orchestrator/router.rs
  Rule validation:    orchestrator/rule_engine.rs

Agents:               src-tauri/src/agents/
  Excel:              agents/analyst/mod.rs
  Word/PPT:           agents/office_master/mod.rs
  Browser (UIA):      agents/web_researcher/mod.rs
  Folder scan:        agents/folder_scanner/mod.rs
  Email/Calendar:     agents/outlook/mod.rs
  MCP skills:         agents/converter/mod.rs

Infrastructure:       src-tauri/src/
  LLM providers:      llm_gateway/mod.rs
  MCP host:           mcp/mod.rs
  Workflows:          workflow/mod.rs
  WebSocket:          websocket/mod.rs
  System (tray etc):  system/mod.rs
  IPC commands:       commands.rs
  App config:         lib.rs

Config/Data:
  Rules:              rules/default.yaml
  Workflows:          workflows/*.yaml
  App config:         config.example.yaml

Docs:
  Master Plan:        docs/MASTER_PLAN.md
  Amendment v1.1:     docs/MASTER_PLAN_AMENDMENT_v1.1.md
  Handoff Protocol:   docs/SESSION_HANDOFF_PROTOCOL.md
  Handoff State:      docs/HANDOFF.md (created in next session)
```

### Decisions That Cannot Be Changed Without User Approval

```
D1:  Flutter for mobile (not React Native)
D2:  QR expiry = 5 minutes
D3:  Outlook send = HITL HIGH
D4:  Lock-screen agents = ON by default
D5:  Tailscale = optional
D6:  Folder scan max = 200 files
D7:  Briefing time = user-configurable
D8:  Session handoff protocol = mandatory
D9:  Rust + Tauri v2 (not Go/Wails)
D10: 7 agents total (Analyst, OfficeMaster, WebResearcher, Converter, FolderScanner, Outlook, System)
D11: COM Automation for Office integration (not OpenXML-only)
D12: Windows UI Automation for browser (not Selenium)
D13: MCP for extensibility (JSON-RPC 2.0 over stdio)
D14: YAML for rules and workflows
D15: DashMap for concurrent state
D16: Hybrid LLM mode (cloud primary → local fallback)
```

---

*This document is the process contract for Office Hub development. Every session participant (human or AI) must follow this protocol. Violations should be flagged immediately.*

*Document owner: Office Hub Project Lead*  
*Effective: Immediately*  
*Review: After every 5 sessions or at each phase boundary*