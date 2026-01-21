<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Agent State Machine

## Overview

The Loom agent uses an explicit, event-driven state machine to manage conversation flow and tool
execution. This design provides:

- **Predictable behavior**: All state transitions are explicit and testable
- **Clear ownership**: Each state carries its required context (conversation, retries, etc.)
- **Graceful error recovery**: Built-in retry mechanisms with bounded attempts
- **Clean separation**: The state machine logic is decoupled from I/O operations

The state machine receives `AgentEvent`s and returns `AgentAction`s that the caller must execute.
This inversion of control allows the caller to manage async operations (LLM calls, tool execution)
while the state machine remains synchronous and pure.

**Source files:**

- [`crates/loom-core/src/state.rs`](../crates/loom-core/src/state.rs) - State and event type
  definitions
- [`crates/loom-core/src/agent.rs`](../crates/loom-core/src/agent.rs) - State machine implementation

---

## States

### `AgentState` Enum

| State                   | Description                                | Fields                                                                                          |
| ----------------------- | ------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `WaitingForUserInput`   | Idle state, ready to accept user messages  | `conversation: ConversationContext`                                                             |
| `CallingLlm`            | Making a request to the LLM provider       | `conversation: ConversationContext`, `retries: u32`                                             |
| `ProcessingLlmResponse` | Handling a completed LLM response          | `conversation: ConversationContext`, `response: LlmResponse`                                    |
| `ExecutingTools`        | Running one or more tool calls in parallel | `conversation: ConversationContext`, `executions: Vec<ToolExecutionStatus>`                     |
| `PostToolsHook`         | Running post-tool hooks (e.g., auto-commit)| `conversation: ConversationContext`, `pending_llm_request: LlmRequest`, `completed_tools: Vec<CompletedToolInfo>` |
| `Error`                 | Recoverable error with retry capability    | `conversation: ConversationContext`, `error: AgentError`, `retries: u32`, `origin: ErrorOrigin` |
| `ShuttingDown`          | Graceful shutdown in progress              | (none)                                                                                          |

### State Details

#### WaitingForUserInput

The initial and terminal state for user turns. The agent is idle and awaits user input. The
conversation context preserves all prior messages.

#### CallingLlm

Active LLM request in flight. The `retries` counter tracks how many retry attempts have been made
for the current request.

#### ProcessingLlmResponse

Transient state for examining an LLM response. Immediately transitions to either `ExecutingTools`
(if tool calls present) or `WaitingForUserInput` (if text-only response).

#### ExecutingTools

Tracks multiple concurrent tool executions via `Vec<ToolExecutionStatus>`. Each execution progresses
through `Pending` → `Running` → `Completed`.

#### PostToolsHook

Runs post-tool hooks after tool execution completes. This state enables features like auto-commit
that need to run after file-modifying tools (e.g., `edit_file`, `bash`). The state carries:
- `pending_llm_request`: The next LLM request to send after hooks complete
- `completed_tools`: Information about which tools completed, used to decide which hooks to run

#### Error

Holds the failed state with retry information. The `origin` field (`Llm`, `Tool`, or `Io`)
determines retry strategy.

#### ShuttingDown

Terminal state. No transitions out; the agent should be dropped after reaching this state.

---

## Events

### `AgentEvent` Enum

| Event                   | Description                         | Payload                                            |
| ----------------------- | ----------------------------------- | -------------------------------------------------- |
| `UserInput`             | User submitted a message            | `Message`                                          |
| `LlmEvent`              | Event from the LLM provider         | `LlmEvent` (see sub-variants)                      |
| `ToolProgress`          | Progress update from a running tool | `ToolProgressEvent`                                |
| `ToolCompleted`         | A tool execution finished           | `call_id: String`, `outcome: ToolExecutionOutcome` |
| `PostToolsHookCompleted`| Post-tool hooks have finished       | `action_taken: bool`                               |
| `RetryTimeoutFired`     | Retry backoff timer expired         | (none)                                             |
| `ShutdownRequested`     | Graceful shutdown requested         | (none)                                             |

### LlmEvent Sub-variants

| Sub-variant     | Description                                 | Fields                                                               |
| --------------- | ------------------------------------------- | -------------------------------------------------------------------- |
| `TextDelta`     | Incremental text content from the assistant | `content: String`                                                    |
| `ToolCallDelta` | Incremental tool call data during streaming | `call_id: String`, `tool_name: String`, `arguments_fragment: String` |
| `Completed`     | The completion finished successfully        | `LlmResponse`                                                        |
| `Error`         | An error occurred during streaming          | `LlmError`                                                           |

### ToolExecutionOutcome

| Variant   | Description                | Fields                                         |
| --------- | -------------------------- | ---------------------------------------------- |
| `Success` | Tool executed successfully | `call_id: String`, `output: serde_json::Value` |
| `Error`   | Tool execution failed      | `call_id: String`, `error: ToolError`          |

---

## State Transitions

### Transition Table

| Current State           | Event                              | New State               | Action                |
| ----------------------- | ---------------------------------- | ----------------------- | --------------------- |
| `WaitingForUserInput`   | `UserInput(msg)`                   | `CallingLlm`            | `SendLlmRequest`      |
| `CallingLlm`            | `LlmEvent::TextDelta`              | `CallingLlm`            | `DisplayMessage`      |
| `CallingLlm`            | `LlmEvent::ToolCallDelta`          | `CallingLlm`            | `WaitForInput`        |
| `CallingLlm`            | `LlmEvent::Completed`              | `ProcessingLlmResponse` | (internal processing) |
| `CallingLlm`            | `LlmEvent::Error` (retries < max)  | `Error`                 | `WaitForInput`        |
| `CallingLlm`            | `LlmEvent::Error` (retries >= max) | `WaitingForUserInput`   | `DisplayError`        |
| `ProcessingLlmResponse` | (has tool calls)                   | `ExecutingTools`        | `ExecuteTools`        |
| `ProcessingLlmResponse` | (no tool calls)                    | `WaitingForUserInput`   | `WaitForInput`        |
| `ExecutingTools`        | `ToolCompleted` (some pending)     | `ExecutingTools`        | `WaitForInput`        |
| `ExecutingTools`        | `ToolCompleted` (all done, mutating)| `PostToolsHook`        | `RunPostToolsHook`    |
| `ExecutingTools`        | `ToolCompleted` (all done, no mutation)| `CallingLlm`        | `SendLlmRequest`      |
| `PostToolsHook`         | `PostToolsHookCompleted`           | `CallingLlm`            | `SendLlmRequest`      |
| `Error` (origin=Llm)    | `RetryTimeoutFired`                | `CallingLlm`            | `SendLlmRequest`      |
| _any state_             | `ShutdownRequested`                | `ShuttingDown`          | `Shutdown`            |
| _invalid transition_    | _any_                              | (unchanged)             | `WaitForInput`        |

---

## Actions

### `AgentAction` Enum

Actions are returned to the caller indicating what I/O operation to perform:

| Action            | Description                             | Payload                          |
| ----------------- | --------------------------------------- | -------------------------------- |
| `SendLlmRequest`  | Send a request to the LLM provider      | `LlmRequest`                     |
| `ExecuteTools`    | Execute the specified tool calls        | `Vec<ToolCall>`                  |
| `RunPostToolsHook`| Run post-tool hooks (e.g., auto-commit) | `completed_tools: Vec<CompletedToolInfo>` |
| `WaitForInput`    | Wait for the next event (idle)          | (none)                           |
| `DisplayMessage`  | Show a message to the user              | `String`                         |
| `DisplayError`    | Show an error to the user               | `String`                         |
| `Shutdown`        | Terminate the agent                     | (none)                           |

---

## Design Decisions

### Why Explicit State Machine vs Implicit

1. **Testability**: Every state and transition can be unit tested in isolation. Property-based tests
   verify invariants like "shutdown always succeeds from any state".

2. **Debuggability**: State transitions are logged with `tracing::info!`, making it easy to trace
   agent behavior in production.

3. **No Hidden State**: All context is carried explicitly in state variants. There are no ambient
   flags or mutable fields that could get out of sync.

4. **Exhaustive Matching**: Rust's `match` ensures all state/event combinations are handled. New
   events or states trigger compiler errors until addressed.

### Why Events Are Processed Synchronously

The `handle_event` method is synchronous and returns immediately:

```rust
pub fn handle_event(&mut self, event: AgentEvent) -> AgentResult<AgentAction>
```

**Rationale:**

1. **Separation of Concerns**: The state machine decides _what_ to do; the caller decides _how_ to
   do it (async, parallel, etc.).

2. **Backpressure**: The caller controls the pace of event delivery. No internal queues or
   background tasks.

3. **Determinism**: Given the same sequence of events, the state machine produces the same sequence
   of actions—essential for testing and replay.

4. **Flexibility**: The caller can implement different execution strategies (single-threaded, tokio,
   async-std) without changing the state machine.

### How Conversation Context Is Threaded

Each state variant that needs conversation history carries its own `ConversationContext`:

```rust
pub enum AgentState {
	WaitingForUserInput {
		conversation: ConversationContext,
	},
	CallingLlm {
		conversation: ConversationContext,
		retries: u32,
	},
	// ...
}
```

During transitions, the context is cloned and updated:

- `UserInput` → message appended to conversation
- `LlmEvent::Completed` → assistant message appended
- `ToolCompleted` (all done) → tool result messages appended

This ensures the conversation history is always consistent with the current state.

---

## Mermaid State Diagram

```mermaid
stateDiagram-v2
    [*] --> WaitingForUserInput : Agent::new()
    
    WaitingForUserInput --> CallingLlm : UserInput
    
    CallingLlm --> CallingLlm : TextDelta / ToolCallDelta
    CallingLlm --> ProcessingLlmResponse : Completed
    CallingLlm --> Error : Error (retries < max)
    CallingLlm --> WaitingForUserInput : Error (retries >= max)
    
    ProcessingLlmResponse --> ExecutingTools : has tool calls
    ProcessingLlmResponse --> WaitingForUserInput : no tool calls
    
    ExecutingTools --> ExecutingTools : ToolCompleted (some pending)
    ExecutingTools --> PostToolsHook : ToolCompleted (all done, mutating)
    ExecutingTools --> CallingLlm : ToolCompleted (all done, no mutation)
    
    PostToolsHook --> CallingLlm : PostToolsHookCompleted
    
    Error --> CallingLlm : RetryTimeoutFired
    
    WaitingForUserInput --> ShuttingDown : ShutdownRequested
    CallingLlm --> ShuttingDown : ShutdownRequested
    ProcessingLlmResponse --> ShuttingDown : ShutdownRequested
    ExecutingTools --> ShuttingDown : ShutdownRequested
    PostToolsHook --> ShuttingDown : ShutdownRequested
    Error --> ShuttingDown : ShutdownRequested
    
    ShuttingDown --> [*]
```

---

## Extension Guide

### Adding a New State

1. **Add variant to `AgentState`** in `state.rs`:
   ```rust
   pub enum AgentState {
   	// existing variants...
   	NewState {
   		conversation: ConversationContext,
   		custom_field: CustomType,
   	},
   }
   ```

2. **Update `name()` method** for logging:
   ```rust
   Self::NewState { .. } => "NewState",
   ```

3. **Update `conversation()` accessors** in `agent.rs` to handle the new variant.

4. **Add transition handlers** in `handle_event()`:
   ```rust
   (AgentState::NewState { conversation, .. }, AgentEvent::SomeEvent) => {
       // transition logic
   }
   ```

5. **Add tests** verifying transitions to/from the new state.

### Adding a New Event

1. **Add variant to `AgentEvent`** in `state.rs`:
   ```rust
   pub enum AgentEvent {
   	// existing variants...
   	NewEvent { payload: PayloadType },
   }
   ```

2. **Handle the event** in each relevant state within `handle_event()`.

3. **Update the catch-all pattern** if needed—invalid transitions log a warning and return
   `WaitForInput`.

4. **Add property tests** verifying the event is handled correctly from all reachable states.

### Adding a New Action

1. **Add variant to `AgentAction`** in `agent.rs`:
   ```rust
   pub enum AgentAction {
   	// existing variants...
   	NewAction(PayloadType),
   }
   ```

2. **Return the action** from appropriate transition handlers.

3. **Update callers** to handle the new action variant.

### Testing Guidelines

- **Unit tests**: Verify specific state transitions in isolation
- **Property tests**: Verify invariants hold across all configurations:
  - "Agent always starts in WaitingForUserInput"
  - "Retry count never exceeds max_retries"
  - "Shutdown always succeeds from any state"
- **Integration tests**: Verify end-to-end flows through multiple transitions
