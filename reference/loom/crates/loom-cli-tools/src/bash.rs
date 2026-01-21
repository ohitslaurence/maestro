// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_common_core::{ToolContext, ToolError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::Tool;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_TIMEOUT_SECS: u64 = 300;
const MAX_OUTPUT_BYTES: usize = 256 * 1024; // 256KB per stream

#[derive(Debug, Deserialize)]
struct BashArgs {
	command: String,
	cwd: Option<PathBuf>,
	timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct BashResult {
	exit_code: Option<i32>,
	stdout: String,
	stderr: String,
	timed_out: bool,
	truncated: bool,
}

pub struct BashTool;

impl BashTool {
	pub fn new() -> Self {
		Self
	}

	fn validate_cwd(cwd: &Path, workspace_root: &Path) -> Result<PathBuf, ToolError> {
		let absolute_path = if cwd.is_absolute() {
			cwd.to_path_buf()
		} else {
			workspace_root.join(cwd)
		};

		let canonical = absolute_path.canonicalize().map_err(|_| {
			ToolError::InvalidArguments(format!(
				"working directory does not exist: {}",
				absolute_path.display()
			))
		})?;

		let workspace_canonical = workspace_root
			.canonicalize()
			.map_err(|_| ToolError::FileNotFound(workspace_root.to_path_buf()))?;

		if !canonical.starts_with(&workspace_canonical) {
			return Err(ToolError::PathOutsideWorkspace(canonical));
		}

		Ok(canonical)
	}

	fn truncate_output(output: &[u8], max_bytes: usize) -> (String, bool) {
		if output.len() <= max_bytes {
			(String::from_utf8_lossy(output).to_string(), false)
		} else {
			let truncated_bytes = &output[..max_bytes];
			let content = String::from_utf8_lossy(truncated_bytes).to_string();
			(content, true)
		}
	}
}

impl Default for BashTool {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl Tool for BashTool {
	fn name(&self) -> &str {
		"bash"
	}

	fn description(&self) -> &str {
		"Execute shell commands in the workspace directory"
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
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
		})
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		let args: BashArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;

		let timeout_secs = args
			.timeout_secs
			.unwrap_or(DEFAULT_TIMEOUT_SECS)
			.min(MAX_TIMEOUT_SECS);

		let working_dir = match &args.cwd {
			Some(cwd) => Self::validate_cwd(cwd, &ctx.workspace_root)?,
			None => ctx.workspace_root.clone(),
		};

		tracing::debug!(
			command = %args.command,
			cwd = %working_dir.display(),
			timeout_secs = timeout_secs,
			"executing bash command"
		);

		let mut cmd = Command::new("sh");
		cmd.arg("-c").arg(&args.command).current_dir(&working_dir);

		let result = timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

		let (exit_code, stdout, stderr, timed_out, truncated) = match result {
			Ok(Ok(output)) => {
				let (stdout, stdout_truncated) = Self::truncate_output(&output.stdout, MAX_OUTPUT_BYTES);
				let (stderr, stderr_truncated) = Self::truncate_output(&output.stderr, MAX_OUTPUT_BYTES);
				let truncated = stdout_truncated || stderr_truncated;

				tracing::debug!(
					exit_code = ?output.status.code(),
					stdout_len = output.stdout.len(),
					stderr_len = output.stderr.len(),
					truncated = truncated,
					"bash command completed"
				);

				(output.status.code(), stdout, stderr, false, truncated)
			}
			Ok(Err(e)) => {
				tracing::warn!(error = %e, "bash command failed to execute");
				return Err(ToolError::Io(e.to_string()));
			}
			Err(_) => {
				tracing::warn!(
					command = %args.command,
					timeout_secs = timeout_secs,
					"bash command timed out"
				);
				(None, String::new(), String::new(), true, false)
			}
		};

		let result = BashResult {
			exit_code,
			stdout,
			stderr,
			timed_out,
			truncated,
		};

		serde_json::to_value(result).map_err(|e| ToolError::Serialization(e.to_string()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use tempfile::TempDir;

	fn setup_workspace() -> TempDir {
		tempfile::tempdir().unwrap()
	}

	#[tokio::test]
	async fn bash_executes_simple_command() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"command": "echo hello"}), &ctx)
			.await
			.unwrap();

		assert_eq!(result["exit_code"], 0);
		assert_eq!(result["stdout"].as_str().unwrap().trim(), "hello");
		assert_eq!(result["timed_out"], false);
	}

	#[tokio::test]
	async fn bash_captures_stderr() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"command": "echo error >&2"}), &ctx)
			.await
			.unwrap();

		assert_eq!(result["exit_code"], 0);
		assert_eq!(result["stderr"].as_str().unwrap().trim(), "error");
	}

	#[tokio::test]
	async fn bash_returns_exit_code() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"command": "exit 42"}), &ctx)
			.await
			.unwrap();

		assert_eq!(result["exit_code"], 42);
	}

	#[tokio::test]
	async fn bash_respects_cwd() {
		let workspace = setup_workspace();
		let subdir = workspace.path().join("subdir");
		std::fs::create_dir(&subdir).unwrap();

		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"command": "pwd", "cwd": "subdir"}), &ctx)
			.await
			.unwrap();

		assert_eq!(result["exit_code"], 0);
		let stdout = result["stdout"].as_str().unwrap().trim();
		assert!(
			stdout.ends_with("subdir"),
			"Expected path ending with 'subdir', got: {stdout}"
		);
	}

	#[tokio::test]
	async fn bash_rejects_cwd_outside_workspace() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"command": "pwd", "cwd": "/tmp"}), &ctx)
			.await;

		assert!(matches!(
			result,
			Err(ToolError::PathOutsideWorkspace(_)) | Err(ToolError::InvalidArguments(_))
		));
	}

	#[tokio::test]
	async fn bash_rejects_cwd_path_traversal() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({"command": "pwd", "cwd": "../../../tmp"}),
				&ctx,
			)
			.await;

		assert!(matches!(
			result,
			Err(ToolError::PathOutsideWorkspace(_)) | Err(ToolError::InvalidArguments(_))
		));
	}

	#[tokio::test]
	async fn bash_times_out() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({"command": "sleep 10", "timeout_secs": 1}),
				&ctx,
			)
			.await
			.unwrap();

		assert_eq!(result["timed_out"], true);
		assert!(result["exit_code"].is_null());
	}

	#[tokio::test]
	async fn bash_truncates_large_output() {
		let workspace = setup_workspace();
		let tool = BashTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		// Generate output larger than MAX_OUTPUT_BYTES
		let result = tool
			.invoke(serde_json::json!({"command": "yes | head -c 300000"}), &ctx)
			.await
			.unwrap();

		assert_eq!(result["exit_code"], 0);
		assert_eq!(result["truncated"], true);
		let stdout_len = result["stdout"].as_str().unwrap().len();
		assert!(stdout_len <= MAX_OUTPUT_BYTES);
	}

	proptest! {
		/// Verifies that any printable ASCII command output is captured correctly.
		/// Note: We trim both sides for comparison since echo adds a newline and
		/// the shell may normalize whitespace.
		#[test]
		fn captures_command_output(content in "[a-zA-Z0-9]{1,100}") {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let workspace = setup_workspace();
				let tool = BashTool::new();
				let ctx = ToolContext::new(workspace.path().to_path_buf());

				let result = tool
					.invoke(serde_json::json!({"command": format!("echo '{}'", content)}), &ctx)
					.await
					.unwrap();

				prop_assert_eq!(result["exit_code"].as_i64().unwrap(), 0);
				prop_assert_eq!(result["stdout"].as_str().unwrap().trim(), content);
				Ok(())
			}).unwrap();
		}

		/// Verifies that exit codes are preserved.
		#[test]
		fn preserves_exit_code(code in 0i32..128) {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let workspace = setup_workspace();
				let tool = BashTool::new();
				let ctx = ToolContext::new(workspace.path().to_path_buf());

				let result = tool
					.invoke(serde_json::json!({"command": format!("exit {}", code)}), &ctx)
					.await
					.unwrap();

				prop_assert_eq!(result["exit_code"].as_i64().unwrap() as i32, code);
				Ok(())
			}).unwrap();
		}
	}
}
