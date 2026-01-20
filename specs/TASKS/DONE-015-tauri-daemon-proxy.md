# Task: Implement Tauri Daemon Proxy Layer

## Objective

Build a thin proxy layer in the Tauri app that connects to the remote maestro-daemon, proxies commands from the React frontend, and forwards events back. The Mac app becomes a pure viewer with no local state.

## Architecture

```
┌─────────────────────────────────────────┐
│            macOS (Tauri App)            │
│                                         │
│  ┌─────────────┐    ┌────────────────┐  │
│  │   React UI  │◄──►│  Tauri Commands│  │
│  │  xterm.js   │    │  (thin proxy)  │  │
│  └─────────────┘    └───────┬────────┘  │
│                             │           │
│                    ┌────────▼────────┐  │
│                    │  DaemonClient   │  │
│                    │  - TCP connect  │  │
│                    │  - JSON-RPC     │  │
│                    │  - Event fwd    │  │
│                    └────────┬────────┘  │
└─────────────────────────────┼───────────┘
                              │ TCP (Tailscale)
                              ▼
                    ┌─────────────────────┐
                    │   maestro-daemon    │
                    │   (VPS)             │
                    └─────────────────────┘
```

## Design Decisions

| Decision | Answer |
|----------|--------|
| Connection lifecycle | Connect on app start, reconnect on disconnect |
| Config storage | `daemon.json` in Tauri app data dir |
| Auth token storage | In `daemon.json` (local dev) or system keychain (future) |
| Event forwarding | Tauri events emitted to frontend |
| Error handling | Surface connection errors to UI, allow manual reconnect |
| Multiple daemons | Single daemon connection in v1 |

---

## Configuration

### daemon.json

Stored in Tauri app data directory (`~/Library/Application Support/com.maestro.app/`):

```json
{
  "host": "100.102.242.109",
  "port": 4733,
  "token": "secret"
}
```

Or using Tailscale MagicDNS:
```json
{
  "host": "gondor",
  "port": 4733,
  "token": "secret"
}
```

---

## Tauri Commands (Frontend API)

All commands proxy to daemon. Frontend doesn't know about TCP/JSON-RPC.

### Connection Management

#### `daemon_connect`
```typescript
invoke('daemon_connect') → Promise<{connected: boolean}>
```
Connect to daemon using stored config. Called on app start.

#### `daemon_disconnect`
```typescript
invoke('daemon_disconnect') → Promise<void>
```
Disconnect from daemon.

#### `daemon_status`
```typescript
invoke('daemon_status') → Promise<{
  connected: boolean,
  host?: string,
  port?: number
}>
```

#### `daemon_configure`
```typescript
invoke('daemon_configure', {host: string, port: number, token: string}) → Promise<void>
```
Save daemon config. Disconnects if connected, does not auto-reconnect.

### Session Commands (Proxy to Daemon)

#### `list_sessions`
```typescript
invoke('list_sessions') → Promise<Session[]>
// Session = {path: string, name: string}
```

#### `session_info`
```typescript
invoke('session_info', {sessionId: string}) → Promise<SessionInfo>
// SessionInfo = {path: string, name: string, has_git: boolean}
```

### Terminal Commands (Proxy to Daemon)

#### `terminal_open`
```typescript
invoke('terminal_open', {
  sessionId: string,
  terminalId: string,
  cols: number,
  rows: number
}) → Promise<{terminal_id: string}>
```

#### `terminal_write`
```typescript
invoke('terminal_write', {
  sessionId: string,
  terminalId: string,
  data: string
}) → Promise<void>
```

#### `terminal_resize`
```typescript
invoke('terminal_resize', {
  sessionId: string,
  terminalId: string,
  cols: number,
  rows: number
}) → Promise<void>
```

#### `terminal_close`
```typescript
invoke('terminal_close', {
  sessionId: string,
  terminalId: string
}) → Promise<void>
```

### Git Commands (Proxy to Daemon)

#### `git_status`
```typescript
invoke('git_status', {sessionId: string}) → Promise<GitStatus>
```

#### `git_diff`
```typescript
invoke('git_diff', {sessionId: string}) → Promise<GitDiff>
```

#### `git_log`
```typescript
invoke('git_log', {sessionId: string, limit?: number}) → Promise<GitLog>
```

---

## Tauri Events (Daemon → Frontend)

Events from daemon are forwarded as Tauri events.

### `daemon:connected`
```typescript
{connected: true}
```

### `daemon:disconnected`
```typescript
{reason?: string}
```

### `daemon:terminal_output`
```typescript
{session_id: string, terminal_id: string, data: string}
```

### `daemon:terminal_exited`
```typescript
{session_id: string, terminal_id: string, exit_code: number | null}
```

---

## Implementation Plan

### Phase 1: DaemonClient Core

1. **Config loading**
   - Load/save `daemon.json` from app data dir
   - Validate config on load

2. **TCP connection**
   - Async connect with timeout
   - Line-delimited JSON-RPC read/write
   - Auth on connect

3. **Request/response handling**
   - Request ID tracking
   - Response matching
   - Timeout handling

4. **Event forwarding**
   - Parse server events (no `id` field)
   - Emit as Tauri events

5. **Reconnection logic**
   - Detect disconnection
   - Auto-reconnect with backoff
   - Emit status events

### Phase 2: Tauri Commands

1. **Connection commands**
   - `daemon_connect`, `daemon_disconnect`, `daemon_status`, `daemon_configure`

2. **Proxy commands**
   - Implement all session/terminal/git commands
   - Map errors to user-friendly messages

### Phase 3: Frontend Integration

1. **Update hooks**
   - Replace direct Tauri command calls with daemon-proxied versions
   - Handle connection state

2. **Connection UI**
   - Show connection status
   - Settings for daemon host/port/token
   - Manual reconnect button

---

## File Structure

```
app/src-tauri/src/
  daemon/
    mod.rs          # Module exports
    client.rs       # DaemonClient - TCP, JSON-RPC, reconnect
    config.rs       # Config loading/saving
    protocol.rs     # Request/response/event types
    commands.rs     # Tauri command handlers
  lib.rs            # Register daemon commands
```

---

## Error Handling

### Connection Errors
- `daemon_not_configured` - No daemon.json
- `daemon_connection_failed` - TCP connect failed
- `daemon_auth_failed` - Token rejected
- `daemon_disconnected` - Lost connection mid-operation

### Proxy Errors
Pass through daemon error codes:
- `session_not_found`
- `terminal_not_found`
- `terminal_exists`
- `git_error`

---

## State Management

```rust
struct DaemonState {
    client: Option<DaemonClient>,
    config: Option<DaemonConfig>,
}

struct DaemonClient {
    stream: TcpStream,
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    pending_requests: HashMap<u64, oneshot::Sender<Result<Value>>>,
    next_id: AtomicU64,
}
```

---

## Acceptance Criteria

- [ ] App loads daemon config from `daemon.json`
- [ ] `daemon_connect` establishes TCP connection and authenticates
- [ ] `daemon_disconnect` cleanly closes connection
- [ ] `daemon_status` returns connection state
- [ ] `daemon_configure` saves config to disk
- [ ] All session/terminal/git commands proxy to daemon
- [ ] `terminal_output` events forwarded to frontend
- [ ] `terminal_exited` events forwarded to frontend
- [ ] Connection errors surface to UI
- [ ] Auto-reconnect on disconnect with backoff
- [ ] Existing terminal UI works with remote daemon

---

## Testing

```bash
# 1. Configure daemon (in app or manually create daemon.json)
# 2. Start app - should auto-connect
# 3. Verify sessions list populates
# 4. Open terminal - should work like local
# 5. Kill daemon - app should show disconnected
# 6. Restart daemon - app should reconnect
```

---

## Migration Notes

### Commands to Remove/Replace

Current local commands in `lib.rs` that will be replaced:

- `list_sessions` → proxy to daemon
- `terminal_open` → proxy to daemon
- `terminal_write` → proxy to daemon
- `terminal_resize` → proxy to daemon
- `terminal_close` → proxy to daemon
- `get_git_status` → proxy to daemon (rename to `git_status`)
- `get_git_diffs` → proxy to daemon (rename to `git_diff`)
- `get_git_log` → proxy to daemon (rename to `git_log`)

Keep local:
- `spawn_session`, `stop_session` - will be agent harness commands (future)

### Frontend Changes

Update service layer to:
1. Check daemon connection before commands
2. Handle `daemon:disconnected` events
3. Show connection status in UI
