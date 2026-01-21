<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Error Handling

## Overview

Loom uses a structured, type-safe approach to error handling based on the
[thiserror](https://docs.rs/thiserror) crate. The design philosophy prioritizes:

- **Explicit error types** over generic `anyhow::Error` in library code
- **Automatic error conversion** via `From` impls for ergonomic propagation
- **Recoverable vs fatal** distinction through error variants and retry mechanisms
- **Structured logging** with tracing for observability

All error types are defined in [`crates/loom-core/src/error.rs`](../crates/loom-core/src/error.rs).

## Error Type Hierarchy

### AgentError (Top-Level)

The root error type for all agent operations:

```rust
#[derive(Error, Debug)]
pub enum AgentError {
	#[error("LLM error: {0}")]
	Llm(#[from] LlmError),

	#[error("Tool error: {0}")]
	Tool(#[from] ToolError),

	#[error("Invalid state: {0}")]
	InvalidState(String),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Operation timed out: {0}")]
	Timeout(String),

	#[error("Internal error: {0}")]
	Internal(String),
}
```

| Variant        | Purpose                                                     |
| -------------- | ----------------------------------------------------------- |
| `Llm`          | Wraps LLM-specific errors (API failures, rate limits, etc.) |
| `Tool`         | Wraps tool execution errors                                 |
| `InvalidState` | State machine received an invalid event for current state   |
| `Io`           | File system and I/O errors                                  |
| `Timeout`      | Operation exceeded configured timeout                       |
| `Internal`     | Unexpected internal errors (bugs)                           |

### LlmError

Errors from LLM API interactions:

```rust
#[derive(Clone, Error, Debug)]
pub enum LlmError {
	#[error("HTTP error: {0}")]
	Http(String),

	#[error("API error: {0}")]
	Api(String),

	#[error("Request timed out")]
	Timeout,

	#[error("Invalid response: {0}")]
	InvalidResponse(String),

	#[error("Rate limited: retry after {retry_after_secs:?} seconds")]
	RateLimited { retry_after_secs: Option<u64> },
}
```

| Variant           | Transient? | Description                                   |
| ----------------- | ---------- | --------------------------------------------- |
| `Http`            | Yes        | Network/transport failures                    |
| `Api`             | Maybe      | API returned an error (check message)         |
| `Timeout`         | Yes        | Request exceeded `llm_timeout`                |
| `InvalidResponse` | No         | Malformed or unparseable response             |
| `RateLimited`     | Yes        | Rate limit hit; includes optional retry delay |

### ToolError

Errors from tool execution:

```rust
#[derive(Clone, Error, Debug)]
pub enum ToolError {
	#[error("Tool not found: {0}")]
	NotFound(String),

	#[error("Invalid arguments: {0}")]
	InvalidArguments(String),

	#[error("IO error: {0}")]
	Io(String),

	#[error("Tool execution timed out")]
	Timeout,

	#[error("Internal error: {0}")]
	Internal(String),

	#[error("Target not found: {0}")]
	TargetNotFound(String),

	#[error("Path outside workspace: {0}")]
	PathOutsideWorkspace(PathBuf),

	#[error("File not found: {0}")]
	FileNotFound(PathBuf),

	#[error("Serialization error: {0}")]
	Serialization(String),
}
```

| Variant                | Description                              |
| ---------------------- | ---------------------------------------- |
| `NotFound`             | LLM requested a tool that doesn't exist  |
| `InvalidArguments`     | Tool arguments failed validation         |
| `Io`                   | File/network I/O during tool execution   |
| `Timeout`              | Tool exceeded `tool_timeout`             |
| `Internal`             | Bug in tool implementation               |
| `TargetNotFound`       | Search/grep target doesn't exist         |
| `PathOutsideWorkspace` | Security: path escapes workspace sandbox |
| `FileNotFound`         | Requested file doesn't exist             |
| `Serialization`        | JSON/serde failures                      |

## Error Propagation

### From Implementations

Errors convert automatically via `#[from]` attribute:

```rust
// LlmError -> AgentError
impl From<LlmError> for AgentError { ... }

// ToolError -> AgentError  
impl From<ToolError> for AgentError { ... }

// std::io::Error -> AgentError
impl From<std::io::Error> for AgentError { ... }

// std::io::Error -> ToolError (converts to string to satisfy Clone)
impl From<std::io::Error> for ToolError {
    fn from(err: std::io::Error) -> Self {
        ToolError::Io(err.to_string())
    }
}
```

### AgentResult Type Alias

The standard result type for agent operations:

```rust
pub type AgentResult<T> = Result<T, AgentError>;
```

### Propagation Pattern

Errors bubble up through layers using `?` operator:

```
Tool Implementation
        ↓ ToolError
Tool Registry  
        ↓ AgentError::Tool
Agent State Machine
        ↓ AgentAction::DisplayError
CLI/UI Layer
```

## Recovery Strategies

### Automatic Retry for LLM Errors

The agent state machine implements automatic retry for transient LLM errors:

1. **LLM error occurs** in `CallingLlm` state
2. **Retry count incremented** and checked against `max_retries`
3. **If retries available**: transition to `Error` state with `ErrorOrigin::Llm`
4. **`RetryTimeoutFired` event** triggers retry (external timer schedules this)
5. **Retry transitions** back to `CallingLlm` state

```rust
// From crates/loom-core/src/agent.rs
(
    AgentState::CallingLlm { conversation, retries },
    AgentEvent::LlmEvent(LlmEvent::Error(e)),
) => {
    let new_retries = *retries + 1;
    warn!(
        error = %e,
        retries = new_retries,
        max_retries = self.config.max_retries,
        "LLM error"
    );
    if new_retries < self.config.max_retries {
        // Transition to Error state, will retry on RetryTimeoutFired
        self.state = AgentState::Error { ... };
        AgentAction::WaitForInput
    } else {
        // Max retries reached, give up
        self.state = AgentState::WaitingForUserInput { ... };
        AgentAction::DisplayError(AgentError::Llm(e).to_string())
    }
}
```

### Configuration

From [`crates/loom-core/src/config.rs`](../crates/loom-core/src/config.rs):

```rust
pub struct AgentConfig {
	pub max_retries: u32,       // Default: 3
	pub tool_timeout: Duration, // Default: 30s
	pub llm_timeout: Duration,  /* Default: 120s
	                             * ... */
}
```

### Error State

The `Error` state in the state machine:

```rust
Error {
    conversation: ConversationContext,
    error: AgentError,
    retries: u32,
    origin: ErrorOrigin,  // Llm, Tool, or Io
}
```

The `ErrorOrigin` enum determines recovery behavior:

- `ErrorOrigin::Llm` → eligible for automatic retry
- `ErrorOrigin::Tool` → reported to LLM for self-correction
- `ErrorOrigin::Io` → typically fatal

### RetryTimeoutFired Event

External systems (runtime/scheduler) fire `AgentEvent::RetryTimeoutFired` after a backoff delay:

```rust
(
    AgentState::Error {
        conversation,
        retries,
        origin: ErrorOrigin::Llm,
        ..
    },
    AgentEvent::RetryTimeoutFired,
) => {
    // Reconstruct LLM request and transition back to CallingLlm
    self.state = AgentState::CallingLlm {
        conversation: conv,
        retries: new_retries,
    };
    AgentAction::CallLlm(request)
}
```

## User-Facing Errors

### DisplayError Action

When errors must be shown to users, the agent emits `AgentAction::DisplayError`:

```rust
pub enum AgentAction {
	// ...
	DisplayError(String),
	// ...
}
```

The error is converted to a string using `Display` trait:

```rust
AgentAction::DisplayError(AgentError::Llm(e).to_string())
```

### CLI Presentation

The CLI layer receives `DisplayError` actions and formats them appropriately:

- Colored output (typically red)
- Error context preserved in message
- Recovery suggestions when applicable

## Design Decisions

### Why thiserror Over anyhow in Libraries

| thiserror                   | anyhow                        |
| --------------------------- | ----------------------------- |
| Typed error variants        | Erased error type             |
| Pattern matching on errors  | String-based error inspection |
| Compile-time exhaustiveness | Runtime error checking        |
| Clear API contracts         | Flexible but opaque           |

**Library code (loom-core)** uses `thiserror` because:

1. Callers need to match on specific error types for recovery
2. Error variants form part of the public API contract
3. State machine logic depends on error origin

### Why anyhow in Binary Crate

**Binary crates (CLI)** may use `anyhow` because:

1. Top-level code often just displays errors
2. No downstream callers need to match on variants
3. Simpler error context chaining with `.context()`

### Clone Constraints on Error Types

`LlmError` and `ToolError` derive `Clone`:

```rust
#[derive(Clone, Error, Debug)]
pub enum LlmError { ... }

#[derive(Clone, Error, Debug)]
pub enum ToolError { ... }
```

This is required because:

1. Errors are stored in `AgentState::Error` which may be cloned
2. State machine transitions need to copy conversation context
3. `std::io::Error` doesn't implement `Clone`, so `ToolError::Io` stores `String`

## Logging Errors

Errors are logged with structured tracing fields:

```rust
use tracing::{debug, info, warn};

warn!(
		error = %e,                      // Display format
		retries = new_retries,           // Retry count
		max_retries = self.config.max_retries,
		"LLM error"
);

info!(
	from = old_state_name,
	to = "Error",
	"state transition (will retry)"
);
```

### Best Practices

1. **Use structured fields** over string interpolation:
   ```rust
   // Good
   warn!(error = %e, tool_name = %name, "tool failed");

   // Avoid
   warn!("tool {} failed: {}", name, e);
   ```

2. **Include context** relevant to debugging:
   - Current state
   - Retry counts
   - Operation identifiers

3. **Choose appropriate levels**:
   - `error!` - Unrecoverable failures
   - `warn!` - Recoverable errors (retries, tool failures)
   - `info!` - State transitions
   - `debug!` - Detailed operation tracing

4. **Use Display (`%`) for errors**:
   ```rust
   warn!(error = %e, "operation failed");
   ```
