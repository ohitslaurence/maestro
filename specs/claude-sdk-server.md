# Claude SDK Server

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview

### Purpose

A per-workspace HTTP server that wraps the Claude Agent SDK, exposing an OpenCode-compatible REST + SSE API. This allows Maestro to orchestrate Claude Code agents using the same frontend pipeline as OpenCode.

### Goals

1. **OpenCode API parity**: Match OpenCode's REST endpoints and SSE event schema so the frontend needs zero changes.
2. **Full CLI parity**: Load Claude Code presets, settings, tools, and permission modes to match the CLI experience.
3. **Per-workspace isolation**: Each workspace gets its own server process with independent session state.
4. **Session resume**: Support resuming previous conversations via session ID.
5. **Daemon-managed lifecycle**: Daemon spawns/stops server processes; Tauri app controls via daemon RPC.

### Non-Goals

- PTY streaming for raw terminal output (future task).
- Multi-tenant auth (single-user, local-first for now).
- Custom tool registration (use SDK built-in tools + MCP).

---

## 2. Architecture

### Components

```
┌─────────────────────────────────────────────────────────────────┐
│                     Maestro Tauri App                           │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  React Frontend                                          │   │
│  │  - ThreadView (renders OpenCode-compatible events)       │   │
│  │  - useAgentSession hook (listens to agent:stream_event)  │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              │ Tauri IPC                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Rust Backend                                            │   │
│  │  - daemon/client.rs (JSON-RPC to daemon)                 │   │
│  │  - claudecode_adapter.rs (SDK→StreamEvent mapping)       │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                               │
                               │ TCP JSON-RPC
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Maestro Daemon (VPS)                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Workspace Manager                                       │   │
│  │  - spawn_claude_server(workspace_id, path) → port        │   │
│  │  - stop_claude_server(workspace_id)                      │   │
│  │  - list_claude_servers() → [{workspace_id, port, pid}]   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              │ spawns                           │
│                              ▼                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Claude SDK Server (Bun) [per workspace]                 │   │
│  │  - HTTP REST API (sessions, messages)                    │   │
│  │  - SSE event stream                                      │   │
│  │  - Wraps @anthropic-ai/claude-agent-sdk                  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Dependencies

| Component | Dependency | Version |
|-----------|------------|---------|
| Server | `@anthropic-ai/claude-agent-sdk` | latest |
| Server | `bun` | 1.x |
| Server | `hono` (or similar lightweight HTTP) | 4.x |
| Daemon | Existing Rust daemon | -- |
| Tauri | Existing claudecode_adapter.rs | -- |

### Module/Folder Layout

```
daemon/
├── claude-sdk-server/           # Bun/TypeScript server
│   ├── package.json
│   ├── tsconfig.json
│   ├── src/
│   │   ├── index.ts             # Entry point, HTTP server setup
│   │   ├── routes/
│   │   │   ├── sessions.ts      # /session endpoints
│   │   │   ├── messages.ts      # /session/:id/message endpoint
│   │   │   └── events.ts        # /event SSE endpoint
│   │   ├── sdk/
│   │   │   ├── agent.ts         # Wraps query() with hooks
│   │   │   ├── hooks.ts         # SDK hook handlers
│   │   │   └── permissions.ts   # Permission auto-approve logic
│   │   ├── events/
│   │   │   ├── emitter.ts       # SSE broadcast
│   │   │   └── mapper.ts        # SDK message → OpenCode event
│   │   ├── storage/
│   │   │   └── sessions.ts      # In-memory + file session state
│   │   └── types.ts             # Shared types
│   └── bun.lockb
└── src/
    └── claude_server.rs         # Daemon spawn/stop logic
```

---

## 3. Data Model

### Core Types

#### Session

```typescript
interface Session {
  id: string;                    // UUID
  workspaceId: string;           // Matches daemon workspace
  directory: string;             // Absolute path on VPS
  title: string;                 // User-visible title
  parentId?: string;             // For branched sessions
  resumeId?: string;             // SDK resume token
  modelId?: string;              // e.g., "claude-sonnet-4-20250514"
  createdAt: number;             // Unix ms
  updatedAt: number;             // Unix ms
  status: 'idle' | 'busy' | 'error';
  permission: PermissionMode;
  summary?: string;              // Auto-generated summary
}

type PermissionMode = 'default' | 'acceptEdits' | 'bypassPermissions';
```

#### Message

```typescript
interface MessageInfo {
  id: string;                    // UUID
  sessionId: string;
  role: 'user' | 'assistant';
  createdAt: number;             // Unix ms
  completedAt?: number;          // Unix ms (for assistant)
  modelId?: string;              // e.g., "claude-sonnet-4-20250514"
  providerId?: string;           // e.g., "anthropic"
  cost?: number;                 // USD
  tokens?: { input: number; output: number };
  error?: string;
}
```

#### Part (OpenCode-compatible)

```typescript
type Part =
  | TextPart
  | ReasoningPart
  | ToolPart
  | StepStartPart
  | StepFinishPart
  | RetryPart;

interface TextPart {
  id: string;
  messageId: string;
  type: 'text';
  text: string;
}

interface ReasoningPart {
  id: string;
  messageId: string;
  type: 'reasoning';
  text: string;
}

interface ToolPart {
  id: string;
  messageId: string;
  type: 'tool';
  toolUseId: string;
  toolName: string;
  input: unknown;
  output?: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  error?: string;
}

interface StepStartPart {
  id: string;
  messageId: string;
  type: 'step-start';
}

interface StepFinishPart {
  id: string;
  messageId: string;
  type: 'step-finish';
  usage?: { input: number; output: number };
  cost?: number;
}

interface RetryPart {
  id: string;
  messageId: string;
  type: 'retry';
  attempt: number;
  reason: string;
}
```

### Storage Schema

Sessions and messages are stored as JSON files per workspace:

```
~/.maestro/claude/{workspace_id}/
├── sessions/
│   └── {session_id}.json        # Session metadata + resumeId
├── messages/
│   └── {session_id}/
│       └── {message_id}.json    # Message + parts
└── index.json                   # Session list for fast queries
```

---

## 4. Interfaces

### Public APIs (HTTP)

All endpoints are prefixed with the server's base URL (e.g., `http://localhost:{port}`).

#### `GET /session`

List sessions for this workspace.

**Query params:**
- `start?: number` - Only sessions updated after this Unix ms
- `limit?: number` - Max results (default 50)
- `search?: string` - Filter by title

**Response:** `Session[]`

#### `POST /session`

Create a new session.

**Body:**
```json
{
  "title": "Fix auth bug",
  "parentId": null,
  "permission": "acceptEdits",
  "modelId": "claude-sonnet-4-20250514"
}
```

**Response:** `Session`

#### `GET /session/:id`

Get session details.

**Response:** `Session`

#### `POST /session/:id/message`

Send a user message and stream the assistant response.

**Body:**
```json
{
  "parts": [
    { "type": "text", "text": "Find bugs in auth.py" }
  ]
}
```

**Response:** `{ info: MessageInfo, parts: Part[] }` (final state after streaming completes)

**Side effect:** Emits SSE events during execution.

#### `POST /session/:id/abort`

Abort the current execution.

**Response:** `{ ok: true }`

#### `GET /event`

SSE event stream for this workspace.

**Response:** Server-Sent Events stream. Each event:
```
data: {"type":"message.part.updated","properties":{...}}
```

### Internal APIs

#### Daemon RPC

```rust
// In daemon, exposed via existing JSON-RPC:

/// Spawn a Claude SDK server for a workspace.
/// Returns the assigned port.
fn spawn_claude_server(workspace_id: String, directory: String) -> Result<u16, Error>;

/// Stop a running Claude SDK server.
fn stop_claude_server(workspace_id: String) -> Result<(), Error>;

/// List all running Claude SDK servers.
fn list_claude_servers() -> Vec<ClaudeServerInfo>;

struct ClaudeServerInfo {
    workspace_id: String,
    directory: String,
    port: u16,
    pid: u32,
    status: ServerStatus,
}

enum ServerStatus {
    Starting,
    Ready,
    Error(String),
}
```

### Events (SSE)

Event types match OpenCode SDK v2:

| Event | Payload | When |
|-------|---------|------|
| `session.created` | `{ info: Session }` | New session created |
| `session.updated` | `{ info: Session }` | Session metadata changed |
| `session.status` | `{ sessionId, status: { type, attempt?, message? } }` | Status transition |
| `session.error` | `{ sessionId, error }` | Unrecoverable error |
| `message.updated` | `{ info: MessageInfo }` | Message created/completed |
| `message.part.updated` | `{ part: Part, delta?: string }` | Part created/updated |
| `message.part.removed` | `{ sessionId, messageId, partId }` | Part removed |
| `permission.asked` | `{ id, sessionId, permission, tool?, patterns? }` | Permission requested |
| `permission.replied` | `{ sessionId, requestId, reply }` | Permission answered |

---

## 5. Workflows

### Main Flow: Send Message

```
1. Frontend calls POST /session/:id/message with user prompt
2. Server creates user MessageInfo, emits message.updated
3. Server creates assistant MessageInfo, emits message.updated
4. Server emits session.status { type: 'busy' }
5. Server calls SDK query() with:
   - prompt: user text
   - resume: session.resumeId (if resuming)
   - options: { cwd, allowedTools, permissionMode, ... }
6. For each SDK message:
   a. Map to Part (text/reasoning/tool)
   b. Emit message.part.updated with delta
   c. If tool_use: emit tool Part with status: 'pending' → 'running'
   d. If tool_result: update tool Part with output, status: 'completed'
7. On SDK completion:
   a. Update assistant MessageInfo with tokens, cost
   b. Emit message.updated with completedAt
   c. Save session.resumeId for future resume
   d. Emit session.status { type: 'idle' }
8. Return final { info, parts } to HTTP response
```

### Resume Flow

```
1. Frontend calls POST /session/:id/message on existing session
2. Server loads session.resumeId from storage
3. Server passes resume token to SDK query()
4. SDK restores conversation context
5. Continue as normal flow from step 5
```

### Abort Flow

```
1. Frontend calls POST /session/:id/abort
2. Server sends SIGTERM to SDK subprocess (or uses SDK abort API)
3. Emit session.status { type: 'idle' }
4. Return { ok: true }
```

### Permission Flow (Auto-Approve for MVP)

```
1. SDK emits permission request via hook
2. Server auto-approves based on permissionMode:
   - 'bypassPermissions': approve all
   - 'acceptEdits': approve reads/writes, prompt for others
   - 'default': prompt for all (but auto-approve for MVP)
3. Emit permission.asked and permission.replied events for UI awareness
4. Return approval to SDK hook
```

### Edge Cases

| Case | Handling |
|------|----------|
| Server crash mid-execution | Daemon detects exit, sets ServerStatus::Error. Frontend shows error. Session resumeId preserved. |
| SDK rate limit | SDK handles retry internally. Server emits retry Part. |
| Invalid session ID | Return 404. |
| Concurrent messages to same session | Return 409 Conflict (one message at a time). |
| Workspace directory doesn't exist | Return 400 on spawn. |

### Retry/Backoff

SDK handles retry internally. Server exposes retry events:

```typescript
// When SDK retries, emit:
{
  type: 'message.part.updated',
  properties: {
    part: {
      type: 'retry',
      attempt: 2,
      reason: 'Rate limited, retrying in 5s'
    }
  }
}
```

---

## 6. Error Handling

### Error Types

```typescript
enum ErrorCode {
  SESSION_NOT_FOUND = 'SESSION_NOT_FOUND',
  SESSION_BUSY = 'SESSION_BUSY',
  SDK_ERROR = 'SDK_ERROR',
  PERMISSION_DENIED = 'PERMISSION_DENIED',
  RATE_LIMITED = 'RATE_LIMITED',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
}

interface ApiError {
  code: ErrorCode;
  message: string;
  details?: unknown;
}
```

### Recovery Strategy

| Error | Recovery |
|-------|----------|
| SESSION_NOT_FOUND | Frontend removes stale session from UI |
| SESSION_BUSY | Frontend shows "session busy" indicator |
| SDK_ERROR | Emit session.error, session remains usable for retry |
| RATE_LIMITED | SDK retries internally; server emits retry Part |
| INTERNAL_ERROR | Log full stack, return generic error, session state preserved |

---

## 7. Observability

### Logs

Server logs to stderr in JSON format:

```json
{"level":"info","ts":1234567890,"msg":"session created","sessionId":"...","directory":"/project"}
{"level":"error","ts":1234567890,"msg":"SDK error","sessionId":"...","error":"..."}
```

Log levels: `debug`, `info`, `warn`, `error`

Daemon captures server stderr and forwards to daemon logs.

### Metrics

Not required for MVP. Future: expose `/metrics` endpoint with:
- `claude_sdk_sessions_total` (counter)
- `claude_sdk_messages_total` (counter)
- `claude_sdk_tokens_total{type="input|output"}` (counter)
- `claude_sdk_request_duration_seconds` (histogram)

### Traces

Not required for MVP. Future: OpenTelemetry integration.

---

## 8. Security and Privacy

### AuthZ/AuthN

- **MVP**: No auth on server (daemon handles network security via Tailscale).
- **API Key**: Server reads `ANTHROPIC_API_KEY` from environment (set by daemon on spawn).
- **Future**: Add bearer token auth if exposing beyond local network.

### Data Handling

- Sessions and messages stored in `~/.maestro/claude/` on VPS.
- API key never logged or stored.
- User prompts and assistant responses logged at debug level only.
- File contents from tools not logged (too large).

---

## 9. Migration or Rollout

### Compatibility Notes

- Server implements OpenCode SDK v2 API shape exactly.
- Existing frontend ThreadView should work without changes.
- claudecode_adapter.rs may need minor updates to match event shapes.

### Rollout Plan

1. **Phase 1**: Implement server, test standalone with curl/httpie.
2. **Phase 2**: Add daemon spawn/stop commands, test via Tauri.
3. **Phase 3**: Wire frontend to use Claude sessions alongside OpenCode.
4. **Phase 4**: Add resume support.
5. **Phase 5**: CLI parity refinements (presets, settings loading).

---

## 10. Design Decisions

1. **Port allocation**: Configurable range (default 9100-9199). Daemon tracks used ports and assigns next available.

2. **Process supervision**: Auto-restart once on crash. If server crashes twice, mark as Error and require explicit re-spawn.

3. **Settings loading**: Enable `['user', 'project', 'local']` to match CLI behavior.

4. **Model selection**: Configurable per-session. Pass `model` option to SDK if specified, otherwise use SDK default.

5. **Cost tracking**: Aggregate per-session. Surface in session.summary and message.cost fields. Future: aggregate across workspace.

## 11. Open Questions

1. **MCP servers**: Should the server support MCP tool servers? If so, how to configure?

2. **Subagents**: How to handle `Task` tool spawning subagents? Same server or new process?

---

## Appendix A: SDK Configuration

To match CLI behavior, the server should call `query()` with:

```typescript
import { query } from '@anthropic-ai/claude-agent-sdk';

const stream = query({
  prompt: userText,
  resume: session.resumeId,
  options: {
    cwd: session.directory,
    systemPrompt: { type: 'preset', preset: 'claude_code' },
    tools: { type: 'preset', preset: 'claude_code' },
    settingSources: ['user', 'project', 'local'],
    permissionMode: session.permission,
    maxTurns: 100,  // or configurable
    // For streaming partial messages:
    // includePartialMessages: true,
  }
});

for await (const message of stream) {
  // Map to events and emit
}
```

## Appendix B: Example Event Sequence

User sends "Read package.json":

```
→ POST /session/abc/message { parts: [{ type: "text", text: "Read package.json" }] }

← SSE: message.updated { info: { id: "m1", role: "user", ... } }
← SSE: message.updated { info: { id: "m2", role: "assistant", ... } }
← SSE: session.status { sessionId: "abc", status: { type: "busy" } }
← SSE: message.part.updated { part: { type: "text", text: "I'll read" }, delta: "I'll read" }
← SSE: message.part.updated { part: { type: "text", text: "I'll read package.json" }, delta: " package.json" }
← SSE: message.part.updated { part: { type: "tool", toolName: "Read", status: "pending" } }
← SSE: message.part.updated { part: { type: "tool", toolName: "Read", status: "running" } }
← SSE: message.part.updated { part: { type: "tool", toolName: "Read", status: "completed", output: "{...}" } }
← SSE: message.part.updated { part: { type: "text", text: "The package.json contains..." }, delta: "The package.json contains..." }
← SSE: message.part.updated { part: { type: "step-finish", usage: { input: 1000, output: 200 }, cost: 0.003 } }
← SSE: message.updated { info: { id: "m2", completedAt: 1234567890, tokens: {...}, cost: 0.003 } }
← SSE: session.status { sessionId: "abc", status: { type: "idle" } }

← HTTP 200: { info: { id: "m2", ... }, parts: [...] }
```
