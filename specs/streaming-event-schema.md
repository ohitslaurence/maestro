# Unified Streaming Event Schema

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview
### Purpose
Define a single streaming event schema that normalizes output from all harnesses and LLM providers so
UI and services can consume a consistent event stream.

### Goals
- One envelope format for all streaming events.
- Consistent ordering, buffering, and termination semantics.
- Compatible with local Tauri events and remote daemon notifications.

### Non-Goals
- Provider-specific parsing logic.
- UI layout and rendering decisions.

---

## 2. Architecture
### Components
- **Harness Adapter**: converts provider-native streams into `StreamEvent`.
- **Session Broker**: attaches ordering metadata and emits events.
- **Frontend Event Hub**: subscribes to `agent:stream_event` and dispatches to reducers.

### Dependencies
- Event channel: `@tauri-apps/api/event`
- Session manager: `app/src-tauri/src/sessions.rs`
- Event emitters: `app/src-tauri/src/lib.rs`
- Frontend hub: `app/src/services/events.ts`

### Module/Folder Layout
- Backend emitter: `app/src-tauri/src/sessions.rs`
- Backend wiring: `app/src-tauri/src/lib.rs`
- Shared TS types: `app/src/types/streaming.ts` (new)
- Frontend event hub: `app/src/services/events.ts`

---

## 3. Data Model
### Core Types
**StreamEvent Envelope**
| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `schemaVersion` | string | yes | `"1.0"` |
| `eventId` | string | yes | Unique per event |
| `sessionId` | string | yes | Maestro session ID |
| `harness` | string | yes | `claude_code`, `open_code`, ... |
| `provider` | string | no | `anthropic`, `openai`, `opencode` |
| `streamId` | string | yes | Stable per assistant response |
| `seq` | number | yes | Monotonic per `streamId` |
| `timestampMs` | number | yes | Unix epoch millis |
| `type` | string | yes | Event type (see below) |
| `payload` | object | yes | Type-specific data |
| `messageId` | string | no | Provider message ID |
| `parentMessageId` | string | no | Optional threading |

### Event Types
- `text_delta`
- `tool_call_delta`
- `tool_call_completed`
- `completed`
- `error`
- `status`
- `thinking_delta` (optional)
- `artifact_delta` (optional)
- `metadata`

### Payload Schemas
`text_delta`
```
{ "text": "...", "role": "assistant" }
```

`tool_call_delta`
```
{ "callId": "tool-1", "toolName": "edit_file", "argumentsDelta": "{\"path\":..." }
```

`tool_call_completed`
```
{ "callId": "tool-1", "toolName": "edit_file", "arguments": {...},
  "output": "ok", "status": "completed", "errorMessage?": "..." }
```

`completed`
```
{ "reason": "stop|length|tool_error|user_abort",
  "usage": { "inputTokens": 1, "outputTokens": 2, "reasoningTokens?": 0 } }
```

`error`
```
{ "code": "provider_error", "message": "...", "recoverable": false, "details?": {...} }
```

`status`
```
{ "state": "idle|processing|waiting|aborted", "detail?": "..." }
```

`metadata`
```
{ "model": "claude-3-5", "latencyMs": 1234, "providerRequestId?": "..." }
```

### Storage Schema (if any)
Streaming events are not persisted by default. Consumers may buffer in-memory by `streamId` to
reconstruct assistant responses.

---

## 4. Interfaces
### Public APIs
- Tauri event: `agent:stream_event`
- Daemon JSON-RPC notification: `session.stream_event`

### Internal APIs
- `emit_stream_event(app_handle, event: StreamEvent)`
- `forward_stream_event(session_id, event)` (daemon client)

### Events (names + payloads)
- `agent:stream_event` emits the `StreamEvent` envelope defined in §3.

---

## 5. Workflows
### Main Flow
```
Provider stream -> Harness Adapter -> StreamEvent envelope -> Session Broker
  -> Tauri event (agent:stream_event) -> Frontend event hub -> UI reducer
```

### Ordering and Buffering Rules
- `seq` strictly increases per `streamId`.
- Consumers buffer out-of-order events until gaps resolve.
- If a gap persists longer than 5 seconds, emit `error` with `code=stream_gap` and continue.
- `completed` is terminal for `streamId`; ignore further events with same `streamId`.

### Edge Cases
- Tool call deltas may arrive before any text delta.
- Mixed text/tool deltas can interleave; consumers must respect `seq`.
- Provider errors mid-stream must emit `error` and close the stream.

### Retry/Backoff
No retries at the stream layer. Retries occur at the session orchestration level.

---

## 6. Error Handling
### Error Types
- `provider_error`
- `stream_gap`
- `protocol_error`
- `tool_error`
- `session_aborted`

### Recovery Strategy
- Surface error in UI and allow new prompt to create a new `streamId`.
- Do not reuse a failed `streamId`.

---

## 7. Observability
### Logs
- Log every `completed` and `error` event with `sessionId` + `streamId`.

### Metrics
- `stream_event_count{type}`
- `stream_latency_ms` (first delta to completed)

### Traces
- Attach `providerRequestId` if provided by harness.

---

## 8. Security and Privacy
### AuthZ/AuthN
- Local Tauri events are trusted; daemon notifications require token auth.

### Data Handling
- Do not persist payloads by default.
- Redact tool outputs and arguments if they include secrets.

---

## 9. Migration or Rollout
### Compatibility Notes
- Existing harness-specific events must be adapted to `StreamEvent`.
- Legacy event names remain until the UI migrates.

### Rollout Plan
1. Emit `agent:stream_event` in the backend adapter.
2. Add frontend event hub + `StreamEvent` types.
3. Migrate OpenCode/Claude UI reducers to new schema.
4. Deprecate old event names.

---

## 10. Open Questions
- Should `streamId` represent a single assistant response or a whole turn?
- Do we need explicit `message_started` and `message_completed` events?
- Should `artifact_delta` be a first-class type or folded into tool results?
