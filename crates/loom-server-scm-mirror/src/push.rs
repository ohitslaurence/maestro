// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

// NOTE: Push operations use git subprocess because gitoxide doesn't yet support push.
// See: https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md
// Track progress at: https://github.com/GitoxideLabs/gitoxide/issues/307

use std::path::Path;
use std::process::Command;

use loom_cli_credentials::{CredentialStore, CredentialValue};
use tracing::{debug, error, info, instrument};

use crate::error::{MirrorError, Result};
use crate::types::{MirrorBranchRule, PushMirror};

#[instrument(skip(credentials), fields(mirror_id = %mirror.id, repo_id = %mirror.repo_id))]
pub async fn push_mirror(
	repo_path: &Path,
	mirror: &PushMirror,
	credentials: &impl CredentialStore,
	branch_rules: &[MirrorBranchRule],
) -> Result<()> {
	if !mirror.enabled {
		debug!("Mirror is disabled, skipping");
		return Ok(());
	}

	let creds = credentials
		.load(&mirror.credential_key)
		.await
		.map_err(|e| MirrorError::CredentialNotFound(e.to_string()))?
		.ok_or_else(|| MirrorError::CredentialNotFound(mirror.credential_key.clone()))?;

	let remote_url = build_authenticated_url(&mirror.remote_url, &creds)?;

	let active_rules: Vec<_> = branch_rules.iter().filter(|r| r.enabled).collect();

	if active_rules.is_empty() {
		push_all_refs(repo_path, &remote_url).await?;
	} else {
		push_matching_refs(repo_path, &remote_url, &active_rules).await?;
	}

	info!(mirror_id = %mirror.id, "Push mirror completed");
	Ok(())
}

fn build_authenticated_url(remote_url: &str, creds: &CredentialValue) -> Result<String> {
	let (username, password) = match creds {
		CredentialValue::ApiKey { key } => ("git", key.expose().to_string()),
		CredentialValue::OAuth { access, .. } => ("oauth2", access.expose().to_string()),
	};

	let url = if remote_url.starts_with("https://") {
		let without_scheme = remote_url.strip_prefix("https://").unwrap();
		format!("https://{}:{}@{}", username, password, without_scheme)
	} else if remote_url.starts_with("http://") {
		let without_scheme = remote_url.strip_prefix("http://").unwrap();
		format!("http://{}:{}@{}", username, password, without_scheme)
	} else {
		return Err(MirrorError::InvalidUrl(format!(
			"unsupported URL scheme: {}",
			remote_url
		)));
	};

	Ok(url)
}

async fn push_all_refs(repo_path: &Path, remote_url: &str) -> Result<()> {
	debug!("Pushing all refs with --mirror");

	let output = Command::new("git")
		.args(["push", "--mirror", remote_url])
		.current_dir(repo_path)
		.output()?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		error!(error = %stderr, "Git push --mirror failed");
		return Err(MirrorError::GitError(stderr.to_string()));
	}

	Ok(())
}

async fn push_matching_refs(
	repo_path: &Path,
	remote_url: &str,
	rules: &[&MirrorBranchRule],
) -> Result<()> {
	let branches = list_branches(repo_path)?;

	let matching_branches: Vec<_> = branches
		.iter()
		.filter(|branch| {
			rules
				.iter()
				.any(|rule| matches_pattern(branch, &rule.pattern))
		})
		.collect();

	if matching_branches.is_empty() {
		debug!("No branches match the mirror rules");
		return Ok(());
	}

	for branch in matching_branches {
		debug!(branch = %branch, "Pushing branch");
		let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);

		let output = Command::new("git")
			.args(["push", remote_url, &refspec])
			.current_dir(repo_path)
			.output()?;

		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			error!(branch = %branch, error = %stderr, "Git push failed");
			return Err(MirrorError::GitError(stderr.to_string()));
		}
	}

	Ok(())
}

fn list_branches(repo_path: &Path) -> Result<Vec<String>> {
	let repo = gix::open(repo_path)
		.map_err(|e| MirrorError::GitError(format!("Failed to open repo: {}", e)))?;

	let refs = repo
		.references()
		.map_err(|e| MirrorError::GitError(format!("Failed to get refs: {}", e)))?;

	let branches = refs
		.prefixed("refs/heads/")
		.map_err(|e| MirrorError::GitError(format!("Failed to list branches: {}", e)))?;

	let mut result = Vec::new();
	for reference in branches.flatten() {
		let name = reference.name().shorten().to_string();
		result.push(name);
	}

	Ok(result)
}

fn matches_pattern(branch: &str, pattern: &str) -> bool {
	if pattern == "*" {
		return true;
	}

	if let Some(prefix) = pattern.strip_suffix("/*") {
		return branch.starts_with(&format!("{}/", prefix));
	}

	branch == pattern
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_secret::SecretString;
	use proptest::prelude::*;
	use std::process::Command;
	use tempfile::TempDir;

	proptest! {
		/// **Property: Exact match is reflexive**
		/// Any branch must match itself as a pattern.
		#[test]
		fn prop_exact_match_reflexive(branch in "[a-zA-Z][a-zA-Z0-9_-]{0,30}") {
			prop_assert!(matches_pattern(&branch, &branch));
		}

		/// **Property: Wildcard matches everything**
		/// The pattern "*" must match any valid branch name.
		#[test]
		fn prop_wildcard_matches_all(branch in "[a-zA-Z][a-zA-Z0-9_/-]{0,50}") {
			prop_assert!(matches_pattern(&branch, "*"));
		}

		/// **Property: Prefix pattern matches correctly**
		/// A branch "prefix/suffix" must match pattern "prefix/*".
		#[test]
		fn prop_prefix_pattern_matches(
			prefix in "[a-zA-Z][a-zA-Z0-9_-]{0,15}",
			suffix in "[a-zA-Z0-9_-]{1,20}"
		) {
			let branch = format!("{}/{}", prefix, suffix);
			let pattern = format!("{}/*", prefix);
			prop_assert!(matches_pattern(&branch, &pattern));
		}

		/// **Property: Non-matching prefix rejected**
		/// A branch with different prefix must not match a prefix/* pattern.
		#[test]
		fn prop_different_prefix_rejected(
			prefix1 in "[a-z]{3,10}",
			prefix2 in "[A-Z]{3,10}",
			suffix in "[a-zA-Z0-9]{1,10}"
		) {
			// Use different cases to ensure prefixes differ
			let branch = format!("{}/{}", prefix1, suffix);
			let pattern = format!("{}/*", prefix2);
			prop_assert!(!matches_pattern(&branch, &pattern));
		}
	}

	#[test]
	fn test_matches_pattern_exact() {
		assert!(matches_pattern("cannon", "cannon"));
		assert!(!matches_pattern("main", "cannon"));
	}

	#[test]
	fn test_matches_pattern_wildcard() {
		assert!(matches_pattern("release/v1.0", "release/*"));
		assert!(matches_pattern("release/v2.0-beta", "release/*"));
		assert!(!matches_pattern("feature/foo", "release/*"));
	}

	#[test]
	fn test_matches_pattern_all() {
		assert!(matches_pattern("any-branch", "*"));
		assert!(matches_pattern("feature/foo", "*"));
	}

	#[test]
	fn test_build_authenticated_url_api_key_https() {
		let creds = CredentialValue::ApiKey {
			key: SecretString::new("mytoken".to_string()),
		};
		let result = build_authenticated_url("https://github.com/test/repo.git", &creds).unwrap();
		assert_eq!(result, "https://git:mytoken@github.com/test/repo.git");
	}

	#[test]
	fn test_build_authenticated_url_oauth_http() {
		let creds = CredentialValue::OAuth {
			access: SecretString::new("oauthtoken".to_string()),
			refresh: SecretString::new("refreshtoken".to_string()),
			expires: 0,
		};
		let result = build_authenticated_url("http://host/path.git", &creds).unwrap();
		assert_eq!(result, "http://oauth2:oauthtoken@host/path.git");
	}

	#[test]
	fn test_build_authenticated_url_invalid_scheme() {
		let creds = CredentialValue::ApiKey {
			key: SecretString::new("token".to_string()),
		};
		let result = build_authenticated_url("ssh://host/repo.git", &creds);
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(matches!(err, MirrorError::InvalidUrl(_)));
		assert!(err.to_string().contains("unsupported"));
	}

	#[test]
	fn test_list_branches_returns_branches() {
		let temp_dir = TempDir::new().unwrap();
		let repo_path = temp_dir.path();

		Command::new("git")
			.args(["init", "--bare"])
			.current_dir(repo_path)
			.output()
			.expect("git init failed");

		std::fs::create_dir_all(repo_path.join("refs/heads")).unwrap();

		let dummy_sha = "0000000000000000000000000000000000000001";
		std::fs::write(
			repo_path.join("refs/heads/main"),
			format!("{}\n", dummy_sha),
		)
		.unwrap();
		std::fs::write(
			repo_path.join("refs/heads/feature-branch"),
			format!("{}\n", dummy_sha),
		)
		.unwrap();

		let branches = list_branches(repo_path).unwrap();

		assert!(branches.contains(&"main".to_string()));
		assert!(branches.contains(&"feature-branch".to_string()));
		assert_eq!(branches.len(), 2);
	}
}
