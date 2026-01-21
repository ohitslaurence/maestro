# Claude SDK UI Integration Implementation Plan

Reference: [claude-sdk-ui.md](../claude-sdk-ui.md)

## Phase 1: Provider selection + entry point
- [x] Add Claude option to the agent provider selector (`claude_code`) (§2.1, §3.1)
- [x] Render `ClaudeThreadView` when provider is Claude in `AgentView` (§2.1, §5.1)
- [x] Disable provider actions when no workspace is selected (§5.1)

## Phase 2: Claude session connection
- [x] Implement `useClaudeSession` to connect, auto-connect, and expose status (§4.1, §5.1)
- [x] Wire `claude_sdk_session_create`, `claude_sdk_session_prompt`, `claude_sdk_session_abort` (§4.1)
- [x] Track `sessionId` + `isPrompting` for UI state (`ClaudeSessionState`) (§3.1)

## Phase 3: Thread view integration
- [x] Build `ClaudeThreadView` using `ThreadMessages`, `ThreadComposer`, and `useOpenCodeThread` (§2.1, §5.1)
- [x] Track pending user messages for immediate UI feedback (§3.1, §5.1)
- [x] Display connection and stream errors with retry action (§6)

## Phase 4: Stream event pipeline
- [x] Ensure `opencode:event` adapts into `agent:stream_event` for Claude payloads (§4.2, §4.3)
- [x] Load history via OpenCode-compatible message endpoint (§4.1)
- [x] Confirm stream ordering is respected by `useOpenCodeThread` buffers (§5.1)

## Phase 5: UI validation + automation
- [x] Add Playwright script `app/scripts/ui-claude-conversation.ts` for Claude conversation flow (§5.1, §9.2)
- [x] Validate provider switch, session creation, streaming response, and stop/abort states (§5.1, §6)

## Files to Create
- `app/scripts/ui-claude-conversation.ts`

## Files to Modify
- `app/src/features/agent/components/AgentView.tsx`
- `app/src/features/agent/components/AgentProviderSelector.tsx`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`
- `app/src/features/claudecode/hooks/useClaudeSession.ts`
- `app/src/features/claudecode/index.ts`
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`
- `app/src/services/events.ts`
- `app/src/services/tauri.ts`
- `app/src/services/web/opencodeAdapter.ts`
- `app/src/services/web/daemon.ts`
- `app/src/types.ts`

## Verification Checklist

### Implementation Checklist
- [x] `cd app && bun run typecheck`
- [ ] `cd app && bun run ui:smoke`
- [ ] UI Feature Validation:
  - [ ] `cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth`
  - [ ] `cd app && bun run dev -- --host 127.0.0.1 --port 1420`
  - [ ] `cd app && bun scripts/ui-claude-conversation.ts`

### Manual QA Checklist (do not mark—human verification)
- [ ]? Claude provider connects and stays stable across workspace switch
- [ ]? Conversation renders streaming text and tool rows
- [ ]? Abort returns UI to idle without losing session state

## Notes
- Requires `ANTHROPIC_API_KEY` to be set on the daemon host for real conversations.
