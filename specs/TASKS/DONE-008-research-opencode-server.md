# Task: Research Open Code Server Protocol

## Objective

Research Open Code's client/server architecture to understand how Maestro can integrate with it as an agent harness.

## Background

From initial research:
- Repo: `sst/opencode` (formerly `anomalyco/opencode`)
- Has client/server architecture
- "allows OpenCode to run on your computer, while you can drive it remotely from a mobile app"
- Desktop app exists (beta) - Tauri-based

## Questions to Answer

1. **Protocol**: What protocol does Open Code use? (JSON-RPC, REST, gRPC, WebSocket?)
2. **Session management**: How to spawn/attach to sessions?
3. **Event model**: How are events streamed to clients?
4. **Authentication**: How does auth work between client and server?
5. **Desktop app**: How does their Tauri app communicate with the server?

## Resources to Research

### Open Code Repository
- GitHub: `sst/opencode`
- Look for server implementation code
- Protocol definitions
- Client SDK or examples

### Desktop App
- How is it structured?
- What can we learn from their Tauri integration?

## Output

Create `specs/OPENCODE_RESEARCH.md` with:

### 1. Architecture Overview
- Server component location and structure
- Client-server communication flow
- Deployment model (local daemon vs remote)

### 2. Protocol Details
- Transport layer (TCP, WebSocket, HTTP?)
- Message format (JSON-RPC, custom?)
- Request/response patterns
- Event/notification patterns

### 3. Session Management
- How to list running sessions
- How to spawn new sessions
- How to attach to existing sessions
- Session lifecycle states

### 4. Event Streaming
- What events are emitted?
- How to subscribe?
- Output streaming approach

### 5. Authentication
- Auth mechanism
- Token format
- Security considerations

### 6. Integration Options for Maestro
- Direct protocol integration
- SDK usage (if available)
- Comparison to Claude Code integration

### 7. Recommendations
- Recommended integration approach
- Code structure to reference
- Gaps/limitations

## Method

1. Index Open Code repo via Nia
2. Explore server implementation
3. Find protocol definitions
4. Review desktop app code
5. Look for any documentation

## Constraints

- Focus on stable/documented interfaces
- Note experimental features
- Compare to CodexMonitor's Codex integration
