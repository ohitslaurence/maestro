// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Authentication HTTP handlers.
//!
//! Implements auth endpoints per the auth-abac-system.md specification:
//! - OAuth provider listing
//! - Current user retrieval
//! - Logout
//! - Magic link authentication
//! - Device code flow for CLI/VS Code
//!
//! # Security Considerations
//!
//! - Tokens in URLs/headers should be scanned for accidental logging
//! - Email addresses are PII and should be handled carefully

use axum::{
	extract::State,
	http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
	response::{IntoResponse, Redirect},
	Json,
};
pub use loom_server_api::auth::{
	AuthErrorResponse, AuthProvidersResponse, AuthSuccessResponse, CurrentUserResponse,
	DeviceCodeCompleteRequest, DeviceCodeCompleteResponse, DeviceCodePollRequest,
	DeviceCodePollResponse, DeviceCodeStartResponse, MagicLinkRequest, WsTokenResponse,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::{generate_access_token, SessionType};
use loom_server_auth_devicecode::{DeviceCode, DEVICE_CODE_EXPIRY_MINUTES};
use loom_server_auth_magiclink::{verify_magic_link_token, MagicLink};
use loom_server_session::{AuthMethod, SessionRequest};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	client_info::ClientInfo,
	i18n::{resolve_user_locale, t, t_fmt},
	oauth_state::{generate_nonce, generate_state, sanitize_redirect},
};

#[utoipa::path(
    get,
    path = "/auth/providers",
    responses(
        (status = 200, description = "List of available auth providers", body = AuthProvidersResponse)
    ),
    tag = "auth"
)]
/// Lists available authentication providers.
///
/// Returns an array of provider names based on server configuration (e.g.,
/// `["github", "google", "magic_link"]`). Clients should use this to display
/// login options.
#[tracing::instrument(skip(state))]
pub async fn get_providers(State(state): State<AppState>) -> impl IntoResponse {
	let mut providers = Vec::new();

	if state.github_oauth.is_some() {
		providers.push("github".to_string());
	}
	if state.google_oauth.is_some() {
		providers.push("google".to_string());
	}
	if state.okta_oauth.is_some() {
		providers.push("okta".to_string());
	}
	if state.smtp_client.is_some() {
		providers.push("magic_link".to_string());
	}

	Json(AuthProvidersResponse { providers })
}

#[utoipa::path(
    get,
    path = "/auth/me",
    responses(
        (status = 200, description = "Current authenticated user", body = CurrentUserResponse),
        (status = 401, description = "Not authenticated", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Returns the current authenticated user's information.
///
/// Requires a valid session cookie or access token. Returns user ID,
/// display name, email, and avatar URL.
///
/// # Errors
/// Returns 401 Unauthorized if the request lacks valid authentication.
#[tracing::instrument(skip(current_user))]
pub async fn get_current_user(RequireAuth(current_user): RequireAuth) -> impl IntoResponse {
	tracing::debug!(user_id = %current_user.user.id, "Retrieved current user");
	Json(CurrentUserResponse {
		id: current_user.user.id.to_string(),
		display_name: current_user.user.display_name.clone(),
		username: current_user.user.username.clone(),
		email: current_user.user.primary_email.clone(),
		avatar_url: current_user.user.avatar_url.clone(),
		locale: current_user.user.locale.clone(),
		global_roles: current_user
			.user
			.global_roles()
			.iter()
			.map(|r| r.to_string())
			.collect(),
		created_at: current_user.user.created_at,
	})
}

#[utoipa::path(
    get,
    path = "/auth/ws-token",
    responses(
        (status = 200, description = "WebSocket authentication token", body = WsTokenResponse),
        (status = 401, description = "Not authenticated", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Returns a short-lived token for WebSocket first-message authentication.
///
/// This endpoint solves the problem of HttpOnly session cookies not being
/// accessible to JavaScript. The returned token can be used in the WebSocket
/// first message: `{"type": "auth", "token": "ws_xxx"}`.
///
/// # Security
/// - Token expires in 30 seconds
/// - Token can only be used once (single-use)
/// - Requires valid session cookie authentication
///
/// # Usage
/// 1. Call this endpoint to get a token
/// 2. Connect to WebSocket at /api/ws/sessions/{session_id}
/// 3. Send first message: {"type": "auth", "token": "<token>"}
#[tracing::instrument(skip(state, current_user))]
pub async fn get_ws_token(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
) -> impl IntoResponse {
	use loom_server_auth::ws_token::{generate_ws_token, WS_TOKEN_EXPIRY_SECONDS};

	let (token, token_hash) = generate_ws_token();

	if let Err(e) = state
		.session_repo
		.create_ws_token(&current_user.user.id, &token_hash)
		.await
	{
		tracing::error!(error = %e, user_id = %current_user.user.id, "Failed to create WS token");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AuthErrorResponse {
				error: "token_creation_failed".to_string(),
				message: "Failed to create WebSocket token".to_string(),
			}),
		)
			.into_response();
	}

	tracing::debug!(user_id = %current_user.user.id, "WS token created");

	Json(WsTokenResponse {
		token,
		expires_in: WS_TOKEN_EXPIRY_SECONDS,
	})
	.into_response()
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    responses(
        (status = 200, description = "Logout successful", body = AuthSuccessResponse)
    ),
    tag = "auth"
)]
/// Logs out the current user and invalidates their session.
///
/// Deletes the server-side session (if session-based auth) and clears the
/// session cookie. Always returns success even if session deletion fails.
///
/// # Security
/// Session invalidation is best-effort; cookie is always cleared.
#[tracing::instrument(skip(state, current_user))]
pub async fn logout(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::Logout)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.build(),
	);

	tracing::info!(user_id = %current_user.user.id, "User logged out");
	// If session-based auth, delete the session
	if let Some(session_id) = current_user.session_id {
		if let Err(e) = state.session_repo.delete_session(&session_id).await {
			tracing::warn!(error = %e, "Failed to delete session during logout");
		}
	}

	// Build response with cleared session cookie
	let mut headers = HeaderMap::new();
	let cookie_name = &state.auth_config.session_cookie_name;
	let clear_cookie = format!("{cookie_name}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax");
	if let Ok(value) = HeaderValue::from_str(&clear_cookie) {
		headers.insert(SET_COOKIE, value);
	}

	(
		headers,
		Json(AuthSuccessResponse {
			message: t(locale, "server.api.auth.logged_out").to_string(),
		}),
	)
}

#[utoipa::path(
    post,
    path = "/auth/magic-link",
    request_body = MagicLinkRequest,
    responses(
        (status = 200, description = "Magic link sent", body = AuthSuccessResponse),
        (status = 400, description = "Invalid email", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Requests a magic link for passwordless login.
///
/// Generates a magic link token, stores it hashed in the database,
/// and sends an email to the user with the verification link.
///
/// # Security
/// Always returns success to prevent email enumeration attacks.
/// Magic link tokens are hashed with Argon2 before storage.
///
/// # Errors
/// Returns 400 Bad Request only for obviously invalid email format.
#[tracing::instrument(skip(state, payload))]
pub async fn request_magic_link(
	State(state): State<AppState>,
	Json(payload): Json<MagicLinkRequest>,
) -> impl IntoResponse {
	let locale = state.default_locale.as_str();
	let email = payload.email.trim().to_lowercase();

	// Basic email validation
	if !email.contains('@') || email.len() < 5 {
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_email".to_string(),
				message: t(locale, "server.api.auth.invalid_email").to_string(),
			}),
		)
			.into_response();
	}

	// Always return success to prevent email enumeration
	let success_response = Json(AuthSuccessResponse {
		message: t(locale, "server.api.auth.check_email").to_string(),
	});

	// Invalidate any existing magic links for this email
	if let Err(e) = state
		.session_repo
		.invalidate_magic_links_for_email(&email)
		.await
	{
		tracing::warn!(error = %e, email = %email, "Failed to invalidate existing magic links");
	}

	// Create new magic link
	let (magic_link, plaintext_token) = MagicLink::new(&email);

	// Store the magic link (hashed)
	if let Err(e) = state
		.session_repo
		.create_magic_link(&email, &magic_link.token_hash)
		.await
	{
		tracing::error!(error = %e, email = %email, "Failed to store magic link");
		return success_response.into_response();
	}

	// Send email if email service is configured
	let Some(email_service) = &state.email_service else {
		// SECURITY: Never log the plaintext token - it allows account takeover
		tracing::warn!(
			"Email service not configured, magic link not sent - user will not receive email"
		);
		return success_response.into_response();
	};

	let verification_url = format!(
		"{}/auth/magic-link/verify?token={}",
		state.base_url, plaintext_token
	);

	let request = loom_server_email::EmailRequest::MagicLink { verification_url };

	if let Err(e) = email_service.send(&email, request, None).await {
		tracing::error!(error = %e, email = %email, "Failed to send magic link email");
	} else {
		state.audit_service.log(
			AuditLogBuilder::new(AuditEventType::MagicLinkRequested)
				.details(serde_json::json!({
					"email": &email,
				}))
				.build(),
		);

		tracing::info!(email = %email, "Magic link email sent");
	}

	success_response.into_response()
}

#[utoipa::path(
    post,
    path = "/auth/device/start",
    responses(
        (status = 200, description = "Device code flow started", body = DeviceCodeStartResponse),
        (status = 500, description = "Internal error", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Starts the device code flow for CLI/VS Code authentication.
///
/// Returns a device code for polling and a user code for the user to enter
/// at the verification URL. The CLI polls with the device code while the
/// user authenticates in a browser with the user code.
///
/// # Security
/// Device codes expire after [`DEVICE_CODE_EXPIRY_MINUTES`].
/// Never log device_code or user_code as they grant authentication.
#[tracing::instrument(skip(state))]
pub async fn device_start(State(state): State<AppState>) -> impl IntoResponse {
	let locale = state.default_locale.as_str();
	let device_code = DeviceCode::new();
	tracing::info!("Device code flow started");

	if let Err(e) = state
		.session_repo
		.create_device_code(&device_code.device_code, &device_code.user_code)
		.await
	{
		tracing::error!(error = %e, "Failed to store device code");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AuthErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	let verification_url = format!("{}/device", state.base_url);

	state
		.audit_service
		.log(AuditLogBuilder::new(AuditEventType::DeviceCodeStarted).build());

	Json(DeviceCodeStartResponse {
		device_code: device_code.device_code,
		user_code: device_code.user_code,
		verification_url,
		expires_in: DEVICE_CODE_EXPIRY_MINUTES * 60,
	})
	.into_response()
}

#[utoipa::path(
    post,
    path = "/auth/device/poll",
    request_body = DeviceCodePollRequest,
    responses(
        (status = 200, description = "Device code status", body = DeviceCodePollResponse),
        (status = 400, description = "Invalid device code", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Polls the status of a device code authentication flow.
///
/// CLI calls this endpoint repeatedly until the user completes authentication
/// in the browser or the device code expires. Returns `pending`, `completed`
/// (with access token), or `expired`.
///
/// # Security
/// The device_code in the request body is a secret. Never log it.
/// Access tokens are only returned once on completion.
#[tracing::instrument(skip(state, payload))]
pub async fn device_poll(
	State(state): State<AppState>,
	Json(payload): Json<DeviceCodePollRequest>,
) -> impl IntoResponse {
	let locale = state.default_locale.as_str();
	if Uuid::parse_str(&payload.device_code).is_err() {
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_device_code".to_string(),
				message: t(locale, "server.api.auth.invalid_device_code").to_string(),
			}),
		)
			.into_response();
	}

	let device_code_result = match state
		.session_repo
		.get_device_code(&payload.device_code)
		.await
	{
		Ok(result) => result,
		Err(e) => {
			tracing::error!(error = %e, "Failed to get device code");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let Some((_user_code, user_id, is_completed)) = device_code_result else {
		return Json(DeviceCodePollResponse::Expired).into_response();
	};

	if is_completed {
		if let Some(user_id) = user_id {
			let (plaintext_token, token_hash) = generate_access_token();

			if let Err(e) = state
				.session_repo
				.create_access_token(&user_id, &token_hash, "device-code-auth", SessionType::Cli)
				.await
			{
				tracing::error!(error = %e, user_id = %user_id, "Failed to create access token");
				return (
					StatusCode::INTERNAL_SERVER_ERROR,
					Json(AuthErrorResponse {
						error: "internal_error".to_string(),
						message: t(locale, "server.api.error.internal").to_string(),
					}),
				)
					.into_response();
			}

			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::DeviceCodeCompleted)
					.actor(AuditUserId::new(user_id.into_inner()))
					.details(serde_json::json!({
						"session_type": "cli",
					}))
					.build(),
			);

			tracing::info!(user_id = %user_id, "Device code authentication completed");
			return Json(DeviceCodePollResponse::Completed {
				access_token: plaintext_token,
			})
			.into_response();
		}
	}

	Json(DeviceCodePollResponse::Pending).into_response()
}

#[utoipa::path(
    post,
    path = "/auth/device/complete",
    request_body = DeviceCodeCompleteRequest,
    responses(
        (status = 200, description = "Device code completed", body = DeviceCodeCompleteResponse),
        (status = 400, description = "Invalid or expired user code", body = AuthErrorResponse),
        (status = 401, description = "Not authenticated", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Completes device code flow by linking it to the authenticated user.
///
/// Called by the web UI after the user logs in and enters the user code.
/// Marks the device code as completed with the current user's ID, allowing
/// the polling CLI to receive an access token.
///
/// # Security
/// Requires authentication. User codes should not be logged.
///
/// # Errors
/// - 400 Bad Request: Invalid or expired user code
/// - 401 Unauthorized: Not authenticated
#[tracing::instrument(skip(state, current_user, payload))]
pub async fn device_complete(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Json(payload): Json<DeviceCodeCompleteRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_code = payload.user_code.trim();

	if user_code.is_empty() {
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_user_code".to_string(),
				message: t(locale, "server.api.auth.invalid_user_code").to_string(),
			}),
		)
			.into_response();
	}

	match state
		.session_repo
		.complete_device_code(user_code, &current_user.user.id)
		.await
	{
		Ok(true) => {
			// SECURITY: Don't log user_code as it's a secret
			tracing::info!(user_id = %current_user.user.id, "Device code completed by user");
			Json(DeviceCodeCompleteResponse {
				message: t(locale, "server.api.auth.device_authorized").to_string(),
			})
			.into_response()
		}
		Ok(false) => (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_user_code".to_string(),
				message: t(locale, "server.api.auth.invalid_user_code").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to complete device code");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

// =============================================================================
// OAuth provider endpoints
// =============================================================================

/// Response for OAuth login initiation.
#[derive(Debug, Serialize, ToSchema)]
pub struct OAuthRedirectResponse {
	pub redirect_url: String,
}

/// Query parameters for OAuth callback.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuthCallbackQuery {
	pub code: Option<String>,
	pub state: Option<String>,
	pub error: Option<String>,
}

/// Query parameters for magic link verification.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MagicLinkVerifyQuery {
	pub token: String,
}

/// Query parameters for OAuth login initiation.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OAuthLoginQuery {
	/// Optional redirect URL after successful authentication
	pub redirect: Option<String>,
}

#[utoipa::path(
    get,
    path = "/auth/login/github",
    params(
        ("redirect" = Option<String>, Query, description = "Redirect URL after successful authentication")
    ),
    responses(
        (status = 200, description = "GitHub OAuth redirect URL", body = OAuthRedirectResponse),
        (status = 501, description = "Not configured", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Initiates GitHub OAuth flow.
///
/// Returns a redirect URL to GitHub's authorization endpoint. The client
/// should redirect the user to this URL to begin the OAuth flow.
///
/// # Errors
/// Returns 501 Not Implemented if GitHub OAuth is not configured.
#[tracing::instrument(skip(state, query), fields(provider = "github"))]
pub async fn login_github(
	State(state): State<AppState>,
	axum::extract::Query(query): axum::extract::Query<OAuthLoginQuery>,
) -> impl IntoResponse {
	let locale = state.default_locale.as_str();
	tracing::debug!("Initiating GitHub OAuth flow");
	let Some(github_client) = &state.github_oauth else {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AuthErrorResponse {
				error: "not_configured".to_string(),
				message: t_fmt(
					locale,
					"server.api.auth.oauth_not_configured",
					&[("provider", "GitHub")],
				),
			}),
		)
			.into_response();
	};

	let oauth_state = generate_state();
	let redirect_url_param = query
		.redirect
		.as_deref()
		.map(|r| sanitize_redirect(Some(r)));
	state
		.oauth_state_store
		.store(
			oauth_state.clone(),
			"github".to_string(),
			None,
			redirect_url_param,
		)
		.await;

	let redirect_url = github_client.authorization_url(&oauth_state);

	Json(OAuthRedirectResponse { redirect_url }).into_response()
}

#[utoipa::path(
    get,
    path = "/auth/login/google",
    params(
        ("redirect" = Option<String>, Query, description = "Redirect URL after successful authentication")
    ),
    responses(
        (status = 200, description = "Google OAuth redirect URL", body = OAuthRedirectResponse),
        (status = 501, description = "Not configured", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Initiates Google OAuth flow.
///
/// Returns a redirect URL to Google's authorization endpoint. The client
/// should redirect the user to this URL to begin the OAuth flow.
///
/// # Errors
/// Returns 501 Not Implemented if Google OAuth is not configured.
#[tracing::instrument(skip(state, query), fields(provider = "google"))]
pub async fn login_google(
	State(state): State<AppState>,
	axum::extract::Query(query): axum::extract::Query<OAuthLoginQuery>,
) -> impl IntoResponse {
	let locale = state.default_locale.as_str();
	tracing::debug!("Initiating Google OAuth flow");
	let Some(google_client) = &state.google_oauth else {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AuthErrorResponse {
				error: "not_configured".to_string(),
				message: t_fmt(
					locale,
					"server.api.auth.oauth_not_configured",
					&[("provider", "Google")],
				),
			}),
		)
			.into_response();
	};

	let oauth_state = generate_state();
	let nonce = generate_nonce();
	let redirect_url_param = query
		.redirect
		.as_deref()
		.map(|r| sanitize_redirect(Some(r)));
	state
		.oauth_state_store
		.store(
			oauth_state.clone(),
			"google".to_string(),
			Some(nonce.clone()),
			redirect_url_param,
		)
		.await;

	let redirect_url = google_client.authorization_url(&oauth_state, &nonce);

	Json(OAuthRedirectResponse { redirect_url }).into_response()
}

#[utoipa::path(
    get,
    path = "/auth/login/okta",
    params(
        ("redirect" = Option<String>, Query, description = "Redirect URL after successful authentication")
    ),
    responses(
        (status = 200, description = "Okta OAuth redirect URL", body = OAuthRedirectResponse),
        (status = 501, description = "Not configured", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Initiates Okta OAuth flow.
///
/// Returns a redirect URL to Okta's authorization endpoint. The client
/// should redirect the user to this URL to begin the OAuth flow.
///
/// # Errors
/// Returns 501 Not Implemented if Okta OAuth is not configured.
#[tracing::instrument(skip(state, query), fields(provider = "okta"))]
pub async fn login_okta(
	State(state): State<AppState>,
	axum::extract::Query(query): axum::extract::Query<OAuthLoginQuery>,
) -> impl IntoResponse {
	let locale = state.default_locale.as_str();
	tracing::debug!("Initiating Okta OAuth flow");
	let Some(okta_client) = &state.okta_oauth else {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AuthErrorResponse {
				error: "not_configured".to_string(),
				message: t_fmt(
					locale,
					"server.api.auth.oauth_not_configured",
					&[("provider", "Okta")],
				),
			}),
		)
			.into_response();
	};

	let oauth_state = generate_state();
	let nonce = generate_nonce();
	let redirect_url_param = query
		.redirect
		.as_deref()
		.map(|r| sanitize_redirect(Some(r)));
	state
		.oauth_state_store
		.store(
			oauth_state.clone(),
			"okta".to_string(),
			Some(nonce.clone()),
			redirect_url_param,
		)
		.await;

	let redirect_url = okta_client.authorization_url(&oauth_state, &nonce);

	Json(OAuthRedirectResponse { redirect_url }).into_response()
}

#[utoipa::path(
    get,
    path = "/auth/github/callback",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from GitHub"),
        ("state" = Option<String>, Query, description = "State parameter for CSRF protection"),
        ("error" = Option<String>, Query, description = "Error code if authorization failed")
    ),
    responses(
        (status = 302, description = "Redirect to dashboard on success"),
        (status = 400, description = "Invalid callback", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Handles the GitHub OAuth callback.
///
/// Exchanges the authorization code for tokens and creates a session.
/// On success, redirects to dashboard with session cookie set.
///
/// # Security
/// Authorization codes are single-use. Never log the code or access token.
///
/// # Errors
/// - 400 Bad Request: Missing code/state, invalid state, token exchange failed
#[tracing::instrument(skip(state, query, headers), fields(provider = "github"))]
pub async fn callback_github(
	State(state): State<AppState>,
	headers: HeaderMap,
	axum::extract::Query(query): axum::extract::Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
	let client_info = ClientInfo::from_headers(&headers, state.geoip_service.as_ref());
	let locale = state.default_locale.as_str();
	if let Some(error) = &query.error {
		tracing::warn!(error = %error, "GitHub OAuth error");
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "oauth_error".to_string(),
				message: format!("GitHub authorization failed: {error}"),
			}),
		)
			.into_response();
	}

	let Some(github_client) = &state.github_oauth else {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AuthErrorResponse {
				error: "not_configured".to_string(),
				message: t_fmt(
					locale,
					"server.api.auth.oauth_not_configured",
					&[("provider", "GitHub")],
				),
			}),
		)
			.into_response();
	};

	let (Some(code), Some(oauth_state)) = (&query.code, &query.state) else {
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_request".to_string(),
				message: "Missing code or state parameter".to_string(),
			}),
		)
			.into_response();
	};

	let state_entry = match state
		.oauth_state_store
		.validate_and_consume(oauth_state, "github")
		.await
	{
		Some(entry) => entry,
		None => {
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "invalid_state".to_string(),
					message: "Invalid or expired state parameter".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token_response = match github_client.exchange_code(code).await {
		Ok(resp) => resp,
		Err(e) => {
			tracing::error!(error = %e, "Failed to exchange GitHub code");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "token_exchange_failed".to_string(),
					message: "Failed to exchange authorization code".to_string(),
				}),
			)
				.into_response();
		}
	};

	let github_user = match github_client
		.get_user(token_response.access_token.expose())
		.await
	{
		Ok(user) => user,
		Err(e) => {
			tracing::error!(error = %e, "Failed to get GitHub user");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "user_fetch_failed".to_string(),
					message: "Failed to fetch user information".to_string(),
				}),
			)
				.into_response();
		}
	};

	let email = if let Some(email) = &github_user.email {
		email.clone()
	} else {
		match github_client
			.get_emails(token_response.access_token.expose())
			.await
		{
			Ok(emails) => {
				match emails
					.into_iter()
					.find(|e| e.primary && e.verified)
					.map(|e| e.email)
				{
					Some(email) => email,
					None => {
						tracing::warn!(login = %github_user.login, "No verified email found for GitHub user");
						return (
							StatusCode::BAD_REQUEST,
							Json(AuthErrorResponse {
								error: "no_verified_email".to_string(),
								message: t(locale, "server.api.auth.no_verified_email").to_string(),
							}),
						)
							.into_response();
					}
				}
			}
			Err(e) => {
				tracing::warn!(error = %e, login = %github_user.login, "Failed to fetch GitHub emails");
				return (
					StatusCode::BAD_REQUEST,
					Json(AuthErrorResponse {
						error: "no_verified_email".to_string(),
						message: t(locale, "server.api.auth.no_verified_email").to_string(),
					}),
				)
					.into_response();
			}
		}
	};

	let display_name = github_user
		.name
		.unwrap_or_else(|| github_user.login.clone());

	let preferred_username = github_user.login.clone();

	complete_oauth_login(
		&state,
		&email,
		&display_name,
		github_user.avatar_url,
		AuthMethod::GitHub,
		client_info,
		state_entry.redirect_url,
		Some(&preferred_username),
	)
	.await
}

#[utoipa::path(
    get,
    path = "/auth/google/callback",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from Google"),
        ("state" = Option<String>, Query, description = "State parameter for CSRF protection"),
        ("error" = Option<String>, Query, description = "Error code if authorization failed")
    ),
    responses(
        (status = 302, description = "Redirect to dashboard on success"),
        (status = 400, description = "Invalid callback", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Handles the Google OAuth callback.
///
/// Exchanges the authorization code for tokens and creates a session.
/// On success, redirects to dashboard with session cookie set.
///
/// # Security
/// Authorization codes are single-use. Never log the code or access token.
///
/// # Errors
/// - 400 Bad Request: Missing code/state, invalid state, token exchange failed
#[tracing::instrument(skip(state, query, headers), fields(provider = "google"))]
pub async fn callback_google(
	State(state): State<AppState>,
	headers: HeaderMap,
	axum::extract::Query(query): axum::extract::Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
	let client_info = ClientInfo::from_headers(&headers, state.geoip_service.as_ref());
	let locale = state.default_locale.as_str();
	if let Some(error) = &query.error {
		tracing::warn!(error = %error, "Google OAuth error");
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "oauth_error".to_string(),
				message: format!("Google authorization failed: {error}"),
			}),
		)
			.into_response();
	}

	let Some(google_client) = &state.google_oauth else {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AuthErrorResponse {
				error: "not_configured".to_string(),
				message: t_fmt(
					locale,
					"server.api.auth.oauth_not_configured",
					&[("provider", "Google")],
				),
			}),
		)
			.into_response();
	};

	let (Some(code), Some(oauth_state)) = (&query.code, &query.state) else {
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_request".to_string(),
				message: "Missing code or state parameter".to_string(),
			}),
		)
			.into_response();
	};

	let state_entry = match state
		.oauth_state_store
		.validate_and_consume(oauth_state, "google")
		.await
	{
		Some(entry) => entry,
		None => {
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "invalid_state".to_string(),
					message: "Invalid or expired state parameter".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token_response = match google_client.exchange_code(code).await {
		Ok(resp) => resp,
		Err(e) => {
			tracing::error!(error = %e, "Failed to exchange Google code");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "token_exchange_failed".to_string(),
					message: "Failed to exchange authorization code".to_string(),
				}),
			)
				.into_response();
		}
	};

	let google_user = match google_client
		.get_user_info(token_response.access_token.expose())
		.await
	{
		Ok(user) => user,
		Err(e) => {
			tracing::error!(error = %e, "Failed to get Google user");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "user_fetch_failed".to_string(),
					message: "Failed to fetch user information".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !google_user.email_verified {
		tracing::warn!(email = %google_user.email, "Google email not verified");
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "email_not_verified".to_string(),
				message: t(locale, "server.api.auth.email_not_verified").to_string(),
			}),
		)
			.into_response();
	}

	let display_name = google_user
		.name
		.unwrap_or_else(|| google_user.email.clone());

	complete_oauth_login(
		&state,
		&google_user.email,
		&display_name,
		google_user.picture,
		AuthMethod::Google,
		client_info,
		state_entry.redirect_url,
		None,
	)
	.await
}

#[utoipa::path(
    get,
    path = "/auth/okta/callback",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from Okta"),
        ("state" = Option<String>, Query, description = "State parameter for CSRF protection"),
        ("error" = Option<String>, Query, description = "Error code if authorization failed")
    ),
    responses(
        (status = 302, description = "Redirect to dashboard on success"),
        (status = 400, description = "Invalid callback", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Handles the Okta OAuth callback.
///
/// Exchanges the authorization code for tokens and creates a session.
/// On success, redirects to dashboard with session cookie set.
///
/// # Security
/// Authorization codes are single-use. Never log the code or access token.
///
/// # Errors
/// - 400 Bad Request: Missing code/state, invalid state, token exchange failed
#[tracing::instrument(skip(state, query, headers), fields(provider = "okta"))]
pub async fn callback_okta(
	State(state): State<AppState>,
	headers: HeaderMap,
	axum::extract::Query(query): axum::extract::Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
	let client_info = ClientInfo::from_headers(&headers, state.geoip_service.as_ref());
	let locale = state.default_locale.as_str();
	if let Some(error) = &query.error {
		tracing::warn!(error = %error, "Okta OAuth error");
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "oauth_error".to_string(),
				message: format!("Okta authorization failed: {error}"),
			}),
		)
			.into_response();
	}

	let Some(okta_client) = &state.okta_oauth else {
		return (
			StatusCode::NOT_IMPLEMENTED,
			Json(AuthErrorResponse {
				error: "not_configured".to_string(),
				message: t_fmt(
					locale,
					"server.api.auth.oauth_not_configured",
					&[("provider", "Okta")],
				),
			}),
		)
			.into_response();
	};

	let (Some(code), Some(oauth_state)) = (&query.code, &query.state) else {
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "invalid_request".to_string(),
				message: "Missing code or state parameter".to_string(),
			}),
		)
			.into_response();
	};

	let state_entry = match state
		.oauth_state_store
		.validate_and_consume(oauth_state, "okta")
		.await
	{
		Some(entry) => entry,
		None => {
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "invalid_state".to_string(),
					message: "Invalid or expired state parameter".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token_response = match okta_client.exchange_code(code).await {
		Ok(resp) => resp,
		Err(e) => {
			tracing::error!(error = %e, "Failed to exchange Okta code");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "token_exchange_failed".to_string(),
					message: "Failed to exchange authorization code".to_string(),
				}),
			)
				.into_response();
		}
	};

	let okta_user = match okta_client
		.get_user_info(token_response.access_token.expose())
		.await
	{
		Ok(user) => user,
		Err(e) => {
			tracing::error!(error = %e, "Failed to get Okta user");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "user_fetch_failed".to_string(),
					message: "Failed to fetch user information".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !okta_user.email_verified.unwrap_or(false) {
		tracing::warn!(email = %okta_user.email, "Okta email not verified");
		return (
			StatusCode::BAD_REQUEST,
			Json(AuthErrorResponse {
				error: "email_not_verified".to_string(),
				message: t(locale, "server.api.auth.email_not_verified").to_string(),
			}),
		)
			.into_response();
	}

	let display_name = okta_user.name.unwrap_or_else(|| okta_user.email.clone());

	complete_oauth_login(
		&state,
		&okta_user.email,
		&display_name,
		None,
		AuthMethod::Okta,
		client_info,
		state_entry.redirect_url,
		okta_user.preferred_username.as_deref(),
	)
	.await
}

/// Complete OAuth login by finding/creating user and creating session.
#[allow(clippy::too_many_arguments)]
async fn complete_oauth_login(
	state: &AppState,
	email: &str,
	display_name: &str,
	avatar_url: Option<String>,
	auth_method: AuthMethod,
	client_info: ClientInfo,
	redirect_url: Option<String>,
	preferred_username: Option<&str>,
) -> axum::response::Response {
	let locale = state.default_locale.as_str();

	let request = loom_server_provisioning::ProvisioningRequest::oauth(
		email,
		display_name,
		avatar_url,
		preferred_username.map(|s| s.to_string()),
	);
	let user = match state.user_provisioning.provision_user(request).await {
		Ok(user) => user,
		Err(loom_server_provisioning::ProvisioningError::SignupsDisabled) => {
			return Redirect::to("/login?error=signups_disabled").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, email = %email, "Failed to provision user");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let session_request =
		SessionRequest::new(user.id, auth_method, client_info.into()).with_email(email);

	let session_response = match state.session_service.create_session(session_request).await {
		Ok(resp) => resp,
		Err(e) => {
			tracing::error!(error = %e, user_id = %user.id, "Failed to create session");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	tracing::info!(user_id = %user.id, email = %email, auth_method = %auth_method, "User authenticated via OAuth");

	let mut headers = HeaderMap::new();
	if let Ok(value) = HeaderValue::from_str(&session_response.cookie_header) {
		headers.insert(SET_COOKIE, value);
	}

	let final_redirect = sanitize_redirect(redirect_url.as_deref());
	(headers, Redirect::to(&final_redirect)).into_response()
}

#[utoipa::path(
    get,
    path = "/auth/magic-link/verify",
    params(
        ("token" = String, Query, description = "Magic link token from email")
    ),
    responses(
        (status = 302, description = "Redirect to dashboard on success"),
        (status = 400, description = "Invalid or expired token", body = AuthErrorResponse)
    ),
    tag = "auth"
)]
/// Verifies a magic link token and creates a session.
///
/// Validates the magic link token from the query parameter, creates a session
/// for the user, and redirects to the dashboard with a session cookie.
///
/// # Security
/// Magic link tokens are single-use and hashed with Argon2 for storage.
/// Never log the token from the URL.
///
/// # Errors
/// Returns 400 Bad Request if the token is invalid or expired.
#[tracing::instrument(skip(state, query, headers))]
pub async fn verify_magic_link(
	State(state): State<AppState>,
	headers: HeaderMap,
	axum::extract::Query(query): axum::extract::Query<MagicLinkVerifyQuery>,
) -> impl IntoResponse {
	let client_info = ClientInfo::from_headers(&headers, state.geoip_service.as_ref());
	let locale = state.default_locale.as_str();
	let token = &query.token;

	// Get all pending magic links and verify against each using Argon2
	let pending_links = match state.session_repo.get_pending_magic_links().await {
		Ok(links) => links,
		Err(e) => {
			tracing::error!(error = %e, "Failed to get pending magic links");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	// Find matching magic link using Argon2 verification
	let matching_link = pending_links
		.into_iter()
		.find(|(_, _, stored_hash)| verify_magic_link_token(token, stored_hash));

	let (link_id, email) = match matching_link {
		Some((id, email, _)) => (id, email),
		None => {
			tracing::debug!("Magic link not found or invalid token");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "invalid_token".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	// Atomically claim the magic link to prevent TOCTOU race conditions.
	// Only one request can successfully claim a link - if another request
	// verified the same link concurrently, this will return false.
	match state.session_repo.claim_magic_link(&link_id).await {
		Ok(true) => {}
		Ok(false) => {
			tracing::debug!(link_id = %link_id, "Magic link already claimed by another request");
			return (
				StatusCode::BAD_REQUEST,
				Json(AuthErrorResponse {
					error: "invalid_token".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, link_id = %link_id, "Failed to claim magic link");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	}

	// Provision user
	let request = loom_server_provisioning::ProvisioningRequest::magic_link(&email);
	let user = match state.user_provisioning.provision_user(request).await {
		Ok(user) => user,
		Err(loom_server_provisioning::ProvisioningError::SignupsDisabled) => {
			return Redirect::to("/login?error=signups_disabled").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, email = %email, "Failed to provision user");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let session_request =
		SessionRequest::new(user.id, AuthMethod::MagicLink, client_info.into()).with_email(&email);

	let session_response = match state.session_service.create_session(session_request).await {
		Ok(resp) => resp,
		Err(e) => {
			tracing::error!(error = %e, user_id = %user.id, "Failed to create session");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AuthErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	tracing::info!(user_id = %user.id, email = %email, "User authenticated via magic link");

	let mut response_headers = HeaderMap::new();
	if let Ok(value) = HeaderValue::from_str(&session_response.cookie_header) {
		response_headers.insert(SET_COOKIE, value);
	}

	(response_headers, Redirect::to("/")).into_response()
}

#[utoipa::path(
    get,
    path = "/auth/device",
    responses(
        (status = 200, description = "Device code entry page")
    ),
    tag = "auth"
)]
/// Serves the device code entry page.
///
/// Returns HTML for the device code entry page where users enter the code
/// displayed by CLI/VS Code to complete authentication.
#[tracing::instrument(skip(headers, state))]
pub async fn device_page(headers: HeaderMap, State(state): State<AppState>) -> impl IntoResponse {
	let locale = resolve_locale_from_headers(&headers, &state.default_locale);
	let dir = if loom_common_i18n::is_rtl(locale) {
		"rtl"
	} else {
		"ltr"
	};

	let title = loom_common_i18n::t(locale, "server.auth.device.title");
	let heading = loom_common_i18n::t(locale, "server.auth.device.heading");
	let subtitle = loom_common_i18n::t(locale, "server.auth.device.subtitle");
	let code_label = loom_common_i18n::t(locale, "server.auth.device.code_label");
	let submit = loom_common_i18n::t(locale, "server.auth.device.submit");
	let authorizing = loom_common_i18n::t(locale, "server.auth.device.authorizing");
	let authorized = loom_common_i18n::t(locale, "server.auth.device.authorized");
	let expiry_help = loom_common_i18n::t(locale, "server.auth.device.expiry_help");
	let success_msg = loom_common_i18n::t(locale, "server.auth.device.success");
	let invalid_format = loom_common_i18n::t(locale, "server.auth.device.invalid_format");
	let error_msg = loom_common_i18n::t(locale, "server.auth.device.error");
	let auth_failed = loom_common_i18n::t(locale, "server.auth.device.auth_failed");

	let html = format!(
		r#"<!DOCTYPE html>
<html lang="{locale}" dir="{dir}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: #f5f5f5;
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 20px;
        }}
        .container {{
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
            padding: 40px;
            max-width: 400px;
            width: 100%;
        }}
        h1 {{ font-size: 24px; font-weight: 600; color: #1a1a1a; margin-bottom: 8px; text-align: center; }}
        .subtitle {{ color: #666; font-size: 14px; text-align: center; margin-bottom: 32px; }}
        .form-group {{ margin-bottom: 24px; }}
        label {{ display: block; font-size: 14px; font-weight: 500; color: #333; margin-bottom: 8px; }}
        input[type="text"] {{
            width: 100%;
            padding: 12px 16px;
            font-size: 18px;
            font-family: monospace;
            letter-spacing: 2px;
            text-align: center;
            border: 1px solid #ddd;
            border-radius: 6px;
            outline: none;
            transition: border-color 0.2s;
        }}
        input[type="text"]:focus {{ border-color: #0066cc; }}
        input[type="text"]::placeholder {{ color: #bbb; letter-spacing: 2px; }}
        button {{
            width: 100%;
            padding: 14px;
            font-size: 16px;
            font-weight: 500;
            color: white;
            background: #0066cc;
            border: none;
            border-radius: 6px;
            cursor: pointer;
            transition: background 0.2s;
        }}
        button:hover {{ background: #0052a3; }}
        button:disabled {{ background: #ccc; cursor: not-allowed; }}
        .message {{ padding: 12px 16px; border-radius: 6px; font-size: 14px; margin-bottom: 24px; text-align: center; }}
        .message.success {{ background: #e6f4ea; color: #1e7e34; border: 1px solid #b7dfb9; }}
        .message.error {{ background: #fce8e6; color: #c5221f; border: 1px solid #f5c6c4; }}
        .message.hidden {{ display: none; }}
        .help-text {{ font-size: 12px; color: #888; text-align: center; margin-top: 24px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>{heading}</h1>
        <p class="subtitle">{subtitle}</p>
        <div id="message" class="message hidden"></div>
        <form id="deviceForm">
            <div class="form-group">
                <label for="userCode">{code_label}</label>
                <input type="text" id="userCode" name="user_code" placeholder="XXX-XXX-XXX"
                    maxlength="11" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false" required>
            </div>
            <button type="submit" id="submitBtn">{submit}</button>
        </form>
        <p class="help-text">{expiry_help}</p>
    </div>
    <script>
        const MSGS = {{
            authorizing: "{authorizing}",
            authorized: "{authorized}",
            submit: "{submit}",
            success: "{success_msg}",
            invalidFormat: "{invalid_format}",
            error: "{error_msg}",
            authFailed: "{auth_failed}"
        }};
        const form = document.getElementById('deviceForm');
        const input = document.getElementById('userCode');
        const submitBtn = document.getElementById('submitBtn');
        const messageDiv = document.getElementById('message');
        input.addEventListener('input', function(e) {{
            let value = e.target.value.replace(/[^0-9]/g, '');
            if (value.length > 9) value = value.slice(0, 9);
            let formatted = '';
            for (let i = 0; i < value.length; i++) {{
                if (i > 0 && i % 3 === 0) formatted += '-';
                formatted += value[i];
            }}
            e.target.value = formatted;
        }});
        function showMessage(text, isError) {{
            messageDiv.textContent = text;
            messageDiv.className = 'message ' + (isError ? 'error' : 'success');
        }}
        form.addEventListener('submit', async function(e) {{
            e.preventDefault();
            const userCode = input.value.trim();
            if (!/^\d{{3}}-\d{{3}}-\d{{3}}$/.test(userCode)) {{
                showMessage(MSGS.invalidFormat, true);
                return;
            }}
            submitBtn.disabled = true;
            submitBtn.textContent = MSGS.authorizing;
            messageDiv.className = 'message hidden';
            try {{
                const response = await fetch('/auth/device/complete', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ user_code: userCode }}),
                    credentials: 'include'
                }});
                const data = await response.json();
                if (response.ok) {{
                    showMessage(MSGS.success, false);
                    input.disabled = true;
                    submitBtn.textContent = MSGS.authorized;
                }} else {{
                    showMessage(data.message || MSGS.authFailed, true);
                    submitBtn.disabled = false;
                    submitBtn.textContent = MSGS.submit;
                }}
            }} catch (err) {{
                showMessage(MSGS.error, true);
                submitBtn.disabled = false;
                submitBtn.textContent = MSGS.submit;
            }}
        }});
        input.focus();
    </script>
</body>
</html>"#
	);

	(
		StatusCode::OK,
		[("Content-Type", "text/html; charset=utf-8")],
		html,
	)
}

fn resolve_locale_from_headers<'a>(headers: &HeaderMap, default_locale: &'a str) -> &'a str {
	if let Some(accept_lang) = headers.get("Accept-Language").and_then(|v| v.to_str().ok()) {
		for part in accept_lang.split(',') {
			let lang = part.split(';').next().unwrap_or("").trim();
			let lang_base = lang.split('-').next().unwrap_or(lang);
			if loom_common_i18n::is_supported(lang_base) {
				return match lang_base {
					"en" => "en",
					"es" => "es",
					"ar" => "ar",
					_ => default_locale,
				};
			}
		}
	}
	default_locale
}
