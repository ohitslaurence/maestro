# Agent State Machine and Post-Tool Hooks

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview
### Purpose
Define a deterministic session state machine that orchestrates LLM turns, tool execution, and post-tool
hooks across all harnesses. Provide a shared event model so backend and UI remain consistent.

### Goals
- Single state machine shared by all harness adapters.
- Separate orchestration logic from I/O execution.
- First-class tool lifecycle and post-tool hook stage.
- Structured state/tool/hook events for UI consumption.
- Support cancellation and bounded retries.

### Non-Goals
- Tool registry design or implementations.
- UI layout and styling decisions.
- Remote daemon transport specifics beyond event names.

---

## 2. Architecture
### Components
- **AgentStateMachine**: pure transition logic, no I/O.
- **HarnessAdapter**: maps harness output into `AgentEvent` inputs.
- **ToolRunner**: executes tool calls and returns status.
- **PostToolHookRunner**: runs hook pipeline after mutating tools.
- **SessionBroker**: session registry, event loop, event emission.
- **Frontend Session Reducer**: consumes state/tool/hook events.

### Execution Model
- All state transitions happen in `handle_event` and are synchronous.
- I/O occurs outside the state machine (harness I/O, tool execution, hooks).
- Only one active LLM request per session (`CallingLlm`).
- Tool calls may run concurrently, but each batch must finish before moving to hooks.
- Post-tool hooks are single-threaded per session to preserve determinism.
- Each session owns a FIFO event queue; events are processed serially.
- Transition logic must be pure: no filesystem, network, or IPC in `handle_event`.
- `AgentAction` is advisory; callers must emit events for success/failure outcomes.

### Concurrency and Ordering
- Sessions are isolated; events for a session are processed in arrival order.
- Tool execution may be parallel, but tool completion events are re-queued in timestamp order.
- Stream events (`agent:stream_event`) are ordered by `seq` per `streamId`.
- State events (`agent:state_event`) are emitted after internal state mutation, before UI reducers.

### Hook Pipeline
- Hook evaluation happens once per tool batch, after all tool runs complete.
- Hooks receive the full set of completed tool runs for filtering.
- Hook execution is sequential and runs in the session workspace root.
- Hook environment is restricted to allowlisted keys; all other env vars are stripped.
- Hook output is captured and emitted as `hook_lifecycle` events (no streaming to LLM).

### Event Queue Semantics
- Events are processed at-most-once; callers are responsible for re-queueing retries.
- If an event cannot be applied (invalid transition), the machine emits `session_error` and remains
  in the prior state.

### Dependencies
- Orchestration: `app/src-tauri/src/sessions.rs`
- Event emitters: `app/src-tauri/src/lib.rs`
- Streaming output events: `specs/streaming-event-schema.md`
- Session persistence: `specs/session-persistence.md`
- Frontend event hub: `app/src/services/events.ts`

### Configuration Sources
- Hook configuration is loaded from `app_data_dir()/hooks.json` using the same pattern as
  `app/src-tauri/src/daemon/config.rs`.
- If the file is missing, post-tool hooks are disabled.
- If the file is invalid, emit `session_error` with `code=hook_config_invalid` and disable hooks.
- Hook configs are cached per app launch; no hot reload in v1.

### Module/Folder Layout
- `app/src-tauri/src/agent_state.rs` (new): state machine types + transitions
- `app/src-tauri/src/sessions.rs`: session registry + event loop
- `app/src-tauri/src/tools.rs` (existing or new): tool dispatch + metadata
- `app/src-tauri/src/hooks.rs` (new): post-tool hook runner
- `app/src/types/agent.ts` (new): shared TS enums/unions
- `app/src/hooks/useAgentSession.ts`: state reducer wiring

---

## 3. Data Model
### Core Types
| Type | Purpose | Notes |
| --- | --- | --- |
| `AgentStateKind` | High-level session state | Mirrors UI state + orchestration |
| `AgentState` | Full in-memory state | Carries retries, active stream, tool runs |
| `AgentEvent` | Inputs to state machine | User input, harness output, tool lifecycle |
| `AgentAction` | Outputs from state machine | Drives I/O: LLM calls, tools, hooks |
| `ToolRunStatus` | Tool execution lifecycle | `Queued`, `Running`, `Succeeded`, `Failed`, `Canceled` |
| `HookRunStatus` | Hook lifecycle | Same statuses as tools |
| `ToolCall` | Normalized tool invocation | Includes `mutating` flag |
| `ToolRunRecord` | Tool execution metadata | Start/end, attempt, error |
| `HookConfig` | Hook definition | Command, timeout, failure policy |

### Identifier Formats
- `SessionId`: `sess_<uuid>`
- `ToolRunId`: `toolrun_<uuid>`
- `HookRunId`: `hookrun_<uuid>`
- `StreamId`: `turn_<uuid>`

### AgentState (Rust)
```rust
pub struct AgentState {
    pub kind: AgentStateKind,
    pub active_stream_id: Option<String>,
    pub retries: u32,
    pub pending_tool_calls: Vec<ToolCall>,
    pub tool_runs: Vec<ToolRunRecord>,
    pub hook_runs: Vec<HookRunRecord>,
    pub last_error: Option<AgentError>,
}
```

### AgentState Payloads (Rust)
```rust
pub enum AgentStatePayload {
    Idle,
    Starting { harness: String, project_path: String },
    Ready { last_user_message_id: Option<String> },
    CallingLlm { stream_id: String, request_id: String, attempt: u32 },
    ProcessingResponse { stream_id: String },
    ExecutingTools { tool_run_ids: Vec<String> },
    PostToolsHook { hook_run_ids: Vec<String> },
    Error { error: AgentError },
    Stopping { reason: String },
    Stopped { exit_code: Option<i32> },
}
```

### AgentStateSnapshot (Persistence)
```rust
pub struct AgentStateSnapshot {
    pub kind: AgentStateKind,
    pub active_stream_id: Option<String>,
    pub pending_tool_calls: Vec<ToolCall>,
    pub tool_runs: Vec<ToolRunRecord>,
    pub hook_runs: Vec<HookRunRecord>,
    pub last_error: Option<AgentError>,
}
```

### AgentStateKind (Rust)
```rust
pub enum AgentStateKind {
    Idle,
    Starting,
    Ready,              // waiting for user input
    CallingLlm,         // request in flight
    ProcessingResponse, // parse response, collect tool calls
    ExecutingTools,
    PostToolsHook,
    Error,
    Stopping,
    Stopped,
}
```

### AgentEvent (Rust)
```rust
pub enum AgentEvent {
    UserInput { session_id: String, text: String },
    HarnessStream { session_id: String, stream_event: StreamEvent },
    ToolRequested { session_id: String, call: ToolCall },
    ToolStarted { session_id: String, run_id: String },
    ToolCompleted { session_id: String, run_id: String, status: ToolRunStatus },
    HookStarted { session_id: String, run_id: String, tool_run_id: String },
    HookCompleted { session_id: String, run_id: String, status: HookRunStatus },
    RetryTimeout { session_id: String, target: RetryTarget },
    StopRequested { session_id: String },
    HarnessExited { session_id: String, code: Option<i32> },
}
```

### RetryTarget and AgentError
```rust
pub enum RetryTarget {
    Llm,
    Tool { run_id: String },
    Hook { run_id: String },
}

pub struct AgentError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub source: ErrorSource,
}

pub enum ErrorSource {
    Harness,
    Tool,
    Hook,
    Orchestrator,
}
```

### AgentAction (Rust)
```rust
pub enum AgentAction {
    SendToHarness { session_id: String, input: String },
    ExecuteTools { session_id: String, tools: Vec<ToolCall> },
    RunPostToolHooks { session_id: String, tool_runs: Vec<String> },
    EmitStateChange { session_id: String, from: AgentStateKind, to: AgentStateKind },
    StopHarness { session_id: String },
    Wait,
}
```

### ToolCall and Mutating Tools
```rust
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub mutating: bool, // true for file edits, git ops, or shell commands
}
```

#### Mutating Tool Classification
- Default: tools are non-mutating unless marked otherwise by ToolRunner.
- Mutating tool categories:
  - File edits: `edit_file`, `write_file`, `apply_patch`
  - Shell execution: `bash`, `run_command`
  - Git operations: `git_commit`, `git_checkout`, `git_apply`
- Post-tool hooks run only if any tool in the batch is mutating.

#### Initial Mutating Tool List (v1)
- `read_file`, `list_files`, `grep`, `search` are non-mutating.
- `edit_file`, `write_file`, `apply_patch`, `bash`, `run_command`, `git_*` are mutating.
- Harness adapters may override classification when tools are unknown.

### ToolRunRecord
```rust
pub struct ToolRunRecord {
    pub run_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub mutating: bool,
    pub status: ToolRunStatus,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub attempt: u32,
    pub error: Option<String>,
}
```

### HookConfig and HookRunRecord
```rust
pub struct HookConfig {
    pub name: String,
    pub command: Vec<String>,
    pub timeout_ms: u64,
    pub failure_policy: HookFailurePolicy,
    pub tool_filter: HookToolFilter,
}

pub enum HookFailurePolicy {
    FailSession,
    WarnContinue,
    Retry { max_attempts: u32, delay_ms: u64 },
}

pub enum HookToolFilter {
    AnyMutating,
    ToolNames(Vec<String>),
}

pub struct HookRunRecord {
    pub run_id: String,
    pub hook_name: String,
    pub tool_run_ids: Vec<String>,
    pub status: HookRunStatus,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub attempt: u32,
    pub error: Option<String>,
}
```

### HookConfig JSON (hooks.json)
```json
{
  "hooks": [
    {
      "name": "auto_commit",
      "command": ["git", "commit", "-am", "Auto-commit"],
      "timeout_ms": 120000,
      "failure_policy": { "type": "fail_session" },
      "tool_filter": { "type": "any_mutating" }
    }
  ]
}
```

### Hook Defaults
- `failure_policy`: `fail_session`
- `timeout_ms`: 120000
- `tool_filter`: `any_mutating`

### Storage Schema (if any)
State machine data is in-memory only. Persistent snapshots are stored via
`specs/session-persistence.md` and must include:
- `AgentStateKind`
- pending tool calls
- tool/hook run summaries

---

## 4. Interfaces
### Public APIs
Tauri commands exposed to the frontend:
- `spawn_session(project_path, harness, config) -> SessionId`
- `attach_session(session_id) -> SessionSummary`
- `send_input(session_id, text) -> void`
- `stop_session(session_id) -> void`
- `list_sessions() -> SessionSummary[]`
- `retry_tool(session_id, tool_run_id) -> void`
- `retry_hook(session_id, hook_run_id) -> void`

### Internal APIs
- `handle_event(event) -> AgentAction`
- `run_tools(session_id, tools) -> Vec<ToolRunId>`
- `run_post_tool_hooks(session_id, tool_run_ids) -> Vec<HookRunId>`
- `emit_state_event(event: AgentStateEvent) -> void`

### Events (names + payloads)
Tauri event channel: `agent:state_event`

Payload union (`AgentStateEvent`):
- `state_changed`
  - `{ sessionId, from, to, reason, timestampMs, streamId? }`
- `tool_lifecycle`
  - `{ sessionId, runId, callId, toolName, mutating, status, attempt, startedAtMs, finishedAtMs?, error? }`
- `hook_lifecycle`
  - `{ sessionId, runId, hookName, toolRunIds, status, attempt, startedAtMs, finishedAtMs?, error? }`
- `session_error`
  - `{ sessionId, code, message, retryable, source }`

#### State Change Reasons
- `user_input`
- `stream_completed`
- `tools_requested`
- `tools_completed`
- `hooks_completed`
- `stop_requested`
- `harness_exited`

#### State Event Envelope
Every `agent:state_event` payload MUST include:
- `eventId`: unique identifier for the state event
- `timestampMs`: Unix epoch millis
- `sessionId`: session the event belongs to

Streaming output events are emitted separately via `agent:stream_event`
(see `specs/streaming-event-schema.md`).

---

## 5. Workflows
### State Transition Table (Selected)
| From | Event | To | Action |
| --- | --- | --- | --- |
| `Ready` | `UserInput` | `CallingLlm` | `SendToHarness` |
| `CallingLlm` | `HarnessStream(completed)` | `ProcessingResponse` | internal |
| `ProcessingResponse` | tool calls present | `ExecutingTools` | `ExecuteTools` |
| `ProcessingResponse` | no tool calls | `Ready` | `EmitStateChange` |
| `ExecutingTools` | all tools done + mutating | `PostToolsHook` | `RunPostToolHooks` |
| `ExecutingTools` | all tools done + non-mutating | `CallingLlm` | `SendToHarness` |
| `PostToolsHook` | hooks done | `CallingLlm` | `SendToHarness` |
| `*` | `StopRequested` | `Stopping` | `StopHarness` |
| `Stopping` | `HarnessExited` | `Stopped` | `EmitStateChange` |

### State Transition Table (Additional)
| From | Event | To | Action |
| --- | --- | --- | --- |
| `Idle` | `spawn_session` | `Starting` | internal |
| `Starting` | `HarnessExited(code=0)` | `Ready` | `EmitStateChange` |
| `CallingLlm` | `HarnessStream(error)` | `Error` | `EmitStateChange` |
| `ExecutingTools` | `ToolCompleted(Failed)` | `Error` | `EmitStateChange` |
| `PostToolsHook` | `HookCompleted(Failed)` | `Error` | `EmitStateChange` |
| `Error` | `RetryTimeout(target=Llm)` | `CallingLlm` | `SendToHarness` |
| `Error` | `RetryTimeout(target=Tool)` | `ExecutingTools` | `ExecuteTools` |
| `Error` | `RetryTimeout(target=Hook)` | `PostToolsHook` | `RunPostToolHooks` |

### State Diagram (ASCII)
```
Idle -> Starting -> Ready -> CallingLlm -> ProcessingResponse
                                   |             |
                                   |             +-> ExecutingTools -> PostToolsHook -> CallingLlm
                                   |             |
                                   |             +-> Ready (no tools)
                                   |
                                   +-> Error -> RetryTimeout -> CallingLlm

* StopRequested from any state -> Stopping -> HarnessExited -> Stopped
```

### Main Flow
```
User input
  -> Ready -> CallingLlm
  -> HarnessStream events (text/tool deltas)
  -> ProcessingResponse
  -> ExecutingTools (if tool calls exist)
  -> PostToolsHook (if any mutating tool)
  -> CallingLlm (tool results)
  -> Ready
```

### LLM Request Assembly
1. On `UserInput`, create new `streamId` and set `active_stream_id`.
2. Reset `pending_tool_calls` and increment `retries` to `1` for the new stream.
3. Emit `state_changed` with reason `user_input` and `streamId`.
4. `SendToHarness` action includes full conversation context + tool definitions.
5. While `CallingLlm`, stream deltas are forwarded via `agent:stream_event` without state changes.
6. On `HarnessStream(completed)`, transition to `ProcessingResponse` and freeze the stream buffer.

### Tool Output Message Mapping
- Each tool result is appended to the conversation as a tool response message:
  - `role=tool`, `tool_call_id=call_id`, `content=output`.
- Tool outputs are appended in the same order as tool calls.
- The next `SendToHarness` includes both the original assistant message and tool outputs.

### Example Event Sequence (Single Tool)
```
state_changed(Ready -> CallingLlm, streamId=turn_1)
stream_event(text_delta)
stream_event(tool_call_delta)
stream_event(completed)
state_changed(CallingLlm -> ProcessingResponse)
state_changed(ProcessingResponse -> ExecutingTools)
tool_lifecycle(started)
tool_lifecycle(completed)
state_changed(ExecutingTools -> PostToolsHook)
hook_lifecycle(started)
hook_lifecycle(completed)
state_changed(PostToolsHook -> CallingLlm)
stream_event(text_delta)
stream_event(completed)
state_changed(CallingLlm -> Ready)
```

### Session Lifecycle Flow
```
spawn_session
  -> Starting
  -> Ready
attach_session
  -> Ready (no transition if already active)
stop_session
  -> Stopping -> Stopped
```

### Harness Readiness
- If the harness provides a readiness signal, the session transitions to `Ready` only after it.
- If no readiness signal exists, transition to `Ready` immediately after spawn succeeds.

### Tool Batching Rules
- Tool calls are collected per LLM response and executed as a batch.
- The order of execution is stable by the order of `tool_call` appearance.
- Tools may run in parallel, but the tool batch completes only when all runs finish.
- Tool outputs are appended to the conversation in the same order as tool calls.
- If any tool in the batch is mutating, the batch is considered mutating.

### Post-Tool Hook Flow
1. Collect completed tool runs for the batch.
2. Filter hooks by `HookToolFilter`.
3. Execute hooks in configuration order.
4. Emit `hook_lifecycle` on start/complete; apply failure policy.
5. Proceed to `CallingLlm` with tool outputs.

### Tool + Hook Flow (ASCII)
```
CallingLlm
  -> ProcessingResponse
  -> ExecutingTools
       | ToolStarted
       | ToolCompleted
  -> PostToolsHook
       | HookStarted
       | HookCompleted
  -> CallingLlm
```

### Edge Cases
- Tool failure transitions to `Error` and emits `tool_lifecycle` with `Failed`.
- Hook failure follows policy: `fail-session` (default) or `warn-continue`.
- Stop during tool/hook marks run `Canceled` and transitions to `Stopping`.
- Harness exit transitions to `Error` or `Stopped` depending on exit code.

### Streaming Interaction
- `HarnessStream(text_delta)` updates the active stream buffer; no state transition.
- `HarnessStream(tool_call_delta)` appends to pending tool calls; no state transition.
- `HarnessStream(completed)` triggers `ProcessingResponse` and closes the active stream.
- `HarnessStream(error)` transitions to `Error` and emits `session_error`.

### Cancellation Flow
```
StopRequested
  -> Stopping
  -> StopHarness
  -> HarnessExited
  -> Stopped
```

### Error and Retry Flow
```
Tool failure
  -> Error
  -> session_error (retryable)
  -> RetryTimeout
  -> ExecutingTools
```

### Timeouts
- `llm_timeout_ms`: 120000 per request (guardrail, harness-specific).
- `tool_timeout_ms`: default 300000, configurable per tool.
- `hook_timeout_ms`: per `HookConfig.timeout_ms`.

### Invariants
- At most one active LLM request per session.
- Post-tool hooks only run after tool batch completion.
- `streamId` changes only when a new LLM request is initiated.
- `AgentStateKind::Ready` implies no pending tool calls.
- Every tool run has exactly one terminal status (`Succeeded`, `Failed`, `Canceled`).
- Hook runs never overlap; only one hook runs at a time per session.
- State events are emitted for every transition and include a reason.

### Event Emission Ordering
- Emit `tool_lifecycle` completion before `state_changed` to `PostToolsHook` or `CallingLlm`.
- Emit `hook_lifecycle` completion before `state_changed` to `CallingLlm`.
- Emit `session_error` before transitioning to `Error`.

### Retry/Backoff
- `max_llm_retries`: 2 with exponential backoff (250ms, 1s).
- `max_tool_retries`: 1 with fixed delay 500ms.
- `max_hook_retries`: 0 by default (hooks are best-effort).
- Retry is triggered by `RetryTimeout` events.

---

## 6. Error Handling
### Error Types
- `session_not_found`
- `state_transition_invalid`
- `harness_failed`
- `tool_execution_failed`
- `hook_execution_failed`
- `streaming_failed`

### Error Classification
| Error | Retryable | Source | Default Action |
| --- | --- | --- | --- |
| `session_not_found` | no | orchestrator | `session_error` + ignore event |
| `state_transition_invalid` | no | orchestrator | `session_error` + keep state |
| `harness_failed` | yes | harness | `Error` + allow retry |
| `tool_execution_failed` | yes | tool | `Error` + allow retry |
| `hook_execution_failed` | policy | hook | `Error` or `Ready` |
| `streaming_failed` | yes | harness | `Error` + allow retry |

### Recovery Strategy
- Transition to `Error` and emit `session_error`.
- Allow explicit retry actions for tools or harness.
- Stop request always wins and moves to `Stopping`.
- If retries are exhausted, transition to `Ready` with `last_error` retained.

---

## 7. Observability
### Logs
- State transitions with `session_id` and `from/to` values.
- Tool/hook start + completion with duration.
- Harness exit code and reason.

### Log Fields
- `session_id`, `stream_id`, `run_id`
- `tool_name`, `hook_name`
- `attempt`, `duration_ms`
- `error_code`

### Metrics
- `agent_state_transitions_total{from,to}`
- `tool_run_duration_ms`
- `hook_run_duration_ms`
- `session_errors_total{code}`

### Traces
- Session turn span includes LLM request + tool batch + hook pipeline.
- Tool and hook spans include `run_id` and `tool_name`/`hook_name`.

### Traces
- One trace per session turn with spans for LLM call, tool runs, and hooks.

---

## 8. Security and Privacy
### AuthZ/AuthN
- Local Tauri commands are trusted; remote daemon uses token auth.
- Tool/hook execution restricted to workspace root.

### Data Handling
- Redact tool arguments in logs unless debug enabled.
- Hook environment variables filtered via allowlist.
- Hook working directory defaults to the session workspace root.

---

## 9. Migration or Rollout
### Compatibility Notes
- Existing harness outputs must map into `AgentEvent::HarnessStream`.
- Post-tool hooks disabled by default until configured.

### Rollout Plan
1. Implement state machine types and transition logging.
2. Emit `agent:state_event` for existing sessions.
3. Introduce hook runner behind feature flag.
4. Expose hook status in UI.

---

## 10. Open Questions
- Should post-tool hooks be per tool, per harness, or per session?
- Do we need a `Paused` state for long-running tool chains?
- Should tool retries be controlled by harness or orchestrator?
