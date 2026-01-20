# Orchestrator Roadmap

## Architecture Reminder

```
┌─────────────────────┐                  ┌─────────────────────────────────┐
│   macOS (Tauri)     │                  │         VPS (Tailscale)         │
│                     │                  │                                 │
│  React + xterm.js   │     TCP          │  maestro-daemon                 │
│  Rust Proxy Layer   │◄────────────────►│    ├── Terminal PTY             │
│  (thin client)      │   JSON-RPC       │    ├── Git Operations           │
│                     │                  │    ├── Session Manager          │
└─────────────────────┘                  │    └── Agent Harnesses          │
                                         └─────────────────────────────────┘
```

**The daemon runs remotely on VPS. The Mac app is a thin client.**

---

## Phase 0: Setup (Complete)
- [x] Initialize git repo
- [x] Create spec document
- [x] Add CodexMonitor as git subtree (`reference/codex-monitor/`)
- [x] Scaffold Tauri app (`app/`)
- [x] Get app running locally on Mac
- [x] Create README.md and AGENTS.md

## Phase 1: Deep Analysis (Complete)
- [x] Analyze CodexMonitor architecture
- [x] Document IPC patterns, state management, event flow
- [x] Document terminal implementation (PTY + xterm.js)
- [x] Document git integration
- [x] Document diff rendering (@pierre/diffs)
- [x] Document remote backend daemon (JSON-RPC)
- [x] Document UI patterns (feature-sliced architecture)
- [x] Create recommendations (copy/adapt/skip/build)

**Output:** `specs/CODEX_MONITOR_ANALYSIS.md`

## Phase 2: Frontend Scaffolding (Complete)
- [x] Feature-sliced architecture (Task 005)
- [x] Tauri IPC wrapper (Task 002)
- [x] Event hub pattern (Task 003)
- [x] Resizable panels (Task 006)
- [x] Basic layout (sidebar + main panel)
- [x] Terminal UI (xterm.js + TerminalPanel)
- [x] Git UI components (GitStatusPanel, DiffViewer)
- [x] Sessions feature extraction

**Note:** Current terminal/git code runs locally in Tauri. This was scaffolding to validate the UI patterns. For the real architecture, these operations must run on the VPS daemon.

## Phase 3: Remote Daemon (Current - BLOCKING)

**This is the critical path.** Without the daemon, we cannot test the real architecture.

- [x] **Daemon implementation** (Task 014 - DONE)
  - [x] TCP listener with JSON-RPC protocol
  - [x] Token authentication
  - [x] Session discovery/management
  - [x] Terminal PTY (reuse portable-pty logic)
  - [x] Git operations (reuse sessions.rs git code)
  - [x] Event streaming to clients

- [x] **Tauri proxy layer** (Task 015 - DONE)
  - [x] Connect to daemon on startup
  - [x] Proxy terminal commands to daemon
  - [x] Proxy git commands to daemon
  - [x] Forward daemon events to React

- [ ] **Frontend daemon integration** (Task 016 - NEXT)
  - [ ] Update services to use new command/event names
  - [ ] Add daemon connection hook and UI
  - [ ] Update session/terminal hooks for new formats
  - [ ] Settings modal for daemon configuration

- [ ] **End-to-end testing**
  - [ ] Run daemon on VPS
  - [ ] Connect Tauri app from Mac
  - [ ] Verify terminal works
  - [ ] Verify git status/diff works

## Phase 4: Git Integration Polish
- [ ] Commit history panel
- [ ] File tree with change indicators
- [ ] Session switching with buffer restore

## Phase 5: Agent Protocol Integration
- [x] Research Claude Code SDK (Task 007)
- [x] Research Open Code server protocol (Task 008)
- [ ] Implement Claude Code harness
- [ ] Implement Open Code harness
- [ ] Spawn agents from UI

## Phase 6: Advanced Orchestration
- [ ] Multi-project workspace support
- [ ] Git worktree management
- [ ] Ralph Wiggins loop implementation
- [ ] Task queue system
- [ ] Meta-orchestrator agent

---

## Immediate Next Steps

1. **Build daemon** (Task 014) - TCP server with terminal + git
2. **Convert Tauri to proxy** - Forward commands to daemon
3. **Deploy daemon to VPS** - Test real architecture
4. **Iterate on UI** - Now that e2e works

## Task Specs

Active:
- `016-frontend-daemon-integration.md` - Frontend updates for daemon (NEXT)

Completed:
- `DONE-001` through `DONE-015` - See `specs/TASKS/`

## Commands Reference

```bash
# Run Tauri dev (Mac)
cd app && bun run tauri:dev

# Build Tauri app
cd app && bun run tauri build

# Type check
cd app && bun run typecheck

# Run daemon (VPS) - after implementation
maestro-daemon --listen 0.0.0.0:4733 --token "$MAESTRO_TOKEN"

# Update CodexMonitor reference
git subtree pull --prefix=reference/codex-monitor https://github.com/Dimillian/CodexMonitor.git main --squash
```
