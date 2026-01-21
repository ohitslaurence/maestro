# Claude SDK UI Integration

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview

### Purpose

Surface Claude SDK conversations in the Maestro UI by connecting to the daemon-managed
Claude SDK server, streaming OpenCode-compatible events, and rendering them in the
existing thread UI.

### Goals

1. Provide a Claude provider option in the agent selector.
2. Auto-connect to the Claude SDK server per workspace and expose connection status.
3. Create Claude sessions, send prompts, and support abort from the UI.
4. Reuse OpenCode thread rendering (`ThreadMessages`, `ThreadComposer`, `useOpenCodeThread`).
5. Show connection and stream errors with clear retry affordances.
6. Enable automated UI validation of a Claude conversation via Playwright.

### Non-Goals

- Building a Claude-specific message renderer or alternate thread UI.
- UI for model/permission selection beyond the server defaults.
- Persisting conversation history in the UI (handled by the server).

---

## 2. Architecture

### Components

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Maestro UI (React)                            │
│  ┌────────────────────────────┐       ┌──────────────────────────────┐  │
│  │ AgentView / Provider Select│       │ ClaudeThreadView             │  │
│  └───────────────┬────────────┘       │ - useClaudeSession            │  │
│                  │                    │ - useOpenCodeThread           │  │
│                  ▼                    └──────────────┬───────────────┘  │
│            Tauri invoke/bridge                      │                  │
└─────────────────────────────────────────────────────┼──────────────────┘
                                                      │ agent:stream_event
                                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                            Maestro Daemon                               │
│  - claude_sdk_* RPC commands                                            │
│  - opencode:event SSE (from Claude SDK server)                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Dependencies

| Component | Dependency | Notes |
| --- | --- | --- |
| UI | `specs/claude-sdk-server.md` | Server emits OpenCode-compatible events |
| UI | `specs/streaming-event-schema.md` | Event envelope for `agent:stream_event` |
| UI | React + Tauri bridge | `services/tauri.ts`, `services/events.ts` |
| UI tests | Playwright | `app/scripts/ui-*.ts` automation |

### Module/Folder Layout

```
app/src/features/agent/components/AgentView.tsx
app/src/features/agent/components/AgentProviderSelector.tsx
app/src/features/claudecode/components/ClaudeThreadView.tsx
app/src/features/claudecode/hooks/useClaudeSession.ts
app/src/features/opencode/hooks/useOpenCodeThread.ts
app/src/services/tauri.ts
app/src/services/events.ts
app/src/services/web/opencodeAdapter.ts
app/src/services/web/daemon.ts
app/src/types.ts
app/scripts/ui-claude-conversation.ts
```

---

## 3. Data Model

### Core Types

```typescript
type AgentHarness = "open_code" | "claude_code";

type ClaudeSessionState = {
  isConnected: boolean;
  isConnecting: boolean;
  connectionError: string | null;
  sessionId: string | null;
  isPrompting: boolean;
};

type PendingUserMessage = {
  id: string;
  text: string;
  timestamp: number;
};

type OpenCodeThreadStatus = "idle" | "processing" | "error";
```

### Storage Schema

None. The UI keeps ephemeral state only. Session persistence lives in
`~/.maestro/claude/` per `specs/claude-sdk-server.md`.

---

## 4. Interfaces

### Public APIs

Tauri commands invoked by the UI (see `app/src/services/tauri.ts`):

| Command | Input | Output | Notes |
| --- | --- | --- | --- |
| `claude_sdk_connect_workspace` | `{ workspaceId, workspacePath }` | `{ workspaceId, baseUrl }` | Spawns/attaches server |
| `claude_sdk_status` | `{ workspaceId }` | `{ connected, baseUrl? }` | Used for auto-connect |
| `claude_sdk_session_create` | `{ workspaceId, title? }` | `{ id }` | New session |
| `claude_sdk_session_prompt` | `{ workspaceId, sessionId, message }` | `{ info, parts }` | Streams via SSE |
| `claude_sdk_session_abort` | `{ workspaceId, sessionId }` | `{ ok }` | Abort active stream |
| `opencode_session_messages` | `{ workspaceId, sessionId }` | `Message[]` | History rehydration |

### Internal APIs

- `services/web/daemon.ts` emits `daemon:opencode_event`, then uses `OpenCodeAdapter`
  to emit `agent:stream_event`.
- `services/events.ts` exposes `subscribeStreamEvents()` for `agent:stream_event`.

### Events (names + payloads)

The UI consumes `StreamEvent` envelopes from `agent:stream_event` (schema v1.0),
including at minimum:

| Event Type | Purpose |
| --- | --- |
| `text_delta` | Incremental assistant text |
| `thinking_delta` | Incremental reasoning text (optional) |
| `tool_call_delta` | Tool call in progress |
| `tool_call_completed` | Tool call completion + output |
| `status` | Session processing state |
| `completed` | Stream completion with usage |
| `error` | Stream error |

---

## 5. Workflows

### Main Flow

1. User selects a workspace and switches provider to Claude.
2. `useClaudeSession` checks `claude_sdk_status` and connects if needed.
3. User sends a prompt from `ThreadComposer`.
4. UI creates a session if needed, then calls `claude_sdk_session_prompt`.
5. Daemon streams `opencode:event`, which becomes `agent:stream_event`.
6. `useOpenCodeThread` buffers and renders thread items in `ThreadMessages`.
7. UI updates status to idle on `completed` or `status` events.

### Edge Cases

| Case | Handling |
| --- | --- |
| Connect failure | Show error + retry button in `ClaudeThreadView` |
| Session busy | Ignore new prompt, keep UI in processing state |
| Daemon disconnect | Surface error, disable composer until reconnect |

### Retry/Backoff

No UI-side backoff. The UI surfaces the error and relies on manual retry.

---

## 6. Error Handling

| Error | Source | UI Response |
| --- | --- | --- |
| Connection error | `claude_sdk_connect_workspace` | Show error banner + retry |
| Stream error | `agent:stream_event` (`error`) | Set status `error`, show message |
| Abort | `claude_sdk_session_abort` | Return to idle, keep session |
| Session busy | `claude_sdk_session_prompt` | Keep processing indicator |

---

## 7. Observability

### Logs

- UI logs connection failures and prompt errors to the console.
- Daemon stream issues surface via `agent:stream_event` errors.

### Metrics

Not required for UI. Defer to daemon/server metrics in `specs/claude-sdk-server.md`.

### Traces

Not required for UI.

---

## 8. Security and Privacy

### AuthZ/AuthN

- UI does not store or transmit API keys.
- Daemon handles authentication and network access.

### Data Handling

- Prompt and response text flow through the daemon event stream only.
- UI stores transient pending messages in memory only.

---

## 9. Migration or Rollout

### Compatibility Notes

- Requires `agent:stream_event` to conform to `specs/streaming-event-schema.md`.
- Claude SDK server must emit OpenCode-compatible events (see `specs/claude-sdk-server.md`).

### Rollout Plan

1. Add the Claude provider selection and thread view wiring.
2. Validate event streaming against `useOpenCodeThread`.
3. Add Playwright UI validation for conversation flow.

---

## 10. Open Questions

1. Should provider selection persist per workspace or globally?
2. Do we need a dedicated Claude session list in the UI?
3. Should workspace IDs differ from workspace paths for Claude connections?
