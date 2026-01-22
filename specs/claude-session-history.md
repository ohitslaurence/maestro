# Claude Session History and Resume UI

**Status:** Planned
**Version:** 1.0
**Last Updated:** 2026-01-22

---

## 1. Overview
### Purpose
Expose persisted Claude SDK sessions in the Maestro UI so users can browse past
sessions, rehydrate message history, and resume work without losing context.

### Goals
- List Claude sessions per workspace from the daemon-managed Claude SDK server.
- Select a session to load message history into the existing thread UI.
- Resume an existing session by sending new prompts against its session ID.
- Keep the existing streaming UI and event pipeline (OpenCode-compatible events).
- Provide clear UX when history is unavailable or the server is disconnected.

### Non-Goals
- OpenCode session history UI (handled separately).
- Replacing the Claude SDK server persistence layer.
- Cross-workspace or multi-device session synchronization.

---

## 2. Architecture
### Components

```
┌──────────────────────────────────────────────────────────────────┐
│                       Maestro UI (React)                         │
│  ┌───────────────────────┐     ┌──────────────────────────────┐  │
│  │ ClaudeSessionList     │     │ ClaudeThreadView             │  │
│  │ - list/resume         │     │ - useClaudeSession           │  │
│  │ - selection state     │     │ - useOpenCodeThread          │  │
│  └───────────┬───────────┘     └─────────────┬────────────────┘  │
│              │                                 agent:stream_event │
│              ▼                                                  │
│                     Tauri IPC (services/tauri.ts)               │
└──────────────┬──────────────────────────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────────────────────────┐
│                        Maestro Daemon (Rust)                     │
│  - claude_sdk_session_list                                       │
│  - claude_sdk_session_messages                                   │
└──────────────┬──────────────────────────────────────────────────┘
               │ HTTP
               ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Claude SDK Server (Bun)                       │
│  - GET /session                                                 │
│  - GET /session/:id/message (history)                           │
└──────────────────────────────────────────────────────────────────┘
```

### Dependencies

| Component | Dependency | Notes |
| --- | --- | --- |
| UI | `specs/claude-sdk-ui.md` | Extends the current Claude UI integration |
| UI | `specs/streaming-event-schema.md` | Uses existing `agent:stream_event` flow |
| Daemon | `daemon/src/handlers/claude_sdk.rs` | Adds history proxy endpoint |
| Server | `daemon/claude-server/src/server.ts` | Adds history REST endpoint |

### Module/Folder Layout

```
app/src/features/claudecode/components/ClaudeSessionList.tsx
app/src/features/claudecode/hooks/useClaudeSessions.ts
app/src/features/claudecode/components/ClaudeThreadView.tsx
app/src/features/claudecode/hooks/useClaudeSession.ts
app/src/features/opencode/hooks/useOpenCodeThread.ts
app/src/services/tauri.ts
daemon/src/handlers/claude_sdk.rs
daemon/src/protocol.rs
daemon/claude-server/src/server.ts
```

---

## 3. Data Model
### Core Types

```typescript
type ClaudeSessionSummary = {
  id: string;
  title: string;
  parentID?: string;
  time: { created: number; updated: number };
  settings: { maxTurns: number; systemPrompt: { mode: string }; disallowedTools?: string[] };
};

type ClaudeMessageInfo = {
  id: string;
  sessionID: string;
  role: "user" | "assistant";
  time: { created: number; completed?: number };
  summary?: { title?: string; body?: string | null };
  parts?: ClaudePart[];
};

type ClaudePart = {
  id: string;
  messageID: string;
  type: "text" | "reasoning" | "tool" | "step-start" | "step-finish" | "retry" | "agent" | "compaction";
  text?: string;
  tool?: string;
  input?: unknown;
  output?: unknown;
  error?: unknown;
  time?: { start?: number; end?: number };
};
```

### Storage Schema (if any)
No UI storage changes. The Claude SDK server continues to persist sessions and
messages in `~/.maestro/claude/<workspace-hash>/sessions.sqlite`.

---

## 4. Interfaces
### Public APIs

| Layer | Interface | Purpose |
| --- | --- | --- |
| Claude server | `GET /session` | List sessions for the workspace |
| Claude server | `GET /session/:id/message` | Return message + part history for a session |
| Daemon RPC | `claude_sdk_session_list` | Proxy session list to the UI |
| Daemon RPC | `claude_sdk_session_messages` | Proxy session history to the UI |
| Tauri | `claudeSdkSessionList(workspaceId)` | UI session list fetch |
| Tauri | `claudeSdkSessionMessages(workspaceId, sessionId)` | UI history fetch |

### Internal APIs
- `OpenCodeRegistry::proxy_get(base_url, "/session/:id/message")` in the daemon.
- `useOpenCodeThread` history loader receives Claude history responses.

### Events (names + payloads)
The UI continues to consume SSE events via `agent:stream_event`, including:
`message.updated`, `message.part.updated`, `session.created`, `session.updated`,
`session.status`, and `session.error`.

---

## 5. Workflows
### Main Flow
1. User selects a workspace and switches provider to Claude.
2. ClaudeThreadView renders a Claude-only session list and calls `claudeSdkSessionList`.
3. User selects a session; UI calls `claudeSdkSessionMessages` and hydrates
   `useOpenCodeThread` with message history.
4. User sends a new prompt; UI calls `claudeSdkSessionPrompt` using the selected
   session ID.
5. Streaming events append to the hydrated thread as normal.

### Edge Cases

| Case | Handling |
| --- | --- |
| No sessions returned | Show empty state with "New Session" CTA |
| Session not found (404) | Remove from list and prompt to refresh |
| Server disconnected | Show reconnect error and disable selection |
| History load fails | Show error banner and allow retry |

### Retry/Backoff (if any)
Manual retry from the UI; SSE reconnection remains handled by the daemon.

---

## 6. Error Handling
### Error Types
- `CLAUDE_SDK_NOT_CONNECTED` (daemon)
- `SESSION_NOT_FOUND` (server 404)
- `HISTORY_LOAD_FAILED` (UI wrapper error)

### Recovery Strategy
- Prompt reconnect when the daemon reports disconnected.
- Remove stale sessions on 404 and re-fetch list.
- Retry history load on transient failures.

---

## 7. Observability
### Logs
- UI logs history load failures with session ID.
- Daemon logs Claude history proxy errors.

### Metrics
Not required for UI; defer to daemon logging.

### Traces
Not required.

---

## 8. Security and Privacy
### AuthZ/AuthN
No new auth surface. Daemon continues to own network security.

### Data Handling
Message history is fetched from the daemon over IPC only and kept in memory.

---

## 9. Migration or Rollout
### Compatibility Notes
- New history endpoint is additive to the Claude SDK server API.
- UI defaults to "New Session" when history is unavailable.

### Rollout Plan
1. Add the Claude server history endpoint.
2. Add daemon + Tauri command wiring.
3. Wire UI session list and history hydration.

---

## 10. Open Questions
1. Do we need a per-session retention limit in the UI to cap history size?
2. Should history loading be paginated or full by default?
