# Task: Claude SDK Server Streaming (Text-Only MVP)

## Objective

Stream Claude SDK text deltas as OpenCode `message.part.updated` events so the existing OpenCode thread UI works without changes.

## Scope

- Enable `includePartialMessages` (or SDK equivalent) to stream deltas.
- Emit OpenCode `message.part.updated` with `delta` per chunk and final `text` part on completion.
- Emit `message.updated` on user + assistant creation and on completion.
- Emit `session.status` busy/idle transitions.

## Event mapping (text-only)

- On prompt submit:
  - `message.updated` for user message (summary.title = full prompt)
  - `message.updated` for assistant message (created)
  - `session.status` type=busy

- On streaming token chunk:
  - `message.part.updated` with `part.type = "text"`
  - `part.text` = accumulated text
  - `delta` = chunk

- On completion:
  - `message.updated` assistant with `time.completed`
  - `session.status` type=idle
  - `session.idle`

## Implementation notes

- Maintain in-memory per-message accumulator and persist parts incrementally.
- Use stable part ID per assistant message so updates replace existing part.
- Persist each delta (append or replace) to storage to allow replay.

## Acceptance criteria

- UI shows live streaming text for Claude SDK sessions.
- After restart, thread renders completed text from DB.
- No tool/permission events required in this task.
