// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Request and response types for GitHub App API operations.

use serde::{Deserialize, Serialize};

fn default_per_page() -> u32 {
	30
}

fn default_page() -> u32 {
	1
}

/// Code search request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchRequest {
	/// Search query (GitHub search syntax).
	pub query: String,
	/// Repository owner.
	pub owner: String,
	/// Repository name.
	pub repo: String,
	/// Results per page (max 100, default 30).
	#[serde(default = "default_per_page")]
	pub per_page: u32,
	/// Page number (default 1).
	#[serde(default = "default_page")]
	pub page: u32,
}

impl CodeSearchRequest {
	/// Create a new code search request.
	pub fn new(query: impl Into<String>, owner: impl Into<String>, repo: impl Into<String>) -> Self {
		Self {
			query: query.into(),
			owner: owner.into(),
			repo: repo.into(),
			per_page: default_per_page(),
			page: default_page(),
		}
	}

	/// Set the number of results per page.
	pub fn with_per_page(mut self, per_page: u32) -> Self {
		self.per_page = per_page.clamp(1, 100);
		self
	}

	/// Set the page number.
	pub fn with_page(mut self, page: u32) -> Self {
		self.page = page.max(1);
		self
	}

	/// Build the full GitHub search query string.
	pub fn to_github_query(&self) -> String {
		format!("{} repo:{}/{}", self.query, self.owner, self.repo)
	}
}

/// Code search response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchResponse {
	/// Total number of matching results.
	pub total_count: u32,
	/// Whether results are incomplete (rate limiting, etc.).
	pub incomplete_results: bool,
	/// Search result items.
	pub items: Vec<CodeSearchItem>,
}

/// Single code search result item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchItem {
	/// File name.
	pub name: String,
	/// Full path within repository.
	pub path: String,
	/// Git blob SHA.
	pub sha: String,
	/// URL to view on GitHub.
	pub html_url: String,
	/// Full repository name (owner/repo).
	pub repository_full_name: String,
	/// Search relevance score.
	pub score: f64,
}

/// Repository info request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfoRequest {
	/// Repository owner.
	pub owner: String,
	/// Repository name.
	pub repo: String,
}

impl RepoInfoRequest {
	/// Create a new repository info request.
	pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
		Self {
			owner: owner.into(),
			repo: repo.into(),
		}
	}
}

/// Repository metadata.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
	/// GitHub repository ID.
	pub id: i64,
	/// Full repository name (owner/repo).
	pub full_name: String,
	/// Repository description.
	pub description: Option<String>,
	/// Whether the repository is private.
	pub private: bool,
	/// Default branch name.
	pub default_branch: String,
	/// Primary language.
	pub language: Option<String>,
	/// Number of stars.
	pub stargazers_count: u32,
	/// URL to view on GitHub.
	pub html_url: String,
}

/// File contents request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentsRequest {
	/// Repository owner.
	pub owner: String,
	/// Repository name.
	pub repo: String,
	/// File path within repository.
	pub path: String,
	/// Git ref (branch, tag, or SHA). Defaults to default branch.
	#[serde(rename = "ref")]
	pub git_ref: Option<String>,
}

impl FileContentsRequest {
	/// Create a new file contents request.
	pub fn new(owner: impl Into<String>, repo: impl Into<String>, path: impl Into<String>) -> Self {
		Self {
			owner: owner.into(),
			repo: repo.into(),
			path: path.into(),
			git_ref: None,
		}
	}

	/// Set the git ref (branch, tag, or SHA).
	pub fn with_ref(mut self, git_ref: impl Into<String>) -> Self {
		self.git_ref = Some(git_ref.into());
		self
	}
}

/// File contents response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContents {
	/// File name.
	pub name: String,
	/// Full path within repository.
	pub path: String,
	/// Git blob SHA.
	pub sha: String,
	/// File size in bytes.
	pub size: u64,
	/// Content encoding (usually "base64").
	pub encoding: String,
	/// Encoded file content.
	pub content: String,
}

impl FileContents {
	/// Decode the base64 content to bytes.
	pub fn decode_content(&self) -> Result<Vec<u8>, base64::DecodeError> {
		use base64::{engine::general_purpose::STANDARD, Engine};
		let content_no_newlines: String = self
			.content
			.chars()
			.filter(|c| !c.is_whitespace())
			.collect();
		STANDARD.decode(content_no_newlines)
	}

	/// Decode the base64 content to a UTF-8 string.
	pub fn decode_content_string(&self) -> Result<String, Box<dyn std::error::Error>> {
		let bytes = self.decode_content()?;
		Ok(String::from_utf8(bytes)?)
	}
}

/// GitHub App installation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
	/// Installation ID.
	pub id: i64,
	/// Account that owns the installation.
	pub account: InstallationAccount,
	/// Repository selection mode ("all" or "selected").
	pub repository_selection: String,
	/// Suspension timestamp (ISO8601) if suspended.
	pub suspended_at: Option<String>,
}

/// Account that owns a GitHub App installation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationAccount {
	/// Account ID.
	pub id: i64,
	/// Account login name.
	pub login: String,
	/// Account type ("User" or "Organization").
	#[serde(rename = "type")]
	pub account_type: String,
}

/// Access token response from GitHub.
#[derive(Debug, Clone, Deserialize)]
pub struct AccessTokenResponse {
	/// The installation access token.
	pub token: String,
	/// Token expiration timestamp (ISO8601).
	pub expires_at: String,
}

/// GitHub API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubErrorResponse {
	/// Error message.
	pub message: String,
	/// Documentation URL.
	pub documentation_url: Option<String>,
}

/// Webhook payload for installation events.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationWebhookPayload {
	/// The action that triggered the webhook.
	pub action: String,
	/// The installation.
	pub installation: Installation,
	/// Repositories affected (for some events).
	#[serde(default)]
	pub repositories: Vec<WebhookRepository>,
	/// Repositories added (for installation_repositories events).
	#[serde(default)]
	pub repositories_added: Vec<WebhookRepository>,
	/// Repositories removed (for installation_repositories events).
	#[serde(default)]
	pub repositories_removed: Vec<WebhookRepository>,
}

/// Repository information in webhook payloads.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRepository {
	/// Repository ID.
	pub id: i64,
	/// Repository name (not full name).
	pub name: String,
	/// Full repository name (owner/repo).
	pub full_name: String,
	/// Whether the repository is private.
	pub private: bool,
}

/// Server response for app info endpoint.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfoResponse {
	/// Whether the GitHub App is configured.
	pub configured: bool,
	/// App slug (if configured).
	pub app_slug: Option<String>,
	/// Installation URL (if configured).
	pub installation_url: Option<String>,
}

/// Server response for installation status by repo.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationStatusResponse {
	/// Whether the app is installed for this repo.
	pub installed: bool,
	/// Installation ID (if installed).
	pub installation_id: Option<i64>,
	/// Account login (if installed).
	pub account_login: Option<String>,
	/// Account type (if installed).
	pub account_type: Option<String>,
	/// Repository selection mode (if installed).
	pub repositories_selection: Option<String>,
}

impl InstallationStatusResponse {
	/// Create a response indicating the app is not installed.
	pub fn not_installed() -> Self {
		Self {
			installed: false,
			installation_id: None,
			account_login: None,
			account_type: None,
			repositories_selection: None,
		}
	}

	/// Create a response indicating the app is installed.
	pub fn installed(
		installation_id: i64,
		account_login: String,
		account_type: String,
		repositories_selection: String,
	) -> Self {
		Self {
			installed: true,
			installation_id: Some(installation_id),
			account_login: Some(account_login),
			account_type: Some(account_type),
			repositories_selection: Some(repositories_selection),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_code_search_request_to_github_query() {
		let request = CodeSearchRequest::new("struct Config", "my-org", "my-repo");
		assert_eq!(
			request.to_github_query(),
			"struct Config repo:my-org/my-repo"
		);
	}

	#[test]
	fn test_code_search_request_with_per_page() {
		let request = CodeSearchRequest::new("test", "owner", "repo").with_per_page(50);
		assert_eq!(request.per_page, 50);
	}

	#[test]
	fn test_code_search_request_clamps_per_page() {
		let request = CodeSearchRequest::new("test", "owner", "repo").with_per_page(200);
		assert_eq!(request.per_page, 100);
	}

	#[test]
	fn test_file_contents_request_with_ref() {
		let request = FileContentsRequest::new("owner", "repo", "src/main.rs").with_ref("develop");
		assert_eq!(request.git_ref, Some("develop".to_string()));
	}

	#[test]
	fn test_installation_status_not_installed() {
		let response = InstallationStatusResponse::not_installed();
		assert!(!response.installed);
		assert!(response.installation_id.is_none());
	}

	#[test]
	fn test_installation_status_installed() {
		let response = InstallationStatusResponse::installed(
			12345,
			"my-org".to_string(),
			"Organization".to_string(),
			"selected".to_string(),
		);
		assert!(response.installed);
		assert_eq!(response.installation_id, Some(12345));
		assert_eq!(response.account_login, Some("my-org".to_string()));
	}
}
