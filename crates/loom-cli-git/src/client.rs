// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;

use async_trait::async_trait;

use crate::error::GitError;

/// Result of a git diff operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitDiff {
	/// The raw diff content.
	pub content: String,
	/// List of files that changed.
	pub files_changed: Vec<String>,
	/// Number of insertions.
	pub insertions: usize,
	/// Number of deletions.
	pub deletions: usize,
}

impl GitDiff {
	/// Returns true if there are no changes.
	pub fn is_empty(&self) -> bool {
		self.content.is_empty() && self.files_changed.is_empty()
	}
}

/// Trait abstracting git operations for testability.
#[async_trait]
pub trait GitClient: Send + Sync {
	/// Check if the path is inside a git repository.
	async fn is_repository(&self, path: &Path) -> bool;

	/// Get the diff of all changes (staged + unstaged).
	async fn diff_all(&self, path: &Path) -> Result<GitDiff, GitError>;

	/// Get the diff of staged changes only.
	async fn diff_staged(&self, path: &Path) -> Result<GitDiff, GitError>;

	/// Get the diff of unstaged changes only (working tree vs index).
	async fn diff_unstaged(&self, path: &Path) -> Result<GitDiff, GitError>;

	/// Stage all changes in the repository.
	async fn stage_all(&self, path: &Path) -> Result<(), GitError>;

	/// Create a commit with the given message.
	async fn commit(&self, path: &Path, message: &str) -> Result<String, GitError>;

	/// Get list of changed files (staged + unstaged).
	async fn changed_files(&self, path: &Path) -> Result<Vec<String>, GitError>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	// Property: GitDiff::is_empty returns true iff content is empty AND
	// files_changed is empty.
	//
	// Why this test is important: The is_empty method is used to determine if there
	// are meaningful changes to commit. If either the content or the files_changed
	// list has data, the diff should NOT be considered empty. This property ensures
	// the logic correctly represents the "no changes" state.
	proptest! {
			#[test]
			fn prop_is_empty_iff_both_empty(
					content in ".*",
					files in prop::collection::vec("[a-z/]+\\.[a-z]+", 0..5),
					insertions in 0usize..100,
					deletions in 0usize..100,
			) {
					let diff = GitDiff {
							content: content.clone(),
							files_changed: files.clone(),
							insertions,
							deletions,
					};

					let expected_empty = content.is_empty() && files.is_empty();
					prop_assert_eq!(diff.is_empty(), expected_empty);
			}
	}

	/// Test: Default GitDiff is empty.
	///
	/// Why this test is important: The Default trait implementation should
	/// produce an empty diff (no content, no files, zero stats). This is the
	/// natural starting state and must satisfy is_empty().
	#[test]
	fn test_default_is_empty() {
		let diff = GitDiff::default();
		assert!(diff.is_empty());
		assert_eq!(diff.content, "");
		assert!(diff.files_changed.is_empty());
		assert_eq!(diff.insertions, 0);
		assert_eq!(diff.deletions, 0);
	}

	/// Test: GitDiff with only content is not empty.
	///
	/// Why this test is important: Even if no files are listed, having diff
	/// content means there are changes. This ensures partial diff information is
	/// still recognized as non-empty.
	#[test]
	fn test_with_content_not_empty() {
		let diff = GitDiff {
			content: "diff --git a/file.txt".to_string(),
			files_changed: vec![],
			insertions: 0,
			deletions: 0,
		};
		assert!(!diff.is_empty());
	}

	/// Test: GitDiff with only files_changed is not empty.
	///
	/// Why this test is important: Even if the raw diff content is empty (e.g.,
	/// only file mode changes), having files in the changed list means there are
	/// changes.
	#[test]
	fn test_with_files_not_empty() {
		let diff = GitDiff {
			content: String::new(),
			files_changed: vec!["file.txt".to_string()],
			insertions: 0,
			deletions: 0,
		};
		assert!(!diff.is_empty());
	}
}
