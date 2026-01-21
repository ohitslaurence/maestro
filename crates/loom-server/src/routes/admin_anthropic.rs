// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Admin routes for Anthropic OAuth pool management.
//!
//! Provides endpoints for managing Claude Max OAuth accounts in the pool:
//! - List accounts with status
//! - Add new accounts via OAuth flow (two-step: initiate + submit code)
//! - Remove accounts
//!
//! # OAuth Flow
//!
//! Since we use Anthropic's public OAuth client, we cannot use custom redirect URIs.
//! The flow is:
//! 1. POST /api/admin/anthropic/oauth/initiate - Get auth URL and state token
//! 2. User opens URL in browser, authorizes, and copies the code from Anthropic's page
//! 3. POST /api/admin/anthropic/oauth/complete - Submit the code to exchange for tokens
//!
//! # Security
//!
//! All endpoints require `SystemRole::Admin`.

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_common_secret::SecretString;
use loom_server_llm_anthropic::{
	exchange_code, OAuthCredentials, Pkce, CLIENT_ID, REDIRECT_URI, SCOPES,
};
use url::Url;

pub use loom_server_api::admin::{
	AccountDetailsResponse, AccountStatus, AccountsSummary, AddAccountResponse, AdminErrorResponse,
	AnthropicAccountsResponse, InitiateOAuthRequest, InitiateOAuthResponse, RemoveAccountResponse,
	SubmitOAuthCodeRequest,
};

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	oauth_state::generate_state,
};

const ANTHROPIC_ADMIN_PROVIDER: &str = "anthropic-admin";

/// List all Anthropic OAuth accounts with status.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns [`AnthropicAccountsResponse`] with account list and summary.
///
/// # Errors
///
/// - `401 Unauthorized`: Not authenticated
/// - `403 Forbidden`: Not system admin
/// - `501 Not Implemented`: Not in OAuth pool mode
#[utoipa::path(
	get,
	path = "/api/admin/anthropic/accounts",
	responses(
		(status = 200, description = "List of accounts", body = AnthropicAccountsResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 501, description = "Not in OAuth pool mode", body = AdminErrorResponse)
	),
	tag = "admin-anthropic"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn list_accounts(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized anthropic accounts list attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let llm_service = match &state.llm_service {
		Some(service) => service,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "LLM service not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !llm_service.is_anthropic_oauth_pool() {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AdminErrorResponse {
				error: "not_implemented".to_string(),
				message: "Anthropic is not configured in OAuth pool mode".to_string(),
			}),
		)
			.into_response();
	}

	let accounts = match llm_service.anthropic_account_details().await {
		Some(details) => details
			.into_iter()
			.map(AccountDetailsResponse::from)
			.collect::<Vec<_>>(),
		None => vec![],
	};

	let mut available = 0;
	let mut cooling_down = 0;
	let mut disabled = 0;

	for account in &accounts {
		match account.status {
			AccountStatus::Available => available += 1,
			AccountStatus::CoolingDown => cooling_down += 1,
			AccountStatus::Disabled => disabled += 1,
		}
	}

	let summary = AccountsSummary {
		total: accounts.len(),
		available,
		cooling_down,
		disabled,
	};

	tracing::info!(
		actor_id = %current_user.user.id,
		total = summary.total,
		available = summary.available,
		"Listed Anthropic accounts"
	);

	(
		StatusCode::OK,
		Json(AnthropicAccountsResponse { accounts, summary }),
	)
		.into_response()
}

/// Initiate OAuth flow to add a new Anthropic account.
///
/// Returns an authorization URL that the admin should open in their browser.
/// After authorizing, Anthropic will display a code on their page.
/// The admin should then call the complete endpoint with that code.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns [`InitiateOAuthResponse`] with:
/// - `redirect_url`: URL to open in browser
/// - `state`: State token to use when submitting the code
///
/// # Errors
///
/// - `401 Unauthorized`: Not authenticated
/// - `403 Forbidden`: Not system admin
/// - `501 Not Implemented`: Not in OAuth pool mode
#[utoipa::path(
	post,
	path = "/api/admin/anthropic/oauth/initiate",
	request_body = InitiateOAuthRequest,
	responses(
		(status = 200, description = "OAuth authorization URL and state", body = InitiateOAuthResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 501, description = "Not in OAuth pool mode", body = AdminErrorResponse)
	),
	tag = "admin-anthropic"
)]
#[tracing::instrument(skip(state, body), fields(actor_id = %current_user.user.id))]
pub async fn initiate_oauth(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(body): Json<InitiateOAuthRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized anthropic OAuth initiation attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let llm_service = match &state.llm_service {
		Some(service) => service,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "LLM service not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !llm_service.is_anthropic_oauth_pool() {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AdminErrorResponse {
				error: "not_implemented".to_string(),
				message: "Anthropic is not configured in OAuth pool mode".to_string(),
			}),
		)
			.into_response();
	}

	let pkce = Pkce::generate();
	let oauth_state = generate_state();

	let mut auth_url =
		Url::parse("https://claude.ai/oauth/authorize").expect("Invalid authorize URL");
	{
		let mut params = auth_url.query_pairs_mut();
		params.append_pair("client_id", CLIENT_ID);
		params.append_pair("response_type", "code");
		params.append_pair("redirect_uri", REDIRECT_URI);
		params.append_pair("scope", SCOPES);
		params.append_pair("code_challenge", &pkce.challenge);
		params.append_pair("code_challenge_method", "S256");
		params.append_pair("state", &oauth_state);
	}

	state
		.oauth_state_store
		.store(
			oauth_state.clone(),
			ANTHROPIC_ADMIN_PROVIDER.to_string(),
			Some(pkce.verifier),
			body.redirect_after,
		)
		.await;

	tracing::info!(
		actor_id = %current_user.user.id,
		state = %oauth_state,
		"Initiated Anthropic OAuth flow"
	);

	(
		StatusCode::OK,
		Json(InitiateOAuthResponse {
			redirect_url: auth_url.to_string(),
			state: oauth_state,
		}),
	)
		.into_response()
}

/// Complete OAuth flow by submitting the authorization code.
///
/// After the admin authorizes on Anthropic's page, they will see a code.
/// Submit that code along with the state from the initiate response.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Request
///
/// Body ([`SubmitOAuthCodeRequest`]):
/// - `code`: The authorization code from Anthropic's callback page
/// - `state`: The state token from the initiate response
///
/// # Response
///
/// Returns [`AddAccountResponse`] with the new account ID.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid state or code
/// - `401 Unauthorized`: Not authenticated
/// - `403 Forbidden`: Not system admin
/// - `501 Not Implemented`: Not in OAuth pool mode
#[utoipa::path(
	post,
	path = "/api/admin/anthropic/oauth/complete",
	request_body = SubmitOAuthCodeRequest,
	responses(
		(status = 200, description = "Account added successfully", body = AddAccountResponse),
		(status = 400, description = "Invalid state or code", body = AdminErrorResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 501, description = "Not in OAuth pool mode", body = AdminErrorResponse)
	),
	tag = "admin-anthropic"
)]
#[tracing::instrument(skip(state, body), fields(actor_id = %current_user.user.id))]
pub async fn complete_oauth(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(body): Json<SubmitOAuthCodeRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized anthropic OAuth completion attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let llm_service = match &state.llm_service {
		Some(service) => service,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "LLM service not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !llm_service.is_anthropic_oauth_pool() {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AdminErrorResponse {
				error: "not_implemented".to_string(),
				message: "Anthropic is not configured in OAuth pool mode".to_string(),
			}),
		)
			.into_response();
	}

	let entry = match state
		.oauth_state_store
		.validate_and_consume(&body.state, ANTHROPIC_ADMIN_PROVIDER)
		.await
	{
		Some(e) => e,
		None => {
			tracing::warn!(state = %body.state, "Invalid or expired OAuth state");
			return (
				StatusCode::BAD_REQUEST,
				Json(AdminErrorResponse {
					error: "invalid_state".to_string(),
					message: "Invalid or expired OAuth state. Please start the OAuth flow again.".to_string(),
				}),
			)
				.into_response();
		}
	};

	let verifier = match entry.nonce {
		Some(v) => v,
		None => {
			tracing::error!("Missing PKCE verifier in OAuth state");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: "Missing PKCE verifier. Please start the OAuth flow again.".to_string(),
				}),
			)
				.into_response();
		}
	};

	// Anthropic's callback page displays code#state - strip the fragment if present
	let code = body.code.split('#').next().unwrap_or(&body.code);

	let exchange_result = match exchange_code(code, &body.state, &verifier).await {
		Ok(result) => result,
		Err(e) => {
			tracing::error!(error = %e, "Failed to exchange OAuth code");
			return (
				StatusCode::BAD_REQUEST,
				Json(AdminErrorResponse {
					error: "exchange_failed".to_string(),
					message: format!("Failed to exchange code: {e}"),
				}),
			)
				.into_response();
		}
	};

	let (access, refresh, expires) = match exchange_result {
		loom_server_llm_anthropic::ExchangeResult::Success {
			access,
			refresh,
			expires,
		} => (access, refresh, expires),
		loom_server_llm_anthropic::ExchangeResult::Failed { error } => {
			tracing::error!(error = %error, "OAuth token exchange failed");
			return (
				StatusCode::BAD_REQUEST,
				Json(AdminErrorResponse {
					error: "exchange_failed".to_string(),
					message: format!("Token exchange failed: {error}"),
				}),
			)
				.into_response();
		}
	};

	let account_id = format!("claude-max-{}", chrono::Utc::now().timestamp());
	let credentials = OAuthCredentials::new(
		SecretString::new(refresh),
		SecretString::new(access),
		expires,
	);

	if let Err(e) = llm_service
		.add_anthropic_account(account_id.clone(), credentials)
		.await
	{
		tracing::error!(error = %e, account_id = %account_id, "Failed to add account to pool");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AdminErrorResponse {
				error: "add_failed".to_string(),
				message: format!("Failed to add account: {e}"),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		account_id = %account_id,
		"Added Anthropic OAuth account to pool"
	);

	(StatusCode::OK, Json(AddAccountResponse { account_id })).into_response()
}

/// Remove an Anthropic OAuth account from the pool.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns [`RemoveAccountResponse`] with the removed account ID.
///
/// # Errors
///
/// - `401 Unauthorized`: Not authenticated
/// - `403 Forbidden`: Not system admin
/// - `404 Not Found`: Account not found
/// - `501 Not Implemented`: Not in OAuth pool mode
#[utoipa::path(
	delete,
	path = "/api/admin/anthropic/accounts/{id}",
	params(
		("id" = String, Path, description = "Account ID to remove")
	),
	responses(
		(status = 200, description = "Account removed", body = RemoveAccountResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Account not found", body = AdminErrorResponse),
		(status = 501, description = "Not in OAuth pool mode", body = AdminErrorResponse)
	),
	tag = "admin-anthropic"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, account_id = %id))]
pub async fn remove_account(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized anthropic account removal attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let llm_service = match &state.llm_service {
		Some(service) => service,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "LLM service not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !llm_service.is_anthropic_oauth_pool() {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AdminErrorResponse {
				error: "not_implemented".to_string(),
				message: "Anthropic is not configured in OAuth pool mode".to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = llm_service.remove_anthropic_account(&id).await {
		let error_msg = e.to_string();
		if error_msg.contains("not found") {
			return (
				StatusCode::NOT_FOUND,
				Json(AdminErrorResponse {
					error: "not_found".to_string(),
					message: format!("Account '{}' not found", id),
				}),
			)
				.into_response();
		}

		tracing::error!(error = %e, account_id = %id, "Failed to remove account");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AdminErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		account_id = %id,
		"Removed Anthropic OAuth account"
	);

	(StatusCode::OK, Json(RemoveAccountResponse { removed: id })).into_response()
}

#[cfg(test)]
mod tests {
	#[test]
	fn test_strip_fragment_from_code() {
		let code_with_fragment =
			"1sCIWJJZXLfYfURhCUQ1rOq7yPCyFESxFp7BjKLlxFzhl3aJ#3b191854-6359-45d3-93ff-65a0b0c158b4";
		let code = code_with_fragment
			.split('#')
			.next()
			.unwrap_or(code_with_fragment);
		assert_eq!(code, "1sCIWJJZXLfYfURhCUQ1rOq7yPCyFESxFp7BjKLlxFzhl3aJ");
	}

	#[test]
	fn test_strip_fragment_no_fragment() {
		let code_without_fragment = "1sCIWJJZXLfYfURhCUQ1rOq7yPCyFESxFp7BjKLlxFzhl3aJ";
		let code = code_without_fragment
			.split('#')
			.next()
			.unwrap_or(code_without_fragment);
		assert_eq!(code, "1sCIWJJZXLfYfURhCUQ1rOq7yPCyFESxFp7BjKLlxFzhl3aJ");
	}

	#[test]
	fn test_strip_fragment_empty_fragment() {
		let code_empty_fragment = "someCode#";
		let code = code_empty_fragment
			.split('#')
			.next()
			.unwrap_or(code_empty_fragment);
		assert_eq!(code, "someCode");
	}

	#[test]
	fn test_strip_fragment_multiple_hashes() {
		let code_multiple = "code#state#extra";
		let code = code_multiple.split('#').next().unwrap_or(code_multiple);
		assert_eq!(code, "code");
	}
}
