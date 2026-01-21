# Agent State Machine and Post-Tool Hooks Implementation Plan

Reference: [agent-state-machine.md](../agent-state-machine.md)

## Phase 1: Core types and transitions
- [x] Add Rust enums/structs for `AgentStateKind`, `AgentEvent`, `AgentAction`, `ToolRunStatus` (See §3)
- [x] Implement `handle_event` transition function in `app/src-tauri/src/agent_state.rs` (See §5)
- [x] Add minimal unit tests for valid/invalid transitions (See §5, §6)

Exit Criteria
- [x] `app/src-tauri/src/agent_state.rs` compiles with the new enums and transition logic (See §3, §5)
- [x] Unit tests cover at least one valid and one invalid transition (See §5, §6)
- [x] `AgentStateKind` is included in session summaries (stubbed with TODO for Phase 2 wiring)

## Phase 2: Session integration and events
- [x] Wire session loop to state machine in `app/src-tauri/src/sessions.rs` (See §2, §4)
- [x] Emit `agent:state_event` from `app/src-tauri/src/lib.rs` (See §4)
- [x] Extend session summaries to include `AgentStateKind` (See §3)

Exit Criteria
- [x] Spawning a session emits a `state_changed` event (`Starting` -> `Ready`) (See §4, §5)
- [x] `agent:state_event` includes `eventId`, `timestampMs`, and `sessionId` (See §4)
- [ ] No panics when attaching to an existing session (manual smoke check)

## Phase 3: Tool lifecycle plumbing
- [x] Normalize tool calls and mark mutating tools in ToolRunner (See §3, §5)
- [ ] Emit `tool_lifecycle` events on start/complete (See §4)

Exit Criteria
- [x] Tool run records include `mutating` flag and `attempt` count (See §3)
- [ ] `tool_lifecycle` events fire for start and completion (See §4, §5)
- [ ] Mutating tool batch transitions to `PostToolsHook` (See §5)

## Phase 4: Post-tool hooks
- [ ] Add hook runner with policy config (See §2, §5, §6)
- [ ] Emit `hook_lifecycle` events, handle failure policy (See §5, §6)

Exit Criteria
- [ ] `hooks.json` config is loaded from app data dir or hooks are disabled (See §2)
- [ ] Hook failure policy respected (`fail_session` vs `warn_continue`) (See §6)
- [ ] `hook_lifecycle` events fire for each hook run (See §4)

## Phase 5: Frontend wiring
- [ ] Add TS types in `app/src/types/agent.ts` (See §3, §4)
- [ ] Subscribe to `agent:state_event` in event hub and update hooks (See §4, §5)

Exit Criteria
- [ ] Frontend compiles with new TS types (See §3, §4)
- [ ] UI reflects `AgentStateKind` updates for at least one session (manual)

## Files to Create
- `app/src-tauri/src/agent_state.rs`
- `app/src-tauri/src/tools.rs`
- `app/src-tauri/src/hooks.rs`
- `app/src/types/agent.ts`

## Files to Modify
- `app/src-tauri/src/sessions.rs`
- `app/src-tauri/src/lib.rs`
- `app/src/services/events.ts`
- `app/src/hooks/useAgentSession.ts`

## Verification Checklist
- [ ] `bun run typecheck`
- [ ] Manual: spawn session, observe state/tool/hook events in UI
