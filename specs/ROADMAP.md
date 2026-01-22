# Maestro Roadmap

**Last Updated:** 2026-01-22

See [VISION.md](./VISION.md) for the full product vision.

---

## Current State

**Foundation complete.** Daemon runs on VPS, Mac app connects as thin client. Terminal PTY works. Session discovery works. Basic styling in place.

**Agent harness work in progress.** Claude SDK server specced, partially implemented. OpenCode integration working but needs polish.

---

## Phase A: Foundation (Complete)

- [x] Initialize git repo
- [x] Scaffold Tauri app
- [x] Feature-sliced architecture
- [x] Tauri IPC wrapper
- [x] Event hub pattern
- [x] Resizable panels
- [x] Terminal UI (xterm.js)
- [x] Daemon implementation (TCP JSON-RPC)
- [x] Tauri proxy layer to daemon
- [x] Frontend daemon integration
- [x] macOS glass effect
- [x] Design token system

## Phase B: Agent Harnesses (Current)

**Goal:** Full chat experience with Claude Code and OpenCode.

- [x] Claude SDK server spec
- [x] Claude SDK UI spec
- [x] Streaming event schema spec
- [x] State machine wiring spec
- [ ] **Claude SDK server implementation** ← next
- [ ] **Claude SDK UI integration**
- [ ] Model selection (composer options)
- [ ] Extended thinking toggle
- [ ] Dynamic tool approvals
- [ ] OpenCode thread UI polish

**Blocking issues:**
- Need to validate Claude Agent SDK integration end-to-end
- Permission flow UX not finalized

## Phase C: Git Polish

**Goal:** Full git workflow without leaving the app.

- [ ] Git panel in main layout (status + diff)
- [ ] Split/unified diff viewer
- [ ] Stage/unstage individual files
- [ ] Commit flow (message composer, gritty integration)
- [ ] History browser
- [ ] Branch indicator in session info

## Phase D: Workspaces & Projects

**Goal:** Organize work by workspace (personal/work) and project.

- [ ] Workspace data model + persistence
- [ ] Workspace switcher in sidebar
- [ ] Project list per workspace
- [ ] Recent/favorite projects
- [ ] Auto-discover projects from workspace root

## Phase E: Spec System

**Goal:** First-class spec-driven development.

- [ ] Scan specs/ folder for existing specs
- [ ] Spec viewer (markdown render + task list)
- [ ] "New Spec" command → agent interview flow
- [ ] Spec templates (customizable per project)
- [ ] Plan progress tracking (tasks completed)

## Phase F: Native Agent Loop

**Goal:** Automated agent iteration with full visibility.

- [ ] Agent loop engine (Rust or Bun subprocess)
- [ ] Loop configuration UI (spec, model, iterations)
- [ ] Real-time iteration monitoring
- [ ] Live output streaming
- [ ] Completion detection
- [ ] Postmortem generation (Opus analysis)
- [ ] Run history + replay

## Phase G: Advanced Orchestration

**Goal:** Multi-agent, multi-project coordination.

- [ ] Multi-project workspace support
- [ ] Git worktree management
- [ ] Sub-agent spawning
- [ ] Task queue system
- [ ] Meta-orchestrator agent

---

## Immediate Next Steps

1. **Claude SDK server** - Complete implementation, test with curl
2. **Wire frontend** - Connect Claude provider to thread UI
3. **Git panel** - Add to main layout, show for selected session
4. **Workspace model** - Design persistence, add switcher

---

## Commands Reference

```bash
# Run Tauri dev (Mac)
cd app && bun run tauri:dev

# Build Tauri app
cd app && bun run tauri build

# Type check
cd app && bun run typecheck

# Run daemon (VPS)
cd daemon && cargo run -- --listen 0.0.0.0:4733 --token "$MAESTRO_TOKEN"

# Run daemon locally (no auth)
cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth

# Update CodexMonitor reference
git subtree pull --prefix=reference/codex-monitor https://github.com/Dimillian/CodexMonitor.git main --squash
```

---

## Spec Index

Core specs driving current work:

| Spec | Status | Phase |
|------|--------|-------|
| [VISION.md](./VISION.md) | Active | -- |
| [claude-sdk-server.md](./claude-sdk-server.md) | Draft | B |
| [claude-sdk-ui.md](./claude-sdk-ui.md) | Draft | B |
| [composer-options.md](./composer-options.md) | Draft | B |
| [dynamic-tool-approvals.md](./dynamic-tool-approvals.md) | Draft | B |
| [git-diff-ui.md](./git-diff-ui.md) | Draft | C |
| [session-persistence.md](./session-persistence.md) | Draft | D |
| [agent-loop-terminal-ux.md](./agent-loop-terminal-ux.md) | Draft | F |

See [README.md](./README.md) for full spec index with plans and code locations.
