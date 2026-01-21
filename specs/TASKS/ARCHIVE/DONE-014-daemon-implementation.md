# Task: Implement Remote Daemon

## Objective

Build a standalone Rust daemon that runs on a remote VPS, exposing terminal PTY, git operations, and session management over JSON-RPC. The Tauri app on Mac connects to this daemon via Tailscale.

## Architecture

```
┌─────────────────────┐                  ┌─────────────────────────────────┐
│   macOS (Tauri)     │                  │         VPS (Tailscale)         │
│                     │                  │                                 │
│  React + xterm.js   │                  │  maestro-daemon                 │
│        ▲            │     TCP          │    ├── Terminal PTY (portable-pty)
│        │            │◄────────────────►│    ├── Git Operations           │
│  Rust Proxy Layer   │   JSON-RPC       │    ├── Session Manager          │
│  (thin client)      │   over TCP       │    └── Agent Harnesses (future) │
│                     │                  │                                 │
└─────────────────────┘                  └─────────────────────────────────┘
```

**Key difference from CodexMonitor:** Their daemon spawns `codex app-server` which has its own JSON-RPC protocol. Our daemon is agent-agnostic - it provides raw PTY access and git operations. Agent harnesses will be added later.

## Why Remote Daemon

1. **Agents run on VPS** - Claude Code, Open Code run where the code lives
2. **PTY must be local to agents** - Terminal needs to attach to processes on VPS
3. **Git operations on VPS** - Status, diffs, commits happen where repo is
4. **Mac is just a viewer** - React UI + thin proxy, no local state

---

## Design Decisions Summary

| Decision | Answer |
|----------|--------|
| Session ID format | **Absolute path.** `/home/user/project` is the ID. No opaque IDs. |
| Sessions config | `sessions.json` in `--data-dir` (default `~/.local/share/maestro/`) |
| Multiple clients per terminal | **No.** Single owner per terminal. Disconnect → terminal closed. |
| Auth failure | Error response, allow retry, connection stays open. 30s timeout → close. |
| Large diffs | **Truncate at 1MB.** No streaming in v1. `truncated: true` flag returned. |
| Naming convention | **`snake_case` everywhere.** `terminal_output`, `git_status`, etc. |

---

## Design Decisions (Detail)

### Session Identity

**Session ID = absolute path to project directory.**

No opaque IDs. No ID/path mapping. The absolute path IS the identifier.

```json
{"method": "list_sessions", "params": {}}
// Returns:
{"result": [{"path": "/home/user/projects/foo", "name": "foo"}, ...]}
```

### Session Configuration & Persistence

Sessions are configured via `sessions.json` in the data directory:

```json
{
  "sessions": [
    {"path": "/home/user/projects/foo", "name": "foo"},
    {"path": "/home/user/projects/bar", "name": "bar"}
  ]
}
```

- `path`: absolute path (required, must exist)
- `name`: display name (optional, defaults to directory basename)

**Location:** `--data-dir` flag or `$MAESTRO_DATA_DIR` or `~/.local/share/maestro/`

Future: `session_add` / `session_remove` RPC methods to modify at runtime.

### Authentication

Token-based auth. Token source (in order):
1. `--token` CLI flag
2. `$MAESTRO_DAEMON_TOKEN` env var

**Auth flow:**
- Client must send `auth` as first request
- Non-`auth` requests before authentication → error response (`auth_required`), **connection stays open**
- Failed `auth` → error response (`auth_failed`), **allow retry, connection stays open**
- Auth timeout: **30 seconds from connection open → close socket**
- `--insecure-no-auth` flag for local dev only (skips auth requirement)

**On client disconnect:** All terminals owned by that client are closed immediately.

### Naming Convention

**All method and event names use `snake_case`.** No exceptions.

- Methods: `terminal_open`, `terminal_write`, `terminal_close`, `git_status`, `git_diff`
- Events: `terminal_output`, `terminal_exited`
- Params: `session_id`, `terminal_id` (snake_case)

Do NOT use kebab-case (`terminal-output`) or camelCase (`terminalOutput`).

### Terminal Lifecycle

**Shell:** `$SHELL` env var, fallback to `/bin/bash`

**Working directory:** Session path (the session ID)

**Environment:** Inherit daemon's environment plus:
- `TERM=xterm-256color`
- `COLORTERM=truecolor`

**Cleanup:**
- `terminal_close` → explicit close, kills PTY process
- Client disconnect → implicit close of all terminals owned by that client
- PTY process exits → `terminal_exited` event sent to client, terminal cleaned up

**Terminal ID:** Client-provided string (e.g., "main"). Unique within session.
Key format internally: `{sessionPath}:{terminalId}`

### Concurrency

**v1: Single client per terminal. No sharing.**

- Each terminal is owned by the client that opened it
- `terminal_output` events **only** sent to the owning client
- Other clients **can** open separate terminals in the same session (different terminal_id)
- If owning client disconnects → **terminal is closed immediately**
- No multi-client broadcast in v1

Future consideration: multi-client broadcast mode with explicit opt-in.

### Git Operations & Large Diffs

**Max response size:** 1MB total for `git_diff` content.

**No streaming in v1.** Response is truncated if it exceeds limit.

If diff exceeds limit:
```json
{
  "result": {
    "files": [...],  // files that fit within limit
    "truncated": true,
    "truncated_files": ["path/to/large/file.bin"]
  }
}
```

Files are processed in order; once 1MB is reached, remaining files go to `truncated_files`.

Future: streaming or pagination for large diffs (not v1).

### Error Model

```json
{
  "id": 1,
  "error": {
    "code": "session_not_found",
    "message": "Session not found: /nonexistent/path"
  }
}
```

**Standard error codes:**
- `auth_required` - Request before authentication
- `auth_failed` - Invalid token
- `invalid_params` - Missing or malformed parameters
- `session_not_found` - Session path doesn't exist in config
- `terminal_not_found` - Terminal ID not open
- `terminal_exists` - Terminal ID already open in session
- `git_error` - Git command failed (message has details)
- `internal_error` - Unexpected daemon error

---

## Protocol

Line-delimited JSON-RPC over TCP.

### Request
```json
{"id": 1, "method": "terminal_open", "params": {"session_id": "/home/user/project", "terminal_id": "main", "cols": 80, "rows": 24}}
```

### Success Response
```json
{"id": 1, "result": {"terminal_id": "main"}}
```

### Error Response
```json
{"id": 1, "error": {"code": "session_not_found", "message": "Session not found: /nonexistent"}}
```

### Server→Client Events (no id)
```json
{"method": "terminal_output", "params": {"session_id": "/home/user/project", "terminal_id": "main", "data": "$ "}}
```

---

## RPC Methods

### Authentication

#### `auth`
```
params: {token: string}
result: {ok: true}
error:  auth_failed
```

### Sessions

#### `list_sessions`
```
params: {}
result: [{path: string, name: string}]
```

#### `session_info`
```
params: {session_id: string}
result: {path: string, name: string, has_git: boolean}
error:  session_not_found
```

### Terminal

#### `terminal_open`
```
params: {session_id: string, terminal_id: string, cols: u16, rows: u16}
result: {terminal_id: string}
error:  session_not_found, terminal_exists
```

#### `terminal_write`
```
params: {session_id: string, terminal_id: string, data: string}
result: {}
error:  terminal_not_found
```

#### `terminal_resize`
```
params: {session_id: string, terminal_id: string, cols: u16, rows: u16}
result: {}
error:  terminal_not_found
```

#### `terminal_close`
```
params: {session_id: string, terminal_id: string}
result: {}
error:  terminal_not_found
```

### Git

#### `git_status`
```
params: {session_id: string}
result: {
  branch_name: string,
  staged_files: [{path, status, additions, deletions}],
  unstaged_files: [{path, status, additions, deletions}],
  total_additions: i32,
  total_deletions: i32
}
error:  session_not_found, git_error
```

#### `git_diff`
```
params: {session_id: string}
result: {
  files: [{path: string, diff: string}],
  truncated: boolean,
  truncated_files: [string]  // only if truncated
}
error:  session_not_found, git_error
```

#### `git_log`
```
params: {session_id: string, limit?: u32}  // default 40
result: {
  entries: [{sha, summary, author, timestamp}],
  ahead: i32,
  behind: i32,
  upstream: string | null
}
error:  session_not_found, git_error
```

### Events (daemon → client)

#### `terminal_output`
```
params: {session_id: string, terminal_id: string, data: string}
```

#### `terminal_exited`
```
params: {session_id: string, terminal_id: string, exit_code: i32 | null}
```

---

## Implementation Plan

### Phase 1: Minimal Daemon (this task)

1. **Binary setup**
   - Separate crate: `daemon/` in repo root
   - CLI: `--listen`, `--token`, `--data-dir`, `--insecure-no-auth`
   - TCP listener with tokio

2. **Protocol layer**
   - Line-delimited JSON-RPC parsing
   - Request/response/event types
   - Error serialization

3. **Auth**
   - Token validation
   - 30s auth timeout
   - Connection state machine (unauthenticated → authenticated)

4. **Session management**
   - Load `sessions.json` on startup
   - Validate paths exist
   - `list_sessions`, `session_info` handlers

5. **Terminal PTY**
   - Port `terminal.rs` logic
   - Track terminal ownership per client
   - Cleanup on client disconnect
   - `terminal_output` event streaming

6. **Git operations**
   - Port `sessions.rs` git functions
   - Add truncation for large diffs
   - Run in session directory

### Phase 2: Tauri Proxy Layer (separate task)

- Connect to daemon on startup
- Proxy commands, forward events
- Handle reconnection

### Phase 3: Agent Harnesses (future)

- Claude Code harness
- Open Code harness

---

## File Structure

```
daemon/
  Cargo.toml
  src/
    main.rs           # Entry, arg parsing, TCP accept loop
    config.rs         # CLI args, sessions.json loading
    protocol.rs       # JSON-RPC types, parsing, serialization
    connection.rs     # Per-client state machine, auth, dispatch
    state.rs          # DaemonState (sessions, terminals, clients)
    terminal.rs       # PTY management
    git.rs            # Git operations
    handlers/
      mod.rs
      auth.rs
      sessions.rs
      terminal.rs
      git.rs
```

---

## Configuration

### CLI Arguments

```
maestro-daemon [OPTIONS]

OPTIONS:
  --listen <ADDR>        Bind address [default: 127.0.0.1:4733]
  --token <TOKEN>        Auth token (or set MAESTRO_DAEMON_TOKEN)
  --data-dir <PATH>      Data directory [default: ~/.local/share/maestro]
  --insecure-no-auth     Disable auth (dev only)
  -h, --help             Print help
```

### sessions.json

```json
{
  "sessions": [
    {"path": "/home/user/projects/foo"},
    {"path": "/home/user/projects/bar", "name": "Bar Project"}
  ]
}
```

---

## Testing

```bash
# Start daemon
maestro-daemon --listen 127.0.0.1:4733 --token secret --data-dir ./test-data

# Auth
echo '{"id":1,"method":"auth","params":{"token":"secret"}}' | nc localhost 4733

# List sessions
echo '{"id":2,"method":"list_sessions","params":{}}' | nc localhost 4733

# Open terminal (need persistent connection for this)
# Use a test client or socat for interactive testing
```

---

## Acceptance Criteria

- [ ] Daemon binary builds standalone (`cargo build -p daemon`)
- [ ] Accepts TCP connections on configurable port
- [ ] Auth timeout closes socket after 30s
- [ ] Token auth works (rejects invalid, allows retry)
- [ ] `list_sessions` returns sessions from config
- [ ] `session_info` returns session details
- [ ] `terminal_open` spawns PTY with correct shell/cwd/env
- [ ] `terminal_write` sends input to PTY
- [ ] `terminal_output` events stream to owning client only
- [ ] `terminal_resize` resizes PTY
- [ ] `terminal_close` kills PTY
- [ ] Client disconnect cleans up owned terminals
- [ ] `git_status` returns branch and file status
- [ ] `git_diff` returns diffs with truncation for large files
- [ ] `git_log` returns commit history
- [ ] All errors use standard error codes

---

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
portable-pty = "0.8"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```
