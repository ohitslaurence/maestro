# Orchestrator Roadmap

## Phase 0: Setup (Current)
- [x] Initialize git repo
- [x] Create spec document
- [x] Add CodexMonitor as git subtree (`reference/codex-monitor/`)
- [x] Scaffold Tauri app (`app/`)
- [ ] Get app running locally on Mac (see below)
- [ ] Verify dev environment works

### Build on Mac
```bash
cd app
bun install
bun run tauri:dev
```

Requires: Rust toolchain, Xcode Command Line Tools

## Phase 1: Deep Analysis (Agent Task)
Spin off agent to analyze CodexMonitor and document:

### Frontend (React)
- [ ] Component structure and patterns
- [ ] State management approach
- [ ] Terminal integration (xterm.js setup)
- [ ] `@pierre/diffs` integration
- [ ] Git UI components (diff viewer, commit list, branch list)
- [ ] WebSocket/event handling patterns
- [ ] Styling approach (CSS modules? Tailwind? Plain CSS?)

### Backend (Rust/Tauri)
- [ ] Tauri command structure
- [ ] Terminal PTY handling (`portable-pty`)
- [ ] Git operations implementation
- [ ] State management
- [ ] Event emission to frontend
- [ ] Remote backend daemon (JSON-RPC protocol)

### Key Files to Study
- `src-tauri/src/terminal.rs` - Terminal handling
- `src-tauri/src/git.rs` - Git operations
- `src-tauri/src/backend/` - Remote daemon POC
- `src/features/git/` - Git UI components
- `src/features/terminal/` - Terminal UI
- `src/utils/diff.ts` - Diff utilities

### Output
Create `specs/CODEX_MONITOR_ANALYSIS.md` with:
- Architecture overview
- Key patterns to copy
- Files to reference for each feature
- Recommended adaptations for our use case

## Phase 2: Core Infrastructure
- [ ] Basic Tauri app with sidebar + main panel layout
- [ ] Terminal component (xterm.js)
- [ ] WebSocket connection to daemon
- [ ] Daemon skeleton (Rust, runs on VPS)
- [ ] Session discovery (list running agents)

## Phase 3: Terminal Integration
- [ ] Daemon: PTY management for sessions
- [ ] Daemon: Attach to existing tmux sessions (interim)
- [ ] Frontend: Render terminal output
- [ ] Frontend: Send input to terminal
- [ ] Bidirectional streaming working

## Phase 4: Git Integration
- [ ] Daemon: Git status per session
- [ ] Daemon: Git diff (staged/unstaged)
- [ ] Daemon: Git log
- [ ] Frontend: Diff viewer with `@pierre/diffs`
- [ ] Frontend: Commit history panel
- [ ] Frontend: File tree with change indicators

## Phase 5: Agent Protocol Integration
- [ ] Research Claude Agent SDK server mode
- [ ] Research Open Code server protocol
- [ ] Implement Claude Code integration
- [ ] Implement Open Code integration
- [ ] Spawn agents from UI (v2 feature)

## Phase 6: Advanced Orchestration
- [ ] Multi-project workspace support
- [ ] Git worktree management
- [ ] Ralph Wiggins loop implementation
- [ ] Task queue system
- [ ] Meta-orchestrator agent

---

## Immediate Next Steps

1. **Scaffold Tauri app** (this session)
2. **Get it running locally** (this session)
3. **Kick off Phase 1 analysis agent** (next)
4. **Start Phase 2 in parallel** (next)

## Commands Reference

```bash
# Update CodexMonitor subtree
git subtree pull --prefix=reference/codex-monitor https://github.com/Dimillian/CodexMonitor.git main --squash

# Run Tauri dev
npm run tauri dev

# Build Tauri app
npm run tauri build
```
