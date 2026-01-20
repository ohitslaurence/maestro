# Task: OpenCode Server Manager in Daemon

## Objective

Add daemon-side support to spawn and manage OpenCode servers per workspace, and bridge OpenCode SSE events to Maestro.

## Background

OpenCode provides an HTTP server (Bun) with REST APIs and SSE events:
- Server entry: `external/opencode/packages/opencode/src/server/server.ts`
- SSE endpoint: `GET /event`
- Sessions live inside a server instance; multiple sessions per workspace
- Event types include `message.part.updated`, `message.updated`, `session.status`, `session.diff`, `permission.asked`, and `pty.*`

We decided:
- Run OpenCode in the daemon
- One OpenCode server per workspace
- No auth for now

Codex Monitor spawns the app-server on workspace connect and stores the session in backend state. We should mirror that model for OpenCode servers in the daemon.

## Scope

### Server lifecycle
- Spawn `opencode serve --hostname 127.0.0.1 --port 0` for a workspace path
- Track process pid + base URL per workspace
- Shutdown on explicit disconnect or daemon shutdown
- Health check: verify `/event` or `/path` responds

### Event bridge
- Start SSE subscription for each workspace server
- Forward events to Tauri/frontend via existing daemon event channel
- Keep event payloads as close to OpenCode as possible (typed wrappers)

### API surface (daemon protocol)
- `opencode_connect_workspace({ workspaceId, path })`
- `opencode_disconnect_workspace({ workspaceId })`
- `opencode_status({ workspaceId })`
- `opencode_session_list({ workspaceId })`
- `opencode_session_create({ workspaceId, title? })`
- `opencode_session_prompt({ workspaceId, sessionId, parts, providerId, modelId })`
- `opencode_session_abort({ workspaceId, sessionId })`

## Files to touch

Daemon
- `daemon/src/state.rs` add OpenCode server registry
- `daemon/src/handlers/` add OpenCode request handlers
- `daemon/src/handlers/events.rs` or equivalent to emit `opencode:event` messages
- `daemon/src/handlers/sessions.rs` optional: extend session model to include OpenCode

Tauri bridge
- `app/src-tauri/src/daemon/protocol.rs` add OpenCode RPC types
- `app/src-tauri/src/daemon/commands.rs` add OpenCode proxy commands

Frontend service wrappers
- `app/src/services/tauri.ts`
- `app/src/services/events.ts`

## Event payload mapping

Forward OpenCode event payloads as:
```
type: "opencode:event"
payload: { workspaceId, event: { type, properties } }
```

Key event types to support initially:
- `message.updated`
- `message.part.updated` (with delta streaming)
- `message.part.removed`
- `session.status`
- `session.error`
- `session.diff`
- `permission.asked`
- `permission.replied`
- `pty.created` / `pty.updated` / `pty.exited` / `pty.deleted`

## Acceptance criteria

- `opencode_connect_workspace` spawns a server and stores a running handle
- Daemon can report status (running + base URL) per workspace
- SSE stream events from the server are bridged to frontend
- Servers are cleaned up on disconnect or daemon shutdown
- No auth or TLS is required to connect (local only)

## Notes / Risks

- OpenCode requires Bun; daemon host must have it available
- `opencode serve` default port is 4096; with `--port 0` we must parse actual bound port
- SSE reconnects should be resilient to transient disconnects
- OpenCode server is per-directory; use workspace path for `x-opencode-directory` or query params
