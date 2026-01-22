# Composer Options Implementation Plan

Reference: [composer-options.md](../composer-options.md)

## Phase 1: Server - Models Endpoint

- [x] Create `GET /models` endpoint using SDK `supportedModels()` (§4)
- [x] Implement 5-minute TTL cache, per-workspace (§4)
- [x] Return `FALLBACK_MODELS` on SDK call failure (§4, §6)
- [x] Register route in `index.ts` (§2) — N/A: server is single-file; route added inline in `server.ts`

## Phase 2: Server - Thinking Support

- [x] Add `maxThinkingTokens` field to `Session` type (§3)
- [x] Update `CreateSessionRequest` to accept `maxThinkingTokens` (§4)
- [x] Update `SendMessageRequest` to accept `maxThinkingTokens` override (§4)
- [x] Wire `maxThinkingTokens` in `buildSdkOptions()` (§4)
- [x] Add `modelId` and `maxThinkingTokens` to SSE events (§4)

## Phase 3: Tauri Commands [BLOCKED by: Phase 1, 2]

- [x] Add `max_thinking_tokens` param to `claude_sdk_session_prompt` command (§4)
- [ ] Add `claude_sdk_models` command to fetch available models (§4)
- [x] Update Rust types for new params

## Phase 4: Frontend Service Layer [BLOCKED by: Phase 3]

- [ ] Update `claudeSdkSessionPrompt` service to accept `maxThinkingTokens` option
- [ ] Add `claudeSdkModels` service function
- [ ] Add `ThinkingMode` type and `THINKING_BUDGETS` mapping (§3)

## Phase 5: UI Components [BLOCKED by: Phase 4]

- [ ] Create `ModelSelector.tsx` dropdown component (§2)
- [ ] Create `ThinkingModeSelector.tsx` dropdown component (§2)
- [ ] Create `ComposerOptions.tsx` container component (§2)
- [ ] Style components to match existing design tokens
- [ ] Disable dropdowns during message streaming (§5)

## Phase 6: Hook Integration [BLOCKED by: Phase 5]

- [ ] Update `useClaudeSession` to include model/thinking state
- [ ] Create `useComposerOptions` hook (§2)
- [ ] Fetch models on session connect via `GET /models`
- [ ] Wire state to prompt function

## Phase 7: Thread View Integration [BLOCKED by: Phase 6]

- [ ] Add `ComposerOptions` to `ClaudeThreadView` above composer (§2)
- [ ] Pass selected options through to prompt call
- [ ] Show current model/thinking state in UI

## Files to Create

- ~~`daemon/claude-server/src/routes/models.ts`~~ — Not needed: server is single-file (`server.ts`)
- `app/src/features/claudecode/components/ComposerOptions.tsx`
- `app/src/features/claudecode/components/ModelSelector.tsx`
- `app/src/features/claudecode/components/ThinkingModeSelector.tsx`
- `app/src/features/claudecode/hooks/useComposerOptions.ts`

## Files to Modify

- ~~`daemon/claude-server/src/types.ts`~~ — Not needed: server is single-file
- ~~`daemon/claude-server/src/routes/messages.ts`~~ — Not needed: server is single-file
- ~~`daemon/claude-server/src/routes/sessions.ts`~~ — Not needed: server is single-file
- ~~`daemon/claude-server/src/sdk/agent.ts`~~ — Not needed: server is single-file
- ~~`daemon/claude-server/src/index.ts`~~ — Not needed: server is single-file
- `daemon/claude-server/src/server.ts` — All server changes go here
- `app/src-tauri/src/lib.rs`
- `app/src/services/tauri.ts`
- `app/src/features/claudecode/hooks/useClaudeSession.ts`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`

## Verification Checklist

### Implementation Checklist

- [ ] `cd daemon/claude-server && bun run typecheck`
- [ ] `cd app && bun run typecheck`
- [ ] Server returns model list from `/models` endpoint
- [ ] Model selection persists for session
- [ ] Thinking mode maps correctly to `maxThinkingTokens`
- [ ] Per-message thinking override works in prompt request
- [ ] UI Feature Validation:
  - [ ] `cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth`
  - [ ] `cd app && bun run dev -- --host 127.0.0.1 --port 1420`
  - [ ] `cd app && bun scripts/ui-composer-options.ts`

### Manual QA Checklist (do not mark—human verification)

- [ ]? Model dropdown displays available models
- [ ]? Thinking mode selector shows all options (off/low/medium/high/max)
- [ ]? Selected model appears in SSE events
- [ ]? Extended thinking produces reasoning blocks in response
- [ ]? Dropdowns disabled during streaming

## Notes

- Phase 1: SDK's `supportedModels()` requires an active query context. Call during first message or cache globally on server init.
- Phase 5: Design tokens for dropdowns should match existing select components if any exist.
- Per-message model override removed from scope (§1 Non-Goals). Model is session-level only.
- `undefined` vs `0` semantics clarified in §3: `undefined` = inherit, `0` = no thinking.
