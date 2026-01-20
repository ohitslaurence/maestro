# Orchestrator Roadmap

## Phase 0: Setup (Complete)
- [x] Initialize git repo
- [x] Create spec document
- [x] Add CodexMonitor as git subtree (`reference/codex-monitor/`)
- [x] Scaffold Tauri app (`app/`)
- [x] Get app running locally on Mac
- [x] Create README.md and AGENTS.md

### Build on Mac
```bash
cd app
bun install
bun run tauri:dev
```

Requires: Rust toolchain, Xcode Command Line Tools

## Phase 1: Deep Analysis (Complete)
- [x] Analyze CodexMonitor architecture
- [x] Document IPC patterns, state management, event flow
- [x] Document terminal implementation (PTY + xterm.js)
- [x] Document git integration (git2 crate)
- [x] Document diff rendering (@pierre/diffs)
- [x] Document remote backend daemon (JSON-RPC)
- [x] Document UI patterns (feature-sliced architecture)
- [x] Create recommendations (copy/adapt/skip/build)

**Output:** `specs/CODEX_MONITOR_ANALYSIS.md`

### Implementation Tasks Created
See `specs/TASKS/` for detailed task specs:
- `002-tauri-ipc-wrapper.md` - Type-safe IPC wrapper pattern
- `003-event-hub-pattern.md` - Single-listen event subscription
- `004-terminal-implementation.md` - PTY + xterm.js
- `005-feature-sliced-architecture.md` - Component/hook separation
- `006-resizable-panels.md` - Draggable panels with persistence

### Research Tasks Created
- `007-research-claude-code-sdk.md` - Claude Code programmatic interface
- `008-research-opencode-server.md` - Open Code server protocol

## Phase 2: Core Infrastructure (Current)
- [ ] Feature-sliced architecture (Task 005)
- [ ] Tauri IPC wrapper (Task 002)
- [ ] Event hub pattern (Task 003)
- [ ] Resizable panels (Task 006)
- [ ] Basic layout (sidebar + main panel)

## Phase 3: Terminal Integration
- [ ] Terminal implementation (Task 004)
- [ ] PTY management in Rust backend
- [ ] xterm.js frontend integration
- [ ] Bidirectional streaming
- [ ] Session switching with buffer restore

## Phase 4: Git Integration
- [ ] Daemon: Git status per session
- [ ] Daemon: Git diff (staged/unstaged)
- [ ] Daemon: Git log
- [ ] Frontend: Diff viewer with `@pierre/diffs`
- [ ] Frontend: Commit history panel
- [ ] Frontend: File tree with change indicators

## Phase 5: Agent Protocol Integration
- [ ] Research Claude Code SDK (Task 007)
- [ ] Research Open Code server protocol (Task 008)
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

1. **Research tasks** - Run Task 007 (Claude Code) and Task 008 (Open Code)
2. **Feature-sliced architecture** - Restructure app per Task 005
3. **Tauri IPC wrapper** - Implement per Task 002
4. **Event hub** - Implement per Task 003
5. **Terminal** - Implement per Task 004

## Commands Reference

```bash
# Update CodexMonitor subtree
git subtree pull --prefix=reference/codex-monitor https://github.com/Dimillian/CodexMonitor.git main --squash

# Run Tauri dev
npm run tauri dev

# Build Tauri app
npm run tauri build
```
