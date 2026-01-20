# Task: CodexMonitor Deep Analysis

## Objective
Analyze the CodexMonitor codebase (`reference/codex-monitor/`) and document the patterns, architecture, and implementation details we need to adapt for our agent orchestrator.

## Output
Create `specs/CODEX_MONITOR_ANALYSIS.md` with findings organized by section below.

---

## 1. Architecture Overview

Analyze and document:
- Overall project structure
- How Tauri frontend/backend communicate (IPC patterns)
- State management approach (React side)
- Event flow (backend → frontend)

Key files:
- `src-tauri/src/lib.rs` - Main Tauri setup
- `src/services/tauri.ts` - Frontend IPC wrapper
- `src/services/events.ts` - Event handling

---

## 2. Terminal Implementation

This is critical - we need to stream terminal I/O from remote agents.

Analyze:
- How `portable-pty` is used to spawn/manage PTY sessions
- How terminal output is streamed to frontend
- How user input is sent back to PTY
- xterm.js integration on frontend
- Resize handling
- Session lifecycle (open, close, cleanup)

Key files:
- `src-tauri/src/terminal.rs` - PTY handling
- `src/features/terminal/components/TerminalPanel.tsx`
- `src/features/terminal/hooks/useTerminalSession.ts`
- `src/features/terminal/hooks/useTerminalController.ts`

Document:
- The exact Tauri commands exposed
- Event names for terminal output
- Data format for terminal data
- Any buffering/throttling patterns

---

## 3. Git Integration

We need git status, diffs, log, branches per session.

Analyze:
- How `git2` crate is used for git operations
- What git data is exposed to frontend
- How diffs are generated and formatted
- Staged vs unstaged change handling
- Commit history retrieval

Key files:
- `src-tauri/src/git.rs` - Git operations
- `src-tauri/src/git_utils.rs` - Utilities
- `src/features/git/hooks/useGitStatus.ts`
- `src/features/git/hooks/useGitDiffs.ts`
- `src/features/git/hooks/useGitLog.ts`

Document:
- Tauri commands for git operations
- Data structures returned (TypeScript types)
- How often git status is polled/refreshed

---

## 4. Diff Rendering (@pierre/diffs)

Analyze:
- How `@pierre/diffs` is integrated
- Worker pool setup for syntax highlighting
- Options/configuration used
- How diffs are parsed and rendered

Key files:
- `src/features/git/components/GitDiffViewer.tsx`
- `src/features/git/components/DiffBlock.tsx`
- `src/utils/diff.ts`
- `src/utils/diffsWorker.ts`

Document:
- Import patterns
- Configuration options
- CSS/styling approach
- Performance optimizations (virtualization)

---

## 5. Workspace/Session Management

Analyze:
- How workspaces are defined and persisted
- Session discovery and tracking
- Worktree support implementation
- How CWD is associated with sessions

Key files:
- `src-tauri/src/workspaces.rs`
- `src-tauri/src/state.rs`
- `src/features/workspaces/hooks/useWorkspaces.ts`

Document:
- Workspace data structure
- Persistence mechanism (where stored?)
- How workspaces map to agent sessions

---

## 6. Remote Backend (Daemon POC)

This is key for our VPS daemon architecture.

Analyze:
- How the daemon exposes JSON-RPC over TCP
- Authentication mechanism
- What methods are implemented
- Event streaming approach
- How frontend would connect to remote daemon

Key files:
- `src-tauri/src/backend/` - Entire directory
- `src-tauri/src/bin/codex_monitor_daemon.rs`
- `REMOTE_BACKEND_POC.md`

Document:
- Protocol format (JSON-RPC specifics)
- Auth handshake flow
- Method signatures
- Event notification format
- Connection lifecycle

---

## 7. UI Patterns

Analyze:
- Component structure in `src/features/`
- Layout system (sidebar, main panel, tabs)
- Styling approach (CSS files, any patterns)
- Responsive/layout hooks

Key files:
- `src/features/layout/` - Layout components
- `src/features/app/components/Sidebar.tsx`
- `src/styles/` - CSS files

Document:
- Feature-sliced architecture pattern
- Common component patterns
- State lifting patterns
- CSS organization

---

## 8. Recommendations for Our Implementation

Based on analysis, provide:

1. **Copy directly** - Code/patterns we can use almost as-is
2. **Adapt** - Patterns that need modification for our use case
3. **Skip** - Codex-specific stuff we don't need
4. **Build new** - Things CodexMonitor doesn't have that we need

For each, explain why and how.

---

## Constraints

- Focus on implementation details, not high-level descriptions
- Include specific code snippets where helpful
- Note any gotchas or non-obvious patterns
- Flag any dependencies we might want to swap out
- Keep findings actionable for implementation

## Time Budget

This is a research task - be thorough. Read the actual code, don't just skim.
