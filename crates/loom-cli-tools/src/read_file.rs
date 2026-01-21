// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_common_core::{ToolContext, ToolError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::Tool;

const DEFAULT_MAX_BYTES: u64 = 1024 * 1024; // 1MB

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
	path: PathBuf,
	max_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ReadFileResult {
	path: PathBuf,
	contents: String,
	truncated: bool,
}

pub struct ReadFileTool;

impl ReadFileTool {
	pub fn new() -> Self {
		Self
	}

	fn validate_path(path: &PathBuf, workspace_root: &Path) -> Result<PathBuf, ToolError> {
		let absolute_path = if path.is_absolute() {
			path.clone()
		} else {
			workspace_root.join(path)
		};

		let canonical = absolute_path
			.canonicalize()
			.map_err(|_| ToolError::FileNotFound(absolute_path.clone()))?;

		let workspace_canonical = workspace_root
			.canonicalize()
			.map_err(|_| ToolError::FileNotFound(workspace_root.to_path_buf()))?;

		if !canonical.starts_with(&workspace_canonical) {
			return Err(ToolError::PathOutsideWorkspace(canonical));
		}

		Ok(canonical)
	}
}

impl Default for ReadFileTool {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl Tool for ReadFileTool {
	fn name(&self) -> &str {
		"read_file"
	}

	fn description(&self) -> &str {
		"Read the contents of a file from the workspace"
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {
						"path": {
								"type": "string",
								"description": "Path to the file to read (absolute or relative to workspace)"
						},
						"max_bytes": {
								"type": "integer",
								"description": "Maximum number of bytes to read (default: 1MB)"
						}
				},
				"required": ["path"]
		})
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		let args: ReadFileArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;
		let max_bytes = args.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);

		tracing::debug!(
				path = %args.path.display(),
				max_bytes = max_bytes,
				"reading file"
		);

		let canonical_path = Self::validate_path(&args.path, &ctx.workspace_root)?;

		let metadata = tokio::fs::metadata(&canonical_path).await?;
		let file_size = metadata.len();
		let truncated = file_size > max_bytes;

		let contents = if truncated {
			tracing::info!(
					path = %canonical_path.display(),
					file_size = file_size,
					max_bytes = max_bytes,
					"file truncated due to size limit"
			);
			let bytes = tokio::fs::read(&canonical_path).await?;
			String::from_utf8_lossy(&bytes[..max_bytes as usize]).to_string()
		} else {
			tokio::fs::read_to_string(&canonical_path).await?
		};

		tracing::debug!(
				path = %canonical_path.display(),
				bytes_read = contents.len(),
				truncated = truncated,
				"file read complete"
		);

		let result = ReadFileResult {
			path: canonical_path,
			contents,
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
	async fn read_file_returns_contents() {
		let workspace = setup_workspace();
		let file_path = workspace.path().join("test.txt");
		std::fs::write(&file_path, "hello world").unwrap();

		let tool = ReadFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"path": "test.txt"}), &ctx)
			.await
			.unwrap();

		assert_eq!(result["contents"], "hello world");
		assert_eq!(result["truncated"], false);
	}

	#[tokio::test]
	async fn read_file_truncates_large_files() {
		let workspace = setup_workspace();
		let file_path = workspace.path().join("large.txt");
		let content = "x".repeat(1000);
		std::fs::write(&file_path, &content).unwrap();

		let tool = ReadFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({"path": "large.txt", "max_bytes": 100}),
				&ctx,
			)
			.await
			.unwrap();

		assert_eq!(result["contents"].as_str().unwrap().len(), 100);
		assert_eq!(result["truncated"], true);
	}

	#[tokio::test]
	async fn read_file_rejects_path_outside_workspace() {
		let workspace = setup_workspace();
		let tool = ReadFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"path": "/etc/passwd"}), &ctx)
			.await;

		assert!(matches!(
			result,
			Err(ToolError::PathOutsideWorkspace(_)) | Err(ToolError::FileNotFound(_))
		));
	}

	#[tokio::test]
	async fn read_file_rejects_path_traversal() {
		let workspace = setup_workspace();
		let tool = ReadFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"path": "../../../etc/passwd"}), &ctx)
			.await;

		assert!(matches!(
			result,
			Err(ToolError::PathOutsideWorkspace(_)) | Err(ToolError::FileNotFound(_))
		));
	}

	proptest! {
			/// Verifies that any file content written to the workspace can be read back exactly.
			/// This property ensures read operations are lossless for content within size limits.
			#[test]
			fn roundtrip_file_content(content in "[a-zA-Z0-9 \n]{0,1000}") {
					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &content).unwrap();

							let tool = ReadFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let result = tool
									.invoke(serde_json::json!({"path": "test.txt"}), &ctx)
									.await
									.unwrap();

							prop_assert_eq!(result["contents"].as_str().unwrap(), content);
							Ok(())
					}).unwrap();
			}
	}
}
