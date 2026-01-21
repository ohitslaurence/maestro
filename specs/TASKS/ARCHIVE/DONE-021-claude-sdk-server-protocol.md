# Task: Claude Agent SDK Server Protocol (OpenCode-Compatible)

## Objective

Define a minimal server protocol for Claude Agent SDK that mirrors OpenCode’s REST + SSE event model, so the frontend can reuse the same thread rendering pipeline.

## Background

Claude Code does not expose a native server like OpenCode/Codex. The Agent SDK is a library (`query()` stream) and must be embedded in our own service. We can build a daemon-side HTTP server with OpenCode‑compatible endpoints and event payloads.

OpenCode reference
- Event stream: `GET /event`
- Events include `message.part.updated`, `message.updated`, `session.status`, `permission.asked`, `pty.*`
- Message parts are structured (text, reasoning, tool, patch, snapshot, etc.)

Claude Agent SDK reference
- `query()` streams messages
- Supports sessions via `resume`
- Hooks for tool use, approvals, session start/end

## Proposed protocol (OpenCode‑style)

### Endpoints (mirror OpenCode SDK v2)
- `GET /session`
  - query: `directory?`, `roots?`, `start?`, `search?`, `limit?`
  - response: `Session[]`
- `POST /session`
  - query: `directory?`
  - body: `{ parentID?, title?, permission? }`
  - response: `Session`
- `GET /session/:id`
  - query: `directory?`
  - response: `Session`
- `POST /session/:id/message`
  - query: `directory?`
  - body: `{ messageID?, model?, agent?, noReply?, system?, variant?, parts: PartInput[] }`
  - response: `{ info: AssistantMessage, parts: Part[] }`
- `POST /session/:id/abort`
  - response: `204` or `{ ok: true }`
- `GET /event`
  - query: `directory?`
  - response: `Event` (SSE, raw event payload)
- `GET /global/event`
  - response: `{ directory: string, payload: Event }`

### Event envelope (match OpenCode)
- Instance stream: raw `Event` payload
- Global stream: `{ directory, payload: Event }`

Event example:
```
{
  "type": "message.part.updated",
  "properties": {
    "part": { "id": "p1", "messageID": "m1", "type": "text", "text": "hello" },
    "delta": "hello"
  }
}
```

### Event types (subset for MVP)
- `message.updated` `{ info: MessageInfo }`
- `message.part.updated` `{ part: Part, delta? }`
- `message.part.removed` `{ sessionID, messageID, partID }`
- `session.status` `{ sessionID, status: { type: "idle" | "busy" | "retry", attempt?, message?, next? } }`
- `session.idle` `{ sessionID }`
- `session.created` `{ info: Session }`
- `session.updated` `{ info: Session }`
- `session.deleted` `{ info: Session }`
- `session.diff` `{ sessionID, diff: FileDiff[] }`
- `session.error` `{ sessionID?, error? }`
- `permission.asked` `{ id, sessionID, permission, patterns, metadata, always, tool? }`
- `permission.replied` `{ sessionID, requestID, reply: "once" | "always" | "reject" }`
- `pty.created` / `pty.updated` / `pty.exited` / `pty.deleted` (optional)

### Message/Part schema (OpenCode‑compatible subset)
- `Session`
  - `{ id, slug, projectID, directory, parentID?, title, version, time: { created, updated, compacting?, archived? }, summary?, share?, permission?, revert? }`
- `MessageInfo`
  - `{ id, sessionID, role: "user" | "assistant", time: { created, completed? }, modelID, providerID, cost?, tokens?, error?, parentID? }`
- `PartInput` (for POST /session/:id/message)
  - `TextPartInput` `{ id?, type: "text", text, synthetic?, ignored?, time?, metadata? }`
  - `FilePartInput` `{ id?, type: "file", mime, filename?, url, source? }`
  - `AgentPartInput` `{ id?, type: "agent", name, source? }`
  - `SubtaskPartInput` `{ id?, type: "subtask", prompt, description, agent, model?, command? }`
- `Part` (streamed)
  - `text` / `reasoning` / `tool` / `patch` / `snapshot` / `step-start` / `step-finish` / `agent` / `retry` / `compaction`

## Mapping: Agent SDK → event types

Use SDK hooks to emit events:
- `SessionStart` → `session.created` + `session.status` (busy)
- Streaming assistant tokens → `message.part.updated` with `delta`
- Tool start/finish → `message.part.updated` tool state transitions
- `Stop` hook → `session.status` idle + `step-finish`
- Approval prompts → `permission.asked` / `permission.replied`

## Compatibility contract (no unanswered questions)

- Session list uses `start` (updated since, ms) + `limit` (no cursor).
- SSE instance stream is raw `Event` (no wrapper); global stream wraps with `directory`.
- All payloads should conform to OpenCode SDK v2 shapes in `external/opencode/packages/sdk/js/src/v2/gen/types.gen.ts`.
- The frontend can treat Claude SDK sessions as OpenCode sessions with no UI changes.

## Implementation sketch (daemon)

1) Embed a Node service (TS) in daemon process or spawn a child service.
2) Each workspace spawns a server instance bound to localhost.
3) Maintain session registry: `{ sessionId -> sdk.resumeId }`.
4) SSE fan‑out to connected clients (Maestro frontend via daemon bridge).
5) Normalize all events to OpenCode schema.

## Acceptance criteria

- Protocol documented in the repo
- Event payloads are compatible with OpenCode UI rendering pipeline
- Supports sessions + resume without the CLI/TUI

## Follow-up tasks

- `023-claude-sdk-server-storage.md` for session/message persistence
- `024-claude-sdk-server-streaming.md` for text streaming deltas

## Notes / Risks

- Agent SDK uses API keys; must not rely on Claude.ai login
- Branding: avoid calling it “Claude Code” in product UI
- If we want exact CLI parity, we may still need PTY proxy for some workflows
