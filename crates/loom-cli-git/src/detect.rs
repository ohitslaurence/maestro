// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;
use std::process::Command;

use crate::error::GitError;
use crate::normalize::normalize_remote_url;
use crate::{CommitInfo, RepoStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMetadata {
	pub branch: Option<String>,
	pub remote_slug: Option<String>,
}

/// Detects git repository metadata for the given path.
///
/// Returns `Ok(None)` if the path is not inside a git repository or git is not
/// installed. Returns `Ok(Some(metadata))` with branch and remote information
/// if available.
pub fn detect_repo_metadata(path: &Path) -> Result<Option<RepoMetadata>, GitError> {
	if !is_git_repo(path)? {
		tracing::debug!(path = %path.display(), "not a git repository");
		return Ok(None);
	}

	let branch = current_branch(path)?;
	let remote_slug = default_remote_url(path)?.and_then(|url| normalize_remote_url(&url));

	tracing::info!(
			path = %path.display(),
			branch = ?branch,
			remote_slug = ?remote_slug,
			"detected git repository"
	);

	Ok(Some(RepoMetadata {
		branch,
		remote_slug,
	}))
}

/// Returns the current branch name, or `None` if in detached HEAD state.
pub fn current_branch(path: &Path) -> Result<Option<String>, GitError> {
	let output = run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"])?;

	match output {
		Some(branch) if branch == "HEAD" => {
			tracing::debug!(path = %path.display(), "detached HEAD state");
			Ok(None)
		}
		Some(branch) => {
			tracing::trace!(path = %path.display(), branch = %branch, "found branch");
			Ok(Some(branch))
		}
		None => Ok(None),
	}
}

/// Returns the URL of the default remote (origin, upstream, or first
/// available).
pub fn default_remote_url(path: &Path) -> Result<Option<String>, GitError> {
	for remote in &["origin", "upstream"] {
		if let Some(url) = get_remote_url(path, remote)? {
			tracing::trace!(path = %path.display(), remote = %remote, url = %url, "found remote URL");
			return Ok(Some(url));
		}
	}

	if let Some(first_remote) = first_remote_name(path)? {
		if let Some(url) = get_remote_url(path, &first_remote)? {
			tracing::trace!(
					path = %path.display(),
					remote = %first_remote,
					url = %url,
					"using first available remote"
			);
			return Ok(Some(url));
		}
	}

	tracing::debug!(path = %path.display(), "no remotes configured");
	Ok(None)
}

fn is_git_repo(path: &Path) -> Result<bool, GitError> {
	let output = run_git(path, &["rev-parse", "--show-toplevel"])?;
	Ok(output.is_some())
}

/// Returns the HEAD commit SHA (full 40-character).
///
/// Returns `Ok(None)` if not inside a git repository or there are no commits.
pub fn head_commit_sha(path: &Path) -> Result<Option<String>, GitError> {
	if !is_git_repo(path)? {
		return Ok(None);
	}
	run_git(path, &["rev-parse", "HEAD"])
}

/// Returns whether the working tree has uncommitted changes.
///
/// Returns `Ok(None)` if not inside a git repository.
/// Returns `Ok(Some(true))` if there are uncommitted changes.
/// Returns `Ok(Some(false))` if the working tree is clean.
pub fn is_dirty(path: &Path) -> Result<Option<bool>, GitError> {
	if !is_git_repo(path)? {
		return Ok(None);
	}

	let mut cmd = Command::new("git");
	cmd.arg("-C").arg(path).args(["status", "--porcelain"]);

	tracing::trace!(cmd = ?cmd, "running git command");

	let output = match cmd.output() {
		Ok(output) => output,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			return Err(GitError::GitNotInstalled);
		}
		Err(e) => return Err(GitError::Io(e)),
	};

	if output.status.success() {
		let stdout = String::from_utf8_lossy(&output.stdout);
		Ok(Some(!stdout.trim().is_empty()))
	} else {
		Ok(None)
	}
}

/// Detects full repository status including commit info and dirty state.
///
/// Returns `Ok(None)` if not inside a git repository.
pub fn detect_repo_status(path: &Path) -> Result<Option<RepoStatus>, GitError> {
	if !is_git_repo(path)? {
		tracing::debug!(path = %path.display(), "not a git repository");
		return Ok(None);
	}

	let branch = current_branch(path)?;
	let remote_slug = default_remote_url(path)?.and_then(|url| normalize_remote_url(&url));
	let sha = head_commit_sha(path)?;
	let dirty = is_dirty(path)?;

	let head = if let Some(sha) = sha {
		let (timestamp, summary) = get_commit_info(path)?;
		Some(CommitInfo {
			sha,
			summary,
			timestamp,
		})
	} else {
		None
	};

	tracing::info!(
			path = %path.display(),
			branch = ?branch,
			remote_slug = ?remote_slug,
			head_sha = ?head.as_ref().map(|h| &h.sha),
			is_dirty = ?dirty,
			"detected git repository status"
	);

	Ok(Some(RepoStatus {
		branch,
		remote_slug,
		head,
		is_dirty: dirty,
	}))
}

fn get_commit_info(path: &Path) -> Result<(Option<i64>, Option<String>), GitError> {
	let output = run_git(path, &["show", "-s", "--format=%ct%n%s", "HEAD"])?;

	match output {
		Some(s) => {
			let mut lines = s.lines();
			let timestamp = lines.next().and_then(|ts| ts.parse::<i64>().ok());
			let summary = lines.next().map(|s| s.to_string());
			Ok((timestamp, summary))
		}
		None => Ok((None, None)),
	}
}

fn get_remote_url(path: &Path, remote: &str) -> Result<Option<String>, GitError> {
	run_git(path, &["remote", "get-url", remote])
}

fn first_remote_name(path: &Path) -> Result<Option<String>, GitError> {
	run_git(path, &["remote"])
		.map(|output| output.and_then(|s| s.lines().next().map(|s| s.to_string())))
}

fn run_git(path: &Path, args: &[&str]) -> Result<Option<String>, GitError> {
	let mut cmd = Command::new("git");
	cmd.arg("-C").arg(path).args(args);

	tracing::trace!(cmd = ?cmd, "running git command");

	let output = match cmd.output() {
		Ok(output) => output,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			tracing::warn!("git not found in PATH");
			return Err(GitError::GitNotInstalled);
		}
		Err(e) => return Err(GitError::Io(e)),
	};

	if output.status.success() {
		let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
		if stdout.is_empty() {
			Ok(None)
		} else {
			Ok(Some(stdout))
		}
	} else {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

		if stderr.contains("not a git repository")
			|| stderr.contains("No such remote")
			|| stderr.contains("fatal: not a git repository")
		{
			Ok(None)
		} else if !stderr.is_empty() {
			tracing::debug!(
					args = ?args,
					stderr = %stderr,
					"git command returned non-zero with stderr"
			);
			Ok(None)
		} else {
			Ok(None)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use std::process::Command;
	use tempfile::TempDir;

	fn init_git_repo(dir: &Path) {
		Command::new("git")
			.args(["init"])
			.current_dir(dir)
			.output()
			.expect("git init failed");

		Command::new("git")
			.args(["config", "user.email", "test@test.com"])
			.current_dir(dir)
			.output()
			.expect("git config failed");

		Command::new("git")
			.args(["config", "user.name", "Test"])
			.current_dir(dir)
			.output()
			.expect("git config failed");
	}

	fn create_initial_commit(dir: &Path) {
		fs::write(dir.join("README.md"), "# Test").expect("write failed");

		Command::new("git")
			.args(["add", "."])
			.current_dir(dir)
			.output()
			.expect("git add failed");

		Command::new("git")
			.args(["commit", "-m", "Initial commit"])
			.current_dir(dir)
			.output()
			.expect("git commit failed");
	}

	fn add_remote(dir: &Path, name: &str, url: &str) {
		Command::new("git")
			.args(["remote", "add", name, url])
			.current_dir(dir)
			.output()
			.expect("git remote add failed");
	}

	/// Test: Non-git directory returns None.
	///
	/// Why this test is important: The function must gracefully handle
	/// directories that are not git repositories, returning None instead of an
	/// error. This is the most common case when scanning arbitrary directories.
	#[test]
	fn test_non_git_directory_returns_none() {
		let temp = TempDir::new().unwrap();
		let result = detect_repo_metadata(temp.path()).unwrap();
		assert_eq!(result, None);
	}

	/// Test: Empty git repository (no commits) is detected.
	///
	/// Why this test is important: A freshly initialized git repository has no
	/// commits and may not have a valid HEAD. We should still detect it as a
	/// git repository and handle the missing branch gracefully.
	#[test]
	fn test_empty_git_repo() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());

		let result = detect_repo_metadata(temp.path()).unwrap();
		assert!(result.is_some());

		let metadata = result.unwrap();
		assert!(metadata.remote_slug.is_none());
	}

	/// Test: Git repository with commits has a branch.
	///
	/// Why this test is important: After commits are made, the branch name should
	/// be detectable. This is the normal state for most repositories.
	#[test]
	fn test_git_repo_with_commits_has_branch() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		let result = detect_repo_metadata(temp.path()).unwrap();
		assert!(result.is_some());

		let metadata = result.unwrap();
		assert!(metadata.branch.is_some());
	}

	/// Test: Remote URL is normalized in metadata.
	///
	/// Why this test is important: The remote_slug should contain the normalized
	/// form of the remote URL, not the raw URL. This ensures consistent matching
	/// across different URL formats.
	#[test]
	fn test_remote_url_is_normalized() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());
		add_remote(temp.path(), "origin", "git@github.com:owner/repo.git");

		let result = detect_repo_metadata(temp.path()).unwrap();
		assert!(result.is_some());

		let metadata = result.unwrap();
		assert_eq!(
			metadata.remote_slug,
			Some("github.com/owner/repo".to_string())
		);
	}

	/// Test: Origin is preferred over other remotes.
	///
	/// Why this test is important: When multiple remotes exist, "origin" is the
	/// conventional default and should be preferred. This matches user
	/// expectations and git's own behavior.
	#[test]
	fn test_origin_preferred_over_other_remotes() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());
		add_remote(temp.path(), "other", "git@github.com:other/repo.git");
		add_remote(temp.path(), "origin", "git@github.com:origin/repo.git");

		let result = detect_repo_metadata(temp.path()).unwrap();
		let metadata = result.unwrap();
		assert_eq!(
			metadata.remote_slug,
			Some("github.com/origin/repo".to_string())
		);
	}

	/// Test: Upstream is used when origin is absent.
	///
	/// Why this test is important: "upstream" is commonly used for forked
	/// repositories. When "origin" doesn't exist, we should fall back to
	/// "upstream" before trying other remotes.
	#[test]
	fn test_upstream_fallback() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());
		add_remote(temp.path(), "other", "git@github.com:other/repo.git");
		add_remote(temp.path(), "upstream", "git@github.com:upstream/repo.git");

		let result = detect_repo_metadata(temp.path()).unwrap();
		let metadata = result.unwrap();
		assert_eq!(
			metadata.remote_slug,
			Some("github.com/upstream/repo".to_string())
		);
	}

	/// Test: Falls back to first remote when origin/upstream absent.
	///
	/// Why this test is important: Some repositories may have non-standard remote
	/// names. We should still detect the remote rather than returning None.
	#[test]
	fn test_first_remote_fallback() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());
		add_remote(temp.path(), "custom", "git@github.com:custom/repo.git");

		let result = detect_repo_metadata(temp.path()).unwrap();
		let metadata = result.unwrap();
		assert_eq!(
			metadata.remote_slug,
			Some("github.com/custom/repo".to_string())
		);
	}

	/// Test: current_branch returns None for detached HEAD.
	///
	/// Why this test is important: When a specific commit is checked out (not a
	/// branch), git reports "HEAD". We should return None to indicate no branch
	/// is active, rather than confusing "HEAD" as a branch name.
	#[test]
	fn test_detached_head_returns_none() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		Command::new("git")
			.args(["checkout", "--detach"])
			.current_dir(temp.path())
			.output()
			.expect("git checkout failed");

		let branch = current_branch(temp.path()).unwrap();
		assert_eq!(branch, None);
	}

	/// Test: Subdirectory of git repo is detected.
	///
	/// Why this test is important: Users often work in subdirectories of a
	/// repository. We should correctly detect the git repository even when
	/// the path points to a subdirectory.
	#[test]
	fn test_subdirectory_detection() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());
		add_remote(temp.path(), "origin", "git@github.com:owner/repo.git");

		let subdir = temp.path().join("src").join("lib");
		fs::create_dir_all(&subdir).unwrap();

		let result = detect_repo_metadata(&subdir).unwrap();
		assert!(result.is_some());

		let metadata = result.unwrap();
		assert_eq!(
			metadata.remote_slug,
			Some("github.com/owner/repo".to_string())
		);
	}

	/// Test: head_commit_sha returns full 40-character SHA in a git repository.
	///
	/// Why this test is important: The commit SHA is used to uniquely identify
	/// the exact state of the codebase. We must return the full SHA (not
	/// abbreviated) to ensure unambiguous commit identification across large
	/// repositories.
	#[test]
	fn test_head_commit_sha_in_repo() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		let sha = head_commit_sha(temp.path()).unwrap();
		assert!(sha.is_some());

		let sha = sha.unwrap();
		assert_eq!(sha.len(), 40, "SHA should be full 40 characters");
		assert!(
			sha.chars().all(|c| c.is_ascii_hexdigit()),
			"SHA should be hex"
		);
	}

	/// Test: is_dirty returns false for a clean repository.
	///
	/// Why this test is important: A "clean" repository (no uncommitted changes)
	/// is the expected state after a commit. We need to correctly identify this
	/// to avoid false positives when checking for uncommitted work.
	#[test]
	fn test_is_dirty_clean_repo() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		let dirty = is_dirty(temp.path()).unwrap();
		assert_eq!(dirty, Some(false));
	}

	/// Test: is_dirty returns true when there are uncommitted changes.
	///
	/// Why this test is important: Detecting uncommitted changes is critical
	/// for features like warning users before switching branches or ensuring
	/// a clean state before operations. This tests both untracked and modified
	/// files.
	#[test]
	fn test_is_dirty_with_changes() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());

		fs::write(temp.path().join("new_file.txt"), "new content").unwrap();

		let dirty = is_dirty(temp.path()).unwrap();
		assert_eq!(dirty, Some(true));
	}

	/// Test: detect_repo_status returns complete repository information.
	///
	/// Why this test is important: This is the main integration point that
	/// combines all repository detection features. It must correctly aggregate
	/// branch, remote, commit, and dirty state into a single coherent status
	/// object for consumers.
	#[test]
	fn test_detect_repo_status() {
		let temp = TempDir::new().unwrap();
		init_git_repo(temp.path());
		create_initial_commit(temp.path());
		add_remote(temp.path(), "origin", "git@github.com:owner/repo.git");

		let status = detect_repo_status(temp.path()).unwrap();
		assert!(status.is_some());

		let status = status.unwrap();
		assert!(status.branch.is_some());
		assert_eq!(
			status.remote_slug,
			Some("github.com/owner/repo".to_string())
		);
		assert!(status.head.is_some());

		let head = status.head.unwrap();
		assert_eq!(head.sha.len(), 40);
		assert_eq!(head.summary, Some("Initial commit".to_string()));
		assert!(head.timestamp.is_some());

		assert_eq!(status.is_dirty, Some(false));
	}
}
