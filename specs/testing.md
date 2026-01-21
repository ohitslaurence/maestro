<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Testing Strategy

## Overview

Loom employs a testing philosophy centered on **property-based testing** to ensure correctness
across the full input space, complemented by targeted unit and integration tests for specific
scenarios and edge cases.

### Core Principles

1. **Property-Based Testing First**: Prefer property tests that verify invariants over example-based
   tests that check specific cases
2. **Document Every Test**: Each test MUST include documentation explaining why it's important and
   what invariant it verifies (per AGENTS.md)
3. **Structured Logging**: All production code uses structured logging (tracing) for observability
4. **Fail Fast, Fail Clearly**: Tests should produce clear error messages that identify the root
   cause

## Test Categories

### Unit Tests (Per-Module)

Standard `#[test]` functions for synchronous, isolated logic testing. Located in `#[cfg(test)]`
modules within each source file.

**Example locations:**

- [`crates/loom-core/src/agent.rs`](file:///home/ghuntley/loom/crates/loom-core/src/agent.rs) -
  State machine transitions
- [`crates/loom-core/src/llm.rs`](file:///home/ghuntley/loom/crates/loom-core/src/llm.rs) -
  Serialization tests
- [`crates/loom-llm-anthropic/src/stream.rs`](file:///home/ghuntley/loom/crates/loom-llm-anthropic/src/stream.rs) -
  SSE parsing

### Property-Based Tests (proptest)

Generative tests that verify invariants hold across randomly generated inputs. Use the `proptest!`
macro from the `proptest` crate.

**Example locations:**

- [`crates/loom-tools/src/edit_file.rs`](file:///home/ghuntley/loom/crates/loom-tools/src/edit_file.rs) -
  Edit operation properties
- [`crates/loom-tools/src/registry.rs`](file:///home/ghuntley/loom/crates/loom-tools/src/registry.rs) -
  Registry invariants
- [`crates/loom-core/src/agent.rs`](file:///home/ghuntley/loom/crates/loom-core/src/agent.rs) -
  State machine properties

### Integration Tests (Tool Execution)

Async tests using `#[tokio::test]` that exercise complete workflows including I/O operations.

**Example locations:**

- [`crates/loom-tools/src/edit_file.rs`](file:///home/ghuntley/loom/crates/loom-tools/src/edit_file.rs) -
  File editing workflows
- [`crates/loom-tools/src/read_file.rs`](file:///home/ghuntley/loom/crates/loom-tools/src/read_file.rs) -
  File reading workflows
- [`crates/loom-http/src/retry.rs`](file:///home/ghuntley/loom/crates/loom-http/src/retry.rs) -
  Retry behavior

## Property-Based Testing with proptest

### Why Property Tests Over Example-Based

| Example-Based Tests             | Property-Based Tests               |
| ------------------------------- | ---------------------------------- |
| Test specific inputs            | Test input _space_                 |
| May miss edge cases             | Explores edge cases automatically  |
| Documents behavior for one case | Documents invariants for all cases |
| Brittle to refactoring          | Robust to implementation changes   |

Property tests are preferred because they:

1. **Discover edge cases** you didn't think of (unicode, empty strings, boundary values)
2. **Verify invariants** that must hold for all valid inputs
3. **Shrink failures** to minimal reproducible examples
4. **Scale testing** to thousands of cases with one test

### Generators and Strategies

proptest provides strategies for generating test data:

```rust
use proptest::prelude::*;

proptest! {
		#[test]
		fn example_property(
				// String matching regex pattern
				name in "[a-zA-Z][a-zA-Z0-9_]{0,30}",
				// Optional value
				max_tokens in proptest::option::of(1u32..10000),
				// Range of values
				temperature in 0.0f32..2.0,
				// Collection with size bounds
				items in prop::collection::vec("[a-z]{1,10}", 0..10),
				// Hash set (unique values)
				unique_names in prop::collection::hash_set("[a-z]{1,10}", 0..5),
		) {
				// Test body using generated values
				prop_assert!(name.len() <= 31);
		}
}
```

**Common strategies used in loom:**

- `"[a-zA-Z0-9]{n,m}"` - Regex-based string generation
- `proptest::option::of(strategy)` - Optional values
- `prop::collection::vec(strategy, range)` - Vector generation
- `prop::collection::hash_set(strategy, range)` - Unique value sets
- `n..m` - Numeric ranges

### Preconditions with prop_assume!

Use `prop_assume!` to filter invalid test cases:

```rust
proptest! {
		#[test]
		fn deletion_test(
				prefix in "[a-z]{5,20}",
				target in "[A-Z]{5,15}",
				suffix in "[a-z]{5,20}"
		) {
				// Skip cases where prefix/suffix contain target
				prop_assume!(!prefix.contains(&target));
				prop_assume!(!suffix.contains(&target));

				// Test proceeds only with valid combinations
		}
}
```

## Key Test Areas

### 1. State Machine Transitions (agent.rs)

The agent state machine tests verify correct transitions between states:

- `WaitingForUserInput` → `CallingLlm` → `ProcessingLlmResponse`
- `ProcessingLlmResponse` → `ExecutingTools` → `CallingLlm` (with results)
- Error recovery: `Error` → `CallingLlm` (via retry)
- Shutdown: Any state → `ShuttingDown`

**Key property tests:**

- `agent_initial_state_invariant` - Agent always starts in WaitingForUserInput
- `user_input_always_triggers_llm_call` - UserInput deterministically triggers CallingLlm
- `retry_count_bounded_by_max_retries` - Retry count never exceeds configured maximum
- `shutdown_always_succeeds` - ShutdownRequested always works from any state

### 2. Tool Execution Correctness (edit_file)

Property tests for file editing verify:

- **Reversibility**: `edit(old→new)` followed by `edit(new→old)` restores original
- **Byte count accuracy**: Reported bytes match actual file sizes
- **Idempotency**: Same edit applied twice produces same result
- **Unicode safety**: Multi-byte UTF-8 sequences handled correctly
- **replace_all completeness**: All occurrences replaced when flag set
- **Surrounding text preservation**: Non-targeted regions unchanged

### 3. Serialization Roundtrips (LlmRequest, Usage)

Tests verify JSON serialization/deserialization consistency:

```rust
proptest! {
		#[test]
		fn serialization_roundtrip_preserves_data(
				model in "[a-z]{1,20}",
				max_tokens in proptest::option::of(1u32..10000),
		) {
				let request = LlmRequest { model, max_tokens, .. };
				let json = serde_json::to_string(&request)?;
				let deserialized: LlmRequest = serde_json::from_str(&json)?;

				prop_assert_eq!(request.model, deserialized.model);
				prop_assert_eq!(request.max_tokens, deserialized.max_tokens);
		}
}
```

### 4. SSE Parsing (stream tests)

Tests for Server-Sent Events parsing in LLM streaming:

- `test_parse_text_delta_event` - Text content deltas parsed correctly
- `test_parse_tool_use_start` - Tool call initiation tracked
- `test_parse_message_stop` - Stream completion produces LlmResponse

### 5. Retry Behavior (http-retry)

Tests verify retry logic invariants:

- `test_non_retryable_error_fails_immediately` - No retries for 4xx errors
- `test_retryable_error_retries_up_to_max_attempts` - Correct retry count
- Exponential backoff calculations
- Jitter application

## Test Documentation Requirements

**Every test MUST document:**

1. **Purpose**: Why this test is important
2. **Invariant**: What property/behavior it verifies
3. **Context**: When this matters (failure scenarios, edge cases)

### Required Format

```rust
/// **Test Name**: Brief description of what's being tested
///
/// **Why this is important**: Explain the significance and potential
/// failure modes this test catches. Include real-world scenarios.
///
/// **Invariant**: Formal statement of the property being verified.
/// Use mathematical notation if helpful (e.g., "∀ inputs: P(x) → Q(x)")
#[test]
fn test_example() {
	// ...
}
```

### Example from codebase

```rust
/// **Property test: Retry count never exceeds max_retries**
/// 
/// This property verifies the retry bound invariant:
/// - After max_retries errors, agent must stop retrying
/// - Agent should transition to WaitingForUserInput at the limit
/// 
/// Prevents infinite retry loops that could exhaust resources.
#[test]
fn retry_count_bounded_by_max_retries(max_retries in 1u32..5) {
    // ...
    prop_assert!(
        matches!(agent.state(), AgentState::WaitingForUserInput { .. }),
        "must return to WaitingForUserInput after max retries"
    );
}
```

## Mock Implementations

### MockLlmClient for Agent Tests

Used in agent tests to simulate LLM behavior without network calls:

```rust
struct MockLlmClient;

#[async_trait]
impl LlmClient for MockLlmClient {
	async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
		Ok(LlmResponse {
			message: Message::assistant("mock response"),
			tool_calls: vec![],
			usage: Some(Usage::default()),
			finish_reason: Some("stop".to_string()),
		})
	}

	async fn complete_streaming(&self, _request: LlmRequest) -> Result<LlmStream, LlmError> {
		// Return stream that immediately completes
	}
}
```

### MockTool for Registry Tests

Used to test tool registration and lookup:

```rust
struct MockTool {
	name: String,
}

#[async_trait]
impl Tool for MockTool {
	fn name(&self) -> &str {
		&self.name
	}

	fn description(&self) -> &str {
		"A mock tool for testing"
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {}
		})
	}

	async fn invoke(
		&self,
		_args: serde_json::Value,
		_ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		Ok(serde_json::json!({"result": "ok"}))
	}
}
```

### MockError for Retry Tests

Used to test retry behavior with controllable retryability:

```rust
#[derive(Debug)]
struct MockError {
	retryable: bool,
}

impl RetryableError for MockError {
	fn is_retryable(&self) -> bool {
		self.retryable
	}
}
```

## Async Testing

### tokio::test Attribute

For async tests, use the `#[tokio::test]` attribute:

```rust
#[tokio::test]
async fn test_file_editing() {
	let workspace = setup_workspace();
	let tool = EditFileTool::new();
	let ctx = ToolContext::new(workspace.path().to_path_buf());

	let result = tool
		.invoke(
			serde_json::json!({"path": "test.txt", "edits": [...]}),
			&ctx,
		)
		.await
		.unwrap();

	assert_eq!(result["edits_applied"], 1);
}
```

### Runtime Creation for proptest

proptest doesn't natively support async. Create a runtime inside the test:

```rust
proptest! {
		#[test]
		fn async_property_test(input in "[a-z]{1,20}") {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						// Async test code here
						let result = async_operation(&input).await;
						prop_assert!(result.is_ok());
						Ok(())  // Return Result for prop_assert! macro
				}).unwrap();
		}
}
```

**Important**: The async block must return `Result<(), TestCaseError>` for `prop_assert!` to work
correctly.

## Running Tests

### All Tests

```bash
cargo test --all
```

### Specific Crate

```bash
cargo test -p loom-core
cargo test -p loom-tools
cargo test -p loom-http
cargo test -p loom-llm-anthropic
cargo test -p loom-llm-openai
```

### Test Filtering

```bash
# Run tests matching pattern
cargo test state_machine

# Run specific test
cargo test test_user_input_transitions_to_calling_llm

# Run tests in specific module
cargo test agent::tests

# Run with output displayed
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored
```

### Proptest Configuration

Control proptest behavior with environment variables:

```bash
# Run more cases for thorough testing
PROPTEST_CASES=1000 cargo test

# Set seed for reproducibility
PROPTEST_SEED=12345 cargo test

# Verbose output for debugging failures
PROPTEST_VERBOSE=1 cargo test
```

## Test Helpers

### Workspace Setup

Use `tempfile::TempDir` for isolated filesystem tests:

```rust
fn setup_workspace() -> tempfile::TempDir {
	tempfile::tempdir().expect("failed to create temp dir")
}
```

### Test Agent Creation

Helper functions for creating agents with default or custom configs:

```rust
fn create_test_agent() -> Agent {
	create_test_agent_with_config(AgentConfig::default())
}

fn create_test_agent_with_config(config: AgentConfig) -> Agent {
	let llm = Arc::new(MockLlmClient);
	let tools = vec![];
	Agent::new(config, llm, tools)
}
```

### Response Builders

Helpers for constructing LLM responses:

```rust
fn create_simple_response(content: &str) -> LlmResponse {
	LlmResponse {
		message: Message::assistant(content),
		tool_calls: vec![],
		usage: Some(Usage::default()),
		finish_reason: Some("stop".to_string()),
	}
}

fn create_response_with_tools(tool_calls: Vec<ToolCall>) -> LlmResponse {
	LlmResponse {
		message: Message::assistant(""),
		tool_calls,
		usage: Some(Usage::default()),
		finish_reason: Some("tool_use".to_string()),
	}
}
```

## Test Dependencies

Add to `Cargo.toml` under `[dev-dependencies]`:

```toml
[dev-dependencies]
proptest = "1.4"
tokio-test = "0.4"
tempfile = "3"
```
