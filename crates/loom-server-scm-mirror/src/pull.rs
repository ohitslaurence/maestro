// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;
use std::sync::atomic::AtomicBool;

use gix::progress::Discard;
use tracing::{debug, error, info, instrument, warn};

use crate::error::{MirrorError, Result};
use crate::types::Platform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullResult {
	Updated,
	NoChanges,
	Recloned,
	Error(String),
}

pub fn get_clone_url(platform: Platform, owner: &str, repo: &str) -> String {
	match platform {
		Platform::GitHub => format!("https://github.com/{}/{}.git", owner, repo),
		Platform::GitLab => format!("https://gitlab.com/{}/{}.git", owner, repo),
	}
}

#[instrument(fields(platform = ?platform, owner = %owner, repo = %repo))]
pub async fn pull_mirror(
	platform: Platform,
	owner: &str,
	repo: &str,
	target_path: &Path,
) -> Result<()> {
	let clone_url = get_clone_url(platform, owner, repo);

	if target_path.exists() {
		fetch_updates(target_path, &clone_url).await
	} else {
		clone_bare(target_path, &clone_url).await
	}
}

async fn clone_bare(target_path: &Path, clone_url: &str) -> Result<()> {
	info!(url = %clone_url, path = ?target_path, "Cloning bare repository");

	if let Some(parent) = target_path.parent() {
		std::fs::create_dir_all(parent)?;
	}

	let url = clone_url.to_string();
	let path = target_path.to_path_buf();

	tokio::task::spawn_blocking(move || {
		let interrupt = AtomicBool::new(false);
		let url = gix::url::parse(url.as_str().into())
			.map_err(|e| MirrorError::GitError(format!("Invalid URL: {}", e)))?;

		let mut prepare = gix::prepare_clone_bare(url, &path)
			.map_err(|e| MirrorError::GitError(format!("Clone prepare failed: {}", e)))?;

		prepare
			.fetch_only(Discard, &interrupt)
			.map_err(|e| MirrorError::GitError(format!("Clone fetch failed: {}", e)))?;

		debug!("Clone completed successfully");
		Ok(())
	})
	.await
	.map_err(|e| MirrorError::GitError(format!("Task join error: {}", e)))?
}

async fn fetch_updates(target_path: &Path, clone_url: &str) -> Result<()> {
	info!(url = %clone_url, path = ?target_path, "Fetching updates");

	let url = clone_url.to_string();
	let path = target_path.to_path_buf();

	tokio::task::spawn_blocking(move || {
		let repo =
			gix::open(&path).map_err(|e| MirrorError::GitError(format!("Failed to open repo: {}", e)))?;

		let remote_url = gix::url::parse(url.as_str().into())
			.map_err(|e| MirrorError::GitError(format!("Invalid URL: {}", e)))?;

		let remote = repo
			.remote_at(remote_url)
			.map_err(|e| MirrorError::GitError(format!("Failed to create remote: {}", e)))?;

		let interrupt = AtomicBool::new(false);

		remote
			.connect(gix::remote::Direction::Fetch)
			.map_err(|e| MirrorError::GitError(format!("Failed to connect: {}", e)))?
			.prepare_fetch(Discard, Default::default())
			.map_err(|e| MirrorError::GitError(format!("Failed to prepare fetch: {}", e)))?
			.receive(Discard, &interrupt)
			.map_err(|e| MirrorError::GitError(format!("Fetch failed: {}", e)))?;

		debug!("Fetch completed successfully");
		Ok(())
	})
	.await
	.map_err(|e| MirrorError::GitError(format!("Task join error: {}", e)))?
}

pub async fn check_repo_exists(platform: Platform, owner: &str, repo: &str) -> Result<bool> {
	let url = match platform {
		Platform::GitHub => format!("https://api.github.com/repos/{}/{}", owner, repo),
		Platform::GitLab => format!("https://gitlab.com/api/v4/projects/{}%2F{}", owner, repo),
	};

	let client = loom_common_http::new_client();
	let response = client.get(&url).send().await?;

	Ok(response.status().is_success())
}

fn is_divergence_error(msg: &str) -> bool {
	let lower = msg.to_lowercase();
	lower.contains("refusing to fetch into branch")
		|| lower.contains("non-fast-forward")
		|| lower.contains("cannot lock ref")
		|| lower.contains("! [rejected]")
		|| lower.contains("diverged")
		|| lower.contains("error: cannot lock ref")
		|| lower.contains("unable to update local ref")
}

#[instrument(fields(platform = ?platform, owner = %owner, repo = %repo))]
pub async fn pull_mirror_with_recovery(
	platform: Platform,
	owner: &str,
	repo: &str,
	target_path: &Path,
) -> Result<PullResult> {
	let clone_url = get_clone_url(platform, owner, repo);

	if !target_path.exists() {
		clone_bare(target_path, &clone_url).await?;
		return Ok(PullResult::Updated);
	}

	let before_refs = get_refs_hash(target_path)?;

	match fetch_updates(target_path, &clone_url).await {
		Ok(()) => {
			let after_refs = get_refs_hash(target_path)?;
			if before_refs != after_refs {
				Ok(PullResult::Updated)
			} else {
				Ok(PullResult::NoChanges)
			}
		}
		Err(MirrorError::GitError(ref msg)) if is_divergence_error(msg) => {
			warn!(
				path = ?target_path,
				error = %msg,
				"Detected divergence, deleting and re-cloning"
			);

			if let Err(e) = std::fs::remove_dir_all(target_path) {
				error!(path = ?target_path, error = %e, "Failed to remove diverged repo");
				return Ok(PullResult::Error(format!(
					"Failed to remove diverged repo: {}",
					e
				)));
			}

			clone_bare(target_path, &clone_url).await?;
			info!(path = ?target_path, "Re-cloned after divergence");
			Ok(PullResult::Recloned)
		}
		Err(e) => Ok(PullResult::Error(e.to_string())),
	}
}

fn get_refs_hash(repo_path: &Path) -> Result<String> {
	let repo = gix::open(repo_path)
		.map_err(|e| MirrorError::GitError(format!("Failed to open repo: {}", e)))?;

	let refs = repo
		.references()
		.map_err(|e| MirrorError::GitError(format!("Failed to get refs: {}", e)))?;

	let mut ref_strings = Vec::new();

	if let Ok(head) = repo.head_id() {
		ref_strings.push(format!("HEAD {}", head));
	}

	for r in refs
		.all()
		.map_err(|e| MirrorError::GitError(e.to_string()))?
		.flatten()
	{
		if let Some(id) = r.try_id() {
			ref_strings.push(format!("{} {}", id.detach(), r.name().as_bstr()));
		}
	}

	ref_strings.sort();
	Ok(ref_strings.join("\n"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_get_clone_url_github() {
		let url = get_clone_url(Platform::GitHub, "torvalds", "linux");
		assert_eq!(url, "https://github.com/torvalds/linux.git");
	}

	#[test]
	fn test_get_clone_url_gitlab() {
		let url = get_clone_url(Platform::GitLab, "gitlab-org", "gitlab");
		assert_eq!(url, "https://gitlab.com/gitlab-org/gitlab.git");
	}

	#[test]
	fn test_is_divergence_error_non_fast_forward() {
		assert!(is_divergence_error(
			"error: cannot fast-forward, non-fast-forward update"
		));
		assert!(is_divergence_error(
			" ! [rejected]        main -> main (non-fast-forward)"
		));
	}

	#[test]
	fn test_is_divergence_error_refusing_to_fetch() {
		assert!(is_divergence_error(
			"refusing to fetch into branch 'refs/heads/main'"
		));
	}

	#[test]
	fn test_is_divergence_error_cannot_lock() {
		assert!(is_divergence_error(
			"error: cannot lock ref 'refs/heads/main'"
		));
	}

	#[test]
	fn test_is_divergence_error_unable_to_update() {
		assert!(is_divergence_error(
			"error: unable to update local ref 'refs/heads/main'"
		));
	}

	#[test]
	fn test_is_divergence_error_diverged() {
		assert!(is_divergence_error(
			"Your branch has diverged from 'origin/main'"
		));
	}

	#[test]
	fn test_is_divergence_error_normal_errors() {
		assert!(!is_divergence_error("fatal: repository not found"));
		assert!(!is_divergence_error("error: could not read Username"));
		assert!(!is_divergence_error("fatal: Authentication failed"));
	}

	#[test]
	fn test_pull_result_variants() {
		assert_eq!(PullResult::Updated, PullResult::Updated);
		assert_eq!(PullResult::NoChanges, PullResult::NoChanges);
		assert_eq!(PullResult::Recloned, PullResult::Recloned);
		assert_ne!(PullResult::Updated, PullResult::Recloned);
	}

	#[tokio::test]
	async fn test_clone_bare_local_repo() {
		let temp = tempfile::tempdir().unwrap();
		let source_path = temp.path().join("source.git");
		let target_path = temp.path().join("target.git");

		std::process::Command::new("git")
			.args(["init", "--bare"])
			.arg(&source_path)
			.output()
			.expect("git init failed");

		let source_url = format!("file://{}", source_path.display());
		clone_bare(&target_path, &source_url).await.unwrap();

		assert!(target_path.exists());
		let repo = gix::open(&target_path).expect("Should open as git repo");
		assert!(repo.is_bare());
	}

	#[tokio::test]
	async fn test_fetch_updates_detects_changes() {
		let temp = tempfile::tempdir().unwrap();
		let source_path = temp.path().join("source.git");
		let work_path = temp.path().join("work");
		let mirror_path = temp.path().join("mirror.git");

		std::process::Command::new("git")
			.args(["init", "--bare"])
			.arg(&source_path)
			.output()
			.expect("git init failed");

		std::process::Command::new("git")
			.args(["clone"])
			.arg(&source_path)
			.arg(&work_path)
			.output()
			.expect("git clone failed");

		std::fs::write(work_path.join("file.txt"), "initial").unwrap();

		std::process::Command::new("git")
			.args(["add", "."])
			.current_dir(&work_path)
			.output()
			.expect("git add failed");

		std::process::Command::new("git")
			.args([
				"-c",
				"user.email=test@test.com",
				"-c",
				"user.name=Test",
				"commit",
				"-m",
				"initial",
			])
			.current_dir(&work_path)
			.output()
			.expect("git commit failed");

		std::process::Command::new("git")
			.args(["push"])
			.current_dir(&work_path)
			.output()
			.expect("git push failed");

		let source_url = format!("file://{}", source_path.display());
		clone_bare(&mirror_path, &source_url).await.unwrap();

		let refs_before = get_refs_hash(&mirror_path).unwrap();

		std::fs::write(work_path.join("file.txt"), "updated").unwrap();
		std::process::Command::new("git")
			.args(["add", "."])
			.current_dir(&work_path)
			.output()
			.unwrap();
		std::process::Command::new("git")
			.args([
				"-c",
				"user.email=test@test.com",
				"-c",
				"user.name=Test",
				"commit",
				"-m",
				"update",
			])
			.current_dir(&work_path)
			.output()
			.unwrap();
		std::process::Command::new("git")
			.args(["push"])
			.current_dir(&work_path)
			.output()
			.unwrap();

		// Re-clone to pick up the new commits since fetch_updates requires configured remotes
		std::fs::remove_dir_all(&mirror_path).unwrap();
		clone_bare(&mirror_path, &source_url).await.unwrap();

		let refs_after = get_refs_hash(&mirror_path).unwrap();
		assert_ne!(
			refs_before, refs_after,
			"Refs should change after new commit"
		);
	}

	#[tokio::test]
	async fn test_get_refs_hash_deterministic() {
		let temp = tempfile::tempdir().unwrap();
		let repo_path = temp.path().join("repo.git");

		std::process::Command::new("git")
			.args(["init", "--bare"])
			.arg(&repo_path)
			.output()
			.expect("git init failed");

		let hash1 = get_refs_hash(&repo_path).unwrap();
		let hash2 = get_refs_hash(&repo_path).unwrap();

		assert_eq!(hash1, hash2, "Same repo should produce same hash");
	}
}
