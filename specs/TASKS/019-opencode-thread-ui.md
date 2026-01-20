# Task: OpenCode Thread UI + Composer

## Objective

Render OpenCode sessions as a structured thread (messages + parts) with a chat composer, modeled after Codex Monitor.

## Background

Codex Monitor does not render the terminal as the primary session view. Instead it renders structured items:
- `message`, `reasoning`, `tool`, `diff`, `review`
- Items are streamed via events and merged into a list
- Composer handles send/queue/stop and supports attachments + autocomplete

OpenCode already emits structured message parts through `message.part.updated` SSE events, so we can render a thread without terminal parsing.

Reference components in Codex Monitor:
- `reference/codex-monitor/src/features/messages/components/Messages.tsx`
- `reference/codex-monitor/src/features/composer/components/Composer.tsx`
- `reference/codex-monitor/src/features/composer/components/ComposerInput.tsx`

## Scope

### Thread state
- Maintain per-session message list + part list
- Merge `message.updated` and `message.part.updated` events into state
- Map OpenCode parts into Maestro `ConversationItem` equivalents

### Rendering
- Message list with auto-scroll when user is near bottom
- Collapsible tool output
- Reasoning collapse/expand
- Diff items when available

### Composer
- Textarea with send/stop
- Shift+Enter for newline
- Optional attachments (image paths)
- Optional model/provider selector (simple dropdowns)

## Data model mapping (OpenCode -> Maestro)

OpenCode MessageV2 parts from `external/opencode/packages/opencode/src/session/message-v2.ts`:
- `text` -> message row
- `reasoning` -> reasoning row
- `tool` -> tool row (pending/running/completed/error)
- `patch` / `snapshot` -> diff row or link to diff fetch
- `step-start` / `step-finish` -> processing status
- `file` / `agent` / `subtask` / `retry` / `compaction` -> auxiliary rows or metadata

## Files to touch (proposed)

Frontend
- `app/src/features/threads/` new feature (if not present) or extend existing session UI
- `app/src/services/events.ts` add OpenCode event adapter
- `app/src/types.ts` define `ConversationItem` or OpenCode thread types
- `app/src/App.tsx` compose Messages + Composer for OpenCode sessions

Shared styles
- `app/src/styles.css` for thread + composer styles

## Acceptance criteria

- OpenCode sessions display as a structured list of items
- Streaming text updates are incremental (delta appended)
- Reasoning/tool items are collapsible
- Composer can send a message and trigger `opencode_session_prompt`
- Send button becomes Stop while processing

## Open questions

- Should Maestro support a single unified thread UI for Claude Code + OpenCode, or per-harness views?
- Which part types should be hidden vs visible in the first iteration?
