# Agent Orchestrator Spec

## Overview
Native macOS app (Tauri) to manage multiple AI coding agents (Claude Code, Open Code) running on a remote VPS.

## Architecture
```
┌─────────────────────┐         ┌─────────────────────────────────┐
│   macOS (Tauri)     │         │         VPS (Tailscale)         │
│                     │         │                                 │
│  React Frontend     │◄──TCP──►│  Agent Daemon (Rust/TS)         │
│  + Rust Shell       │ JSON-RPC│    ├── Session Manager          │
│                     │         │    ├── Terminal PTY             │
│                     │         │    ├── Git Operations           │
│                     │         │    └── Agent Processes          │
└─────────────────────┘         │         (Claude/Open Code)      │
                                └─────────────────────────────────┘
        ▲
        │ Future: lightweight
        ▼ mobile web UI
```

## Tech Stack

### Desktop App (Tauri)
- **Frontend**: React + TypeScript (familiar stack)
- **Backend**: Rust (Tauri provides this)
- **Diff rendering**: `@pierre/diffs` (diffs.com)
- **Terminal**: xterm.js in frontend

### VPS Daemon
- **Option A**: Bun + Elysia + Effect TS (your preference)
- **Option B**: Rust (like CodexMonitor's remote backend POC)
- **Protocol**: JSON-RPC over TCP (or WebSocket)
- **Auth**: Token-based, Tailscale provides network security

## Reference: CodexMonitor
https://github.com/Dimillian/CodexMonitor

Already implements:
- ✅ Tauri (React + Rust)
- ✅ `@pierre/diffs` for diff rendering
- ✅ Terminal via portable-pty
- ✅ Git status, diffs, log, branches
- ✅ Workspace/worktree management
- ✅ **Remote backend POC** - daemon with JSON-RPC over TCP
- ✅ Agent thread management

Key difference: CodexMonitor uses Codex's `app-server` protocol. We'd need to adapt for Claude Code + Open Code.

## Core Features

### v1 - Connect & View
- [ ] VPS daemon: expose running agent sessions
- [ ] List sessions with project/folder info
- [ ] Interactive terminal (attach to existing)
- [ ] Git diff viewer per session (staged/unstaged)
- [ ] Commit history (click to view diff)

### v2 - Spawn Agents
- [ ] Start new agent sessions from UI
- [ ] Assign to project folder
- [ ] Stop/restart agents
- [ ] Memory usage monitoring

### v3 - Parallel Worktrees
- [ ] Spawn multiple agents on same project
- [ ] Auto-create git worktrees
- [ ] Coordinate parallel workloads

### Future
- [ ] Lightweight mobile web UI (via same daemon API)

---

## North Star Vision

### Why Build This
Agent orchestration is the future. Building your own tooling = staying at the forefront, testing new techniques as they emerge, not waiting for vendors.

### Multi-Project Orchestration
- **Scenario**: Frontend repo + backend repo, coordinated work
- Orchestrator can:
  - Spawn agents across different projects
  - Coordinate dependencies ("backend API done → frontend can integrate")
  - Spin off sub-agents to check/verify work
  - Merge results across repos
- Think: one brain coordinating a team of specialists

### Ralph Wiggins Loop
Technique to avoid context degradation:

```
Traditional:
  Agent runs → context fills → compaction → quality degrades

Ralph Wiggins:
  Spec → Agent does small task → KILL → New agent → Next task → KILL → ...
```

**Benefits:**
- Fresh context for each task (no degradation)
- Controlled execution bursts
- Easier checkpointing/rollback
- Better observability (each task is discrete)

**Implementation ideas:**
- Task queue with atomic work units
- Agent spawns, completes task, reports, dies
- Orchestrator tracks overall progress via spec
- State persisted externally (not in agent context)

### Meta-Orchestrator
Long-term: the orchestrator itself becomes an agent that can:
- Read a high-level spec
- Break into tasks
- Spawn worker agents (across repos if needed)
- Monitor progress
- Handle failures/retries
- Coordinate merging
- Report completion

This is the "AI project manager" layer on top of "AI developers"

---

## Open Questions

### Agent Process Model
Claude Code and Open Code are CLIs. Options:
1. **Direct PTY**: Spawn in a PTY, stream I/O to client (like CodexMonitor terminal)
2. **Wrapper**: Build a thin wrapper that speaks a protocol (more control, more work)
3. **Native protocols**: Does Claude Code have an app-server mode like Codex?

Need to research Claude Code's programmatic interface.

### Daemon Language
- **TypeScript (Bun/Elysia)**: Your preference, faster iteration
- **Rust**: More consistent with Tauri backend, CodexMonitor has working reference

Leaning: Start with TypeScript for speed, can port critical paths to Rust later if needed.

### Session Discovery
How does daemon know which processes are agents?
- Explicit registration (daemon spawns them)
- Process scanning (find claude/opencode processes)
- tmux session discovery (current approach, interim)

---

## Research Needed

### Claude Code SDK
- **SDK**: `@anthropic-ai/claude-agent-sdk` (TypeScript & Python)
- Docs: https://docs.claude.com/en/api/agent-sdk/typescript
- Background agent support added recently
- Questions:
  - Server mode like Codex's `app-server`?
  - Programmatic thread/message API?
  - Event streaming?
  - Can we attach to running CLI sessions?

### Open Code Server
- Repo: `anomalyco/opencode`
- **Client/server architecture confirmed**
- "allows OpenCode to run on your computer, while you can drive it remotely from a mobile app"
- Desktop app exists (beta) - Tauri-based
- Questions:
  - Protocol? (JSON-RPC, REST, gRPC?)
  - How to spawn/attach to sessions?
  - Event model?

### Resources to Index
- [ ] Claude Code SDK docs/repo
- [ ] Open Code repo (server implementation)

---

## Decisions

### Daemon Language: Rust ✓
Agent can implement. Consistent with Tauri. CodexMonitor reference available.

### Fork vs Fresh: **Fresh (recommended)**
Reasons:
- CodexMonitor deeply assumes Codex's `app-server` protocol
- Forking = inheriting Codex-specific patterns throughout codebase
- Starting fresh = design for Claude Code + Open Code from day 1
- **But**: heavily reference CodexMonitor, copy patterns/components where applicable

Approach: New repo, steal liberally from CodexMonitor's:
- Tauri project structure
- React component patterns
- `@pierre/diffs` integration
- Terminal handling (portable-pty)
- Git operations

---

## Next Steps

1. **Research** Claude Code SDK + Open Code server
2. **Scaffold** fresh Tauri app
3. **Build** Rust daemon with session list + terminal attach
4. **Integrate** `@pierre/diffs` for git viewer
5. **Connect** to Claude Code / Open Code via their native protocols
