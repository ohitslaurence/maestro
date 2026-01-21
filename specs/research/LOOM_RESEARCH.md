# Loom Research

## Executive Summary

Loom is a Rust-first, multi-crate agent platform that treats orchestration as a pure state machine with
explicit event and action types. Its design emphasizes deterministic agent behavior, server-side
LLM proxying, local-first thread persistence with optional sync, and rich streaming semantics that are
portable across providers. The surrounding platform adds editor integration, a web UI that mirrors the
agent state machine, and a remote execution story built around ephemeral Kubernetes pods (Weaver) with
secure network access via WireGuard tunnels.

Key takeaways for Maestro are the explicit agent state machine with post-tool hooks, the local-first
thread store + pending sync queue, a unified streaming schema (text/tool deltas + completion), and a
clear separation of orchestration from I/O in both backend and UI.

---

## 1. System Architecture and Modularization

### Workspace Structure

Loom is a large Cargo workspace with dedicated crates for core agent logic, tool registry, thread
storage, LLM clients, server APIs, and auxiliary systems. This creates clean layering boundaries and
keeps orchestration logic isolated from transport or UI concerns.

Reference: `reference/loom/README.md`

Core crates called out:
- `loom-core`: agent state machine, core types
- `loom-tools`: tool registry and implementations
- `loom-thread`: thread persistence and sync
- `loom-server`: HTTP API, LLM proxy, thread APIs
- `loom-cli`: REPL-driven client
- `loom-web`: Svelte 5 web client

### Server-Side LLM Proxy

All provider credentials live server-side. Clients call `/proxy/{provider}` and receive normalized
streaming responses. This avoids embedding secrets in the client while keeping streaming consistent.

Reference: `reference/loom/README.md`, `reference/loom/specs/architecture.md`

---

## 2. Agent Orchestration Core

### Explicit State Machine with Event/Action Inversion

The agent core is a synchronous state machine that receives `AgentEvent` and returns `AgentAction`.
The caller performs I/O (LLM calls, tool execution) and then feeds results back as events. This keeps
orchestration deterministic and testable while permitting async I/O externally.

References: `reference/loom/specs/state-machine.md`,
`reference/loom/crates/loom-common-core/src/agent.rs`,
`reference/loom/crates/loom-common-core/src/state.rs`

Key characteristics:
- Predictable transitions with exhaustive `match` handling
- Clean separation of pure state logic from I/O
- Retry and error origin tracking (`Llm`, `Tool`, `Io`)
- Conversation context carried explicitly in each state variant

State overview (selected):
- `WaitingForUserInput`: idle state
- `CallingLlm`: LLM request in flight with retry counters
- `ProcessingLlmResponse`: parse response to determine tool vs text
- `ExecutingTools`: manages concurrent tool executions
- `PostToolsHook`: runs side effects after tool execution (e.g., auto-commit)
- `Error`, `ShuttingDown`

Transition structure (selected):
```
WaitingForUserInput -> CallingLlm -> ProcessingLlmResponse
ProcessingLlmResponse -> ExecutingTools -> PostToolsHook -> CallingLlm
```

### Post-Tool Hooks

The `PostToolsHook` state executes only after mutating tools complete. This isolates post-processing
workflows (auto-commit, telemetry, diagnostics) from tool execution and avoids cluttering the main
loop.

Reference: `reference/loom/specs/state-machine.md`

### Tool Execution Status as First-Class State

Tool executions are tracked in a discriminated union with lifecycle timestamps and optional progress
updates. This enables UI to show progress, and orchestration to handle multiple tools concurrently.

Reference: `reference/loom/specs/tool-system.md`,
`reference/loom/crates/loom-common-core/src/state.rs`

---

## 3. Tool System

### Tool Abstraction

Tools implement a trait with an input schema and `invoke` method. Tool definitions are serialized to
LLM provider formats. The execution context enforces a workspace boundary, preventing tools from
reading or writing outside a configured root.

References: `reference/loom/specs/tool-system.md`,
`reference/loom/crates/loom-common-core/src/tool.rs`

Notable patterns:
- `ToolDefinition` is a minimal, LLM-serializable structure (name/description/schema)
- `ToolContext` passes `workspace_root` as an explicit security boundary
- `ToolRegistry` abstracts tool lookup, and produces the complete tool definition list for LLM

### Tool Execution Lifecycle

Tool executions move through `Pending -> Running -> Completed`, with optional `ToolProgress` events.
This lends itself to UI progress bars and better tracing.

Reference: `reference/loom/specs/tool-system.md`

### Built-in Tools and Secondary LLM Tooling

The tool set includes file read/list/edit capabilities and an `oracle` tool that delegates reasoning
to a secondary LLM provider via the server proxy. This is a clean pattern for multi-model orchestration
while keeping the main agent model focused.

Reference: `reference/loom/specs/tool-system.md`

---

## 4. Thread Persistence and Sync

### Thread Model

Threads are JSON documents with full conversation snapshots, agent state snapshots, and metadata.
IDs use UUID7 for time ordering with a `T-` prefix. The schema includes privacy and visibility flags
for safe sharing.

Reference: `reference/loom/specs/thread-system.md`

Selected fields:
- `conversation.messages`: message snapshots
- `agent_state`: state kind + retries + pending tools
- `metadata`: title, tags, pinned status
- `is_private`: hard local-only sessions (never sync)

### Persistence + Sync Flow

Local writes are always first and atomic. Sync runs best-effort in the background with a pending
queue for retry. This prevents data loss while remaining responsive offline.

Reference: `reference/loom/specs/thread-system.md`

Key mechanics:
- Local store uses XDG paths and atomic rename writes
- Background sync via `SyncingThreadStore` with retry queue
- Pending sync entries persisted in `$XDG_STATE_HOME`
- Optimistic concurrency via `If-Match` with version counter
- Private threads never sync by invariant

### Server API Shape

The thread server exposes REST endpoints for list/get/upsert/delete with version-based conflicts. This
keeps consistency predictable and easy to debug.

Reference: `reference/loom/specs/thread-system.md`

---

## 5. Streaming and LLM Provider Normalization

### Unified Streaming Events

Loom collapses provider-specific streaming into a shared `LlmEvent` union: `TextDelta`,
`ToolCallDelta`, `Completed`, `Error`. This allows the agent loop and UI to be provider-agnostic.

Reference: `reference/loom/specs/streaming.md`

Key aspects:
- Tool call deltas stream incremental JSON fragments
- `Completed` includes final message content + full parsed tool calls
- Error propagation modeled consistently across providers

### Provider-Specific Parsing Strategy

Anthropic and OpenAI streams are parsed into the same `LlmEvent` stream, including tool call
accumulators and stop reason mapping. This keeps client logic stable across providers.

Reference: `reference/loom/specs/streaming.md`

---

## 6. Editor Integration via ACP

Loom implements the Agent Client Protocol (ACP) over stdio JSON-RPC for editor plugins (e.g., VSCode,
Zed). Sessions map to Loom threads, and `session/update` streams deltas back to the editor.

Reference: `reference/loom/specs/acp-system.md`

Notable mapping details:
- `session/new` creates a thread and returns it as `SessionId`
- `session/prompt` runs the agent loop and streams deltas
- `session/cancel` sets a cancellation flag checked by the LLM stream

---

## 7. Loom Web UI Architecture

The web UI is built in Svelte 5 and mirrors the backend state machine using XState. It separates typed
API clients, realtime clients (SSE/WebSocket), state machines, and feature components. This makes it
easy to keep UI state consistent with the backend agent lifecycle.

Reference: `reference/loom/specs/loom-web.md`

Highlights:
- Thread list with virtualization and real-time updates
- Agent state timeline visualization based on `AgentStateKind`
- Tool execution panel driven by `ToolExecutionStatus`
- Dedicated `api/`, `realtime/`, `state/`, `components/` layering

---

## 8. Remote Execution and Isolation (Weaver)

Weaver provisions ephemeral Kubernetes pods to run agents in isolated environments. K8s is the source
of truth; there is no database for weaver state. Weaver lifetimes are TTL-bound with automatic cleanup.

Reference: `reference/loom/specs/weaver-provisioner.md`

Key design points:
- TTL default 4 hours, max 48 hours
- Pod creation via a small REST API
- Security hardened: non-root, restricted capabilities
- Logs streamed via SSE
- Repo cloning and branch selection are part of provision request

This provides a clean path for running tasks remotely without long-lived infra state.

---

## 9. Secure Remote Access (WireGuard Tunnels)

Loom plans a WireGuard tunnel subsystem for secure, low-latency access to weaver pods. The server
coordinates peers and addresses, but does not relay traffic. DERP provides NAT traversal fallback.

Reference: `reference/loom/specs/wgtunnel-system.md`

Key ideas:
- Direct P2P WireGuard preferred, DERP fallback
- Ephemeral weaver keypair per pod
- Persistent device keys per user device
- Server only for control plane (registration, IP allocation)

This can enable secure SSH or service access to remote agent environments without routing traffic
through the coordinator.

---

## 10. Applicability to Maestro

### High-Value Patterns to Reuse

1. Explicit agent state machine with event/action inversion for deterministic orchestration.
2. Post-tool hooks for side effects (auto-commit, diagnostics) in a clean stage.
3. Tool execution lifecycle modeled with progress and status enums.
4. Local-first session persistence with background sync and pending queue.
5. Unified streaming event schema across LLM providers.
6. UI state machines mirroring backend agent state and tool progress.

### Long-Term Inspiration

1. ACP editor integration for IDE-first workflows.
2. Weaver-style ephemeral execution environments for remote scale-out.
3. WireGuard tunnel system for secure remote access without central relay.

---

## 11. Practical, Near-Term Roadmap Ideas for Maestro

These are implementable without building the full Loom platform and can be scoped independently:

1. **Adopt a strict agent state machine** with `AgentEvent` and `AgentAction` enums to keep harness
   implementations deterministic and testable.
2. **Add a post-tool hook stage** for optional workflows (auto-commit, lint, telemetry) triggered only
   when mutating tools run.
3. **Normalize streaming events** across harnesses into a single schema that the UI subscribes to.
4. **Local-first session persistence** with snapshots and an optional sync queue for later server
   integration.
5. **UI state machine mirror** to keep agent status, tool progress, and streaming content consistent.

Each can be implemented incrementally and provides immediate UX and reliability benefits without
waiting for remote execution or multi-device sync.
