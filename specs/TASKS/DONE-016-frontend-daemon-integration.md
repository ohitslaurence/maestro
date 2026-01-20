# Task 016: Frontend Daemon Integration

## Status: DONE

## Objective

Update the React frontend to work with the new daemon proxy layer. Add connection management UI, update service layer to match new command/event names, and provide visual feedback for connection state.

## Background

Task 015 added a Tauri daemon proxy layer that:
- Connects to remote daemon over TCP with JSON-RPC
- Proxies terminal/session/git commands
- Forwards daemon events to frontend via Tauri events
- Auto-connects on startup if configured
- Auto-reconnects with exponential backoff

The frontend currently expects the old local command names and event formats. This task bridges that gap.

## Changes Required

### 1. Update Service Layer (`app/src/services/tauri.ts`)

**Rename commands:**
```typescript
// Old → New
get_git_status → git_status
get_git_diffs → git_diff
get_git_log → git_log
```

**Add daemon management commands:**
```typescript
export type DaemonStatus = {
  connected: boolean;
  host?: string;
  port?: number;
};

export async function daemonConfigure(
  host: string,
  port: number,
  token: string,
): Promise<void>;

export async function daemonConnect(): Promise<{ connected: boolean }>;

export async function daemonDisconnect(): Promise<void>;

export async function daemonStatus(): Promise<DaemonStatus>;
```

**Update session types:**
```typescript
// Old: string[]
// New: SessionInfo[]
export type SessionInfo = {
  path: string;
  name: string;
};

export async function listSessions(): Promise<SessionInfo[]>;
```

### 2. Update Event Subscriptions (`app/src/services/events.ts`)

**Change event names:**
```typescript
// Old → New
"terminal-output" → "daemon:terminal_output"
```

**Add new event subscriptions:**
```typescript
export type DaemonConnectionEvent = {
  connected: boolean;
  reason?: string;
};

export type TerminalExitedEvent = {
  sessionId: string;
  terminalId: string;
  exitCode?: number;
};

export function subscribeDaemonConnection(
  onEvent: (event: DaemonConnectionEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe;

export function subscribeTerminalExited(
  onEvent: (event: TerminalExitedEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe;
```

**Update terminal output event field names:**
```typescript
// Daemon sends snake_case, need to handle:
export type TerminalOutputEvent = {
  session_id: string;  // was sessionId
  terminal_id: string; // was terminalId
  data: string;
};
```

### 3. Update Types (`app/src/types/index.ts`)

Add/update types to match daemon protocol:
```typescript
export type SessionInfo = {
  path: string;
  name: string;
};

export type GitStatusResult = {
  branchName: string;
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  totalAdditions: number;
  totalDeletions: number;
};

export type GitDiffResult = {
  files: GitFileDiff[];
  truncated: boolean;
  truncatedFiles: string[];
};

export type GitLogResult = {
  entries: GitLogEntry[];
  ahead: number;
  behind: number;
  upstream?: string;
};
```

### 4. Update Sessions Hook (`app/src/features/sessions/hooks/useSessions.ts`)

- Change return type from `string[]` to `SessionInfo[]`
- Update session selection to use `SessionInfo.path` as identifier
- Handle daemon disconnection gracefully

### 5. Update Terminal Hook (`app/src/features/terminal/hooks/useTerminalSession.ts`)

- Update event field names (`session_id` → `sessionId` mapping)
- Subscribe to `daemon:terminal_exited` for cleanup
- Handle connection loss (show reconnecting state)

### 6. Add Connection Status Hook (`app/src/features/daemon/hooks/useDaemonConnection.ts`)

```typescript
export type DaemonConnectionState = {
  status: 'disconnected' | 'connecting' | 'connected' | 'error';
  host?: string;
  port?: number;
  error?: string;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  configure: (host: string, port: number, token: string) => Promise<void>;
};

export function useDaemonConnection(): DaemonConnectionState;
```

### 7. Add Settings Modal (`app/src/features/daemon/components/SettingsModal.tsx`)

Modal with:
- Host input (default: localhost or last configured)
- Port input (default: 4733)
- Token input (password field)
- Connect/Disconnect button
- Status indicator
- Error display

### 8. Add Status Indicator (`app/src/features/daemon/components/ConnectionStatus.tsx`)

Small indicator showing:
- 🟢 Connected to `host:port`
- 🟡 Connecting...
- 🔴 Disconnected (click to configure)

Place in sidebar header.

### 9. Update App.tsx

- Add daemon connection state
- Show settings modal on first run or when disconnected
- Pass connection status to components
- Disable session list when disconnected

### 10. Update Styles

Add styles for:
- Settings modal
- Connection status indicator
- Disabled/loading states when disconnected

## File Structure

```
app/src/
├── features/
│   ├── daemon/
│   │   ├── index.ts
│   │   ├── hooks/
│   │   │   └── useDaemonConnection.ts
│   │   └── components/
│   │       ├── SettingsModal.tsx
│   │       └── ConnectionStatus.tsx
│   ├── sessions/
│   │   └── hooks/
│   │       └── useSessions.ts  (update)
│   └── terminal/
│       └── hooks/
│           └── useTerminalSession.ts  (update)
├── services/
│   ├── tauri.ts  (update)
│   └── events.ts  (update)
├── types/
│   └── index.ts  (update)
└── App.tsx  (update)
```

## Implementation Order

1. Update `types/index.ts` with new types
2. Update `services/tauri.ts` with new commands
3. Update `services/events.ts` with new event names
4. Create `features/daemon/` module with hook and components
5. Update `useSessions` hook for new session format
6. Update `useTerminalSession` hook for new event format
7. Update `App.tsx` to integrate daemon connection
8. Add styles
9. Test end-to-end with running daemon

## Testing Checklist

- [ ] App shows settings modal when no daemon configured
- [ ] Can configure daemon host/port/token
- [ ] Status indicator shows connected/disconnected
- [ ] Session list populates from daemon
- [ ] Terminal connects and streams output
- [ ] Terminal input works
- [ ] Git status/diff/log commands work
- [ ] Auto-reconnect works after connection drop
- [ ] Graceful handling of daemon unavailable

## Error Handling

Display user-friendly messages for:
- `daemon_not_configured` → Show settings modal
- `daemon_connection_failed` → "Cannot reach daemon at host:port"
- `daemon_auth_failed` → "Invalid token"
- `daemon_disconnected` → Show reconnecting indicator

## Notes

- The daemon uses snake_case for JSON fields; frontend uses camelCase
- Tauri's serde will handle some conversion, but events come through as-is
- Keep local terminal/git implementations in Rust for potential offline mode later
