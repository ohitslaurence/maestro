# CodexMonitor Deep Analysis

## Executive Summary

CodexMonitor is a macOS Tauri desktop app orchestrating Codex agents across local and remote workspaces. It demonstrates production-grade patterns for:
- IPC between Rust backend and React frontend
- PTY management and streaming terminal I/O
- Git integration via libgit2
- Diff rendering with worker pools
- Remote daemon architecture via JSON-RPC over TCP
- Feature-sliced React architecture with sophisticated panel management

---

## 1. Architecture Overview

### Project Structure
```
src/                          # React + TypeScript frontend
  features/                   # Feature-sliced architecture
    app/                      # Core app orchestration
    terminal/                 # Terminal UI & hooks
    git/                      # Git operations & UI
    workspaces/               # Workspace management
    layout/                   # Resizable panels & responsive layouts
    composer/                 # Message composition UI
    threads/                  # Thread/conversation management
  services/
    tauri.ts                  # Tauri IPC wrapper (all backend calls)
    events.ts                 # Event hub (one native listen per event)
  utils/
    diff.ts                   # Diff parsing
    diffsWorker.ts            # Worker pool setup
src-tauri/                    # Rust backend
  src/
    lib.rs                    # Tauri setup & command handlers
    state.rs                  # Global AppState (workspaces, sessions, terminal_sessions)
    codex.rs                  # App-server client & session spawning
    terminal.rs               # PTY management
    git.rs                    # Git2 wrapper + GitHub CLI integration
    backend/
      app_server.rs           # WorkspaceSession (JSON-RPC to codex app-server)
      events.rs               # Event trait + types
    bin/codex_monitor_daemon.rs   # Remote daemon binary
```

### IPC Patterns

**Tauri Command Flow:**
1. Frontend calls `invoke("command_name", params)` in `src/services/tauri.ts`
2. Maps to `#[tauri::command]` in `src-tauri/src/lib.rs`
3. Handler accesses `AppState` via `State<'_, AppState>`
4. Returns `Result<T, String>` (error serialized as string)

**State Management:**
- `AppState` holds:
  - `workspaces: Mutex<HashMap<String, WorkspaceEntry>>` - persisted to `workspaces.json`
  - `sessions: Mutex<HashMap<String, Arc<WorkspaceSession>>>` - active codex app-server processes
  - `terminal_sessions: Mutex<HashMap<String, Arc<TerminalSession>>>` - active PTY sessions
- Session keys are `workspace_id:terminal_id` for keyed lookup
- All mutations wrapped in `Mutex<T>` for thread safety

**Event Flow:**
1. Backend emits: `app.emit("event-name", payload)`
2. Event hub pattern (events.ts): `createEventHub<T>(eventName)` creates single native listener
3. Multiple React subscribers fan out without duplicating listeners
4. Listeners wrapped in try/catch to prevent blocking

---

## 2. Terminal Implementation

### PTY Management (terminal.rs)

**Libraries:** `portable-pty` 0.8

**Session Structure:**
```rust
pub struct TerminalSession {
    pub id: String,
    pub master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub child: Mutex<Box<dyn portable_pty::Child + Send>>,
}
```

**Opening a Session:**
1. Create PTY via `native_pty_system().openpty(size)`
2. Spawn shell: `CommandBuilder::new(shell_path()).cwd(cwd).arg("-i").env("TERM", "xterm-256color")`
3. Spawn background thread to read PTY output in 8KB chunks
4. Emit `terminal-output` events with data
5. Store session in HashMap

**Writing Input:**
```rust
let session = state.terminal_sessions.lock().await.get(&key)?;
let mut writer = session.writer.lock().await;
writer.write_all(data.as_bytes())?;
```

### Frontend Integration

**xterm.js Setup:**
```typescript
const terminal = new Terminal({
  cursorBlink: true,
  fontSize: 12,
  fontFamily: "Menlo, Monaco, Courier New, monospace",
  scrollback: 5000,
});
const fitAddon = new FitAddon();
terminal.loadAddon(fitAddon);
```

**Output Buffering:**
- Maintains buffer per session, capped at 200KB
- Prevents memory leaks for long-running shells

**Resize Handling:**
- Uses `ResizeObserver` on container
- Calls `fitAddon.fit()` and syncs PTY size

### Key Gotchas

1. PTY reads block until shell exits - must spawn in separate thread
2. Raw terminal data is UTF-8 lossy, buffered at JS level
3. Session resurrection: detect "session not found" and cleanup gracefully

---

## 3. Git Integration

### git2 Crate Usage

**Status Mapping:**
```rust
fn status_for_index(status: Status) -> Option<&'static str> {
    if status.contains(Status::INDEX_NEW) { Some("A") }
    else if status.contains(Status::INDEX_MODIFIED) { Some("M") }
    else if status.contains(Status::INDEX_DELETED) { Some("D") }
    // ...
}
```

**get_git_status:**
1. Opens repo: `Repository::open(path)`
2. Iterates statuses, separates into `staged` vs `unstaged`
3. Returns `Vec<GitFileStatus>` with path, status, added/deleted counts

**Frontend Polling:**
```typescript
const REFRESH_INTERVAL_MS = 3000;
// Polls every 3 seconds if workspace is active
```

### GitHub Integration

Uses `gh` CLI via subprocess:
- `get_github_issues`: Runs `gh issue list --json ...`
- `get_github_pull_requests`: Runs `gh pr list --json ...`
- `get_github_pull_request_diff`: Runs `gh pr diff <number>`

---

## 4. Diff Rendering (@pierre/diffs)

### Worker Pool Setup

```typescript
import WorkerUrl from "@pierre/diffs/worker/worker.js?worker&url";
export function workerFactory(): Worker {
  return new Worker(WorkerUrl, { type: "module" });
}
```

### Diff Rendering

```typescript
const diffOptions = {
  diffStyle: "split",           // Side-by-side view
  hunkSeparators: "line-info",  // Show @@ lines
  overflow: "scroll",
  disableFileHeader: true,
};

<FileDiff
  file={fileDiff}
  options={diffOptions}
  workerFactory={workerFactory}
/>
```

### Key Decisions

1. Worker pool offloads syntax highlighting
2. Lazy memoization - only parse diffs when selected
3. Virtualization via `@tanstack/react-virtual` for large diffs

---

## 5. Workspace/Session Management

### Workspace Types

```rust
pub struct WorkspaceEntry {
    pub id: String,                    // UUID
    pub name: String,
    pub path: String,                  // Absolute path to project
    pub kind: WorkspaceKind,          // "main" or "worktree"
    pub parent_id: Option<String>,    // For worktrees
}
```

### Persistence

- Storage: Tauri app data dir (`~/Library/Application Support/...`)
- Files: `workspaces.json`, `settings.json`
- Format: JSON via serde

### Session Management

```rust
pub struct AppState {
    pub workspaces: Mutex<HashMap<String, WorkspaceEntry>>,
    pub sessions: Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    pub terminal_sessions: Mutex<HashMap<String, Arc<TerminalSession>>>,
}
```

**Key Patterns:**
- `Arc<Mutex<T>>` for shared async state
- Atomic IDs for request correlation in JSON-RPC
- Lazy session spawning - created on-demand

---

## 6. Remote Backend Daemon POC

### Protocol

- TCP line-delimited JSON-RPC over `host:port` (default `127.0.0.1:4732`)

**Auth Handshake:**
```json
{"id": 1, "method": "auth", "params": {"token": "..."}}
```

**Request/Response:**
```json
{"id": 2, "method": "list_workspaces", "params": {}}
{"id": 2, "result": [...]}
```

**Events (no ID):**
```json
{"method": "app-server-event", "params": {"workspace_id": "...", "message": {...}}}
```

### Daemon Implementation

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(handle_client(socket, state.clone()));
    }
}
```

**Implemented Methods:**
- `auth`, `ping`
- `list_workspaces`, `add_workspace`, `remove_workspace`
- `start_thread`, `resume_thread`, `send_user_message`
- `get_git_status`, `get_git_diffs`

### Key Architecture Notes

1. Single TCP connection per client - multiplex by ID
2. Line-delimited JSON - easy to debug
3. Broadcast events to all connected clients
4. Simple token auth (POC level)

---

## 7. UI Patterns

### Feature-Sliced Architecture

```
src/features/
  <feature>/
    components/     # Presentational only
    hooks/          # State + effects
```

**Component Design:**
- Presentational: props → JSX, no Tauri calls
- Hooks: own state, effects, event subscriptions
- Composition hooks: combine multiple hooks

### Layout System

**Resizable Panels:**
```typescript
const [sidebarWidth, setSidebarWidth] = useState(() =>
  readStoredWidth(STORAGE_KEY, DEFAULT, MIN, MAX)
);
// Persist to localStorage on change
```

**Responsive:**
- `DesktopLayout` - 3-column
- `TabletLayout` - collapsed sidebar
- `PhoneLayout` - tabs only

### CSS Organization

```
src/styles/
  base.css, buttons.css, sidebar.css, main.css,
  terminal.css, diff-viewer.css, compact-*.css
```

No CSS-in-JS - pure `.css` files

### Event Hub Pattern

```typescript
function createEventHub<T>(eventName: string) {
  const listeners = new Set<Listener<T>>();
  let unlisten: Unsubscribe | null = null;

  const subscribe = (onEvent: Listener<T>): Unsubscribe => {
    listeners.add(onEvent);
    if (!unlisten) startListening();
    return () => {
      listeners.delete(onEvent);
      if (listeners.size === 0) stopListening();
    };
  };
  return { subscribe };
}
```

---

## 8. Recommendations for Maestro

### Copy Directly

1. **Tauri IPC Wrapper Pattern** - simple function-per-command, type-safe
2. **Event Hub Pattern** - elegant single-listen-per-event
3. **Terminal Implementation** - production-ready PTY + xterm.js
4. **Feature-Sliced Architecture** - clear separation of concerns
5. **Resizable Panels Pattern** - localStorage persistence, constraints

### Adapt

1. **Session Management**
   - CodexMonitor: one agent per workspace
   - Maestro: many agents per workspace
   - Use hierarchical keying: `workspace_id:agent_id:session_id`

2. **Git Integration**
   - Make workspace optional, resolve git root per-agent
   - Cache git operations (3s polling may be expensive at scale)

3. **Remote Daemon Protocol**
   - Consider JWT or mutual TLS instead of token
   - Add protocol versioning

4. **State Management**
   - Consider `DashMap` for lock-free reads at scale
   - Add observability hooks to state mutations

### Skip

1. **Worktree Support** - different isolation model likely
2. **Codex-Specific Commands** - rewrite for harness abstraction
3. **GitHub Integration** - make optional/plugin if needed
4. **Dictation Support** - out of scope
5. **In-App Updater** - handle via different mechanism

### Build New

1. **Multi-Agent Orchestration** - agent groups, scheduling, prioritization
2. **Agent State Lifecycle** - IDLE → RUNNING → PAUSED → FAILED → ARCHIVED
3. **Cross-Agent Output Aggregation** - multiplexed terminal with context
4. **Session Persistence & Replay** - snapshot/restore
5. **Observability & Metrics** - traces, resource monitoring per-agent
6. **Conflict Detection** - file-level locking for concurrent agents

---

## Technical Debt & Gotchas

### Architectural
- No middleware pattern - each hook subscribes independently
- Request correlation relies on client tracking IDs

### Performance
- Git status polling (3s) may be expensive at scale - consider file watchers
- Terminal buffer cap (200KB) loses history for long commands

### Platform-Specific
- macOS private APIs for titlebar effects - not portable
- Whisper dictation not available on Windows

### Maintenance
- JSON-RPC protocol has no versioning
- Storage format (JSON files) not validated on load

---

## Dependency Analysis

### Frontend
- `@tauri-apps/api` - IPC, events
- `@xterm/xterm` + `@xterm/addon-fit` - terminal
- `@pierre/diffs/react` - diff rendering
- `@tanstack/react-virtual` - virtualization

### Backend
- `tauri` 2 - app framework
- `tokio` - async runtime
- `git2` 0.20.3 - libgit2 bindings
- `portable-pty` 0.8 - PTY abstraction

---

## Conclusion

CodexMonitor demonstrates mature patterns for Tauri + React integration, real-time terminal I/O, and Git workflows. For Maestro, adapt the **architecture and patterns** rather than copy-paste code.

**Priority areas to adopt:**
1. Tauri IPC wrapper design
2. Event hub for agent lifecycle events
3. Terminal streaming (PTY + xterm.js)
4. Feature-sliced component structure
5. Remote backend architecture (with auth improvements)
