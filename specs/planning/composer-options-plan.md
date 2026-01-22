# Composer Options Implementation Plan

Reference: [composer-options.md](../composer-options.md)

## Phase 1: Server - Model Selection

- [ ] Add `maxThinkingTokens` field to `Session` type (§1)
- [ ] Update `CreateSessionRequest` to accept `maxThinkingTokens` (§5)
- [ ] Update `SendMessageRequest` to accept `modelId` and `maxThinkingTokens` overrides (§5)
- [ ] Wire `maxThinkingTokens` in `buildSdkOptions()` (§5)
- [ ] Create `GET /models` endpoint using SDK `supportedModels()` (§5)
- [ ] Cache model list with TTL (avoid repeated SDK calls)

## Phase 2: Server - Message Overrides

- [ ] Update `POST /session/:id/message` to accept `modelId` override (§5)
- [ ] Update `POST /session/:id/message` to accept `maxThinkingTokens` override (§5)
- [ ] Pass overrides through to `buildSdkOptions()` (§5)
- [ ] Emit model info in SSE events for UI display

## Phase 3: Tauri Commands [BLOCKED by: Phase 1, 2]

- [ ] Add `model_id` param to `claude_sdk_session_prompt` command (§7)
- [ ] Add `max_thinking_tokens` param to `claude_sdk_session_prompt` command (§7)
- [ ] Add `claude_sdk_models` command to fetch available models (§7)
- [ ] Update Rust types for new params

## Phase 4: Frontend Service Layer [BLOCKED by: Phase 3]

- [ ] Update `claudeSdkSessionPrompt` service to accept options (§8)
- [ ] Add `claudeSdkModels` service function (§8)
- [ ] Add `ThinkingMode` type and `THINKING_BUDGETS` mapping (§3)

## Phase 5: UI Components [BLOCKED by: Phase 4]

- [ ] Create `ModelSelector.tsx` dropdown component (§4)
- [ ] Create `ThinkingModeSelector.tsx` dropdown component (§4)
- [ ] Create `ComposerOptions.tsx` container component (§4)
- [ ] Style components to match existing design tokens

## Phase 6: Hook Integration [BLOCKED by: Phase 5]

- [ ] Update `useClaudeSession` to include model/thinking state (§6)
- [ ] Create `useComposerOptions` hook (§6)
- [ ] Fetch models on session connect
- [ ] Wire state to prompt function

## Phase 7: Thread View Integration [BLOCKED by: Phase 6]

- [ ] Add `ComposerOptions` to `ClaudeThreadView` above composer (§4)
- [ ] Pass selected options through to prompt call
- [ ] Show current model/thinking state in UI

## Files to Create

- `daemon/claude-server/src/routes/models.ts`
- `app/src/features/claudecode/components/ComposerOptions.tsx`
- `app/src/features/claudecode/components/ModelSelector.tsx`
- `app/src/features/claudecode/components/ThinkingModeSelector.tsx`
- `app/src/features/claudecode/hooks/useComposerOptions.ts`

## Files to Modify

- `daemon/claude-server/src/types.ts`
- `daemon/claude-server/src/routes/messages.ts`
- `daemon/claude-server/src/routes/sessions.ts`
- `daemon/claude-server/src/sdk/agent.ts`
- `daemon/claude-server/src/index.ts`
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
- [ ] Per-message overrides work in prompt request

### Manual QA Checklist (do not mark—human verification)

- [ ]? Model dropdown displays available models
- [ ]? Thinking mode selector shows all options
- [ ]? Selected model appears in SSE events
- [ ]? Extended thinking produces reasoning blocks in response

## Notes

- Phase 1: SDK's `supportedModels()` requires an active query context. May need to call during session init or cache globally.
- Phase 5: Design tokens for dropdowns should match existing select components if any exist.
- The spec assumes SDK accepts `maxThinkingTokens` directly (verified in sdk.d.ts line 241).
