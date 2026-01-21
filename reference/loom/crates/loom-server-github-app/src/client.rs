// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! GitHub App client implementation with JWT authentication and token caching.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use loom_common_http::{retry, RetryConfig};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::config::GithubAppConfig;
use crate::error::GithubAppError;
use crate::jwt::generate_app_jwt;
use crate::types::{
	AccessTokenResponse, CodeSearchItem, CodeSearchRequest, CodeSearchResponse, FileContents,
	Installation, Repository,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TOKEN_REFRESH_MARGIN_SECS: u64 = 120;
const JWT_REFRESH_MARGIN_SECS: u64 = 30;
const JWT_VALIDITY_SECS: u64 = 9 * 60;

/// Cached token with expiration tracking.
struct CachedToken {
	token: String,
	expires_at: Instant,
}

impl CachedToken {
	fn new(token: String, valid_for: Duration) -> Self {
		Self {
			token,
			expires_at: Instant::now() + valid_for,
		}
	}

	fn is_valid(&self, margin: Duration) -> bool {
		Instant::now() + margin < self.expires_at
	}
}

/// Client for interacting with GitHub App API.
///
/// Handles JWT generation, installation token management, and GitHub API calls
/// with automatic retry logic, token caching, and 401-aware token refresh.
#[derive(Clone)]
pub struct GithubAppClient {
	http_client: Client,
	config: GithubAppConfig,
	/// Cached App JWT
	app_jwt_cache: Arc<Mutex<Option<CachedToken>>>,
	/// Cached installation tokens keyed by installation_id
	installation_token_cache: Arc<Mutex<HashMap<i64, CachedToken>>>,
	/// Lock for serializing App JWT generation
	app_jwt_lock: Arc<Mutex<()>>,
	/// Locks for serializing per-installation token fetches
	installation_locks: Arc<Mutex<HashMap<i64, Arc<Mutex<()>>>>>,
}

impl GithubAppClient {
	/// Create a new GitHub App client.
	pub fn new(config: GithubAppConfig) -> Result<Self, GithubAppError> {
		let http_client = loom_common_http::builder()
			.timeout(REQUEST_TIMEOUT)
			.build()
			.map_err(|e| GithubAppError::Config(format!("Failed to create HTTP client: {e}")))?;

		info!(
				app_id = config.app_id(),
				base_url = %config.base_url(),
				"Created GitHub App client"
		);

		Ok(Self {
			http_client,
			config,
			app_jwt_cache: Arc::new(Mutex::new(None)),
			installation_token_cache: Arc::new(Mutex::new(HashMap::new())),
			app_jwt_lock: Arc::new(Mutex::new(())),
			installation_locks: Arc::new(Mutex::new(HashMap::new())),
		})
	}

	/// Get the retry configuration.
	pub fn retry_config(&self) -> &RetryConfig {
		&self.config.retry_config
	}

	/// Get the webhook secret, if configured.
	pub fn webhook_secret(&self) -> Option<&str> {
		self.config.webhook_secret()
	}

	/// Get a lock for a specific installation to serialize token fetches.
	async fn get_installation_lock(&self, installation_id: i64) -> Arc<Mutex<()>> {
		let mut locks = self.installation_locks.lock().await;
		locks
			.entry(installation_id)
			.or_insert_with(|| Arc::new(Mutex::new(())))
			.clone()
	}

	/// Invalidate the cached installation token.
	async fn invalidate_installation_token(&self, installation_id: i64) {
		let mut cache = self.installation_token_cache.lock().await;
		if cache.remove(&installation_id).is_some() {
			info!(installation_id, "Invalidated installation token cache");
		}
	}

	/// Invalidate the cached App JWT.
	async fn invalidate_app_jwt(&self) {
		let mut cache = self.app_jwt_cache.lock().await;
		if cache.take().is_some() {
			info!("Invalidated App JWT cache");
		}
	}

	/// Get or generate an App JWT with deduplication.
	///
	/// Uses double-checked locking to prevent concurrent JWT generation.
	#[instrument(skip(self))]
	async fn get_app_jwt(&self) -> Result<String, GithubAppError> {
		// Fast path: check cache without lock
		{
			let cache = self.app_jwt_cache.lock().await;
			if let Some(ref cached) = *cache {
				if cached.is_valid(Duration::from_secs(JWT_REFRESH_MARGIN_SECS)) {
					trace!("Using cached App JWT");
					return Ok(cached.token.clone());
				}
			}
		}

		// Serialize JWT generation
		let _guard = self.app_jwt_lock.lock().await;

		// Double-check cache after acquiring lock
		{
			let cache = self.app_jwt_cache.lock().await;
			if let Some(ref cached) = *cache {
				if cached.is_valid(Duration::from_secs(JWT_REFRESH_MARGIN_SECS)) {
					trace!("Using cached App JWT (post-lock)");
					return Ok(cached.token.clone());
				}
			}
		}

		debug!(app_id = self.config.app_id(), "Generating new App JWT");
		let jwt = generate_app_jwt(self.config.app_id(), self.config.private_key_pem())?;

		let mut cache = self.app_jwt_cache.lock().await;
		*cache = Some(CachedToken::new(
			jwt.clone(),
			Duration::from_secs(JWT_VALIDITY_SECS),
		));

		Ok(jwt)
	}

	/// Get or refresh an installation access token with deduplication.
	///
	/// Uses per-installation locking to prevent concurrent token fetches.
	#[instrument(skip(self))]
	pub(crate) async fn get_installation_token(
		&self,
		installation_id: i64,
	) -> Result<String, GithubAppError> {
		// Fast path: check cache without lock
		{
			let cache = self.installation_token_cache.lock().await;
			if let Some(cached) = cache.get(&installation_id) {
				if cached.is_valid(Duration::from_secs(TOKEN_REFRESH_MARGIN_SECS)) {
					trace!(installation_id, "Using cached installation token");
					return Ok(cached.token.clone());
				}
			}
		}

		// Serialize token fetch per installation
		let lock = self.get_installation_lock(installation_id).await;
		let _guard = lock.lock().await;

		// Double-check cache after acquiring lock
		{
			let cache = self.installation_token_cache.lock().await;
			if let Some(cached) = cache.get(&installation_id) {
				if cached.is_valid(Duration::from_secs(TOKEN_REFRESH_MARGIN_SECS)) {
					trace!(
						installation_id,
						"Using cached installation token (post-lock)"
					);
					return Ok(cached.token.clone());
				}
			}
		}

		debug!(installation_id, "Fetching new installation token");
		let (token, valid_for) = self.fetch_installation_token(installation_id).await?;

		let mut cache = self.installation_token_cache.lock().await;
		cache.insert(installation_id, CachedToken::new(token.clone(), valid_for));

		info!(installation_id, "Installation token refreshed");
		Ok(token)
	}

	/// Fetch a new installation access token from GitHub.
	async fn fetch_installation_token(
		&self,
		installation_id: i64,
	) -> Result<(String, Duration), GithubAppError> {
		let jwt = self.get_app_jwt().await?;

		let url = self
			.config
			.base_url()
			.join(&format!(
				"app/installations/{installation_id}/access_tokens"
			))
			.map_err(|e| GithubAppError::Config(format!("Invalid URL: {e}")))?;

		let response = self
			.http_client
			.post(url)
			.header("Authorization", format!("Bearer {jwt}"))
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.header("User-Agent", "loom-server-github-app")
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					error!("Installation token request timed out");
					return GithubAppError::Timeout;
				}
				error!(error = %e, "Network error fetching installation token");
				GithubAppError::Network(e)
			})?;

		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			let err = map_github_error(status, &body);

			// If 401 on token fetch, invalidate JWT and propagate
			if matches!(err, GithubAppError::Unauthorized) {
				self.invalidate_app_jwt().await;
			}

			return Err(err);
		}

		let token_response: AccessTokenResponse = response.json().await.map_err(|e| {
			error!(error = %e, "Failed to parse access token response");
			GithubAppError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		let valid_for = parse_expiry_duration(&token_response.expires_at)?;

		Ok((token_response.token, valid_for))
	}

	/// Search code within a repository with 401-aware token refresh.
	#[instrument(skip(self), fields(query = %request.query, owner = %request.owner, repo = %request.repo))]
	pub async fn search_code(
		&self,
		installation_id: i64,
		request: CodeSearchRequest,
	) -> Result<CodeSearchResponse, GithubAppError> {
		let github_query = request.to_github_query();

		retry(&self.config.retry_config, || {
			self.search_code_with_refresh(
				installation_id,
				&github_query,
				request.per_page,
				request.page,
			)
		})
		.await
	}

	/// Search code with automatic token refresh on 401.
	async fn search_code_with_refresh(
		&self,
		installation_id: i64,
		query: &str,
		per_page: u32,
		page: u32,
	) -> Result<CodeSearchResponse, GithubAppError> {
		let token = self.get_installation_token(installation_id).await?;

		match self.search_code_inner(&token, query, per_page, page).await {
			Ok(resp) => Ok(resp),
			Err(GithubAppError::Unauthorized) => {
				info!(installation_id, "Got 401, refreshing installation token");
				self.invalidate_installation_token(installation_id).await;
				let fresh_token = self.get_installation_token(installation_id).await?;
				self
					.search_code_inner(&fresh_token, query, per_page, page)
					.await
			}
			Err(e) => Err(e),
		}
	}

	async fn search_code_inner(
		&self,
		token: &str,
		query: &str,
		per_page: u32,
		page: u32,
	) -> Result<CodeSearchResponse, GithubAppError> {
		let mut url = self
			.config
			.base_url()
			.join("search/code")
			.map_err(|e| GithubAppError::Config(format!("Invalid URL: {e}")))?;

		url
			.query_pairs_mut()
			.append_pair("q", query)
			.append_pair("per_page", &per_page.to_string())
			.append_pair("page", &page.to_string());

		debug!(url = %url, "Sending code search request");

		let response = self
			.http_client
			.get(url)
			.header("Authorization", format!("Bearer {token}"))
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.header("User-Agent", "loom-server-github-app")
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					return GithubAppError::Timeout;
				}
				GithubAppError::Network(e)
			})?;

		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(map_github_error(status, &body));
		}

		let github_response: GitHubCodeSearchResponse = response.json().await.map_err(|e| {
			error!(error = %e, "Failed to parse code search response");
			GithubAppError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		debug!(
			total_count = github_response.total_count,
			items_count = github_response.items.len(),
			"Code search completed"
		);

		Ok(CodeSearchResponse {
			total_count: github_response.total_count,
			incomplete_results: github_response.incomplete_results,
			items: github_response
				.items
				.into_iter()
				.map(|item| CodeSearchItem {
					name: item.name,
					path: item.path,
					sha: item.sha,
					html_url: item.html_url,
					repository_full_name: item.repository.full_name,
					score: item.score,
				})
				.collect(),
		})
	}

	/// Get repository metadata with 401-aware token refresh.
	#[instrument(skip(self), fields(owner, repo))]
	pub async fn get_repository(
		&self,
		installation_id: i64,
		owner: &str,
		repo: &str,
	) -> Result<Repository, GithubAppError> {
		retry(&self.config.retry_config, || {
			self.get_repository_with_refresh(installation_id, owner, repo)
		})
		.await
	}

	async fn get_repository_with_refresh(
		&self,
		installation_id: i64,
		owner: &str,
		repo: &str,
	) -> Result<Repository, GithubAppError> {
		let token = self.get_installation_token(installation_id).await?;

		match self.get_repository_inner(&token, owner, repo).await {
			Ok(resp) => Ok(resp),
			Err(GithubAppError::Unauthorized) => {
				info!(installation_id, "Got 401, refreshing installation token");
				self.invalidate_installation_token(installation_id).await;
				let fresh_token = self.get_installation_token(installation_id).await?;
				self.get_repository_inner(&fresh_token, owner, repo).await
			}
			Err(e) => Err(e),
		}
	}

	async fn get_repository_inner(
		&self,
		token: &str,
		owner: &str,
		repo: &str,
	) -> Result<Repository, GithubAppError> {
		let url = self
			.config
			.base_url()
			.join(&format!("repos/{owner}/{repo}"))
			.map_err(|e| GithubAppError::Config(format!("Invalid URL: {e}")))?;

		debug!(url = %url, "Fetching repository info");

		let response = self
			.http_client
			.get(url)
			.header("Authorization", format!("Bearer {token}"))
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.header("User-Agent", "loom-server-github-app")
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					return GithubAppError::Timeout;
				}
				GithubAppError::Network(e)
			})?;

		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(map_github_error(status, &body));
		}

		let repo_response: GitHubRepoResponse = response.json().await.map_err(|e| {
			error!(error = %e, "Failed to parse repository response");
			GithubAppError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		debug!(full_name = %repo_response.full_name, "Repository info fetched");

		Ok(Repository {
			id: repo_response.id,
			full_name: repo_response.full_name,
			description: repo_response.description,
			private: repo_response.private,
			default_branch: repo_response.default_branch,
			language: repo_response.language,
			stargazers_count: repo_response.stargazers_count,
			html_url: repo_response.html_url,
		})
	}

	/// Get file contents from a repository with 401-aware token refresh.
	#[instrument(skip(self), fields(owner, repo, path))]
	pub async fn get_file_contents(
		&self,
		installation_id: i64,
		owner: &str,
		repo: &str,
		path: &str,
		git_ref: Option<&str>,
	) -> Result<FileContents, GithubAppError> {
		retry(&self.config.retry_config, || {
			self.get_file_contents_with_refresh(installation_id, owner, repo, path, git_ref)
		})
		.await
	}

	async fn get_file_contents_with_refresh(
		&self,
		installation_id: i64,
		owner: &str,
		repo: &str,
		path: &str,
		git_ref: Option<&str>,
	) -> Result<FileContents, GithubAppError> {
		let token = self.get_installation_token(installation_id).await?;

		match self
			.get_file_contents_inner(&token, owner, repo, path, git_ref)
			.await
		{
			Ok(resp) => Ok(resp),
			Err(GithubAppError::Unauthorized) => {
				info!(installation_id, "Got 401, refreshing installation token");
				self.invalidate_installation_token(installation_id).await;
				let fresh_token = self.get_installation_token(installation_id).await?;
				self
					.get_file_contents_inner(&fresh_token, owner, repo, path, git_ref)
					.await
			}
			Err(e) => Err(e),
		}
	}

	async fn get_file_contents_inner(
		&self,
		token: &str,
		owner: &str,
		repo: &str,
		path: &str,
		git_ref: Option<&str>,
	) -> Result<FileContents, GithubAppError> {
		let path_encoded = urlencoding::encode(path);
		let mut url = self
			.config
			.base_url()
			.join(&format!("repos/{owner}/{repo}/contents/{path_encoded}"))
			.map_err(|e| GithubAppError::Config(format!("Invalid URL: {e}")))?;

		if let Some(r) = git_ref {
			url.query_pairs_mut().append_pair("ref", r);
		}

		debug!(url = %url, "Fetching file contents");

		let response = self
			.http_client
			.get(url)
			.header("Authorization", format!("Bearer {token}"))
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.header("User-Agent", "loom-server-github-app")
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					return GithubAppError::Timeout;
				}
				GithubAppError::Network(e)
			})?;

		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(map_github_error(status, &body));
		}

		let content_response: GitHubContentResponse = response.json().await.map_err(|e| {
			error!(error = %e, "Failed to parse content response");
			GithubAppError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		debug!(path = %content_response.path, size = content_response.size, "File contents fetched");

		Ok(FileContents {
			name: content_response.name,
			path: content_response.path,
			sha: content_response.sha,
			size: content_response.size,
			encoding: content_response
				.encoding
				.unwrap_or_else(|| "base64".to_string()),
			content: content_response.content.unwrap_or_default(),
		})
	}

	/// List all installations for this app.
	#[instrument(skip(self))]
	pub async fn list_installations(&self) -> Result<Vec<Installation>, GithubAppError> {
		let jwt = self.get_app_jwt().await?;

		retry(&self.config.retry_config, || {
			self.list_installations_inner(&jwt)
		})
		.await
	}

	async fn list_installations_inner(&self, jwt: &str) -> Result<Vec<Installation>, GithubAppError> {
		let url = self
			.config
			.base_url()
			.join("app/installations")
			.map_err(|e| GithubAppError::Config(format!("Invalid URL: {e}")))?;

		debug!(url = %url, "Listing app installations");

		let response = self
			.http_client
			.get(url)
			.header("Authorization", format!("Bearer {jwt}"))
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.header("User-Agent", "loom-server-github-app")
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					return GithubAppError::Timeout;
				}
				GithubAppError::Network(e)
			})?;

		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(map_github_error(status, &body));
		}

		let installations: Vec<Installation> = response.json().await.map_err(|e| {
			error!(error = %e, "Failed to parse installations response");
			GithubAppError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		debug!(count = installations.len(), "Installations listed");

		Ok(installations)
	}

	/// Get the installation for a specific repository.
	#[instrument(skip(self), fields(owner, repo))]
	pub async fn get_repo_installation(
		&self,
		owner: &str,
		repo: &str,
	) -> Result<Installation, GithubAppError> {
		let jwt = self.get_app_jwt().await?;

		retry(&self.config.retry_config, || {
			self.get_repo_installation_inner(&jwt, owner, repo)
		})
		.await
	}

	async fn get_repo_installation_inner(
		&self,
		jwt: &str,
		owner: &str,
		repo: &str,
	) -> Result<Installation, GithubAppError> {
		let url = self
			.config
			.base_url()
			.join(&format!("repos/{owner}/{repo}/installation"))
			.map_err(|e| GithubAppError::Config(format!("Invalid URL: {e}")))?;

		debug!(url = %url, "Getting repository installation");

		let response = self
			.http_client
			.get(url)
			.header("Authorization", format!("Bearer {jwt}"))
			.header("Accept", "application/vnd.github+json")
			.header("X-GitHub-Api-Version", "2022-11-28")
			.header("User-Agent", "loom-server-github-app")
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					return GithubAppError::Timeout;
				}
				GithubAppError::Network(e)
			})?;

		let status = response.status();
		if !status.is_success() {
			if status == StatusCode::NOT_FOUND {
				return Err(GithubAppError::installation_not_found(owner, repo));
			}
			let body = response.text().await.unwrap_or_default();
			return Err(map_github_error(status, &body));
		}

		let installation: Installation = response.json().await.map_err(|e| {
			error!(error = %e, "Failed to parse installation response");
			GithubAppError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		debug!(
			installation_id = installation.id,
			"Repository installation found"
		);

		Ok(installation)
	}

	/// Generate an App JWT for testing/validation.
	pub fn generate_app_jwt(&self) -> Result<String, GithubAppError> {
		generate_app_jwt(self.config.app_id(), self.config.private_key_pem())
	}

	/// Get the installation URL for users.
	pub fn installation_url(&self) -> String {
		self.config.installation_url()
	}

	/// Get the app slug.
	pub fn app_slug(&self) -> &str {
		self.config.app_slug()
	}
}

/// Map GitHub API error responses to GithubAppError.
pub(crate) fn map_github_error(status: StatusCode, body: &str) -> GithubAppError {
	let status_code = status.as_u16();

	match status_code {
		401 => {
			warn!(status = status_code, "Unauthorized request to GitHub");
			GithubAppError::Unauthorized
		}
		403 => {
			if body.to_lowercase().contains("rate limit") || body.to_lowercase().contains("api rate") {
				warn!(status = status_code, "GitHub rate limit exceeded");
				GithubAppError::RateLimited
			} else {
				warn!(status = status_code, "Forbidden request to GitHub");
				GithubAppError::Forbidden
			}
		}
		_ => {
			error!(status = status_code, body = %body, "GitHub API error");
			GithubAppError::ApiError {
				status: status_code,
				message: body.to_string(),
			}
		}
	}
}

/// Parse the expires_at timestamp from GitHub into a Duration.
pub(crate) fn parse_expiry_duration(expires_at: &str) -> Result<Duration, GithubAppError> {
	let expires_at_dt: DateTime<Utc> = expires_at.parse().map_err(|e| {
		GithubAppError::InvalidResponse(format!("Invalid expires_at: {expires_at} - {e}"))
	})?;

	let now = Utc::now();
	let duration = expires_at_dt
		.signed_duration_since(now)
		.to_std()
		.unwrap_or(Duration::ZERO);

	Ok(duration)
}

#[derive(Debug, Deserialize)]
struct GitHubCodeSearchResponse {
	total_count: u32,
	incomplete_results: bool,
	items: Vec<GitHubCodeSearchItem>,
}

#[derive(Debug, Deserialize)]
struct GitHubCodeSearchItem {
	name: String,
	path: String,
	sha: String,
	html_url: String,
	repository: GitHubCodeSearchRepo,
	score: f64,
}

#[derive(Debug, Deserialize)]
struct GitHubCodeSearchRepo {
	full_name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRepoResponse {
	id: i64,
	full_name: String,
	description: Option<String>,
	private: bool,
	default_branch: String,
	language: Option<String>,
	stargazers_count: u32,
	html_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubContentResponse {
	name: String,
	path: String,
	sha: String,
	size: u64,
	encoding: Option<String>,
	content: Option<String>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_cached_token_validity() {
		let token = CachedToken::new("test".to_string(), Duration::from_secs(300));
		assert!(token.is_valid(Duration::from_secs(60)));
		assert!(token.is_valid(Duration::from_secs(200)));
	}

	#[test]
	fn test_map_github_error_unauthorized() {
		let err = map_github_error(StatusCode::UNAUTHORIZED, "Bad credentials");
		assert!(matches!(err, GithubAppError::Unauthorized));
	}

	#[test]
	fn test_map_github_error_rate_limit() {
		let err = map_github_error(StatusCode::FORBIDDEN, "API rate limit exceeded");
		assert!(matches!(err, GithubAppError::RateLimited));
	}

	#[test]
	fn test_map_github_error_forbidden() {
		let err = map_github_error(StatusCode::FORBIDDEN, "Not allowed");
		assert!(matches!(err, GithubAppError::Forbidden));
	}

	#[test]
	fn test_map_github_error_500() {
		let err = map_github_error(StatusCode::INTERNAL_SERVER_ERROR, "Server error");
		assert!(matches!(err, GithubAppError::ApiError { status: 500, .. }));
	}
}
