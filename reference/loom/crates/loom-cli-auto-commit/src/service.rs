// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;
use std::sync::Arc;

use loom_cli_git::GitClient;
use loom_common_core::llm::LlmClient;
use tracing::{debug, error, info, warn};

use crate::config::AutoCommitConfig;
use crate::generator::CommitMessageGenerator;

/// Information about a completed tool execution.
#[derive(Clone, Debug)]
pub struct CompletedToolInfo {
	pub tool_name: String,
	pub succeeded: bool,
}

/// Result of an auto-commit operation.
#[derive(Clone, Debug, Default)]
pub struct AutoCommitResult {
	pub committed: bool,
	pub commit_hash: Option<String>,
	pub message: Option<String>,
	pub files_changed: usize,
	pub skip_reason: Option<String>,
}

impl AutoCommitResult {
	pub fn skipped(reason: impl Into<String>) -> Self {
		Self {
			committed: false,
			skip_reason: Some(reason.into()),
			..Default::default()
		}
	}

	pub fn committed(hash: String, message: String, files_changed: usize) -> Self {
		Self {
			committed: true,
			commit_hash: Some(hash),
			message: Some(message),
			files_changed,
			skip_reason: None,
		}
	}
}

pub struct AutoCommitService<G: GitClient, L: LlmClient> {
	git: Arc<G>,
	generator: CommitMessageGenerator<L>,
	config: AutoCommitConfig,
}

impl<G: GitClient, L: LlmClient> AutoCommitService<G, L> {
	pub fn new(git: Arc<G>, llm: Arc<L>, config: AutoCommitConfig) -> Self {
		Self {
			git: git.clone(),
			generator: CommitMessageGenerator::new(llm, config.clone()),
			config,
		}
	}

	/// Check if auto-commit should run based on completed tools.
	pub fn should_run(&self, completed_tools: &[CompletedToolInfo]) -> bool {
		if !self.config.enabled {
			return false;
		}

		completed_tools
			.iter()
			.any(|t| t.succeeded && self.config.trigger_tools.contains(&t.tool_name))
	}

	/// Run auto-commit if conditions are met.
	pub async fn run(
		&self,
		workspace_root: &Path,
		completed_tools: &[CompletedToolInfo],
	) -> AutoCommitResult {
		if !self.config.enabled {
			debug!("auto-commit disabled");
			return AutoCommitResult::skipped("disabled");
		}

		if !self.should_run(completed_tools) {
			debug!("no trigger tools succeeded");
			return AutoCommitResult::skipped("no trigger tools");
		}

		if !self.git.is_repository(workspace_root).await {
			debug!(path = %workspace_root.display(), "not a git repository");
			return AutoCommitResult::skipped("not a git repository");
		}

		let diff = match self.git.diff_all(workspace_root).await {
			Ok(d) => d,
			Err(e) => {
				error!(error = %e, "failed to get git diff");
				return AutoCommitResult::skipped(format!("git diff failed: {e}"));
			}
		};

		if diff.is_empty() {
			debug!("no changes to commit");
			return AutoCommitResult::skipped("no changes");
		}

		let files_changed = diff.files_changed.len();

		let message = match self.generator.generate(&diff).await {
			Ok(m) => m,
			Err(e) => {
				warn!(error = %e, "commit message generation failed, using fallback");
				"chore: auto-commit from loom".to_string()
			}
		};

		if let Err(e) = self.git.stage_all(workspace_root).await {
			error!(error = %e, "failed to stage changes");
			return AutoCommitResult::skipped(format!("git add failed: {e}"));
		}

		match self.git.commit(workspace_root, &message).await {
			Ok(hash) => {
				info!(
						commit_hash = %hash,
						files_changed = files_changed,
						message = %message,
						"auto-commit successful"
				);
				AutoCommitResult::committed(hash, message, files_changed)
			}
			Err(e) => {
				error!(error = %e, "failed to create commit");
				AutoCommitResult::skipped(format!("git commit failed: {e}"))
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_cli_git::{GitDiff, GitError};
	use proptest::prelude::*;

	proptest! {
			/// Validates that should_run returns false when disabled, regardless of tools.
			/// This is critical for the opt-in safety model - disabled auto-commit must
			/// never trigger, even if trigger tools have succeeded.
			#[test]
			fn should_run_returns_false_when_disabled(
					tool_count in 0usize..10,
			) {
					let tools: Vec<CompletedToolInfo> = (0..tool_count)
							.map(|i| CompletedToolInfo {
									tool_name: if i % 2 == 0 { "edit_file".to_string() } else { "bash".to_string() },
									succeeded: true,
							})
							.collect();

					let config = AutoCommitConfig::disabled();

					struct DummyGit;
					#[async_trait::async_trait]
					impl GitClient for DummyGit {
							async fn is_repository(&self, _: &Path) -> bool { true }
							async fn diff_all(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn diff_staged(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn diff_unstaged(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn stage_all(&self, _: &Path) -> Result<(), GitError> { Ok(()) }
							async fn commit(&self, _: &Path, _: &str) -> Result<String, GitError> {
									Ok("abc123".to_string())
							}
							async fn changed_files(&self, _: &Path) -> Result<Vec<String>, GitError> {
									Ok(Vec::new())
							}
					}

					struct DummyLlm;
					#[async_trait::async_trait]
					impl LlmClient for DummyLlm {
							async fn complete(&self, _: loom_common_core::llm::LlmRequest) -> Result<loom_common_core::llm::LlmResponse, loom_common_core::error::LlmError> {
									unimplemented!()
							}
							async fn complete_streaming(&self, _: loom_common_core::llm::LlmRequest) -> Result<loom_common_core::llm::LlmStream, loom_common_core::error::LlmError> {
									unimplemented!()
							}
					}

					let service = AutoCommitService::new(Arc::new(DummyGit), Arc::new(DummyLlm), config);
					prop_assert!(!service.should_run(&tools));
			}

			/// Validates that should_run returns false when no trigger tools match.
			/// This ensures auto-commit only runs after specific file-modifying operations.
			#[test]
			fn should_run_returns_false_when_no_trigger_tools_match(
					tool_count in 1usize..10,
			) {
					let tools: Vec<CompletedToolInfo> = (0..tool_count)
							.map(|i| CompletedToolInfo {
									tool_name: format!("non_trigger_tool_{i}"),
									succeeded: true,
							})
							.collect();

					let config = AutoCommitConfig::default();

					struct DummyGit;
					#[async_trait::async_trait]
					impl GitClient for DummyGit {
							async fn is_repository(&self, _: &Path) -> bool { true }
							async fn diff_all(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn diff_staged(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn diff_unstaged(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn stage_all(&self, _: &Path) -> Result<(), GitError> { Ok(()) }
							async fn commit(&self, _: &Path, _: &str) -> Result<String, GitError> {
									Ok("abc123".to_string())
							}
							async fn changed_files(&self, _: &Path) -> Result<Vec<String>, GitError> {
									Ok(Vec::new())
							}
					}

					struct DummyLlm;
					#[async_trait::async_trait]
					impl LlmClient for DummyLlm {
							async fn complete(&self, _: loom_common_core::llm::LlmRequest) -> Result<loom_common_core::llm::LlmResponse, loom_common_core::error::LlmError> {
									unimplemented!()
							}
							async fn complete_streaming(&self, _: loom_common_core::llm::LlmRequest) -> Result<loom_common_core::llm::LlmStream, loom_common_core::error::LlmError> {
									unimplemented!()
							}
					}

					let service = AutoCommitService::new(Arc::new(DummyGit), Arc::new(DummyLlm), config);
					prop_assert!(!service.should_run(&tools));
			}

			/// Validates that should_run returns true when enabled and trigger tool succeeded.
			/// This confirms the primary trigger condition works correctly.
			#[test]
			fn should_run_returns_true_when_trigger_tool_succeeded(
					trigger_tool in prop_oneof!["edit_file", "bash"],
			) {
					let tools = vec![CompletedToolInfo {
							tool_name: trigger_tool,
							succeeded: true,
					}];

					let config = AutoCommitConfig::default();

					struct DummyGit;
					#[async_trait::async_trait]
					impl GitClient for DummyGit {
							async fn is_repository(&self, _: &Path) -> bool { true }
							async fn diff_all(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn diff_staged(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn diff_unstaged(&self, _: &Path) -> Result<GitDiff, GitError> {
									Ok(GitDiff::default())
							}
							async fn stage_all(&self, _: &Path) -> Result<(), GitError> { Ok(()) }
							async fn commit(&self, _: &Path, _: &str) -> Result<String, GitError> {
									Ok("abc123".to_string())
							}
							async fn changed_files(&self, _: &Path) -> Result<Vec<String>, GitError> {
									Ok(Vec::new())
							}
					}

					struct DummyLlm;
					#[async_trait::async_trait]
					impl LlmClient for DummyLlm {
							async fn complete(&self, _: loom_common_core::llm::LlmRequest) -> Result<loom_common_core::llm::LlmResponse, loom_common_core::error::LlmError> {
									unimplemented!()
							}
							async fn complete_streaming(&self, _: loom_common_core::llm::LlmRequest) -> Result<loom_common_core::llm::LlmStream, loom_common_core::error::LlmError> {
									unimplemented!()
							}
					}

					let service = AutoCommitService::new(Arc::new(DummyGit), Arc::new(DummyLlm), config);
					prop_assert!(service.should_run(&tools));
			}

			/// Validates that AutoCommitResult::skipped always has committed=false.
			/// This ensures the result type correctly represents skip states.
			#[test]
			fn skipped_result_always_has_committed_false(
					reason in ".{0,100}",
			) {
					let result = AutoCommitResult::skipped(reason);
					prop_assert!(!result.committed);
					prop_assert!(result.skip_reason.is_some());
			}

			/// Validates that AutoCommitResult::committed always has committed=true.
			/// This ensures the result type correctly represents success states.
			#[test]
			fn committed_result_always_has_committed_true(
					hash in "[a-f0-9]{40}",
					message in ".{1,100}",
					files_changed in 1usize..100,
			) {
					let result = AutoCommitResult::committed(hash.clone(), message.clone(), files_changed);
					prop_assert!(result.committed);
					prop_assert_eq!(result.commit_hash, Some(hash));
					prop_assert_eq!(result.message, Some(message));
					prop_assert_eq!(result.files_changed, files_changed);
					prop_assert!(result.skip_reason.is_none());
			}
	}
}
