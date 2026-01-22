# Session Settings Implementation Plan

Reference: [session-settings.md](../session-settings.md)

## Phase 1: Server - Data Model

- [x] Add `SessionSettings` interface with `maxTurns`, `systemPrompt`, `disallowedTools` fields (§3.1)
- [x] Add `SystemPromptConfig` interface with mode field (§3.1)
- [x] Add `DEFAULT_SESSION_SETTINGS` constant (§3.1)
- [x] Add `settings` field to `Session` interface (§3.1)
- [x] Update `CreateSessionRequest` to accept `settings` (§4.1)
- [x] Update session storage to persist settings (§3.2)
- [x] Apply default settings when not provided

## Phase 2: Server - Settings Endpoint

- [x] Create `PATCH /session/:id/settings` endpoint (§4.1)
- [x] Implement merge semantics: undefined=unchanged, null=reset (§4.1)
- [x] Validate `maxTurns` range 1-1000 (§4.1)
- [x] Validate `systemPrompt.content` required for append/custom modes (§4.1)
- [x] Emit `session.updated` SSE event on change (§4.3)
- [x] Return updated session in response

## Phase 3: Server - SDK Integration [BLOCKED by: Phase 1]

- [x] Wire `maxTurns` from settings to `buildSdkOptions()` (§4.2)
- [x] Wire `customSystemPrompt` for custom mode (§4.2)
- [x] Wire `appendSystemPrompt` for append mode (§4.2)
- [x] Wire `disallowedTools` from settings (§4.2)

## Phase 4: Tauri Commands [BLOCKED by: Phase 2]

- [x] Add `claude_sdk_session_settings_update` command
- [x] Add settings to `claude_sdk_session_get` response
- [x] Update Rust types for `SessionSettings`

## Phase 5: Frontend Service Layer [BLOCKED by: Phase 4]

- [x] Add `claudeSdkSessionSettingsUpdate` service function
- [x] Add TypeScript types for `SessionSettings`, `SystemPromptConfig`
- [x] Add `CLAUDE_TOOLS` constant with tool metadata (§Appendix)

## Phase 6: useSessionSettings Hook [BLOCKED by: Phase 5]

- [x] Create `useSessionSettings` hook
- [x] Fetch current settings from session
- [x] Implement `updateSettings` with optimistic update (§5.1)
- [x] Handle error rollback (§6.2)

## Phase 7: UI Components - Modal Shell [BLOCKED by: Phase 6]

- [ ] Create `SessionSettingsModal.tsx` container (§Appendix)
- [ ] Create `SessionSettingsButton.tsx` gear icon
- [ ] Add modal trigger to session header
- [ ] Implement section layout

## Phase 8: UI Components - Execution Section [BLOCKED by: Phase 7]

- [ ] Add max turns input field (§Appendix)
- [ ] Validate range (1-1000)
- [ ] Show helper text explaining the setting

## Phase 9: UI Components - System Prompt Section [BLOCKED by: Phase 7]

- [ ] Create `SystemPromptEditor.tsx` component
- [ ] Implement mode toggle (default/append/custom) (§Appendix)
- [ ] Add textarea for append/custom content
- [ ] Disable textarea when mode is 'default'

## Phase 10: UI Components - Tools Section [BLOCKED by: Phase 7]

- [ ] Create `ToolSelector.tsx` component
- [ ] Display tool checkboxes by category (§Appendix)
- [ ] Implement blocklist selection (checked = disabled)
- [ ] Show tool descriptions on hover

## Phase 11: Thread View Integration [BLOCKED by: Phase 7]

- [ ] Add `SessionSettingsButton` to session header in `ClaudeThreadView`
- [ ] Wire modal open/close state
- [ ] Refresh session data after settings update

## Files to Create

- `app/src/features/claudecode/components/SessionSettingsModal.tsx`
- `app/src/features/claudecode/components/SessionSettingsButton.tsx`
- `app/src/features/claudecode/components/SystemPromptEditor.tsx`
- `app/src/features/claudecode/components/ToolSelector.tsx`
- `app/src/features/claudecode/hooks/useSessionSettings.ts`
- `app/scripts/ui-session-settings.ts`

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
- [ ] Tool disallowedTools list is respected
- [ ] UI Feature Validation:
  - [ ] `cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth`
  - [ ] `cd app && bun run dev -- --host 127.0.0.1 --port 1420`
  - [ ] `cd app && bun scripts/ui-session-settings.ts`

### Manual QA Checklist (do not mark—human verification)

- [ ]? Settings modal opens from gear icon
- [ ]? Max turns input saves and persists
- [ ]? System prompt append mode adds to default
- [ ]? Custom system prompt replaces default
- [ ]? Disabling a tool prevents Claude from using it
- [ ]? Settings survive session resume

## Notes

- Phase 1: Default `maxTurns` is 100. Default `systemPrompt` is `{ mode: 'default' }`. No `allowedTools` field—blocklist only.
- Phase 2: Merge semantics: `null` resets to default, `undefined` leaves unchanged.
- Phase 3: SDK options `customSystemPrompt` and `appendSystemPrompt` are mutually exclusive. Use one based on mode.
- Phase 5: The `CLAUDE_TOOLS` constant should include id, name, description, and category for UI display.
- Phase 9: Lazy initialization: existing sessions without `settings` field get `DEFAULT_SESSION_SETTINGS` applied on read.
