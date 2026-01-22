# Claude SDK Server Restart Resilience

**Status:** In Progress
**Version:** 1.0
**Last Updated:** 2026-01-22

---

## 1. Overview
### Purpose
Ensure Claude SDK server processes remain tied to their workspace and recover
cleanly after crashes without losing session history or breaking SSE delivery.

### References
| File | Reason |
| --- | --- |
| `daemon/src/claude_sdk.rs` | Spawn, monitor, and SSE bridge logic |
| `daemon/src/state.rs` | `DaemonState` where runtime registry lives |
| `daemon/src/handlers/claude_sdk.rs` | Status and connect APIs |
| `daemon/claude-server/src/server.ts` | Server startup and port binding |
| `daemon/src/error.rs` | Error type definitions |

### Goals
- Preserve workspace-to-server affinity across restarts.
- Keep a stable or updated `base_url` when the server restarts.
- Restart the SSE bridge when the server process is replaced.
- Maintain Claude session persistence so resume tokens remain valid.

### Non-Goals
- High-availability clustering or multi-daemon failover.
- Persisting daemon runtime state across machine reboots.
- Replacing the Claude SDK server storage layer.

---

## 2. Architecture
### Components

```
┌──────────────────────────────────────────────────────────────────┐
│                        Maestro Daemon                            │
│  ┌──────────────────────┐   ┌─────────────────────────────────┐  │
│  │ ClaudeSdkServer      │   │ Port / Runtime Registry         │  │
│  │ - child process      │   │ - workspaceId -> port/baseUrl   │  │
│  │ - SSE bridge         │   │ - restart count, status         │  │
│  └──────────┬───────────┘   └─────────────────────────────────┘  │
│             │
│             ▼
│   monitor_process() restarts + refreshes base_url/SSE
└─────────────┬───────────────────────────────────────────────────┘
              │ HTTP + SSE
              ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Claude SDK Server (Bun)                       │
└──────────────────────────────────────────────────────────────────┘
```

### Dependencies

| Component | Dependency | Notes |
| --- | --- | --- |
| Daemon | `daemon/src/claude_sdk.rs` | Spawn + monitor logic |
| Daemon | `daemon/src/state.rs` | Server registry + runtime state |
| Server | `daemon/claude-server/src/server.ts` | Accepts `MAESTRO_PORT` |

### Module/Folder Layout

```
daemon/src/claude_sdk.rs
daemon/src/state.rs
daemon/src/handlers/claude_sdk.rs
```

---

## 3. Data Model
### Core Types

```rust
struct ClaudeServerRuntime {
    workspace_id: String,
    port: u16,
    base_url: String,        // Format: "http://127.0.0.1:{port}"
    restart_count: u32,      // Consecutive failures; resets on successful Ready
    status: ServerStatus,
}

enum ServerStatus {
    Starting,                // Process spawned, awaiting health check
    Ready,                   // Health check passed (GET /health returns 200)
    Error(String),           // Restart threshold exceeded
}
```

### Storage Schema (if any)
No persistent storage. Runtime data is kept in memory within `DaemonState`.

---

## 4. Interfaces
### Public APIs

| Interface | Purpose |
| --- | --- |
| `claude_sdk_status` | Returns current `base_url` + connection state |
| `claude_sdk_connect_workspace` | Uses the latest runtime `base_url` on spawn |

### Internal APIs
- `spawn_server_process(workspace_path, port)` accepts a concrete port.
- `monitor_process` updates runtime state when a restart occurs.

### Events (names + payloads)
No new external events. Internal runtime state changes are surfaced via logs.

---

## 5. Workflows
### Main Flow
1. Daemon allocates an available port and spawns the Claude server with `MAESTRO_PORT`.
2. Daemon sets status to `Starting` and stores `base_url` (`http://127.0.0.1:{port}`).
3. Daemon polls `GET {base_url}/health` (100ms interval, 30s timeout). On 200 OK,
   status transitions to `Ready` and SSE bridge connects.
4. If the process exits unexpectedly, `monitor_process` sets status to `Starting`,
   waits 1s, then respawns using the same port. If bind fails (EADDRINUSE),
   allocate a new port and update `base_url`.
5. On restart, if `base_url` changed, tear down old SSE bridge and start a new one.
6. After 2 consecutive failures (spawn or health-check timeout), set status to
   `Error` and stop automatic retries.
7. Session history survives restarts because the server persists to SQLite storage.

### Edge Cases

| Case | Handling |
| --- | --- |
| Port already in use on bind | Allocate next available port, update `base_url` in runtime state |
| 2 consecutive restart failures | Set status to `Error`, increment `restart_count`, require explicit `claude_sdk_connect_workspace` call to retry |
| SSE bridge URL mismatch | Restart bridge whenever `base_url` differs from the bridge's current target |
| Status query during restart | Return current state (`Starting`) with stale `base_url`; client should retry on `Ready` |
| Concurrent restart attempt | Skip if status is already `Starting`; let in-flight restart complete |

### Retry/Backoff (if any)
- Restart delay: 1s fixed (existing behavior in `claude_sdk.rs`).
- Health-check: 100ms poll interval, 30s timeout.
- SSE reconnect: existing exponential backoff in `claude_sdk.rs`.

---

## 6. Error Handling
### Error Types
- `CLAUDE_SDK_ERROR` when spawn or restart fails.
- `CLAUDE_SDK_NOT_CONNECTED` when the server is removed after repeated crashes.

### Recovery Strategy
- Retry once automatically, then surface error to the UI.
- Require manual reconnect after repeated failures.

---

## 7. Observability
### Logs
- Log port assignment and base_url updates per workspace.
- Log restart attempts with exit status and attempt count.

### Metrics
Not required for MVP.

### Traces
Not required.

---

## 8. Security and Privacy
### AuthZ/AuthN
No changes; daemon remains the security boundary.

### Data Handling
No new data persistence beyond existing server storage.

---

## 9. Migration or Rollout
### Compatibility Notes
- Additive changes; no protocol changes for clients.

### Rollout Plan
1. Implement runtime tracking and port allocation.
2. Update restart flow to refresh `base_url` and SSE bridge.
3. Validate with manual crash/restart tests.

---

## 10. Open Questions
1. Should runtime port mappings be persisted across daemon restarts?
   *Leaning no for MVP; document that workspace reconnect is required after daemon restart.*
2. Do we need a daemon event to notify the UI when `base_url` changes?
   *Likely no if SSE bridge handles reconnection transparently; revisit if clients cache URLs.*
