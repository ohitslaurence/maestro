// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! GitHub App HTTP handlers.

use axum::{
	body::Bytes,
	extract::{Query, State},
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use loom_server_github_app::{
	AppInfoResponse, CodeSearchRequest, CodeSearchResponse, GithubAppError,
	InstallationStatusResponse,
};

pub use loom_server_api::github::*;

use crate::{
	api::AppState,
	db::{GithubInstallation, GithubRepo},
	error::ServerError,
};

#[utoipa::path(
    get,
    path = "/api/github/app",
    responses(
        (status = 200, description = "GitHub App info", body = AppInfoResponse),
        (status = 500, description = "GitHub App not configured", body = crate::error::ErrorResponse)
    ),
    tag = "github"
)]
/// GET /api/github/app - Get GitHub App configuration info.
#[axum::debug_handler]
pub async fn get_github_app_info(State(state): State<AppState>) -> impl IntoResponse {
	match &state.github_client {
		Some(client) => Json(AppInfoResponse {
			configured: true,
			app_slug: Some(client.app_slug().to_string()),
			installation_url: Some(client.installation_url()),
		}),
		None => Json(AppInfoResponse {
			configured: false,
			app_slug: None,
			installation_url: None,
		}),
	}
}

#[utoipa::path(
    get,
    path = "/api/github/installations/by-repo",
    params(GithubInstallationByRepoQuery),
    responses(
        (status = 200, description = "Installation status", body = InstallationStatusResponse),
        (status = 500, description = "GitHub App not configured", body = crate::error::ErrorResponse)
    ),
    tag = "github"
)]
/// GET /api/github/installations/by-repo - Check if app is installed for a repo.
#[axum::debug_handler]
pub async fn get_github_installation_by_repo(
	State(state): State<AppState>,
	Query(params): Query<GithubInstallationByRepoQuery>,
) -> Result<Json<InstallationStatusResponse>, ServerError> {
	match state
		.repo
		.get_github_installation_for_repo(&params.owner, &params.repo)
		.await?
	{
		Some(info) => Ok(Json(InstallationStatusResponse::installed(
			info.installation_id,
			info.account_login,
			info.account_type,
			info.repositories_selection,
		))),
		None => Ok(Json(InstallationStatusResponse::not_installed())),
	}
}

/// POST /api/github/webhook - Handle GitHub App webhook events.
///
/// Security: Requires webhook secret to be configured and valid signature.
#[axum::debug_handler]
pub async fn github_webhook(
	State(state): State<AppState>,
	headers: HeaderMap,
	body: Bytes,
) -> Result<impl IntoResponse, ServerError> {
	let event_type = headers
		.get("X-GitHub-Event")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("unknown");

	tracing::debug!(event_type = %event_type, "github_webhook: received event");

	// 1. Ensure GitHub App is configured
	let client = state.github_client.as_ref().ok_or_else(|| {
		tracing::error!("github_webhook: GitHub App not configured");
		ServerError::ServiceUnavailable("GitHub App is not configured on the server".into())
	})?;

	// 2. Enforce webhook secret presence (security requirement)
	let secret = client.webhook_secret().ok_or_else(|| {
		tracing::error!("github_webhook: webhook secret not configured");
		ServerError::Internal("GitHub webhook secret is not configured on the server".into())
	})?;

	// 3. Extract signature header (required)
	let sig_header = headers
		.get("X-Hub-Signature-256")
		.and_then(|v| v.to_str().ok())
		.ok_or_else(|| {
			tracing::warn!("github_webhook: missing X-Hub-Signature-256 header");
			ServerError::BadRequest("Missing X-Hub-Signature-256 header".into())
		})?;

	// 4. Verify signature
	if let Err(e) = loom_server_github_app::verify_webhook_signature(secret, sig_header, &body) {
		tracing::warn!(error = %e, "github_webhook: signature verification failed");
		return Err(ServerError::Unauthorized(
			"Invalid webhook signature".into(),
		));
	}

	// 5. Process the event
	match event_type {
		"installation" => handle_installation_webhook(&state, &body).await?,
		"installation_repositories" => handle_installation_repos_webhook(&state, &body).await?,
		_ => {
			tracing::debug!(event_type = %event_type, "github_webhook: ignoring event");
		}
	}

	Ok(StatusCode::OK)
}

/// Handle installation webhook events.
async fn handle_installation_webhook(state: &AppState, body: &[u8]) -> Result<(), ServerError> {
	use loom_server_github_app::types::InstallationWebhookPayload;

	let payload: InstallationWebhookPayload = serde_json::from_slice(body)
		.map_err(|e| ServerError::BadRequest(format!("Invalid webhook payload: {e}")))?;

	tracing::info!(
			action = %payload.action,
			installation_id = payload.installation.id,
			account_login = %payload.installation.account.login,
			"github_webhook: installation event"
	);

	let now = chrono::Utc::now().to_rfc3339();

	match payload.action.as_str() {
		"created" => {
			let installation = GithubInstallation {
				installation_id: payload.installation.id,
				account_id: payload.installation.account.id,
				account_login: payload.installation.account.login.clone(),
				account_type: payload.installation.account.account_type.clone(),
				app_slug: None,
				repositories_selection: payload.installation.repository_selection.clone(),
				suspended_at: None,
				created_at: now.clone(),
				updated_at: now.clone(),
			};
			state.repo.upsert_github_installation(&installation).await?;

			let repos: Vec<GithubRepo> = payload
				.repositories
				.iter()
				.map(|r| {
					let (owner, name) = r.full_name.split_once('/').unwrap_or(("", &r.name));
					GithubRepo {
						repository_id: r.id,
						owner: owner.to_string(),
						name: name.to_string(),
						full_name: r.full_name.clone(),
						private: r.private,
						default_branch: None,
					}
				})
				.collect();

			if !repos.is_empty() {
				state
					.repo
					.add_github_installation_repos(payload.installation.id, &repos)
					.await?;
			}
		}
		"deleted" => {
			state
				.repo
				.delete_github_installation(payload.installation.id)
				.await?;
		}
		"suspended" => {
			state
				.repo
				.update_github_installation_suspension(payload.installation.id, Some(&now))
				.await?;
		}
		"unsuspended" => {
			state
				.repo
				.update_github_installation_suspension(payload.installation.id, None)
				.await?;
		}
		_ => {
			tracing::debug!(action = %payload.action, "github_webhook: ignoring installation action");
		}
	}

	Ok(())
}

/// Handle installation_repositories webhook events.
async fn handle_installation_repos_webhook(
	state: &AppState,
	body: &[u8],
) -> Result<(), ServerError> {
	use loom_server_github_app::types::InstallationWebhookPayload;

	let payload: InstallationWebhookPayload = serde_json::from_slice(body)
		.map_err(|e| ServerError::BadRequest(format!("Invalid webhook payload: {e}")))?;

	tracing::info!(
			action = %payload.action,
			installation_id = payload.installation.id,
			added = payload.repositories_added.len(),
			removed = payload.repositories_removed.len(),
			"github_webhook: installation_repositories event"
	);

	if !payload.repositories_added.is_empty() {
		let repos: Vec<GithubRepo> = payload
			.repositories_added
			.iter()
			.map(|r| {
				let (owner, name) = r.full_name.split_once('/').unwrap_or(("", &r.name));
				GithubRepo {
					repository_id: r.id,
					owner: owner.to_string(),
					name: name.to_string(),
					full_name: r.full_name.clone(),
					private: r.private,
					default_branch: None,
				}
			})
			.collect();
		state
			.repo
			.add_github_installation_repos(payload.installation.id, &repos)
			.await?;
	}

	if !payload.repositories_removed.is_empty() {
		let repo_ids: Vec<i64> = payload.repositories_removed.iter().map(|r| r.id).collect();
		state
			.repo
			.remove_github_installation_repos(&repo_ids)
			.await?;
	}

	Ok(())
}

#[utoipa::path(
    post,
    path = "/proxy/github/search-code",
    request_body = GithubSearchCodeRequest,
    responses(
        (status = 200, description = "Code search results", body = CodeSearchResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 500, description = "GitHub error", body = crate::error::ErrorResponse)
    ),
    tag = "github"
)]
/// POST /proxy/github/search-code - Proxy code search requests.
#[axum::debug_handler]
pub async fn proxy_github_search_code(
	State(state): State<AppState>,
	Json(body): Json<GithubSearchCodeRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let client = state.github_client.as_ref().ok_or_else(|| {
		tracing::error!("proxy_github_search_code: GitHub App not configured");
		ServerError::ServiceUnavailable("GitHub App is not configured on the server".into())
	})?;

	let installation = state
		.repo
		.get_github_installation_for_repo(&body.owner, &body.repo)
		.await?
		.ok_or_else(|| {
			ServerError::NotFound(format!(
				"GitHub App not installed for {}/{}",
				body.owner, body.repo
			))
		})?;

	tracing::debug!(
			owner = %body.owner,
			repo = %body.repo,
			query = %body.query,
			installation_id = installation.installation_id,
			"proxy_github_search_code: searching"
	);

	let request = CodeSearchRequest::new(&body.query, &body.owner, &body.repo)
		.with_per_page(body.per_page)
		.with_page(body.page);

	let response = client
		.search_code(installation.installation_id, request)
		.await
		.map_err(map_github_error)?;

	tracing::info!(
			owner = %body.owner,
			repo = %body.repo,
			total_count = response.total_count,
			"proxy_github_search_code: returning results"
	);

	Ok((StatusCode::OK, Json(response)))
}

#[utoipa::path(
    post,
    path = "/proxy/github/repo-info",
    request_body = GithubRepoInfoRequest,
    responses(
        (status = 200, description = "Repository info", body = GithubRepoInfoResponse),
        (status = 404, description = "Repository not found", body = crate::error::ErrorResponse),
        (status = 500, description = "GitHub error", body = crate::error::ErrorResponse)
    ),
    tag = "github"
)]
/// POST /proxy/github/repo-info - Get repository metadata.
#[axum::debug_handler]
pub async fn proxy_github_repo_info(
	State(state): State<AppState>,
	Json(body): Json<GithubRepoInfoRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let client = state.github_client.as_ref().ok_or_else(|| {
		tracing::error!("proxy_github_repo_info: GitHub App not configured");
		ServerError::ServiceUnavailable("GitHub App is not configured on the server".into())
	})?;

	let installation = state
		.repo
		.get_github_installation_for_repo(&body.owner, &body.repo)
		.await?
		.ok_or_else(|| {
			ServerError::NotFound(format!(
				"GitHub App not installed for {}/{}",
				body.owner, body.repo
			))
		})?;

	tracing::debug!(
			owner = %body.owner,
			repo = %body.repo,
			installation_id = installation.installation_id,
			"proxy_github_repo_info: fetching"
	);

	let repo = client
		.get_repository(installation.installation_id, &body.owner, &body.repo)
		.await
		.map_err(map_github_error)?;

	tracing::info!(
			full_name = %repo.full_name,
			private = repo.private,
			"proxy_github_repo_info: returning info"
	);

	Ok((
		StatusCode::OK,
		Json(GithubRepoInfoResponse {
			id: repo.id,
			full_name: repo.full_name,
			description: repo.description,
			private: repo.private,
			default_branch: repo.default_branch,
			language: repo.language,
			stargazers_count: repo.stargazers_count,
			html_url: repo.html_url,
		}),
	))
}

#[utoipa::path(
    post,
    path = "/proxy/github/file-contents",
    request_body = GithubFileContentsRequest,
    responses(
        (status = 200, description = "File contents", body = GithubFileContentsResponse),
        (status = 404, description = "File not found", body = crate::error::ErrorResponse),
        (status = 500, description = "GitHub error", body = crate::error::ErrorResponse)
    ),
    tag = "github"
)]
/// POST /proxy/github/file-contents - Get file contents.
#[axum::debug_handler]
pub async fn proxy_github_file_contents(
	State(state): State<AppState>,
	Json(body): Json<GithubFileContentsRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let client = state.github_client.as_ref().ok_or_else(|| {
		tracing::error!("proxy_github_file_contents: GitHub App not configured");
		ServerError::ServiceUnavailable("GitHub App is not configured on the server".into())
	})?;

	let installation = state
		.repo
		.get_github_installation_for_repo(&body.owner, &body.repo)
		.await?
		.ok_or_else(|| {
			ServerError::NotFound(format!(
				"GitHub App not installed for {}/{}",
				body.owner, body.repo
			))
		})?;

	tracing::debug!(
			owner = %body.owner,
			repo = %body.repo,
			path = %body.path,
			git_ref = ?body.git_ref,
			installation_id = installation.installation_id,
			"proxy_github_file_contents: fetching"
	);

	let contents = client
		.get_file_contents(
			installation.installation_id,
			&body.owner,
			&body.repo,
			&body.path,
			body.git_ref.as_deref(),
		)
		.await
		.map_err(map_github_error)?;

	tracing::info!(
			path = %contents.path,
			size = contents.size,
			"proxy_github_file_contents: returning contents"
	);

	Ok((
		StatusCode::OK,
		Json(GithubFileContentsResponse {
			name: contents.name,
			path: contents.path,
			sha: contents.sha,
			size: contents.size,
			encoding: contents.encoding,
			content: contents.content,
		}),
	))
}

/// Map GitHub App errors to server errors.
fn map_github_error(err: GithubAppError) -> ServerError {
	match err {
		GithubAppError::Timeout => {
			tracing::warn!("GitHub request timed out");
			ServerError::UpstreamTimeout("GitHub API request timed out".into())
		}
		GithubAppError::RateLimited => {
			tracing::warn!("GitHub rate limit exceeded");
			ServerError::ServiceUnavailable("GitHub API rate limit exceeded; try again later".into())
		}
		GithubAppError::Unauthorized => {
			tracing::error!("GitHub unauthorized");
			ServerError::Internal("GitHub App authentication failed".into())
		}
		GithubAppError::Forbidden => {
			tracing::warn!("GitHub forbidden");
			ServerError::Forbidden("Insufficient permissions for this GitHub operation".into())
		}
		GithubAppError::InstallationNotFound { owner, repo } => {
			ServerError::NotFound(format!("GitHub App not installed for {owner}/{repo}"))
		}
		GithubAppError::Network(e) => {
			tracing::error!(error = %e, "GitHub network error");
			ServerError::UpstreamError(format!("Failed to contact GitHub: {e}"))
		}
		GithubAppError::InvalidResponse(msg) => {
			tracing::error!(error = %msg, "Invalid GitHub response");
			ServerError::UpstreamError(format!("Invalid GitHub response: {msg}"))
		}
		GithubAppError::ApiError { status, message } => {
			tracing::warn!(status = status, message = %message, "GitHub API error");
			ServerError::UpstreamError(format!("GitHub error: {status} - {message}"))
		}
		GithubAppError::Config(msg) => {
			tracing::error!(error = %msg, "GitHub config error");
			ServerError::Internal(format!("GitHub App configuration error: {msg}"))
		}
		GithubAppError::Jwt(msg) => {
			tracing::error!(error = %msg, "GitHub JWT error");
			ServerError::Internal(format!("GitHub App JWT error: {msg}"))
		}
		GithubAppError::InvalidWebhookSignature => {
			ServerError::Unauthorized("Invalid webhook signature".into())
		}
	}
}
