// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;

use async_trait::async_trait;
use tokio::process::Command;
use tracing::{debug, trace, warn};

use crate::client::{GitClient, GitDiff};
use crate::error::GitError;

/// Git client implementation using the git CLI.
pub struct CommandGitClient;

impl CommandGitClient {
	pub fn new() -> Self {
		Self
	}
}

impl Default for CommandGitClient {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl GitClient for CommandGitClient {
	async fn is_repository(&self, path: &Path) -> bool {
		run_git(path, &["rev-parse", "--show-toplevel"])
			.await
			.is_ok()
	}

	async fn diff_all(&self, path: &Path) -> Result<GitDiff, GitError> {
		let has_head = run_git(path, &["rev-parse", "HEAD"]).await.is_ok();

		let content = if has_head {
			run_git(path, &["diff", "HEAD"]).await.unwrap_or_default()
		} else {
			run_git(path, &["diff", "--cached"])
				.await
				.unwrap_or_default()
		};

		let stat_output = if has_head {
			run_git(path, &["diff", "--stat", "HEAD"]).await.ok()
		} else {
			run_git(path, &["diff", "--stat", "--cached"]).await.ok()
		};

		let (files_changed, insertions, deletions) =
			parse_diff_stat(stat_output.as_deref().unwrap_or(""));

		debug!(
				path = %path.display(),
				files_count = files_changed.len(),
				insertions,
				deletions,
				"computed diff (all)"
		);

		Ok(GitDiff {
			content,
			files_changed,
			insertions,
			deletions,
		})
	}

	async fn diff_staged(&self, path: &Path) -> Result<GitDiff, GitError> {
		let content = run_git(path, &["diff", "--cached"])
			.await
			.unwrap_or_default();
		let stat_output = run_git(path, &["diff", "--stat", "--cached"]).await.ok();

		let (files_changed, insertions, deletions) =
			parse_diff_stat(stat_output.as_deref().unwrap_or(""));

		debug!(
				path = %path.display(),
				files_count = files_changed.len(),
				insertions,
				deletions,
				"computed diff (staged)"
		);

		Ok(GitDiff {
			content,
			files_changed,
			insertions,
			deletions,
		})
	}

	async fn diff_unstaged(&self, path: &Path) -> Result<GitDiff, GitError> {
		let content = run_git(path, &["diff"]).await.unwrap_or_default();
		let stat_output = run_git(path, &["diff", "--stat"]).await.ok();

		let (files_changed, insertions, deletions) =
			parse_diff_stat(stat_output.as_deref().unwrap_or(""));

		debug!(
				path = %path.display(),
				files_count = files_changed.len(),
				insertions,
				deletions,
				"computed diff (unstaged)"
		);

		Ok(GitDiff {
			content,
			files_changed,
			insertions,
			deletions,
		})
	}

	async fn stage_all(&self, path: &Path) -> Result<(), GitError> {
		run_git(path, &["add", "-A"]).await?;
		debug!(path = %path.display(), "staged all changes");
		Ok(())
	}

	async fn commit(&self, path: &Path, message: &str) -> Result<String, GitError> {
		run_git(path, &["commit", "-m", message]).await?;

		let sha = run_git(path, &["rev-parse", "HEAD"]).await?;

		debug!(path = %path.display(), sha = %sha, "created commit");
		Ok(sha)
	}

	async fn changed_files(&self, path: &Path) -> Result<Vec<String>, GitError> {
		// Get list of changed files (staged + unstaged + untracked)
		let output = run_git(path, &["status", "--porcelain"])
			.await
			.unwrap_or_default();

		let files: Vec<String> = output
			.lines()
			.filter_map(|line| {
				// Porcelain format: XY filename
				// First two chars are status, then space, then filename
				if line.len() > 3 {
					Some(line[3..].to_string())
				} else {
					None
				}
			})
			.collect();

		debug!(
				path = %path.display(),
				files_count = files.len(),
				"listed changed files"
		);

		Ok(files)
	}
}

/// Runs a git command and returns the stdout on success.
async fn run_git(path: &Path, args: &[&str]) -> Result<String, GitError> {
	let mut cmd = Command::new("git");
	cmd.arg("-C").arg(path).args(args);

	trace!(
			cmd = %format!("git -C {} {}", path.display(), args.join(" ")),
			"running git command"
	);

	let output = cmd.output().await.map_err(|e| {
		if e.kind() == std::io::ErrorKind::NotFound {
			warn!("git not found in PATH");
			GitError::GitNotInstalled
		} else {
			GitError::Io(e)
		}
	})?;

	if output.status.success() {
		Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		Err(GitError::CommandFailed {
			cmd: "git",
			args: args.iter().map(|s| s.to_string()).collect(),
			stderr,
		})
	}
}

/// Parses git diff --stat output into (files, insertions, deletions).
fn parse_diff_stat(output: &str) -> (Vec<String>, usize, usize) {
	let mut files = Vec::new();
	let mut insertions = 0;
	let mut deletions = 0;

	for line in output.lines() {
		let line = line.trim();

		if line.contains("file") && line.contains("changed") {
			for part in line.split(',') {
				let part = part.trim();
				if part.contains("insertion") {
					if let Some(num) = part.split_whitespace().next() {
						insertions = num.parse().unwrap_or(0);
					}
				} else if part.contains("deletion") {
					if let Some(num) = part.split_whitespace().next() {
						deletions = num.parse().unwrap_or(0);
					}
				}
			}
		} else if line.contains('|') {
			if let Some(file_part) = line.split('|').next() {
				let file = file_part.trim().to_string();
				if !file.is_empty() {
					files.push(file);
				}
			}
		}
	}

	(files, insertions, deletions)
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use std::fs;
	use std::process::Command as StdCommand;
	use tempfile::TempDir;

	fn init_git_repo(dir: &Path) {
		StdCommand::new("git")
			.args(["init"])
			.current_dir(dir)
			.output()
			.expect("git init failed");

		StdCommand::new("git")
			.args(["config", "user.email", "test@test.com"])
			.current_dir(dir)
			.output()
			.expect("git config failed");

		StdCommand::new("git")
			.args(["config", "user.name", "Test"])
			.current_dir(dir)
			.output()
			.expect("git config failed");
	}

	fn create_initial_commit(dir: &Path) {
		fs::write(dir.join("README.md"), "# Test").expect("write failed");

		StdCommand::new("git")
			.args(["add", "."])
			.current_dir(dir)
			.output()
			.expect("git add failed");

		StdCommand::new("git")
			.args(["commit", "-m", "Initial commit"])
			.current_dir(dir)
			.output()
			.expect("git commit failed");
	}

	/// Test: is_repository returns true for a valid git repository.
	///
	/// Why this test is important: The is_repository method is the foundation for
	/// all other operations. If it incorrectly returns false for valid repos, no
	/// auto-commit functionality will work.
	#[tokio::test]
	async fn test_is_repository_true_for_git_repo() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());

		let client = CommandGitClient::new();
		assert!(client.is_repository(temp.path()).await);
	}

	/// Test: is_repository returns false for non-git directories.
	///
	/// Why this test is important: We must correctly identify directories that
	/// are NOT git repositories to avoid attempting git operations on them, which
	/// would fail with confusing errors.
	#[tokio::test]
	async fn test_is_repository_false_for_non_git() {
		let temp = TempDir::new().unwrap();

		let client = CommandGitClient::new();
		assert!(!client.is_repository(temp.path()).await);
	}

	/// Test: diff_all returns empty diff for clean repository.
	///
	/// Why this test is important: After a commit, the working tree should be
	/// clean and diff_all should return an empty diff. This is the expected
	/// quiescent state.
	#[tokio::test]
	async fn test_diff_all_clean_repo() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		let client = CommandGitClient::new();
		let diff = client.diff_all(temp.path()).await.unwrap();
		assert!(diff.is_empty());
	}

	/// Test: diff_all detects uncommitted changes.
	///
	/// Why this test is important: The primary purpose of diff_all is to detect
	/// changes that need to be committed. This verifies that modifications to
	/// tracked files are correctly captured in the diff output.
	#[tokio::test]
	async fn test_diff_all_with_changes() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		fs::write(temp.path().join("README.md"), "# Modified").unwrap();

		let client = CommandGitClient::new();
		let diff = client.diff_all(temp.path()).await.unwrap();
		assert!(!diff.is_empty());
		assert!(diff.content.contains("Modified"));
	}

	/// Test: stage_all stages all changes including untracked files.
	///
	/// Why this test is important: Using -A flag stages all changes including
	/// new untracked files. This ensures auto-commit captures all changes made
	/// by the agent, including newly created files.
	#[tokio::test]
	async fn test_stage_all_includes_untracked() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		fs::write(temp.path().join("README.md"), "# Modified").unwrap();
		fs::write(temp.path().join("new_file.txt"), "new content").unwrap();

		let client = CommandGitClient::new();
		client.stage_all(temp.path()).await.unwrap();

		let status = StdCommand::new("git")
			.args(["status", "--porcelain"])
			.current_dir(temp.path())
			.output()
			.unwrap();

		let output = String::from_utf8_lossy(&status.stdout);
		assert!(
			output.contains("M  README.md"),
			"tracked file should be staged"
		);
		assert!(
			output.contains("A  new_file.txt"),
			"untracked file should be staged"
		);
	}

	/// Test: commit creates a commit and returns the SHA.
	///
	/// Why this test is important: The commit method must successfully create a
	/// commit and return a valid SHA. This SHA is used for tracking and logging
	/// purposes in auto-commit features.
	#[tokio::test]
	async fn test_commit() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		fs::write(temp.path().join("README.md"), "# Modified content").unwrap();

		let client = CommandGitClient::new();
		client.stage_all(temp.path()).await.unwrap();
		let sha = client.commit(temp.path(), "Test commit").await.unwrap();

		assert_eq!(sha.len(), 40);
		assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));

		let log = StdCommand::new("git")
			.args(["log", "--oneline", "-1"])
			.current_dir(temp.path())
			.output()
			.unwrap();

		let output = String::from_utf8_lossy(&log.stdout);
		assert!(output.contains("Test commit"));
	}

	/// Test: diff_all works on empty repository (no commits yet).
	///
	/// Why this test is important: A freshly initialized repository has no HEAD
	/// commit, so we cannot diff against HEAD. The implementation must handle
	/// this edge case by diffing staged changes instead.
	#[tokio::test]
	async fn test_diff_all_empty_repo() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());

		fs::write(temp.path().join("new_file.txt"), "content").unwrap();

		StdCommand::new("git")
			.args(["add", "."])
			.current_dir(temp.path())
			.output()
			.unwrap();

		let client = CommandGitClient::new();
		let diff = client.diff_all(temp.path()).await.unwrap();

		assert!(!diff.is_empty());
	}

	// Property: parse_diff_stat extracts non-negative counts.
	//
	// Why this test is important: The parse function should never return negative
	// values for insertions/deletions. This property ensures robustness against
	// malformed or unexpected git output.
	proptest! {
			#[test]
			fn prop_parse_diff_stat_non_negative(
					files in 0usize..10,
					insertions in 0usize..1000,
					deletions in 0usize..1000,
			) {
					let stat_line = format!(
							" {} file{} changed, {} insertion{}(+), {} deletion{}(-)",
							files,
							if files == 1 { "" } else { "s" },
							insertions,
							if insertions == 1 { "" } else { "s" },
							deletions,
							if deletions == 1 { "" } else { "s" },
					);

					let (_, ins, del) = parse_diff_stat(&stat_line);
					prop_assert!(ins <= insertions || insertions == 0);
					prop_assert!(del <= deletions || deletions == 0);
			}
	}

	/// Test: parse_diff_stat handles empty input.
	///
	/// Why this test is important: When there are no changes, git diff --stat
	/// produces empty output. The parser must handle this gracefully.
	#[test]
	fn test_parse_diff_stat_empty() {
		let (files, ins, del) = parse_diff_stat("");
		assert!(files.is_empty());
		assert_eq!(ins, 0);
		assert_eq!(del, 0);
	}

	/// Test: parse_diff_stat extracts file names.
	///
	/// Why this test is important: The files_changed list is used to report which
	/// files were modified. This verifies correct extraction from typical stat
	/// output.
	#[test]
	fn test_parse_diff_stat_with_files() {
		let output = " src/main.rs | 10 +++++++---\n lib/utils.rs | 5 ++---\n 2 files changed, 9 \
		              insertions(+), 6 deletions(-)\n";
		let (files, ins, del) = parse_diff_stat(output);

		assert_eq!(files.len(), 2);
		assert!(files.contains(&"src/main.rs".to_string()));
		assert!(files.contains(&"lib/utils.rs".to_string()));
		assert_eq!(ins, 9);
		assert_eq!(del, 6);
	}
}
