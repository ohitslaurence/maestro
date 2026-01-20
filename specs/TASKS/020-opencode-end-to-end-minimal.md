# Task: OpenCode End-to-End Minimal Slice

## Objective

Deliver a minimal vertical slice: daemon spawns OpenCode per workspace, UI creates a session, sends a prompt, and renders streamed text parts.

## Background

We want to validate the session UX for OpenCode before full feature parity. This task ties together the daemon server manager and the thread UI with a minimal set of event types.

OpenCode reference facts:
- HTTP server with REST + SSE at `/event`
- Session endpoints: `GET /session`, `POST /session`, `POST /session/:id/message`
- Streaming event: `message.part.updated` with `delta` for text

## Scope (MVP)

### Daemon
- Spawn server on workspace connect (per workspace)
- SSE bridge for events
- Minimal API: create session, send message, abort

### Frontend
- Create a session if none exists
- Render `text` parts only (ignore reasoning/tool for now)
- Basic composer: text input + send/stop
- Auto-scroll to bottom while streaming

## Implementation steps

1) Add daemon OpenCode server manager (see task 018)
2) Add frontend service wrappers for OpenCode session API
3) Add minimal thread state store:
   - messages list
   - parts list keyed by messageId
   - append `delta` for `message.part.updated`
4) Render text parts as chat bubbles
5) Add composer input to send prompt

## Acceptance criteria

- Selecting a workspace spawns OpenCode server in the daemon
- UI can create a session via OpenCode API
- Sending a prompt streams assistant text into the thread
- UI remains scrollable while streaming

## Out of scope (later)

- Tool output rendering
- Reasoning panels
- Diff rendering
- Attachments
- Permission UI
