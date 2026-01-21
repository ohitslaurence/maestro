# Task: Claude SDK Server Storage + Sessions

## Objective

Persist Claude SDK sessions/messages/parts to disk so the server can list, resume, and render threads like OpenCode.

## Scope

- SQLite-backed storage under `~/.maestro/claude/` (or `$MAESTRO_DATA_DIR/claude/`).
- Session registry compatible with OpenCode `Session` schema.
- Message/part persistence compatible with OpenCode events.
- Resume support using SDK `resume` IDs stored per session.

## Data layout

Default root: `~/.maestro/claude/`.

Per workspace:

```
~/.maestro/claude/<workspace_hash>/
  sessions.sqlite
  config.json (optional)
```

Workspace hash = stable hash of absolute workspace path.

## Tables (minimal)

- `sessions`
  - `id` (pk) OpenCode session ID
  - `slug`
  - `project_id`
  - `directory`
  - `parent_id`
  - `title`
  - `version`
  - `created_at`
  - `updated_at`
  - `resume_id` (Claude SDK)

- `messages`
  - `id` (pk)
  - `session_id`
  - `role`
  - `created_at`
  - `completed_at`
  - `parent_id`
  - `model_id`
  - `provider_id`
  - `agent`
  - `mode`
  - `system`
  - `variant`
  - `summary_title`
  - `summary_body`
  - `cost`
  - `tokens_input`
  - `tokens_output`
  - `tokens_reasoning`
  - `tokens_cache_read`
  - `tokens_cache_write`
  - `error_name`
  - `error_payload` (json)

- `parts`
  - `id` (pk)
  - `session_id`
  - `message_id`
  - `type`
  - `text`
  - `content`
  - `tool`
  - `call_id`
  - `title`
  - `input_json`
  - `output`
  - `error`
  - `hash`
  - `files_json`
  - `time_start`
  - `time_end`
  - `metadata_json`

## API behavior

- `GET /session` returns sessions from DB (filter by `start`, `limit`).
- `GET /session/:id` returns session row.
- `POST /session` writes new row and emits `session.created`.
- `POST /session/:id/message` writes user message immediately; assistant message/parts as they stream.
- `POST /session/:id/abort` marks current run aborted and updates message status.

## Requirements

- Store every event-relevant change so replay works after restart.
- Session resume uses stored `resume_id`.
- Upsert patterns to avoid duplicate message/part IDs on retries.

## Acceptance criteria

- Restarted server can list sessions and rebuild threads for an existing workspace.
- `resume_id` persists and is reused on the next message.
- All OpenCode events reflect persisted state.
