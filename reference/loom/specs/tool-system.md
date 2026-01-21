<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Tool System

## Overview

The tool system enables the Loom agent to interact with the filesystem and perform actions on behalf
of the user. Tools are the primary mechanism by which the LLM can affect the external world—reading
files, listing directories, and making edits to code.

Tools bridge the gap between the LLM's text-based reasoning and concrete filesystem operations,
while enforcing security boundaries to prevent unauthorized access outside the designated workspace.

### Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   LLM Provider  │────▶│   ToolRegistry   │────▶│   Tool Impl     │
│  (tool_use)     │     │   (dispatch)     │     │  (execute)      │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                                │                         │
                                ▼                         ▼
                        ┌──────────────────┐     ┌─────────────────┐
                        │  ToolDefinition  │     │   ToolContext   │
                        │  (for LLM)       │     │  (workspace)    │
                        └──────────────────┘     └─────────────────┘
```

## Core Types

### Tool Trait

The `Tool` trait
([crates/loom-tools/src/registry.rs#L5-L26](file:///home/ghuntley/loom/crates/loom-tools/src/registry.rs#L5-L26))
defines the interface all tools must implement:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
	fn name(&self) -> &str;
	fn description(&self) -> &str;
	fn input_schema(&self) -> serde_json::Value;
	fn to_definition(&self) -> ToolDefinition;
	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError>;
}
```

| Method            | Purpose                                          |
| ----------------- | ------------------------------------------------ |
| `name()`          | Unique identifier used by LLM to invoke the tool |
| `description()`   | Human-readable description shown to the LLM      |
| `input_schema()`  | JSON Schema defining valid input arguments       |
| `to_definition()` | Produces a `ToolDefinition` for LLM registration |
| `invoke()`        | Async execution with validated args and context  |

### ToolDefinition

The `ToolDefinition` struct
([crates/loom-core/src/tool.rs#L6-L10](file:///home/ghuntley/loom/crates/loom-core/src/tool.rs#L6-L10))
is the serializable representation sent to the LLM:

```rust
pub struct ToolDefinition {
	pub name: String,
	pub description: String,
	pub input_schema: serde_json::Value,
}
```

This struct is designed for direct serialization into LLM API requests (Anthropic, OpenAI).

### ToolContext

The `ToolContext` struct
([crates/loom-core/src/tool.rs#L32-L35](file:///home/ghuntley/loom/crates/loom-core/src/tool.rs#L32-L35))
provides execution context:

```rust
pub struct ToolContext {
	pub workspace_root: PathBuf,
}
```

The workspace root is the security boundary—all file operations must occur within this directory.

### ToolRegistry

The `ToolRegistry`
([crates/loom-tools/src/registry.rs#L28-L52](file:///home/ghuntley/loom/crates/loom-tools/src/registry.rs#L28-L52))
manages tool registration and dispatch:

```rust
pub struct ToolRegistry {
	tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
	fn new() -> Self;
	fn register(&mut self, tool: Box<dyn Tool>);
	fn get(&self, name: &str) -> Option<&dyn Tool>;
	fn definitions(&self) -> Vec<ToolDefinition>;
}
```

## Tool Execution States

Tool execution follows a discriminated union pattern
([crates/loom-core/src/state.rs#L38-L58](file:///home/ghuntley/loom/crates/loom-core/src/state.rs#L38-L58)):

```rust
pub enum ToolExecutionStatus {
	Pending {
		call_id: String,
		tool_name: String,
		requested_at: Instant,
	},
	Running {
		call_id: String,
		tool_name: String,
		started_at: Instant,
		last_update_at: Instant,
		progress: Option<ToolProgress>,
	},
	Completed {
		call_id: String,
		tool_name: String,
		started_at: Instant,
		completed_at: Instant,
		outcome: ToolExecutionOutcome,
	},
}
```

### State Transitions

```
┌─────────┐
│ Pending │  (LLM requests tool use)
└────┬────┘
     │
     ▼
┌─────────┐
│ Running │  (execution in progress)
└────┬────┘
     │ progress updates (optional)
     ▼
┌───────────┐
│ Completed │  (success or error)
└───────────┘
```

### ToolExecutionOutcome

```rust
pub enum ToolExecutionOutcome {
	Success {
		call_id: String,
		output: serde_json::Value,
	},
	Error {
		call_id: String,
		error: ToolError,
	},
}
```

### ToolProgress

For long-running operations, tools can report progress:

```rust
pub struct ToolProgress {
	pub fraction: Option<f32>,        // 0.0 to 1.0
	pub message: Option<String>,      // Human-readable status
	pub units_processed: Option<u64>, // Files, bytes, etc.
}
```

## Built-in Tools

### read_file

**Location:**
[crates/loom-tools/src/read_file.rs](file:///home/ghuntley/loom/crates/loom-tools/src/read_file.rs)

Reads file contents with optional truncation for large files.

**Input Schema:**

```json
{
	"type": "object",
	"properties": {
		"path": {
			"type": "string",
			"description": "Path to the file (absolute or relative to workspace)"
		},
		"max_bytes": {
			"type": "integer",
			"description": "Maximum bytes to read (default: 1MB)"
		}
	},
	"required": ["path"]
}
```

**Output:**

```json
{
	"path": "/absolute/path/to/file",
	"contents": "file contents...",
	"truncated": false
}
```

**Behavior:**

- Default limit: 1MB (`DEFAULT_MAX_BYTES = 1024 * 1024`)
- Files exceeding limit are truncated with `truncated: true`
- Uses `String::from_utf8_lossy` for binary-safe reading of truncated content

### list_files

**Location:**
[crates/loom-tools/src/list_files.rs](file:///home/ghuntley/loom/crates/loom-tools/src/list_files.rs)

Lists directory contents with file/directory differentiation.

**Input Schema:**

```json
{
	"type": "object",
	"properties": {
		"root": {
			"type": "string",
			"description": "Root directory (default: workspace root)"
		},
		"max_results": {
			"type": "integer",
			"description": "Maximum entries to return (default: 1000)"
		}
	}
}
```

**Output:**

```json
{
	"entries": [
		{ "path": "/workspace/src", "is_dir": true },
		{ "path": "/workspace/Cargo.toml", "is_dir": false }
	]
}
```

### edit_file

**Location:**
[crates/loom-tools/src/edit_file.rs](file:///home/ghuntley/loom/crates/loom-tools/src/edit_file.rs)

Performs snippet-based text replacement with support for file creation.

**Input Schema:**

```json
{
	"type": "object",
	"properties": {
		"path": {
			"type": "string",
			"description": "Path to the file to edit"
		},
		"edits": {
			"type": "array",
			"items": {
				"type": "object",
				"properties": {
					"old_str": {
						"type": "string",
						"description": "Text to find (empty for new file/append)"
					},
					"new_str": {
						"type": "string",
						"description": "Replacement text"
					},
					"replace_all": {
						"type": "boolean",
						"description": "Replace all occurrences (default: false)"
					}
				},
				"required": ["old_str", "new_str"]
			}
		}
	},
	"required": ["path", "edits"]
}
```

**Output:**

```json
{
	"path": "/absolute/path/to/file",
	"edits_applied": 1,
	"original_bytes": 100,
	"new_bytes": 105
}
```

**Special Behaviors:**

- Empty `old_str`: Creates new file or appends content
- Empty `new_str`: Deletes the matched text
- `replace_all: false` (default): Replaces only first occurrence
- Creates parent directories automatically

### oracle

**Location:**
[crates/loom-tools/src/oracle.rs](file:///home/ghuntley/loom/crates/loom-tools/src/oracle.rs)

Queries OpenAI for additional reasoning or advice via the Loom server proxy. This tool enables the
primary LLM (Claude) to consult a secondary LLM (OpenAI) for complex reasoning tasks, code review,
or specialized knowledge.

**Input Schema:**

```json
{
	"type": "object",
	"properties": {
		"query": {
			"type": "string",
			"description": "The question or task for OpenAI to reason about."
		},
		"model": {
			"type": "string",
			"description": "Model override. Defaults to LOOM_SERVER_ORACLE_MODEL env or 'gpt-4o'."
		},
		"max_tokens": {
			"type": "integer",
			"minimum": 16,
			"maximum": 4096,
			"description": "Maximum tokens in the response (default: 512)."
		},
		"temperature": {
			"type": "number",
			"minimum": 0.0,
			"maximum": 2.0,
			"description": "Sampling temperature (default: 0.2)."
		},
		"system_prompt": {
			"type": "string",
			"description": "Extra guidance for the oracle to customize its behavior."
		}
	},
	"required": ["query"]
}
```

**Output:**

```json
{
	"message": {
		"role": "assistant",
		"content": "OpenAI's response text..."
	},
	"tool_calls": [],
	"usage": {
		"input_tokens": 100,
		"output_tokens": 250
	},
	"finish_reason": "stop"
}
```

**Behavior:**

- Uses `loom-http` for resilience against transient failures (429, 503, timeouts)
- Sends requests to `/proxy/openai/complete` endpoint on the Loom server
- Default system prompt instructs OpenAI to act as a sub-agent providing concise, technically
  accurate advice
- Parameters are clamped to valid ranges (`max_tokens`: 16-4096, `temperature`: 0.0-2.0)

**Configuration:**

- `LOOM_SERVER_URL`: Server URL for proxy requests (default: `http://127.0.0.1:8080`)
- `LOOM_SERVER_ORACLE_MODEL`: Default model when not specified in args (default: `gpt-4o`)

### bash

**Location:**
[crates/loom-tools/src/bash.rs](file:///home/ghuntley/loom/crates/loom-tools/src/bash.rs)

Executes shell commands in the workspace directory.

**Input Schema:**

```json
{
	"type": "object",
	"properties": {
		"command": {
			"type": "string",
			"description": "The shell command to execute"
		},
		"cwd": {
			"type": "string",
			"description": "Working directory relative to workspace (default: workspace root)"
		},
		"timeout_secs": {
			"type": "integer",
			"minimum": 1,
			"maximum": 300,
			"description": "Timeout in seconds (default: 60, max: 300)"
		}
	},
	"required": ["command"]
}
```

**Output:**

```json
{
	"exit_code": 0,
	"stdout": "command output...",
	"stderr": "",
	"timed_out": false,
	"truncated": false
}
```

**Behavior:**

- Executes commands using the system shell (`sh -c` on Unix)
- Working directory defaults to `workspace_root`, can be overridden with `cwd`
- Commands are killed after timeout (default 60s, max 300s)
- Output is truncated to 256KB per stream (stdout/stderr)
- Returns exit code, stdout, stderr, timeout status, and truncation status
- If command is killed by timeout, `exit_code` may be `null` and `timed_out` is `true`

**Security Considerations:**

- Commands run with the same permissions as the Loom process
- The `cwd` parameter is validated to be within workspace boundaries
- No shell escaping is performed—the command is passed directly to `sh -c`
- Users should be aware that arbitrary commands can be executed
- Consider future sandboxing options for untrusted environments

### web_search

**Location:**
[crates/loom-tools/src/web_search.rs](file:///home/ghuntley/loom/crates/loom-tools/src/web_search.rs)

Performs web searches via the Loom server using Google Custom Search Engine (CSE).

**Input Schema:**

```json
{
	"type": "object",
	"properties": {
		"query": {
			"type": "string",
			"description": "Search query string in natural language."
		},
		"max_results": {
			"type": "integer",
			"minimum": 1,
			"maximum": 10,
			"description": "Maximum number of search results to return (default: 5, max: 10)."
		}
	},
	"required": ["query"]
}
```

**Output:**

```json
{
	"results": [
		{
			"title": "Result title",
			"url": "https://example.com/page",
			"snippet": "Brief description of the page content..."
		}
	]
}
```

**Behavior:**

- Uses `loom-http` for resilience against transient failures
- Sends requests to `/proxy/cse` endpoint on the Loom server
- Requires Google CSE to be configured on the server

## Security Considerations

### Path Validation

All tools implement path validation to enforce workspace boundaries:

```rust
fn validate_path(path: &PathBuf, workspace_root: &PathBuf) -> Result<PathBuf, ToolError> {
	let absolute_path = if path.is_absolute() {
		path.clone()
	} else {
		workspace_root.join(path)
	};

	let canonical = absolute_path.canonicalize()?;
	let workspace_canonical = workspace_root.canonicalize()?;

	if !canonical.starts_with(&workspace_canonical) {
		return Err(ToolError::PathOutsideWorkspace(canonical));
	}

	Ok(canonical)
}
```

### Path Traversal Prevention

The system prevents path traversal attacks (`../../../etc/passwd`) through:

1. **Canonicalization**: Resolves symlinks and `..` components to absolute paths
2. **Prefix checking**: Validates the resolved path starts with workspace root
3. **Error on violation**: Returns `ToolError::PathOutsideWorkspace`

### ToolError Types

Security-related errors
([crates/loom-core/src/error.rs#L48-L76](file:///home/ghuntley/loom/crates/loom-core/src/error.rs#L48-L76)):

```rust
pub enum ToolError {
	NotFound(String),
	InvalidArguments(String),
	Io(String),
	Timeout,
	Internal(String),
	TargetNotFound(String),
	PathOutsideWorkspace(PathBuf), // Security boundary violation
	FileNotFound(PathBuf),
	Serialization(String),
}
```

## Design Decisions

### Why Async Tool Invocation

Tools use `async fn invoke()` because:

1. **Non-blocking I/O**: File operations use `tokio::fs` to avoid blocking the runtime
2. **Progress reporting**: Long operations can yield and report progress
3. **Concurrent execution**: Multiple independent tool calls can execute in parallel
4. **Cancellation**: Async enables graceful cancellation via `tokio::select!`

### Why JSON Schema for Input Validation

JSON Schema provides:

1. **LLM compatibility**: Claude and GPT expect JSON Schema format for tool definitions
2. **Self-documenting**: Schema includes descriptions shown to the LLM
3. **Type safety at boundary**: Validates LLM-generated arguments before execution
4. **Extensibility**: Easy to add new parameters with backwards compatibility

### Why Snippet-Based Editing vs Line-Based

Snippet-based editing (`old_str` → `new_str`) was chosen over line-number-based editing because:

1. **LLM accuracy**: LLMs struggle with exact line numbers but excel at text matching
2. **Merge conflict resistance**: Works correctly even if file changes between read and edit
3. **Context preservation**: The matched text serves as verification the edit is applied correctly
4. **Atomic edits**: Either the exact match is found and replaced, or the edit fails
5. **Unicode safety**: String operations naturally respect UTF-8 boundaries

The trade-off is that `old_str` must be unique (when `replace_all: false`), but this is actually a
feature—it prevents ambiguous edits.

## Adding New Tools

### Step-by-Step Guide

1. **Create the tool file** in `crates/loom-tools/src/`:

```rust
// crates/loom-tools/src/my_tool.rs
use async_trait::async_trait;
use loom_core::{ToolContext, ToolError};
use serde::{Deserialize, Serialize};

use crate::Tool;

#[derive(Debug, Deserialize)]
struct MyToolArgs {
	// Define input parameters
	required_param: String,
	optional_param: Option<i32>,
}

#[derive(Debug, Serialize)]
struct MyToolResult {
	// Define output structure
	status: String,
}

pub struct MyTool;

impl MyTool {
	pub fn new() -> Self {
		Self
	}
}

impl Default for MyTool {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl Tool for MyTool {
	fn name(&self) -> &str {
		"my_tool"
	}

	fn description(&self) -> &str {
		"Description shown to the LLM explaining what this tool does"
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {
						"required_param": {
								"type": "string",
								"description": "What this parameter does"
						},
						"optional_param": {
								"type": "integer",
								"description": "Optional configuration"
						}
				},
				"required": ["required_param"]
		})
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		let args: MyToolArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;

		tracing::debug!(
				param = %args.required_param,
				"executing my_tool"
		);

		// Implement tool logic here
		// Use ctx.workspace_root for path operations

		let result = MyToolResult {
			status: "success".to_string(),
		};

		serde_json::to_value(result).map_err(|e| ToolError::Serialization(e.to_string()))
	}
}
```

2. **Export from lib.rs**:

```rust
// crates/loom-tools/src/lib.rs
pub mod my_tool;
pub use my_tool::MyTool;
```

3. **Register with the agent**:

```rust
let mut registry = ToolRegistry::new();
registry.register(Box::new(MyTool::new()));
```

4. **Add property-based tests**:

```rust
#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
			/// Document why this property is important and what it verifies
			#[test]
			fn my_property(input in "...") {
					// Test invariants
			}
	}
}
```

### Checklist

- [ ] Implement all `Tool` trait methods
- [ ] Use structured logging with `tracing`
- [ ] Validate paths against workspace boundary (if applicable)
- [ ] Return appropriate `ToolError` variants
- [ ] Add property-based tests documenting invariants
- [ ] Export from `lib.rs`
- [ ] Register in the tool registry
