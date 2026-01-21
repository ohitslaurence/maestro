# State Machine Wiring Implementation Plan

Reference: [state-machine-wiring.md](../state-machine-wiring.md)

## Phase 1: Backend state-machine wiring
- [x] Route all `agent:stream_event` emissions through the state machine entry point and emit
  `agent:state_event` for every transition (see `state-machine-wiring.md` §2, §4, §5).
- [x] Map each `StreamEvent` into `AgentEvent::HarnessStream` while preserving `sessionId`,
  `streamId`, `seq`, and terminal semantics (see `state-machine-wiring.md` §3, §5).
- [x] Emit `session_error` when stream events arrive for missing sessions instead of silently
  ignoring them (see `state-machine-wiring.md` §6).

## Phase 2: Frontend session state consolidation
- [ ] Update UI views to derive working/idle states from `useAgentSession` and
  `AgentStateKind` rather than stream status enums (see `state-machine-wiring.md` §4, §5).
- [ ] Remove stream-driven status tracking from `useOpenCodeThread`, keeping it focused on
  message/thread payloads only (see `state-machine-wiring.md` §2, §5).

## Phase 3: Cleanup and regression coverage
- [ ] Remove temporary debug logging added to stream adapters or hooks once `agent:state_event`
  updates are verified (see `state-machine-wiring.md` §7).
- [ ] Ensure state transition logs include `session_id` and `stream_id` for triage (see
  `state-machine-wiring.md` §7).

## Files to Create
- None.

## Files to Modify
- `app/src-tauri/src/daemon/client.rs`
- `app/src-tauri/src/sessions.rs`
- `app/src-tauri/src/agent_state.rs`
- `app/src/hooks/useAgentSession.ts`
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`
- `app/src/features/opencode/components/ThreadView.tsx`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`

## Verification Checklist
### Implementation Checklist
- [x] `cd app && bun run typecheck`

### Manual QA Checklist (do not mark—human verification)
- [ ]? Send a prompt with OpenCode, confirm state transitions idle -> calling_llm -> processing_response -> idle in UI.
- [ ]? Send a prompt with Claude, confirm state transitions and working indicator behavior match OpenCode.

## Notes (Optional)
- Map `stream_event.type=completed` directly into a `HarnessStream` transition; do not rely on
  provider-specific status events for state changes.
