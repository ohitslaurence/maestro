// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::client::{GitClient, GitDiff};
use crate::error::GitError;

/// Recorded call to the mock git client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MockCall {
	IsRepository,
	DiffAll,
	DiffStaged,
	DiffUnstaged,
	StageAll,
	Commit(String),
	ChangedFiles,
}

/// Mock git client for testing.
#[derive(Clone, Default)]
pub struct MockGitClient {
	/// Whether is_repository returns true.
	pub is_repo: bool,
	/// Diff to return from diff_all.
	pub diff: GitDiff,
	/// Diff to return from diff_staged.
	pub staged_diff: GitDiff,
	/// Diff to return from diff_unstaged.
	pub unstaged_diff: GitDiff,
	/// If set, stage_all returns this error.
	pub stage_error: Option<String>,
	/// If set, commit returns this error.
	pub commit_error: Option<String>,
	/// SHA to return from commit.
	pub commit_sha: String,
	/// Files to return from changed_files.
	pub changed_files_list: Vec<String>,
	/// Track calls for verification.
	pub calls: Arc<Mutex<Vec<MockCall>>>,
}

impl MockGitClient {
	pub fn new() -> Self {
		Self {
			is_repo: true,
			diff: GitDiff::default(),
			staged_diff: GitDiff::default(),
			unstaged_diff: GitDiff::default(),
			stage_error: None,
			commit_error: None,
			commit_sha: "abc123def456789012345678901234567890abcd".to_string(),
			changed_files_list: Vec::new(),
			calls: Arc::new(Mutex::new(Vec::new())),
		}
	}

	pub fn with_diff(mut self, diff: GitDiff) -> Self {
		self.diff = diff;
		self
	}

	pub fn with_staged_diff(mut self, diff: GitDiff) -> Self {
		self.staged_diff = diff;
		self
	}

	pub fn with_unstaged_diff(mut self, diff: GitDiff) -> Self {
		self.unstaged_diff = diff;
		self
	}

	pub fn with_changed_files(mut self, files: Vec<String>) -> Self {
		self.changed_files_list = files;
		self
	}

	pub fn not_a_repo(mut self) -> Self {
		self.is_repo = false;
		self
	}

	pub fn with_stage_error(mut self, error: impl Into<String>) -> Self {
		self.stage_error = Some(error.into());
		self
	}

	pub fn with_commit_error(mut self, error: impl Into<String>) -> Self {
		self.commit_error = Some(error.into());
		self
	}

	pub fn with_commit_sha(mut self, sha: impl Into<String>) -> Self {
		self.commit_sha = sha.into();
		self
	}

	/// Returns the recorded calls.
	pub fn get_calls(&self) -> Vec<MockCall> {
		self.calls.lock().unwrap().clone()
	}

	/// Clears recorded calls.
	pub fn clear_calls(&self) {
		self.calls.lock().unwrap().clear();
	}

	fn record(&self, call: MockCall) {
		self.calls.lock().unwrap().push(call);
	}
}

#[async_trait]
impl GitClient for MockGitClient {
	async fn is_repository(&self, _path: &Path) -> bool {
		self.record(MockCall::IsRepository);
		self.is_repo
	}

	async fn diff_all(&self, _path: &Path) -> Result<GitDiff, GitError> {
		self.record(MockCall::DiffAll);
		Ok(self.diff.clone())
	}

	async fn diff_staged(&self, _path: &Path) -> Result<GitDiff, GitError> {
		self.record(MockCall::DiffStaged);
		Ok(self.staged_diff.clone())
	}

	async fn diff_unstaged(&self, _path: &Path) -> Result<GitDiff, GitError> {
		self.record(MockCall::DiffUnstaged);
		Ok(self.unstaged_diff.clone())
	}

	async fn stage_all(&self, _path: &Path) -> Result<(), GitError> {
		self.record(MockCall::StageAll);
		if let Some(ref error) = self.stage_error {
			return Err(GitError::CommandFailed {
				cmd: "git",
				args: vec!["add".to_string(), "-A".to_string()],
				stderr: error.clone(),
			});
		}
		Ok(())
	}

	async fn commit(&self, _path: &Path, message: &str) -> Result<String, GitError> {
		self.record(MockCall::Commit(message.to_string()));
		if let Some(ref error) = self.commit_error {
			return Err(GitError::CommandFailed {
				cmd: "git",
				args: vec!["commit".to_string(), "-m".to_string(), message.to_string()],
				stderr: error.clone(),
			});
		}
		Ok(self.commit_sha.clone())
	}

	async fn changed_files(&self, _path: &Path) -> Result<Vec<String>, GitError> {
		self.record(MockCall::ChangedFiles);
		Ok(self.changed_files_list.clone())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use std::path::PathBuf;

	/// Test: MockGitClient records all calls in order.
	///
	/// Why this test is important: The mock is used to verify that code under
	/// test calls the git client methods in the expected order and with expected
	/// arguments. Incorrect recording would lead to false positive tests.
	#[tokio::test]
	async fn test_records_all_calls() {
		let client = MockGitClient::new();
		let path = PathBuf::from("/test");

		client.is_repository(&path).await;
		client.diff_all(&path).await.unwrap();
		client.stage_all(&path).await.unwrap();
		client.commit(&path, "test message").await.unwrap();

		let calls = client.get_calls();
		assert_eq!(
			calls,
			vec![
				MockCall::IsRepository,
				MockCall::DiffAll,
				MockCall::StageAll,
				MockCall::Commit("test message".to_string()),
			]
		);
	}

	/// Test: MockGitClient can be configured to return not-a-repo.
	///
	/// Why this test is important: Testing error paths requires simulating the
	/// case where a directory is not a git repository. The mock must correctly
	/// return false when configured this way.
	#[tokio::test]
	async fn test_not_a_repo_config() {
		let client = MockGitClient::new().not_a_repo();
		let path = PathBuf::from("/test");

		assert!(!client.is_repository(&path).await);
	}

	/// Test: MockGitClient returns configured diff.
	///
	/// Why this test is important: Tests need to verify behavior based on
	/// different diff contents. The mock must correctly return the configured
	/// diff for assertions in tests.
	#[tokio::test]
	async fn test_returns_configured_diff() {
		let diff = GitDiff {
			content: "diff content".to_string(),
			files_changed: vec!["file.txt".to_string()],
			insertions: 5,
			deletions: 3,
		};
		let client = MockGitClient::new().with_diff(diff.clone());
		let path = PathBuf::from("/test");

		let result = client.diff_all(&path).await.unwrap();
		assert_eq!(result, diff);
	}

	/// Test: MockGitClient can simulate stage error.
	///
	/// Why this test is important: Auto-commit code must handle staging failures
	/// gracefully. The mock must be able to simulate these failures for testing
	/// error handling paths.
	#[tokio::test]
	async fn test_stage_error() {
		let client = MockGitClient::new().with_stage_error("permission denied");
		let path = PathBuf::from("/test");

		let result = client.stage_all(&path).await;
		assert!(result.is_err());
	}

	/// Test: MockGitClient can simulate commit error.
	///
	/// Why this test is important: Commits can fail for various reasons (nothing
	/// to commit, hooks failed, etc.). The mock must simulate these for testing
	/// error recovery logic.
	#[tokio::test]
	async fn test_commit_error() {
		let client = MockGitClient::new().with_commit_error("nothing to commit");
		let path = PathBuf::from("/test");

		let result = client.commit(&path, "test").await;
		assert!(result.is_err());
	}

	/// Test: clear_calls resets the call history.
	///
	/// Why this test is important: Tests may need to verify calls in phases,
	/// clearing the history between phases. This verifies the clear
	/// functionality.
	#[tokio::test]
	async fn test_clear_calls() {
		let client = MockGitClient::new();
		let path = PathBuf::from("/test");

		client.is_repository(&path).await;
		assert!(!client.get_calls().is_empty());

		client.clear_calls();
		assert!(client.get_calls().is_empty());
	}

	// Property: Commit call records the exact message provided.
	//
	// Why this test is important: The commit message is critical for traceability.
	// The mock must record the exact message passed, not a modified version,
	// so tests can verify the correct message was used.
	proptest! {
			#[test]
			fn prop_commit_records_exact_message(message in "[a-zA-Z0-9 ]{1,100}") {
					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let client = MockGitClient::new();
							let path = PathBuf::from("/test");

							client.commit(&path, &message).await.unwrap();

							let calls = client.get_calls();
							prop_assert_eq!(calls, vec![MockCall::Commit(message.clone())]);
							Ok(())
					}).unwrap();
			}
	}

	// Property: Multiple MockGitClient clones share call history.
	//
	// Why this test is important: The calls field uses Arc<Mutex<>> so clones
	// share state. This is essential for passing the mock to code under test
	// while retaining the ability to inspect calls afterward.
	proptest! {
			#[test]
			fn prop_clones_share_calls(call_count in 1usize..10) {
					let rt = tokio::runtime::Runtime::new().unwrap();
					rt.block_on(async {
							let client = MockGitClient::new();
							let clone = client.clone();
							let path = PathBuf::from("/test");

							for _ in 0..call_count {
									clone.is_repository(&path).await;
							}

							let original_calls = client.get_calls();
							let clone_calls = clone.get_calls();

							prop_assert_eq!(original_calls.len(), call_count);
							prop_assert_eq!(original_calls, clone_calls);
							Ok(())
					}).unwrap();
			}
	}
}
