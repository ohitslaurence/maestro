// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Authentication middleware for Axum.
//!
//! This module provides middleware and extractors for authenticating requests
//! via session cookies or bearer tokens (API keys and access tokens).
//!
//! # Security Properties
//!
//! - **Token Protection**: All tokens are hashed with SHA-256 before database lookup;
//!   raw tokens are never stored or logged.
//! - **Session Expiry**: Sessions are validated against expiry timestamps on every request.
//! - **API Key Revocation**: Revoked API keys are rejected immediately.
//! - **Dev Mode Bypass**: In development mode (`LOOM_SERVER_AUTH_DEV_MODE=1`), unauthenticated
//!   requests are automatically authenticated as the dev user. This MUST NOT be enabled
//!   in production.
//!
//! # Usage
//!
//! Add the [`auth_layer`] middleware to your router to extract authentication context:
//!
//! ```ignore
//! use axum::Router;
//! use axum::middleware::from_fn_with_state;
//!
//! let app = Router::new()
//!     .route("/api/protected", get(protected_handler))
//!     .layer(from_fn_with_state(state.clone(), auth_layer));
//! ```
//!
//! Then use [`RequireAuth`] or [`OptionalAuth`] extractors in handlers:
//!
//! ```ignore
//! async fn protected_handler(RequireAuth(user): RequireAuth) -> impl IntoResponse {
//!     format!("Hello, {}!", user.user.display_name)
//! }
//! ```

use axum::{
	body::Body,
	extract::{FromRequestParts, State},
	http::{request::Parts, Request, StatusCode},
	middleware::Next,
	response::{IntoResponse, Response},
	Json,
};
use chrono::Utc;
use loom_server_auth::{
	hash_token,
	middleware::{
		extract_bearer_token, extract_session_cookie_with_name, identify_bearer_token, AuthContext,
		BearerTokenType, CurrentUser,
	},
};
use std::sync::Arc;
use tracing::instrument;

use crate::{
	api::AppState,
	db::{ApiKeyRepository, SessionRepository, UserRepository},
	error::ErrorResponse,
};

/// Authentication middleware that extracts auth context from requests.
///
/// This middleware:
/// 1. Extracts session cookie or bearer token from request headers
/// 2. Validates the token against the appropriate repository
/// 3. Stores `AuthContext` as a request extension for downstream handlers
///
/// # Token Priority
///
/// 1. Session cookie (web browser sessions)
/// 2. Bearer token (API keys or access tokens for CLI/IDE)
///
/// # Dev Mode
///
/// In dev mode (`LOOM_SERVER_AUTH_DEV_MODE=1`), if no valid authentication is provided,
/// automatically authenticates as the dev user with full admin privileges.
/// **WARNING**: Never enable dev mode in production.
///
/// # Security
///
/// - Tokens are immediately hashed; raw tokens are never logged
/// - Failed authentication attempts are logged at debug level (no token details)
/// - Successful authentication logs user_id only
#[instrument(
	name = "auth_layer",
	skip(state, request, next),
	fields(
		auth_method = tracing::field::Empty,
		user_id = tracing::field::Empty,
	)
)]
pub async fn auth_layer(
	State(state): State<AppState>,
	mut request: Request<Body>,
	next: Next,
) -> Response {
	let headers = request.headers();
	let span = tracing::Span::current();

	// Try session cookie first
	if let Some(session_token) =
		extract_session_cookie_with_name(headers, &state.auth_config.session_cookie_name)
	{
		if let Some(auth_ctx) =
			authenticate_session(&session_token, &state.session_repo, &state.user_repo).await
		{
			if let Some(ref user) = auth_ctx.current_user {
				span.record("auth_method", "session");
				span.record("user_id", tracing::field::display(&user.user.id));
			}
			request.extensions_mut().insert(auth_ctx);
			return next.run(request).await;
		}
	}

	// Try bearer token
	if let Some(bearer_token) = extract_bearer_token(headers) {
		match identify_bearer_token(&bearer_token) {
			BearerTokenType::ApiKey => {
				if let Some(auth_ctx) =
					authenticate_api_key(&bearer_token, &state.api_key_repo, &state.user_repo).await
				{
					if let Some(ref user) = auth_ctx.current_user {
						span.record("auth_method", "api_key");
						span.record("user_id", tracing::field::display(&user.user.id));
					}
					request.extensions_mut().insert(auth_ctx);
					return next.run(request).await;
				}
			}
			BearerTokenType::AccessToken => {
				if let Some(auth_ctx) =
					authenticate_access_token(&bearer_token, &state.session_repo, &state.user_repo).await
				{
					if let Some(ref user) = auth_ctx.current_user {
						span.record("auth_method", "access_token");
						span.record("user_id", tracing::field::display(&user.user.id));
					}
					request.extensions_mut().insert(auth_ctx);
					return next.run(request).await;
				}
			}
			BearerTokenType::Unknown => {
				tracing::debug!("Unknown bearer token type");
			}
			BearerTokenType::WsToken => {
				tracing::debug!("WS tokens are only valid for WebSocket auth, not HTTP");
			}
		}
	}

	// Dev mode: auto-authenticate as dev user if no valid auth provided
	if state.auth_config.dev_mode {
		if let Some(ref dev_user) = state.dev_user {
			span.record("auth_method", "dev_mode");
			span.record("user_id", tracing::field::display(&dev_user.id));
			tracing::warn!("⚠️  DEV MODE AUTHENTICATION ENABLED - DO NOT USE IN PRODUCTION ⚠️");
			tracing::debug!("Dev mode: authenticating as dev user");
			let current_user = CurrentUser::from_access_token(dev_user.clone());
			request
				.extensions_mut()
				.insert(AuthContext::authenticated(current_user));
			return next.run(request).await;
		}
	}

	// No valid authentication found - store unauthenticated context
	span.record("auth_method", "none");
	request
		.extensions_mut()
		.insert(AuthContext::unauthenticated());
	next.run(request).await
}

/// Authenticate via session cookie.
///
/// Validates the session token against the database and returns an authenticated
/// context if the session is valid and not expired.
///
/// # Security
///
/// - Token is hashed before database lookup
/// - Session expiry is checked
/// - User existence is verified
/// - Last-used timestamp is updated asynchronously
#[instrument(skip(session_token, session_repo, user_repo), fields(session_id = tracing::field::Empty))]
async fn authenticate_session(
	session_token: &str,
	session_repo: &Arc<SessionRepository>,
	user_repo: &Arc<UserRepository>,
) -> Option<AuthContext> {
	let token_hash = hash_token(session_token);

	let session = match session_repo.get_session_by_token_hash(&token_hash).await {
		Ok(Some(session)) => session,
		Ok(None) => {
			tracing::debug!("Session not found for token hash");
			return None;
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to look up session");
			return None;
		}
	};

	tracing::Span::current().record("session_id", tracing::field::display(&session.id));

	// Check if session is expired
	if session.expires_at < Utc::now() {
		tracing::debug!(session_id = %session.id, "Session expired");
		return None;
	}

	// Get the user
	let user = match user_repo.get_user_by_id(&session.user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			tracing::warn!(user_id = %session.user_id, "User not found for valid session");
			return None;
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	// Update session last used (fire and forget)
	let session_id = session.id;
	let session_repo = session_repo.clone();
	tokio::spawn(async move {
		if let Err(e) = session_repo.update_session_last_used(&session_id).await {
			tracing::warn!(error = %e, "Failed to update session last used");
		}
	});

	let current_user = CurrentUser::from_session(user, session.id);
	Some(AuthContext::authenticated(current_user))
}

/// Authenticate via API key bearer token.
///
/// Validates the API key against the database and returns an authenticated
/// context if the key is valid and not revoked.
///
/// # Security
///
/// - Token is hashed before database lookup
/// - Revocation status is checked
/// - User existence is verified
/// - Last-used timestamp is updated asynchronously
#[instrument(skip(api_key_token, api_key_repo, user_repo), fields(api_key_id = tracing::field::Empty))]
async fn authenticate_api_key(
	api_key_token: &str,
	api_key_repo: &Arc<ApiKeyRepository>,
	user_repo: &Arc<UserRepository>,
) -> Option<AuthContext> {
	let token_hash = hash_token(api_key_token);

	let api_key = match api_key_repo.get_api_key_by_hash(&token_hash).await {
		Ok(Some(key)) => key,
		Ok(None) => {
			tracing::debug!("API key not found for token hash");
			return None;
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to look up API key");
			return None;
		}
	};

	tracing::Span::current().record("api_key_id", tracing::field::display(&api_key.id));

	// Check if key is revoked
	if api_key.revoked_at.is_some() {
		tracing::debug!(api_key_id = %api_key.id, "API key is revoked");
		return None;
	}

	// Get the user who created the key
	let user = match user_repo.get_user_by_id(&api_key.created_by).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			tracing::warn!(user_id = %api_key.created_by, "User not found for API key");
			return None;
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	// Update API key last used (fire and forget)
	let api_key_id = api_key.id.to_string();
	let api_key_repo = api_key_repo.clone();
	tokio::spawn(async move {
		if let Err(e) = api_key_repo.update_last_used(&api_key_id).await {
			tracing::warn!(error = %e, "Failed to update API key last used");
		}
	});

	let current_user = CurrentUser::from_api_key(user, api_key.id.into_inner());
	Some(AuthContext::authenticated(current_user))
}

/// Authenticate via access token bearer token (CLI/VS Code).
///
/// Validates the access token against the database and returns an authenticated
/// context if the token is valid.
///
/// # Security
///
/// - Token is hashed before database lookup
/// - User existence is verified
/// - Last-used timestamp is updated asynchronously
#[instrument(skip(access_token, session_repo, user_repo), fields(token_id = tracing::field::Empty))]
async fn authenticate_access_token(
	access_token: &str,
	session_repo: &Arc<SessionRepository>,
	user_repo: &Arc<UserRepository>,
) -> Option<AuthContext> {
	let token_hash = hash_token(access_token);

	let (token_id, user_id) = match session_repo.get_access_token_by_hash(&token_hash).await {
		Ok(Some((id, uid))) => (id, uid),
		Ok(None) => {
			tracing::debug!("Access token not found for token hash");
			return None;
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to look up access token");
			return None;
		}
	};

	tracing::Span::current().record("token_id", tracing::field::display(&token_id));

	// Get the user
	let user = match user_repo.get_user_by_id(&user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			tracing::warn!(user_id = %user_id, "User not found for access token");
			return None;
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	// Update access token last used (fire and forget)
	let session_repo = session_repo.clone();
	tokio::spawn(async move {
		if let Err(e) = session_repo.update_access_token_last_used(&token_id).await {
			tracing::warn!(error = %e, "Failed to update access token last used");
		}
	});

	let current_user = CurrentUser::from_access_token(user);
	Some(AuthContext::authenticated(current_user))
}

pub async fn require_auth_layer(request: Request<Body>, next: Next) -> Response {
	let auth_ctx = request
		.extensions()
		.get::<AuthContext>()
		.cloned()
		.unwrap_or_else(AuthContext::unauthenticated);

	if auth_ctx.current_user.is_some() {
		next.run(request).await
	} else {
		(
			StatusCode::UNAUTHORIZED,
			Json(ErrorResponse {
				error: "unauthorized".to_string(),
				message: "Authentication required".to_string(),
				server_version: None,
				client_version: None,
			}),
		)
			.into_response()
	}
}

/// Extractor that requires authentication.
///
/// Use this in handlers that require an authenticated user.
/// Returns 401 Unauthorized if the request is not authenticated.
///
/// # Security
///
/// - Rejects unauthenticated requests with 401 status
/// - Error response does not leak any authentication details
///
/// # Example
///
/// ```ignore
/// async fn protected_handler(
///     RequireAuth(user): RequireAuth,
/// ) -> impl IntoResponse {
///     format!("Hello, {}!", user.user.display_name)
/// }
/// ```
pub struct RequireAuth(pub CurrentUser);

impl<S> FromRequestParts<S> for RequireAuth
where
	S: Send + Sync,
{
	type Rejection = Response;

	#[instrument(name = "RequireAuth::from_request_parts", skip_all)]
	async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
		let auth_ctx = parts
			.extensions
			.get::<AuthContext>()
			.cloned()
			.unwrap_or_else(AuthContext::unauthenticated);

		match auth_ctx.current_user {
			Some(user) => {
				tracing::debug!(user_id = %user.user.id, "Authentication required: success");
				Ok(RequireAuth(user))
			}
			None => {
				tracing::debug!("Authentication required: no valid credentials");
				let response = (
					StatusCode::UNAUTHORIZED,
					Json(ErrorResponse {
						error: "unauthorized".to_string(),
						message: "Authentication required".to_string(),
						server_version: None,
						client_version: None,
					}),
				);
				Err(response.into_response())
			}
		}
	}
}

/// Extractor for optional authentication.
///
/// Use this in handlers that work with or without authentication.
/// Always succeeds, returning `None` if not authenticated.
///
/// # Example
///
/// ```ignore
/// async fn public_handler(
///     OptionalAuth(maybe_user): OptionalAuth,
/// ) -> impl IntoResponse {
///     match maybe_user {
///         Some(user) => format!("Hello, {}!", user.user.display_name),
///         None => "Hello, guest!".to_string(),
///     }
/// }
/// ```
pub struct OptionalAuth(pub Option<CurrentUser>);

impl<S> FromRequestParts<S> for OptionalAuth
where
	S: Send + Sync,
{
	type Rejection = std::convert::Infallible;

	#[instrument(name = "OptionalAuth::from_request_parts", skip_all)]
	async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
		let auth_ctx = parts
			.extensions
			.get::<AuthContext>()
			.cloned()
			.unwrap_or_else(AuthContext::unauthenticated);

		if let Some(ref user) = auth_ctx.current_user {
			tracing::debug!(user_id = %user.user.id, "Optional auth: authenticated");
		} else {
			tracing::debug!("Optional auth: unauthenticated");
		}

		Ok(OptionalAuth(auth_ctx.current_user))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_hash_token() {
		let token = "test_token_123";
		let hash = hash_token(token);

		// SHA-256 produces 64 hex characters
		assert_eq!(hash.len(), 64);

		// Same input should produce same hash
		assert_eq!(hash, hash_token(token));

		// Different input should produce different hash
		assert_ne!(hash, hash_token("different_token"));
	}

	#[test]
	fn test_hash_token_empty_string() {
		let hash = hash_token("");
		assert_eq!(hash.len(), 64);
	}

	mod property_tests {
		use super::*;

		proptest! {
			/// Verifies that hash_token produces consistent output for the same input.
			/// This is critical for token validation to work correctly.
			#[test]
			fn hash_token_is_deterministic(token in ".*") {
				let hash1 = hash_token(&token);
				let hash2 = hash_token(&token);
				prop_assert_eq!(hash1, hash2, "Hash should be deterministic");
			}

			/// Verifies that hash_token always produces a 64-character hex string.
			/// SHA-256 always produces 32 bytes = 64 hex characters.
			#[test]
			fn hash_token_length_is_always_64(token in ".*") {
				let hash = hash_token(&token);
				prop_assert_eq!(hash.len(), 64, "SHA-256 hash should be 64 hex chars");
			}

			/// Verifies that hash_token output is valid hex.
			#[test]
			fn hash_token_is_valid_hex(token in ".*") {
				let hash = hash_token(&token);
				prop_assert!(
					hash.chars().all(|c| c.is_ascii_hexdigit()),
					"Hash should only contain hex characters"
				);
			}

			/// Verifies that different tokens produce different hashes (collision resistance).
			/// While collisions are theoretically possible, they should be extremely rare.
			#[test]
			fn different_tokens_produce_different_hashes(
				token1 in "[a-zA-Z0-9]{8,32}",
				token2 in "[a-zA-Z0-9]{8,32}"
			) {
				prop_assume!(token1 != token2);
				let hash1 = hash_token(&token1);
				let hash2 = hash_token(&token2);
				prop_assert_ne!(hash1, hash2, "Different tokens should produce different hashes");
			}

			/// Verifies that the raw token never appears in the hash output.
			/// This ensures tokens cannot be recovered from hash values in logs.
			#[test]
			fn hash_does_not_contain_token(token in "[a-zA-Z0-9]{4,20}") {
				let hash = hash_token(&token);
				prop_assert!(
					!hash.contains(&token),
					"Hash should not contain the original token"
				);
			}

			/// Verifies token extraction edge cases - whitespace handling.
			/// Tokens with only whitespace differences should produce different hashes.
			#[test]
			fn whitespace_tokens_are_distinct(token in "[a-zA-Z0-9]{4,20}") {
				let hash_plain = hash_token(&token);
				let hash_leading = hash_token(&format!(" {token}"));
				let hash_trailing = hash_token(&format!("{token} "));
				let hash_both = hash_token(&format!(" {token} "));

				prop_assert_ne!(hash_plain.clone(), hash_leading);
				prop_assert_ne!(hash_plain.clone(), hash_trailing);
				prop_assert_ne!(hash_plain, hash_both);
			}

			/// Verifies that tokens with different casing produce different hashes.
			/// This ensures case-sensitive token validation.
			#[test]
			fn case_sensitive_tokens(token in "[a-z]{4,20}") {
				let hash_lower = hash_token(&token);
				let hash_upper = hash_token(&token.to_uppercase());
				prop_assert_ne!(hash_lower, hash_upper, "Tokens should be case-sensitive");
			}
		}
	}

	mod require_auth_layer_tests {
		use super::*;
		use axum::{body::Body, http::Request, middleware, routing::get, Router};
		use loom_server_auth::{User, UserId};
		use tower::ServiceExt;

		fn test_user() -> User {
			User {
				id: UserId::generate(),
				display_name: "Test User".to_string(),
				username: None,
				primary_email: Some("test@example.com".to_string()),
				avatar_url: None,
				email_visible: true,
				is_system_admin: false,
				is_support: false,
				is_auditor: false,
				created_at: chrono::Utc::now(),
				updated_at: chrono::Utc::now(),
				deleted_at: None,
				locale: None,
			}
		}

		async fn dummy_handler() -> &'static str {
			"ok"
		}

		#[tokio::test]
		async fn test_require_auth_layer_with_valid_auth_proceeds() {
			let app = Router::new()
				.route("/test", get(dummy_handler))
				.layer(middleware::from_fn(require_auth_layer));

			let user = test_user();
			let current_user = CurrentUser::from_access_token(user);
			let auth_ctx = AuthContext::authenticated(current_user);

			let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();
			request.extensions_mut().insert(auth_ctx);

			let response = app.oneshot(request).await.unwrap();
			assert_eq!(response.status(), StatusCode::OK);
		}

		#[tokio::test]
		async fn test_require_auth_layer_unauthenticated_returns_401() {
			let app = Router::new()
				.route("/test", get(dummy_handler))
				.layer(middleware::from_fn(require_auth_layer));

			let auth_ctx = AuthContext::unauthenticated();

			let mut request = Request::builder().uri("/test").body(Body::empty()).unwrap();
			request.extensions_mut().insert(auth_ctx);

			let response = app.oneshot(request).await.unwrap();
			assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
		}

		#[tokio::test]
		async fn test_require_auth_layer_no_context_returns_401() {
			let app = Router::new()
				.route("/test", get(dummy_handler))
				.layer(middleware::from_fn(require_auth_layer));

			let request = Request::builder().uri("/test").body(Body::empty()).unwrap();

			let response = app.oneshot(request).await.unwrap();
			assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
		}
	}
}
