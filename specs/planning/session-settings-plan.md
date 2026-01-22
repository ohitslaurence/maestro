# Session Settings Implementation Plan

Reference: [session-settings.md](../session-settings.md)

## Phase 1: Server - Data Model

- [ ] Add `SessionSettings` type with `maxTurns` field (§1)
- [ ] Add `SystemPromptConfig` union type (§1)
- [ ] Add `settings` field to `Session` interface (§1)
- [ ] Update `CreateSessionRequest` to accept `settings` (§2)
- [ ] Update session storage to persist settings (§8)
- [ ] Default settings when not provided

## Phase 2: Server - Settings Endpoint

- [ ] Create `PATCH /session/:id/settings` endpoint (§3)
- [ ] Validate incoming settings (maxTurns range, etc.)
- [ ] Merge partial settings with existing
- [ ] Emit `session.updated` SSE event on change
- [ ] Return updated session in response

## Phase 3: Server - SDK Integration [BLOCKED by: Phase 1]

- [ ] Wire `maxTurns` from settings to `buildSdkOptions()` (§4)
- [ ] Wire `customSystemPrompt` for custom mode (§4)
- [ ] Wire `appendSystemPrompt` for append mode (§4)
- [ ] Wire `allowedTools` from settings (§4)
- [ ] Wire `disallowedTools` from settings (§4)

## Phase 4: Tauri Commands [BLOCKED by: Phase 2]

- [ ] Add `claude_sdk_session_settings_update` command
- [ ] Add settings to `claude_sdk_session_get` response
- [ ] Update Rust types for `SessionSettings`

## Phase 5: Frontend Service Layer [BLOCKED by: Phase 4]

- [ ] Add `claudeSdkSessionSettingsUpdate` service function
- [ ] Add TypeScript types for `SessionSettings`, `SystemPromptConfig`
- [ ] Add `CLAUDE_TOOLS` constant with tool metadata (§6)

## Phase 6: useSessionSettings Hook [BLOCKED by: Phase 5]

- [ ] Create `useSessionSettings` hook (§5)
- [ ] Fetch current settings from session
- [ ] Implement `updateSettings` with optimistic update
- [ ] Handle error rollback

## Phase 7: UI Components - Modal Shell [BLOCKED by: Phase 6]

- [ ] Create `SessionSettingsModal.tsx` container (§5)
- [ ] Create `SessionSettingsButton.tsx` gear icon (§5)
- [ ] Add modal trigger to session header
- [ ] Implement tab/section layout

## Phase 8: UI Components - Execution Section [BLOCKED by: Phase 7]

- [ ] Add max turns input field (§5)
- [ ] Validate range (1-1000)
- [ ] Show helper text explaining the setting

## Phase 9: UI Components - System Prompt Section [BLOCKED by: Phase 7]

- [ ] Create `SystemPromptEditor.tsx` component (§5)
- [ ] Implement mode toggle (default/append/custom) (§5)
- [ ] Add textarea for append/custom content
- [ ] Preview combined prompt (future)

## Phase 10: UI Components - Tools Section [BLOCKED by: Phase 7]

- [ ] Create `ToolSelector.tsx` component (§5)
- [ ] Display tool checkboxes by category (§6)
- [ ] Implement allowlist vs blocklist mode toggle
- [ ] Show tool descriptions on hover

## Phase 11: Thread View Integration [BLOCKED by: Phase 7]

- [ ] Add `SessionSettingsButton` to session header in `ClaudeThreadView`
- [ ] Wire modal open/close state
- [ ] Refresh session data after settings update

## Files to Create

- `daemon/claude-server/src/routes/settings.ts`
- `app/src/features/claudecode/components/SessionSettingsModal.tsx`
- `app/src/features/claudecode/components/SessionSettingsButton.tsx`
- `app/src/features/claudecode/components/SystemPromptEditor.tsx`
- `app/src/features/claudecode/components/ToolSelector.tsx`
- `app/src/features/claudecode/hooks/useSessionSettings.ts`

## Files to Modify

- `daemon/claude-server/src/types.ts`
- `daemon/claude-server/src/routes/sessions.ts`
- `daemon/claude-server/src/sdk/agent.ts`
- `daemon/claude-server/src/storage/sessions.ts`
- `daemon/claude-server/src/index.ts`
- `app/src-tauri/src/lib.rs`
- `app/src/services/tauri.ts`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`

## Verification Checklist

### Implementation Checklist

- [ ] `cd daemon/claude-server && bun run typecheck`
- [ ] `cd app && bun run typecheck`
- [ ] Settings persist in session storage
- [ ] PATCH endpoint updates settings correctly
- [ ] `maxTurns` is passed to SDK options
- [ ] System prompt modes affect SDK behavior
- [ ] Tool allow/disallow lists are respected

### Manual QA Checklist (do not mark—human verification)

- [ ]? Settings modal opens from gear icon
- [ ]? Max turns input saves and persists
- [ ]? System prompt append mode adds to default
- [ ]? Custom system prompt replaces default
- [ ]? Disabling a tool prevents Claude from using it
- [ ]? Settings survive session resume

## Notes

- Phase 1: Default `maxTurns` is 100 (matches current hardcoded value). Default `systemPrompt` is `{ type: 'default' }`.
- Phase 3: SDK options `customSystemPrompt` and `appendSystemPrompt` are mutually exclusive. Use one based on mode.
- Phase 6: The `CLAUDE_TOOLS` constant should include id, name, description, and category for UI display.
- Phase 9: The "preview" feature for combined prompt is deferred—it would require fetching the SDK's default prompt.
- Phase 10: Consider persisting allowlist/blocklist mode preference per workspace.
