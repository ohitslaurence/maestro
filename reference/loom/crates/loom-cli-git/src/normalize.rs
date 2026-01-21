// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

/// Normalizes a git remote URL to a canonical form: `host/owner/repo`.
///
/// Handles:
/// - SCP-style SSH: `git@github.com:owner/repo.git` -> `github.com/owner/repo`
/// - HTTPS URLs: `https://github.com/owner/repo.git` -> `github.com/owner/repo`
/// - SSH URLs: `ssh://git@gitlab.com/group/repo.git` -> `gitlab.com/group/repo`
///
/// Returns `None` for local paths or unparseable URLs.
pub fn normalize_remote_url(raw: &str) -> Option<String> {
	let raw = raw.trim();

	if raw.is_empty() {
		return None;
	}

	if raw.starts_with('/') || raw.starts_with('.') {
		tracing::debug!(url = raw, "skipping local path");
		return None;
	}

	if let Some(result) = try_parse_scp_style(raw) {
		return Some(result);
	}

	if let Some(result) = try_parse_url_style(raw) {
		return Some(result);
	}

	tracing::debug!(url = raw, "could not parse remote URL");
	None
}

fn try_parse_scp_style(raw: &str) -> Option<String> {
	if !raw.contains(':') || raw.contains("://") {
		return None;
	}

	let (user_host, path) = raw.split_once(':')?;
	let host = user_host.split('@').next_back()?;

	if host.is_empty() || path.is_empty() {
		return None;
	}

	let path = strip_git_suffix(path);
	let host = host.to_lowercase();

	tracing::trace!(original = raw, normalized = %format!("{host}/{path}"), "parsed SCP-style URL");
	Some(format!("{host}/{path}"))
}

fn try_parse_url_style(raw: &str) -> Option<String> {
	let without_scheme = raw
		.strip_prefix("https://")
		.or_else(|| raw.strip_prefix("http://"))
		.or_else(|| raw.strip_prefix("ssh://"))
		.or_else(|| raw.strip_prefix("git://"))?;

	let without_auth = if let Some(at_pos) = without_scheme.find('@') {
		if let Some(slash_pos) = without_scheme.find('/') {
			if at_pos < slash_pos {
				&without_scheme[at_pos + 1..]
			} else {
				without_scheme
			}
		} else {
			&without_scheme[at_pos + 1..]
		}
	} else {
		without_scheme
	};

	let (host, path) = without_auth.split_once('/')?;

	if host.is_empty() || path.is_empty() {
		return None;
	}

	let host_without_port = host.split(':').next().unwrap_or(host);
	let path = strip_git_suffix(path);
	let host = host_without_port.to_lowercase();

	tracing::trace!(original = raw, normalized = %format!("{host}/{path}"), "parsed URL-style remote");
	Some(format!("{host}/{path}"))
}

fn strip_git_suffix(path: &str) -> &str {
	path.strip_suffix(".git").unwrap_or(path)
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_scp_style_github() {
		assert_eq!(
			normalize_remote_url("git@github.com:owner/repo.git"),
			Some("github.com/owner/repo".to_string())
		);
	}

	#[test]
	fn test_scp_style_without_git_suffix() {
		assert_eq!(
			normalize_remote_url("git@github.com:owner/repo"),
			Some("github.com/owner/repo".to_string())
		);
	}

	#[test]
	fn test_https_github() {
		assert_eq!(
			normalize_remote_url("https://github.com/owner/repo.git"),
			Some("github.com/owner/repo".to_string())
		);
	}

	#[test]
	fn test_https_without_git_suffix() {
		assert_eq!(
			normalize_remote_url("https://github.com/owner/repo"),
			Some("github.com/owner/repo".to_string())
		);
	}

	#[test]
	fn test_ssh_url_style() {
		assert_eq!(
			normalize_remote_url("ssh://git@gitlab.com/group/repo.git"),
			Some("gitlab.com/group/repo".to_string())
		);
	}

	#[test]
	fn test_ssh_url_with_port() {
		assert_eq!(
			normalize_remote_url("ssh://git@gitlab.com:22/group/repo.git"),
			Some("gitlab.com/group/repo".to_string())
		);
	}

	#[test]
	fn test_host_lowercased() {
		assert_eq!(
			normalize_remote_url("git@GitHub.COM:Owner/Repo.git"),
			Some("github.com/Owner/Repo".to_string())
		);
	}

	#[test]
	fn test_local_path_absolute() {
		assert_eq!(normalize_remote_url("/path/to/repo"), None);
	}

	#[test]
	fn test_local_path_relative() {
		assert_eq!(normalize_remote_url("./path/to/repo"), None);
		assert_eq!(normalize_remote_url("../path/to/repo"), None);
	}

	#[test]
	fn test_empty_string() {
		assert_eq!(normalize_remote_url(""), None);
	}

	#[test]
	fn test_whitespace_only() {
		assert_eq!(normalize_remote_url("   "), None);
	}

	#[test]
	fn test_nested_group_path() {
		assert_eq!(
			normalize_remote_url("git@gitlab.com:group/subgroup/repo.git"),
			Some("gitlab.com/group/subgroup/repo".to_string())
		);
	}

	#[test]
	fn test_http_url() {
		assert_eq!(
			normalize_remote_url("http://github.com/owner/repo.git"),
			Some("github.com/owner/repo".to_string())
		);
	}

	#[test]
	fn test_git_protocol() {
		assert_eq!(
			normalize_remote_url("git://github.com/owner/repo.git"),
			Some("github.com/owner/repo".to_string())
		);
	}

	// Property: Normalization is idempotent.
	//
	// Why this test is important: Once a URL is normalized, re-normalizing it
	// should produce the same result. This ensures consistency when URLs may
	// be processed multiple times through the system.
	proptest! {
			#[test]
			fn prop_normalization_is_idempotent(
					host in "[a-z]{3,10}\\.[a-z]{2,4}",
					owner in "[a-zA-Z][a-zA-Z0-9_-]{0,20}",
					repo in "[a-zA-Z][a-zA-Z0-9_-]{0,20}"
			) {
					let normalized = format!("{host}/{owner}/{repo}");
					let https_url = format!("https://{normalized}");

					if let Some(first) = normalize_remote_url(&https_url) {
							let https_of_normalized = format!("https://{first}");
							let second = normalize_remote_url(&https_of_normalized);
							prop_assert_eq!(Some(first.clone()), second);
					}
			}
	}

	// Property: Different URL formats for the same repo normalize to the same slug.
	//
	// Why this test is important: Users may configure remotes using different
	// URL formats (SSH vs HTTPS). We need to detect they refer to the same
	// repository for features like workspace detection and remote matching.
	proptest! {
			#[test]
			fn prop_scp_and_https_normalize_same(
					host in "[a-z]{3,10}\\.[a-z]{2,4}",
					owner in "[a-zA-Z][a-zA-Z0-9_-]{1,10}",
					repo in "[a-zA-Z][a-zA-Z0-9_-]{1,10}"
			) {
					let scp_url = format!("git@{host}:{owner}/{repo}.git");
					let https_url = format!("https://{host}/{owner}/{repo}.git");

					let scp_normalized = normalize_remote_url(&scp_url);
					let https_normalized = normalize_remote_url(&https_url);

					prop_assert_eq!(scp_normalized, https_normalized);
			}
	}

	// Property: The .git suffix is always stripped from the result.
	//
	// Why this test is important: The .git suffix is optional and varies by
	// user preference. Stripping it ensures consistent comparison of
	// repositories regardless of how the remote was originally configured.
	proptest! {
			#[test]
			fn prop_git_suffix_always_stripped(
					host in "[a-z]{3,10}\\.[a-z]{2,4}",
					owner in "[a-zA-Z][a-zA-Z0-9_-]{1,10}",
					repo in "[a-zA-Z][a-zA-Z0-9_-]{1,10}"
			) {
					let url_with_git = format!("https://{host}/{owner}/{repo}.git");
					let url_without_git = format!("https://{host}/{owner}/{repo}");

					let with_git = normalize_remote_url(&url_with_git);
					let without_git = normalize_remote_url(&url_without_git);

					prop_assert_eq!(&with_git, &without_git);

					if let Some(ref normalized) = with_git {
							prop_assert!(!normalized.ends_with(".git"));
					}
			}
	}

	// Property: Local paths are never normalized (return None).
	//
	// Why this test is important: Local paths don't represent remote
	// repositories and should not be treated as remote slugs. This prevents
	// false matches when users have local-only repositories.
	proptest! {
			#[test]
			fn prop_local_paths_return_none(path in "/[a-z/]{1,50}") {
					prop_assert_eq!(normalize_remote_url(&path), None);
			}
	}

	// Property: Host is always lowercased in the output.
	//
	// Why this test is important: DNS names are case-insensitive, so
	// "GitHub.com" and "github.com" refer to the same host. Lowercasing
	// ensures consistent matching across different URL capitalizations.
	proptest! {
			#[test]
			fn prop_host_is_lowercased(
					host_base in "[a-zA-Z]{3,10}",
					tld in "[a-zA-Z]{2,4}",
					owner in "[a-zA-Z][a-zA-Z0-9_-]{1,10}",
					repo in "[a-zA-Z][a-zA-Z0-9_-]{1,10}"
			) {
					let host = format!("{host_base}.{tld}");
					let url = format!("https://{host}/{owner}/{repo}");

					if let Some(normalized) = normalize_remote_url(&url) {
							let normalized_host = normalized.split('/').next().unwrap();
							prop_assert_eq!(normalized_host, host.to_lowercase());
					}
			}
	}
}
