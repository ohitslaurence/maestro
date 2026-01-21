// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_common_core::{ToolContext, ToolError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::Tool;

const DEFAULT_MAX_RESULTS: usize = 1000;

#[derive(Debug, Deserialize)]
struct ListFilesArgs {
	root: Option<PathBuf>,
	max_results: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FileEntry {
	path: PathBuf,
	is_dir: bool,
}

#[derive(Debug, Serialize)]
struct ListFilesResult {
	entries: Vec<FileEntry>,
}

pub struct ListFilesTool;

impl ListFilesTool {
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

impl Default for ListFilesTool {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl Tool for ListFilesTool {
	fn name(&self) -> &str {
		"list_files"
	}

	fn description(&self) -> &str {
		"List files and directories in the workspace"
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {
						"root": {
								"type": "string",
								"description": "Root directory to list (default: workspace root)"
						},
						"max_results": {
								"type": "integer",
								"description": "Maximum number of entries to return (default: 1000)"
						}
				}
		})
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		let args: ListFilesArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;
		let max_results = args.max_results.unwrap_or(DEFAULT_MAX_RESULTS);

		let root_path = match &args.root {
			Some(root) => Self::validate_path(root, &ctx.workspace_root)?,
			None => ctx.workspace_root.canonicalize()?,
		};

		tracing::debug!(
				root = %root_path.display(),
				max_results = max_results,
				"listing files"
		);

		let mut entries = Vec::new();
		let mut read_dir = tokio::fs::read_dir(&root_path).await?;

		while let Some(entry) = read_dir.next_entry().await? {
			if entries.len() >= max_results {
				tracing::info!(
						root = %root_path.display(),
						max_results = max_results,
						"result limit reached"
				);
				break;
			}

			let path = entry.path();
			let metadata = entry.metadata().await?;

			entries.push(FileEntry {
				path,
				is_dir: metadata.is_dir(),
			});
		}

		tracing::debug!(
				root = %root_path.display(),
				entries_count = entries.len(),
				"listing complete"
		);

		let result = ListFilesResult { entries };
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
	async fn list_files_returns_entries() {
		let workspace = setup_workspace();
		std::fs::write(workspace.path().join("file1.txt"), "").unwrap();
		std::fs::write(workspace.path().join("file2.txt"), "").unwrap();
		std::fs::create_dir(workspace.path().join("subdir")).unwrap();

		let tool = ListFilesTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool.invoke(serde_json::json!({}), &ctx).await.unwrap();

		let entries = result["entries"].as_array().unwrap();
		assert_eq!(entries.len(), 3);
	}

	#[tokio::test]
	async fn list_files_respects_max_results() {
		let workspace = setup_workspace();
		for i in 0..10 {
			std::fs::write(workspace.path().join(format!("file{i}.txt")), "").unwrap();
		}

		let tool = ListFilesTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(serde_json::json!({"max_results": 5}), &ctx)
			.await
			.unwrap();

		let entries = result["entries"].as_array().unwrap();
		assert_eq!(entries.len(), 5);
	}

	#[tokio::test]
	async fn list_files_identifies_directories() {
		let workspace = setup_workspace();
		std::fs::create_dir(workspace.path().join("subdir")).unwrap();

		let tool = ListFilesTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool.invoke(serde_json::json!({}), &ctx).await.unwrap();

		let entries = result["entries"].as_array().unwrap();
		let subdir_entry = entries
			.iter()
			.find(|e| e["path"].as_str().unwrap().ends_with("subdir"))
			.unwrap();
		assert_eq!(subdir_entry["is_dir"], true);
	}

	#[tokio::test]
	async fn list_files_rejects_path_outside_workspace() {
		let workspace = setup_workspace();
		let tool = ListFilesTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool.invoke(serde_json::json!({"root": "/etc"}), &ctx).await;

		assert!(matches!(result, Err(ToolError::PathOutsideWorkspace(_))));
	}

	proptest! {
			/// Verifies that creating N files results in listing exactly N entries.
			/// This property ensures no files are lost or duplicated during listing.
			#[test]
			fn list_returns_correct_count(file_count in 0usize..20) {
					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							for i in 0..file_count {
									std::fs::write(workspace.path().join(format!("file{i}.txt")), "").unwrap();
							}

							let tool = ListFilesTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let result = tool.invoke(serde_json::json!({}), &ctx).await.unwrap();

							let entries = result["entries"].as_array().unwrap();
							prop_assert_eq!(entries.len(), file_count);
							Ok(())
					}).unwrap();
			}
	}
}
