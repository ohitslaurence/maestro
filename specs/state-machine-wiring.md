# State Machine Wiring

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview
### Purpose
Define the wiring between unified streaming events and the agent state machine so that session
status is derived exclusively from `agent:state_event` while stream content remains sourced from
`agent:stream_event`.

### Goals
- Feed every `StreamEvent` into the state machine as `AgentEvent::HarnessStream`.
- Emit `agent:state_event` for all state transitions driven by stream completion or errors.
- Ensure UI working/idle indicators come from `useAgentSession`.
- Preserve stream ordering and payloads from the streaming event schema.

### Non-Goals
- Redefine `StreamEvent` or `AgentEvent` shapes.
- Change state transition logic in `AgentStateMachine`.
- UI layout or message rendering changes.

---

## 2. Architecture
### Components
- **Harness Adapter**: produces `StreamEvent` per `streaming-event-schema.md`.
- **Daemon Client**: forwards stream events, then invokes the state machine per event.
- **Session Broker**: owns per-session state entries and invokes `process_event_with_tool_emission`.
- **Agent State Machine**: pure transition logic; emits `AgentStateEvent`.
- **Frontend Event Hub**: listens on `agent:state_event`.
- **useAgentSession**: exposes `AgentStateKind` to UI.
- **Thread Views**: derive working/idle state from `useAgentSession`.

### Dependencies
- `specs/agent-state-machine.md`
- `specs/streaming-event-schema.md`

### Module/Folder Layout
- `app/src-tauri/src/daemon/client.rs`
- `app/src-tauri/src/sessions.rs`
- `app/src-tauri/src/agent_state.rs`
- `app/src-tauri/src/lib.rs`
- `app/src/services/events.ts`
- `app/src/hooks/useAgentSession.ts`
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`
- `app/src/features/opencode/components/ThreadView.tsx`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`

### Data Flow Diagram
```
Harness stream -> Adapter -> StreamEvent -> Daemon client
  -> agent:stream_event -> Thread reducers (OpenCode + Claude)
  -> Session broker -> AgentStateMachine -> agent:state_event -> useAgentSession -> UI
```

---

## 3. Data Model
### Core Types
- `StreamEvent` (see `streaming-event-schema.md` §3)
- `AgentEvent::HarnessStream` (see `agent-state-machine.md` §3)
- `AgentStateEvent` (see `agent-state-machine.md` §4)

### StreamEvent to AgentEvent Mapping
- Every incoming `StreamEvent` results in:
  `AgentEvent::HarnessStream { session_id, stream_event }`.
- The mapping preserves `sessionId`, `streamId`, `seq`, `timestampMs`, and `type`.
- `stream_event.type=status` is treated as telemetry; it must not drive UI state directly.

---

## 4. Interfaces
### Public APIs
- Tauri event `agent:stream_event` (stream content).
- Tauri event `agent:state_event` (state transitions and lifecycle events).

### Internal APIs
- `emit_stream_event(app, event: StreamEvent)`
- `process_event_with_tool_emission(app, session_entry, event: AgentEvent)`
- `emit_state_changed(app, event: AgentStateEvent)`

### Events
- `agent:state_event` uses the payloads defined in `agent-state-machine.md` §4.
- `agent:stream_event` remains unchanged per `streaming-event-schema.md` §3.

---

## 5. Workflows
### Main Flow
1. Adapter emits `StreamEvent`.
2. Daemon client emits `agent:stream_event` to the frontend event hub.
3. Daemon client resolves the session entry and calls the state machine with
   `AgentEvent::HarnessStream`.
4. State machine transitions (if applicable) and emits `agent:state_event`.
5. `useAgentSession` updates UI working/idle indicators.

### Completion and Error Handling
- `stream_event.type=completed` must trigger the transition to `ProcessingResponse` as defined in
  `agent-state-machine.md` §5.
- `stream_event.type=error` must transition to `Error` and emit `session_error` as defined in
  `agent-state-machine.md` §6.
- Stream events continue to flow to `agent:stream_event` regardless of state transitions.

### Edge Cases
- If the session ID is unknown, emit `session_error` with `code=session_not_found` and skip
  state mutation for that event.

---

## 6. Error Handling
### Error Types
- `session_not_found`
- `state_transition_invalid`
- `streaming_failed`

### Recovery Strategy
- Emit `session_error` with `sessionId` and `streamId` (if available).
- Keep the existing state unchanged when the transition is invalid.

---

## 7. Observability
### Logs
- Log every stream event forwarded to the state machine with `session_id`, `stream_id`, `type`, `seq`.
- Log `session_not_found` and invalid transition errors.

### Metrics
- `agent_state_transitions_total{from,to}`
- `stream_event_forwarded_total{type}`

### Traces
- Attach `stream_id` to state transition spans.

---

## 8. Security and Privacy
### AuthZ/AuthN
- Follows the existing Tauri event trust model and daemon token auth.

### Data Handling
- Do not log stream payload contents unless debug mode is enabled.

---

## 9. Migration or Rollout
### Compatibility Notes
- Stream event schema remains unchanged; wiring only affects state events.
- UI must migrate to `useAgentSession` to avoid duplicated status logic.

### Rollout Plan
1. Wire stream events to the state machine in the daemon client.
2. Update UI to consume `agent:state_event`.
3. Remove stream-driven status logic and debug logs.

---

## 10. Open Questions
- None.
