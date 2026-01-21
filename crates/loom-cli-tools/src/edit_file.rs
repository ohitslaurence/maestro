// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_common_core::{ToolContext, ToolError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::Tool;

#[derive(Debug, Deserialize)]
struct EditFileArgs {
	path: PathBuf,
	edits: Vec<SnippetEdit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnippetEdit {
	pub old_str: String,
	pub new_str: String,
	pub replace_all: Option<bool>,
}

#[derive(Debug, Serialize)]
struct EditFileResult {
	path: PathBuf,
	edits_applied: usize,
	original_bytes: usize,
	new_bytes: usize,
}

pub struct EditFileTool;

impl EditFileTool {
	pub fn new() -> Self {
		Self
	}

	fn validate_path(path: &PathBuf, workspace_root: &Path) -> Result<PathBuf, ToolError> {
		let absolute_path = if path.is_absolute() {
			path.clone()
		} else {
			workspace_root.join(path)
		};

		if let Ok(canonical) = absolute_path.canonicalize() {
			let workspace_canonical = workspace_root
				.canonicalize()
				.map_err(|_| ToolError::FileNotFound(workspace_root.to_path_buf()))?;

			if !canonical.starts_with(&workspace_canonical) {
				return Err(ToolError::PathOutsideWorkspace(canonical));
			}
			Ok(canonical)
		} else {
			let workspace_canonical = workspace_root
				.canonicalize()
				.map_err(|_| ToolError::FileNotFound(workspace_root.to_path_buf()))?;

			let normalized = if absolute_path.is_absolute() {
				absolute_path.clone()
			} else {
				workspace_canonical.join(&absolute_path)
			};

			if let Some(parent) = normalized.parent() {
				if let Ok(parent_canonical) = parent.canonicalize() {
					if !parent_canonical.starts_with(&workspace_canonical) {
						return Err(ToolError::PathOutsideWorkspace(normalized));
					}
				}
			}

			Ok(normalized)
		}
	}
}

impl Default for EditFileTool {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl Tool for EditFileTool {
	fn name(&self) -> &str {
		"edit_file"
	}

	fn description(&self) -> &str {
		"Edit a file by replacing text snippets"
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
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
														"description": "Text to find and replace (empty string for new file)"
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
								},
								"description": "List of edits to apply"
						}
				},
				"required": ["path", "edits"]
		})
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		let args: EditFileArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;
		let file_path = Self::validate_path(&args.path, &ctx.workspace_root)?;

		tracing::debug!(
				path = %file_path.display(),
				edit_count = args.edits.len(),
				"applying edits"
		);

		let mut content = if file_path.exists() {
			tokio::fs::read_to_string(&file_path).await?
		} else {
			String::new()
		};

		let original_bytes = content.len();
		let mut edits_applied = 0;

		for (index, edit) in args.edits.iter().enumerate() {
			if edit.old_str.is_empty() {
				tracing::debug!(
						path = %file_path.display(),
						edit_index = index,
						new_str_len = edit.new_str.len(),
						"creating new file or appending content"
				);
				content.push_str(&edit.new_str);
				edits_applied += 1;
			} else if !content.contains(&edit.old_str) {
				tracing::warn!(
						path = %file_path.display(),
						edit_index = index,
						target = %edit.old_str,
						"target string not found"
				);
				return Err(ToolError::TargetNotFound(format!(
					"target '{}' not found in {}",
					edit.old_str,
					file_path.display()
				)));
			} else {
				let replace_all = edit.replace_all.unwrap_or(false);
				if replace_all {
					let count = content.matches(&edit.old_str).count();
					content = content.replace(&edit.old_str, &edit.new_str);
					edits_applied += count;
					tracing::debug!(
							path = %file_path.display(),
							edit_index = index,
							occurrences = count,
							"replaced all occurrences"
					);
				} else {
					content = content.replacen(&edit.old_str, &edit.new_str, 1);
					edits_applied += 1;
					tracing::debug!(
							path = %file_path.display(),
							edit_index = index,
							"replaced first occurrence"
					);
				}
			}
		}

		if let Some(parent) = file_path.parent() {
			tokio::fs::create_dir_all(parent).await?;
		}
		tokio::fs::write(&file_path, &content).await?;

		let new_bytes = content.len();

		tracing::info!(
				path = %file_path.display(),
				edits_applied = edits_applied,
				original_bytes = original_bytes,
				new_bytes = new_bytes,
				"edits complete"
		);

		let result = EditFileResult {
			path: file_path,
			edits_applied,
			original_bytes,
			new_bytes,
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
	async fn edit_file_replaces_text() {
		let workspace = setup_workspace();
		let file_path = workspace.path().join("test.txt");
		std::fs::write(&file_path, "hello world").unwrap();

		let tool = EditFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({
						"path": "test.txt",
						"edits": [{"old_str": "world", "new_str": "rust"}]
				}),
				&ctx,
			)
			.await
			.unwrap();

		assert_eq!(result["edits_applied"], 1);

		let content = std::fs::read_to_string(&file_path).unwrap();
		assert_eq!(content, "hello rust");
	}

	#[tokio::test]
	async fn edit_file_creates_new_file() {
		let workspace = setup_workspace();
		let tool = EditFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({
						"path": "new_file.txt",
						"edits": [{"old_str": "", "new_str": "new content"}]
				}),
				&ctx,
			)
			.await
			.unwrap();

		assert_eq!(result["edits_applied"], 1);

		let content = std::fs::read_to_string(workspace.path().join("new_file.txt")).unwrap();
		assert_eq!(content, "new content");
	}

	#[tokio::test]
	async fn edit_file_replaces_all_occurrences() {
		let workspace = setup_workspace();
		let file_path = workspace.path().join("test.txt");
		std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

		let tool = EditFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({
						"path": "test.txt",
						"edits": [{"old_str": "foo", "new_str": "qux", "replace_all": true}]
				}),
				&ctx,
			)
			.await
			.unwrap();

		assert_eq!(result["edits_applied"], 3);

		let content = std::fs::read_to_string(&file_path).unwrap();
		assert_eq!(content, "qux bar qux baz qux");
	}

	#[tokio::test]
	async fn edit_file_returns_error_for_missing_target() {
		let workspace = setup_workspace();
		let file_path = workspace.path().join("test.txt");
		std::fs::write(&file_path, "hello world").unwrap();

		let tool = EditFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({
						"path": "test.txt",
						"edits": [{"old_str": "nonexistent", "new_str": "replacement"}]
				}),
				&ctx,
			)
			.await;

		assert!(matches!(result, Err(ToolError::TargetNotFound(_))));
	}

	#[tokio::test]
	async fn edit_file_rejects_path_outside_workspace() {
		let workspace = setup_workspace();
		let tool = EditFileTool::new();
		let ctx = ToolContext::new(workspace.path().to_path_buf());

		let result = tool
			.invoke(
				serde_json::json!({
						"path": "/etc/passwd",
						"edits": [{"old_str": "", "new_str": "malicious"}]
				}),
				&ctx,
			)
			.await;

		assert!(matches!(result, Err(ToolError::PathOutsideWorkspace(_))));
	}

	proptest! {
			/// Verifies that applying an edit with unique marker strings is reversible.
			/// This property ensures edit operations are predictable when using distinct strings.
			#[test]
			fn edit_is_reversible(
					prefix in "[a-z]{5,20}",
					suffix in "[a-z]{5,20}"
			) {
					let target = "UNIQUE_TARGET_MARKER";
					let replacement = "UNIQUE_REPLACEMENT_MARKER";
					let original = format!("{prefix}{target}{suffix}");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": target, "new_str": replacement}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": replacement, "new_str": target}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							let content = std::fs::read_to_string(&file_path).unwrap();
							prop_assert_eq!(content, original);
							Ok(())
					}).unwrap();
			}

			/// Verifies that the bytes count in the result accurately reflects file changes.
			/// This property ensures the metadata returned is consistent with actual file state.
			#[test]
			fn edit_reports_correct_byte_counts(
					content in "[a-zA-Z0-9]{10,50}",
					replacement in "[a-zA-Z0-9]{1,20}"
			) {
					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &content).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let target = &content[0..5];
							let result = tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": target, "new_str": replacement}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							prop_assert_eq!(result["original_bytes"].as_u64().unwrap() as usize, content.len());

							let actual_content = std::fs::read_to_string(&file_path).unwrap();
							prop_assert_eq!(result["new_bytes"].as_u64().unwrap() as usize, actual_content.len());
							Ok(())
					}).unwrap();
			}

			/// **Idempotency Test**: Verifies that applying the same edit twice with replace_all=false
			/// either succeeds with the same result (if old_str still exists) or fails with TargetNotFound
			/// (if the target was replaced and no longer exists).
			///
			/// **Why this is important**: Idempotency is a critical property for edit operations in
			/// distributed or retry-prone systems. When an edit operation is retried (e.g., due to
			/// network issues or user re-execution), the system should behave predictably.
			///
			/// **Invariant**: After first edit succeeds, second edit either:
			/// 1. Succeeds if old_str != new_str (target still exists after first replacement of first occurrence)
			/// 2. Fails with TargetNotFound if old_str was completely replaced
			#[test]
			fn idempotency_with_replace_all_false(
					prefix in "[a-z]{5,20}",
					old_str in "[A-Z]{5,10}",
					new_str in "[0-9]{5,10}",
					suffix in "[a-z]{5,20}"
			) {
					let original = format!("{prefix}{old_str}{suffix}");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let first_result = tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &old_str, "new_str": &new_str, "replace_all": false}]
									}),
									&ctx,
							)
							.await;

							prop_assert!(first_result.is_ok());
							let content_after_first = std::fs::read_to_string(&file_path).unwrap();

							let second_result = tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &old_str, "new_str": &new_str, "replace_all": false}]
									}),
									&ctx,
							)
							.await;

							if content_after_first.contains(&old_str) {
									prop_assert!(second_result.is_ok());
							} else {
									prop_assert!(matches!(second_result, Err(ToolError::TargetNotFound(_))));
							}
							Ok(())
					}).unwrap();
			}

			/// **Single Replacement Test**: Verifies that with replace_all=false, only the first
			/// occurrence of old_str is replaced, leaving subsequent occurrences intact.
			///
			/// **Why this is important**: Precise control over replacements is essential for surgical
			/// code edits. When editing code, users often want to replace only a specific instance
			/// (e.g., the first function definition) without affecting other identical strings.
			///
			/// **Invariant**: Given content with N occurrences of old_str (N >= 2), after edit with
			/// replace_all=false, the content should have exactly N-1 occurrences of old_str and
			/// exactly 1 occurrence of new_str at the position of the first original occurrence.
			#[test]
			fn single_replacement_only_replaces_first_occurrence(
					prefix in "[a-z]{3,10}",
					target in "[A-Z]{3,8}",
					middle in "[a-z]{3,10}",
					replacement in "[0-9]{3,8}",
					suffix in "[a-z]{3,10}"
			) {
					prop_assume!(target != replacement);
					let original = format!("{prefix}{target}{middle}{target}{suffix}");
					let occurrences_before = original.matches(&target).count();
					prop_assume!(occurrences_before >= 2);

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let result = tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &target, "new_str": &replacement, "replace_all": false}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							prop_assert_eq!(result["edits_applied"].as_u64().unwrap(), 1);

							let content = std::fs::read_to_string(&file_path).unwrap();
							let occurrences_after = content.matches(&target).count();
							let replacement_count = content.matches(&replacement).count();

							prop_assert_eq!(occurrences_after, occurrences_before - 1);
							prop_assert_eq!(replacement_count, 1);
							let expected_prefix = format!("{prefix}{replacement}");
							prop_assert!(content.starts_with(&expected_prefix));
							Ok(())
					}).unwrap();
			}

			/// **Replace All Test**: Verifies that with replace_all=true, all occurrences of old_str
			/// are replaced, and edits_applied reflects the total count.
			///
			/// **Why this is important**: Batch replacements are common for refactoring operations
			/// like renaming variables, updating API calls, or changing import paths across a file.
			/// The tool must guarantee completeness - no occurrences should be missed.
			///
			/// **Invariant**: Given content with N occurrences of old_str, after edit with
			/// replace_all=true: (1) content has 0 occurrences of old_str, (2) content has N
			/// occurrences of new_str, (3) edits_applied == N.
			#[test]
			fn replace_all_replaces_every_occurrence(
					base in "[a-z]{2,5}",
					target in "[A-Z]{3,6}",
					replacement in "[0-9]{3,6}",
					repeat_count in 2usize..6
			) {
					prop_assume!(target != replacement);
					prop_assume!(!replacement.contains(&target));
					prop_assume!(!target.contains(&replacement));

					let original: String = (0..repeat_count)
							.map(|_| format!("{base}{target}"))
							.collect::<Vec<_>>()
							.join("");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let result = tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &target, "new_str": &replacement, "replace_all": true}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							prop_assert_eq!(result["edits_applied"].as_u64().unwrap() as usize, repeat_count);

							let content = std::fs::read_to_string(&file_path).unwrap();
							prop_assert_eq!(content.matches(&target).count(), 0);
							prop_assert_eq!(content.matches(&replacement).count(), repeat_count);
							Ok(())
					}).unwrap();
			}

			/// **No-op Test**: Verifies that when old_str == new_str, the file content remains
			/// unchanged (though the operation still "succeeds" and counts as an edit).
			///
			/// **Why this is important**: This edge case can occur when:
			/// 1. Automated tools generate edits without checking for equality
			/// 2. Template systems produce identical old/new values
			/// 3. Users accidentally specify the same string
			/// The system should handle this gracefully without corrupting data or causing errors.
			/// While semantically a no-op, the implementation counts it as an edit which is acceptable
			/// behavior - the important invariant is data preservation.
			///
			/// **Invariant**: File content before == file content after when old_str == new_str.
			#[test]
			fn noop_when_old_equals_new(
					prefix in "[a-z]{5,20}",
					target in "[A-Z]{5,15}",
					suffix in "[a-z]{5,20}"
			) {
					let original = format!("{prefix}{target}{suffix}");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let result = tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &target, "new_str": &target}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							prop_assert!(result["edits_applied"].as_u64().unwrap() >= 1);

							let content = std::fs::read_to_string(&file_path).unwrap();
							prop_assert_eq!(content, original);
							Ok(())
					}).unwrap();
			}

			/// **Preservation Test**: Verifies that text outside the replaced regions is never
			/// modified during an edit operation.
			///
			/// **Why this is important**: Edit operations must be surgical - they should only affect
			/// the targeted regions. Any modification to surrounding text would constitute data
			/// corruption and could break code or introduce subtle bugs. This is especially critical
			/// for code editing where whitespace, comments, or adjacent code must be preserved exactly.
			///
			/// **Invariant**: Given content = prefix + target + suffix, after replacing target with
			/// replacement, content == prefix + replacement + suffix (prefix and suffix unchanged).
			#[test]
			fn preservation_of_surrounding_text(
					prefix in "[a-z!@#$%^&*()]{5,30}",
					target in "[A-Z]{5,15}",
					replacement in "[0-9]{5,15}",
					suffix in "[a-z!@#$%^&*()]{5,30}"
			) {
					prop_assume!(!prefix.contains(&target));
					prop_assume!(!suffix.contains(&target));
					let original = format!("{prefix}{target}{suffix}");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &target, "new_str": &replacement}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							let content = std::fs::read_to_string(&file_path).unwrap();
							let expected = format!("{prefix}{replacement}{suffix}");
							prop_assert_eq!(&content, &expected);
							prop_assert!(content.starts_with(&prefix));
							prop_assert!(content.ends_with(&suffix));
							Ok(())
					}).unwrap();
			}

			/// **Empty old_str Test (File Creation/Append)**: Verifies that when old_str is empty,
			/// the new_str content is appended to the file (creating it if necessary).
			///
			/// **Why this is important**: Empty old_str is the mechanism for creating new files or
			/// appending content. This is a fundamental operation that must work correctly for:
			/// 1. Creating new files from scratch
			/// 2. Adding content to empty files
			/// 3. Appending to existing files
			///
			/// **Invariant**: After edit with empty old_str, file content ends with new_str.
			#[test]
			fn empty_old_str_appends_content(
					new_content in "[a-zA-Z0-9]{10,50}"
			) {
					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("new_file.txt");

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							let result = tool.invoke(
									serde_json::json!({
											"path": "new_file.txt",
											"edits": [{"old_str": "", "new_str": &new_content}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							prop_assert_eq!(result["edits_applied"].as_u64().unwrap(), 1);
							prop_assert_eq!(result["original_bytes"].as_u64().unwrap(), 0);

							let content = std::fs::read_to_string(&file_path).unwrap();
							prop_assert_eq!(content, new_content);
							Ok(())
					}).unwrap();
			}

			/// **Empty new_str Test (Deletion)**: Verifies that when new_str is empty, the old_str
			/// is deleted from the file content.
			///
			/// **Why this is important**: Deletion is a common editing operation used for:
			/// 1. Removing deprecated code
			/// 2. Cleaning up comments or debug statements
			/// 3. Refactoring by removing obsolete sections
			/// The tool must handle deletion correctly without leaving artifacts or corrupting
			/// surrounding content.
			///
			/// **Invariant**: After edit with empty new_str, file content no longer contains old_str
			/// (for replace_all=true) or contains one fewer occurrence (for replace_all=false).
			#[test]
			fn empty_new_str_deletes_content(
					prefix in "[a-z]{5,20}",
					target in "[A-Z]{5,15}",
					suffix in "[a-z]{5,20}"
			) {
					prop_assume!(!prefix.contains(&target));
					prop_assume!(!suffix.contains(&target));
					let original = format!("{prefix}{target}{suffix}");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": &target, "new_str": ""}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							let content = std::fs::read_to_string(&file_path).unwrap();
							let expected = format!("{prefix}{suffix}");
							prop_assert_eq!(&content, &expected);
							prop_assert!(!content.contains(&target));
							Ok(())
					}).unwrap();
			}

			/// **Unicode Safety Test**: Verifies that edit operations work correctly with multi-byte
			/// UTF-8 characters, including emoji, CJK characters, and combining characters.
			///
			/// **Why this is important**: Modern codebases often contain:
			/// 1. Unicode identifiers (allowed in many languages)
			/// 2. String literals with international text
			/// 3. Comments in various languages
			/// 4. Emoji in comments or string content
			/// The edit tool must handle these correctly without:
			/// - Corrupting multi-byte sequences
			/// - Miscounting byte vs character positions
			/// - Producing invalid UTF-8
			///
			/// **Invariant**: Edit operations on unicode content produce valid UTF-8 output with
			/// the expected replacements applied correctly.
			#[test]
			fn unicode_safety_multibyte_characters(
					ascii_prefix in "[a-z]{3,10}",
					ascii_suffix in "[a-z]{3,10}"
			) {
					let unicode_targets = vec![
							("こんにちは", "さようなら"),
							("🦀🔥", "✨🎉"),
							("中文测试", "日本語テスト"),
							("café", "naïve"),
							("α β γ", "δ ε ζ"),
					];

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							for (target, replacement) in unicode_targets {
									let workspace = setup_workspace();
									let file_path = workspace.path().join("test.txt");
									let original = format!("{ascii_prefix}{target}{ascii_suffix}");
									std::fs::write(&file_path, &original).unwrap();

									let tool = EditFileTool::new();
									let ctx = ToolContext::new(workspace.path().to_path_buf());

									tool.invoke(
											serde_json::json!({
													"path": "test.txt",
													"edits": [{"old_str": target, "new_str": replacement}]
											}),
											&ctx,
									)
									.await
									.unwrap();

									let content = std::fs::read_to_string(&file_path).unwrap();
									let expected = format!("{ascii_prefix}{replacement}{ascii_suffix}");
									prop_assert_eq!(&content, &expected);
									prop_assert!(std::str::from_utf8(content.as_bytes()).is_ok());
							}
							Ok(())
					}).unwrap();
			}

			/// **Unicode Boundary Safety Test**: Verifies that replacements involving unicode
			/// characters at various positions don't corrupt the file or produce invalid UTF-8.
			///
			/// **Why this is important**: String replacement operations must respect UTF-8 character
			/// boundaries. Naive byte-level operations could split multi-byte sequences, producing
			/// invalid UTF-8 that would crash parsers or corrupt data.
			///
			/// **Invariant**: All edit results are valid UTF-8 strings.
			#[test]
			fn unicode_boundary_safety(
					prefix in "[a-z]{1,5}",
					suffix in "[a-z]{1,5}"
			) {
					let unicode_content = "Hello 世界 🌍 Ωmega";
					let target = "世界";
					let replacement = "World";

					let original = format!("{prefix}{unicode_content}{suffix}");

					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let workspace = setup_workspace();
							let file_path = workspace.path().join("test.txt");
							std::fs::write(&file_path, &original).unwrap();

							let tool = EditFileTool::new();
							let ctx = ToolContext::new(workspace.path().to_path_buf());

							tool.invoke(
									serde_json::json!({
											"path": "test.txt",
											"edits": [{"old_str": target, "new_str": replacement}]
									}),
									&ctx,
							)
							.await
							.unwrap();

							let content = std::fs::read_to_string(&file_path).unwrap();
							prop_assert!(std::str::from_utf8(content.as_bytes()).is_ok());
							prop_assert!(content.contains(replacement));
							prop_assert!(!content.contains(target));
							Ok(())
					}).unwrap();
			}
	}
}
