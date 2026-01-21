// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authentication middleware for extracting and validating user sessions and tokens.
//!
//! This module provides:
//! - [`CurrentUser`] - authenticated user context extracted from requests
//! - [`AuthContext`] - auth state for request processing
//! - [`AuthConfig`] - configuration for authentication behavior
//! - Helper functions for extracting session cookies and bearer tokens
//!
//! # Authentication Flow
//!
//! ```text
//! Request → Extract Token/Cookie → Identify Type → Validate → AuthContext
//!                                      │
//!                                      ├── Session Cookie → Session lookup
//!                                      ├── API Key (lk_*) → API key validation
//!                                      └── Access Token (lt_*) → Token validation
//! ```
//!
//! # Security Notes
//!
//! - Session tokens are extracted from cookies (HttpOnly, Secure recommended)
//! - Bearer tokens are extracted from Authorization header
//! - Token values are never logged; use SecretString for any token handling

use crate::access_token::ACCESS_TOKEN_PREFIX;
use crate::api_key::API_KEY_PREFIX;
use crate::{SessionId, User, UserId};
use http::header::{AUTHORIZATION, COOKIE};
use http::HeaderMap;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

/// Default name for the session cookie.
pub const SESSION_COOKIE_NAME: &str = "loom_session";

/// Environment variable to enable dev mode (bypass authentication).
pub const DEV_MODE_ENV_VAR: &str = "LOOM_SERVER_AUTH_DEV_MODE";
pub const LOOM_ENV_VAR: &str = "LOOM_SERVER_ENV";

/// The currently authenticated user, extracted from request context.
///
/// This struct contains all information about the authenticated user,
/// including how they authenticated (session vs API key) and whether
/// they are impersonating another user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
	/// The authenticated user.
	pub user: User,
	/// Session ID if authenticated via session cookie.
	pub session_id: Option<SessionId>,
	/// API key ID if authenticated via API key.
	pub api_key_id: Option<Uuid>,
	/// User ID being impersonated (if admin is impersonating).
	pub impersonating_as: Option<UserId>,
}

impl CurrentUser {
	/// Create a new CurrentUser from a session-based authentication.
	pub fn from_session(user: User, session_id: SessionId) -> Self {
		Self {
			user,
			session_id: Some(session_id),
			api_key_id: None,
			impersonating_as: None,
		}
	}

	/// Create a new CurrentUser from an API key authentication.
	pub fn from_api_key(user: User, api_key_id: Uuid) -> Self {
		Self {
			user,
			session_id: None,
			api_key_id: Some(api_key_id),
			impersonating_as: None,
		}
	}

	/// Create a new CurrentUser from an access token (CLI/VS Code).
	pub fn from_access_token(user: User) -> Self {
		Self {
			user,
			session_id: None,
			api_key_id: None,
			impersonating_as: None,
		}
	}

	/// Set the user being impersonated.
	pub fn with_impersonation(mut self, impersonated_user_id: UserId) -> Self {
		self.impersonating_as = Some(impersonated_user_id);
		self
	}

	/// Returns true if the current user is impersonating another user.
	pub fn is_impersonating(&self) -> bool {
		self.impersonating_as.is_some()
	}

	/// The effective user ID (the one being impersonated, or self).
	///
	/// Use this when checking permissions for resource access.
	pub fn effective_user_id(&self) -> &UserId {
		self.impersonating_as.as_ref().unwrap_or(&self.user.id)
	}

	/// The real actor (admin doing the impersonation, or self).
	///
	/// Use this for audit logging to track who actually performed an action.
	pub fn actor_user_id(&self) -> &UserId {
		&self.user.id
	}

	/// Returns true if authenticated via session cookie.
	pub fn is_session_auth(&self) -> bool {
		self.session_id.is_some()
	}

	/// Returns true if authenticated via API key.
	pub fn is_api_key_auth(&self) -> bool {
		self.api_key_id.is_some()
	}
}

/// Authentication context for request processing.
///
/// This struct is used to pass authentication state through the request pipeline.
#[derive(Debug, Clone, Default)]
pub struct AuthContext {
	/// Whether the request is authenticated.
	pub is_authenticated: bool,
	/// The current user, if authenticated.
	pub current_user: Option<CurrentUser>,
}

impl AuthContext {
	/// Create a new unauthenticated context.
	pub fn unauthenticated() -> Self {
		Self {
			is_authenticated: false,
			current_user: None,
		}
	}

	/// Create a new authenticated context.
	pub fn authenticated(current_user: CurrentUser) -> Self {
		Self {
			is_authenticated: true,
			current_user: Some(current_user),
		}
	}

	/// Get the current user, if authenticated.
	pub fn user(&self) -> Option<&CurrentUser> {
		self.current_user.as_ref()
	}

	/// Require authentication, returning the current user or an error.
	pub fn require_user(&self) -> Result<&CurrentUser, AuthRequired> {
		self.current_user.as_ref().ok_or(AuthRequired)
	}
}

/// Error returned when authentication is required but not present.
#[derive(Debug, Clone, Copy)]
pub struct AuthRequired;

impl std::fmt::Display for AuthRequired {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "authentication required")
	}
}

impl std::error::Error for AuthRequired {}

/// Configuration for authentication middleware.
#[derive(Debug, Clone)]
pub struct AuthConfig {
	/// Enable dev mode (bypass authentication when LOOM_SERVER_AUTH_DEV_MODE=1).
	pub dev_mode: bool,
	/// Name of the session cookie.
	pub session_cookie_name: String,
	/// Disable new user signups (existing users can still log in).
	pub signups_disabled: bool,
}

impl Default for AuthConfig {
	fn default() -> Self {
		Self {
			dev_mode: false,
			session_cookie_name: SESSION_COOKIE_NAME.to_string(),
			signups_disabled: false,
		}
	}
}

impl AuthConfig {
	/// Create a new AuthConfig with default settings.
	pub fn new() -> Self {
		Self::default()
	}

	/// Create AuthConfig from environment variables.
	///
	/// Reads `LOOM_SERVER_AUTH_DEV_MODE` to determine if dev mode should be enabled.
	///
	/// # Panics
	///
	/// Panics if both `LOOM_SERVER_AUTH_DEV_MODE=1` and `LOOM_SERVER_ENV=production` are set,
	/// as dev mode must never be enabled in production environments.
	pub fn from_env() -> Self {
		let dev_mode = std::env::var(DEV_MODE_ENV_VAR)
			.map(|v| v == "1" || v.to_lowercase() == "true")
			.unwrap_or(false);

		let loom_env = std::env::var(LOOM_ENV_VAR).unwrap_or_default();

		if dev_mode && loom_env.to_lowercase() == "production" {
			panic!(
                "FATAL: LOOM_SERVER_AUTH_DEV_MODE=1 is set while LOOM_SERVER_ENV=production. \
                 Dev mode authentication bypass MUST NOT be enabled in production. \
                 Remove LOOM_SERVER_AUTH_DEV_MODE or set LOOM_SERVER_ENV to a non-production value."
            );
		}

		Self {
			dev_mode,
			..Default::default()
		}
	}

	/// Set dev mode.
	pub fn with_dev_mode(mut self, enabled: bool) -> Self {
		self.dev_mode = enabled;
		self
	}

	/// Set the session cookie name.
	pub fn with_session_cookie_name(mut self, name: impl Into<String>) -> Self {
		self.session_cookie_name = name.into();
		self
	}

	/// Set signups disabled.
	pub fn with_signups_disabled(mut self, disabled: bool) -> Self {
		self.signups_disabled = disabled;
		self
	}
}

/// Extract session ID from the Cookie header.
///
/// Parses the Cookie header to find the session cookie (default: `loom_session`).
///
/// # Arguments
///
/// * `headers` - The HTTP request headers
///
/// # Returns
///
/// The session token value if found, or `None` if the cookie is not present.
pub fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
	extract_session_cookie_with_name(headers, SESSION_COOKIE_NAME)
}

/// Extract session ID from the Cookie header with a custom cookie name.
///
/// # Arguments
///
/// * `headers` - The HTTP request headers
/// * `cookie_name` - The name of the session cookie to look for
///
/// # Returns
///
/// The session token value if found, or `None` if the cookie is not present.
pub fn extract_session_cookie_with_name(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
	headers
		.get(COOKIE)?
		.to_str()
		.ok()?
		.split(';')
		.find_map(|cookie| {
			let cookie = cookie.trim();
			let (name, value) = cookie.split_once('=')?;

			if name == cookie_name {
				Some(value.to_string())
			} else {
				None
			}
		})
}

/// Extract bearer token from the Authorization header.
///
/// Expects the format: `Authorization: Bearer <token>`
///
/// # Arguments
///
/// * `headers` - The HTTP request headers
///
/// # Returns
///
/// The bearer token value if found, or `None` if not present or malformed.
///
/// # Security
///
/// The returned token should be treated as a secret. Use [`loom_common_secret::SecretString`]
/// when storing or passing tokens to prevent accidental logging.
#[instrument(level = "trace", skip_all, fields(has_auth_header))]
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
	let auth_header = headers.get(AUTHORIZATION)?;
	let auth_str = auth_header.to_str().ok()?;
	auth_str
		.strip_prefix("Bearer ")
		.map(|token| token.to_string())
}

/// Check if a token is an API key (starts with `lk_`).
///
/// API keys are org-level tokens with scoped permissions.
pub fn is_api_key_token(token: &str) -> bool {
	token.starts_with(API_KEY_PREFIX)
}

/// Check if a token is an access token (starts with `lt_`).
///
/// Access tokens are user-level tokens for CLI/VS Code authentication.
pub fn is_access_token(token: &str) -> bool {
	token.starts_with(ACCESS_TOKEN_PREFIX)
}

/// Determine the type of bearer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BearerTokenType {
	/// API key (org-level, prefixed with `lk_`).
	ApiKey,
	/// Access token (user-level, prefixed with `lt_`).
	AccessToken,
	/// WebSocket token (short-lived, prefixed with `ws_`).
	WsToken,
	/// Unknown token type.
	Unknown,
}

/// Identify the type of a bearer token.
pub fn identify_bearer_token(token: &str) -> BearerTokenType {
	if is_api_key_token(token) {
		BearerTokenType::ApiKey
	} else if is_access_token(token) {
		BearerTokenType::AccessToken
	} else if is_ws_token(token) {
		BearerTokenType::WsToken
	} else {
		BearerTokenType::Unknown
	}
}

/// Check if a token looks like a WebSocket token (ws_ prefix).
pub fn is_ws_token(token: &str) -> bool {
	token.starts_with(crate::ws_token::WS_TOKEN_PREFIX)
}

#[cfg(test)]
mod tests {
	use super::*;
	use http::header::HeaderValue;

	mod current_user {
		use super::*;
		use chrono::Utc;

		fn make_test_user() -> User {
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
				created_at: Utc::now(),
				updated_at: Utc::now(),
				deleted_at: None,
				locale: None,
			}
		}

		#[test]
		fn from_session_creates_session_auth() {
			let user = make_test_user();
			let session_id = SessionId::generate();
			let current_user = CurrentUser::from_session(user.clone(), session_id);

			assert!(current_user.is_session_auth());
			assert!(!current_user.is_api_key_auth());
			assert_eq!(current_user.session_id, Some(session_id));
			assert!(current_user.api_key_id.is_none());
		}

		#[test]
		fn from_api_key_creates_api_key_auth() {
			let user = make_test_user();
			let api_key_id = Uuid::new_v4();
			let current_user = CurrentUser::from_api_key(user.clone(), api_key_id);

			assert!(!current_user.is_session_auth());
			assert!(current_user.is_api_key_auth());
			assert!(current_user.session_id.is_none());
			assert_eq!(current_user.api_key_id, Some(api_key_id));
		}

		#[test]
		fn from_access_token_creates_token_auth() {
			let user = make_test_user();
			let current_user = CurrentUser::from_access_token(user.clone());

			assert!(!current_user.is_session_auth());
			assert!(!current_user.is_api_key_auth());
			assert!(current_user.session_id.is_none());
			assert!(current_user.api_key_id.is_none());
		}

		#[test]
		fn is_impersonating_returns_false_by_default() {
			let user = make_test_user();
			let current_user = CurrentUser::from_session(user, SessionId::generate());
			assert!(!current_user.is_impersonating());
		}

		#[test]
		fn is_impersonating_returns_true_after_set() {
			let user = make_test_user();
			let impersonated_id = UserId::generate();
			let current_user =
				CurrentUser::from_session(user, SessionId::generate()).with_impersonation(impersonated_id);

			assert!(current_user.is_impersonating());
			assert_eq!(current_user.impersonating_as, Some(impersonated_id));
		}

		#[test]
		fn effective_user_id_returns_self_when_not_impersonating() {
			let user = make_test_user();
			let user_id = user.id;
			let current_user = CurrentUser::from_session(user, SessionId::generate());

			assert_eq!(current_user.effective_user_id(), &user_id);
		}

		#[test]
		fn effective_user_id_returns_impersonated_when_impersonating() {
			let user = make_test_user();
			let impersonated_id = UserId::generate();
			let current_user =
				CurrentUser::from_session(user, SessionId::generate()).with_impersonation(impersonated_id);

			assert_eq!(current_user.effective_user_id(), &impersonated_id);
		}

		#[test]
		fn actor_user_id_always_returns_real_user() {
			let user = make_test_user();
			let user_id = user.id;
			let impersonated_id = UserId::generate();
			let current_user =
				CurrentUser::from_session(user, SessionId::generate()).with_impersonation(impersonated_id);

			assert_eq!(current_user.actor_user_id(), &user_id);
		}
	}

	mod auth_context {
		use super::*;
		use chrono::Utc;

		fn make_test_user() -> User {
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
				created_at: Utc::now(),
				updated_at: Utc::now(),
				deleted_at: None,
				locale: None,
			}
		}

		#[test]
		fn unauthenticated_has_no_user() {
			let ctx = AuthContext::unauthenticated();
			assert!(!ctx.is_authenticated);
			assert!(ctx.current_user.is_none());
			assert!(ctx.user().is_none());
		}

		#[test]
		fn authenticated_has_user() {
			let user = make_test_user();
			let current_user = CurrentUser::from_access_token(user);
			let ctx = AuthContext::authenticated(current_user);

			assert!(ctx.is_authenticated);
			assert!(ctx.current_user.is_some());
			assert!(ctx.user().is_some());
		}

		#[test]
		fn require_user_returns_error_when_unauthenticated() {
			let ctx = AuthContext::unauthenticated();
			assert!(ctx.require_user().is_err());
		}

		#[test]
		fn require_user_returns_user_when_authenticated() {
			let user = make_test_user();
			let current_user = CurrentUser::from_access_token(user);
			let ctx = AuthContext::authenticated(current_user);

			assert!(ctx.require_user().is_ok());
		}
	}

	mod auth_config {
		use super::*;
		use std::sync::Mutex;

		static ENV_MUTEX: Mutex<()> = Mutex::new(());

		fn with_env_vars<F, R>(vars: &[(&str, &str)], f: F) -> std::thread::Result<R>
		where
			F: FnOnce() -> R + std::panic::UnwindSafe,
		{
			let _lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
			let original: Vec<_> = vars
				.iter()
				.map(|(k, _)| (*k, std::env::var(*k).ok()))
				.collect();

			for (k, v) in vars {
				std::env::set_var(k, v);
			}

			let result = std::panic::catch_unwind(f);

			for (k, original_val) in &original {
				match original_val {
					Some(v) => std::env::set_var(k, v),
					None => std::env::remove_var(k),
				}
			}

			result
		}

		#[test]
		fn default_has_dev_mode_disabled() {
			let config = AuthConfig::default();
			assert!(!config.dev_mode);
			assert_eq!(config.session_cookie_name, SESSION_COOKIE_NAME);
		}

		#[test]
		fn with_dev_mode_enables_dev_mode() {
			let config = AuthConfig::new().with_dev_mode(true);
			assert!(config.dev_mode);
		}

		#[test]
		fn with_session_cookie_name_sets_name() {
			let config = AuthConfig::new().with_session_cookie_name("custom_session");
			assert_eq!(config.session_cookie_name, "custom_session");
		}

		#[test]
		fn dev_mode_panics_in_production() {
			let result = with_env_vars(
				&[(DEV_MODE_ENV_VAR, "1"), (LOOM_ENV_VAR, "production")],
				AuthConfig::from_env,
			);
			assert!(
				result.is_err(),
				"Expected panic when dev mode enabled in production"
			);
		}

		#[test]
		fn dev_mode_allowed_in_development() {
			let result = with_env_vars(
				&[(DEV_MODE_ENV_VAR, "1"), (LOOM_ENV_VAR, "development")],
				AuthConfig::from_env,
			);
			let config = result.expect("Should not panic in development");
			assert!(config.dev_mode);
		}

		#[test]
		fn dev_mode_allowed_when_loom_env_unset() {
			let result = with_env_vars(&[(DEV_MODE_ENV_VAR, "1"), (LOOM_ENV_VAR, "")], || {
				std::env::remove_var(LOOM_ENV_VAR);
				AuthConfig::from_env()
			});
			let config = result.expect("Should not panic when LOOM_SERVER_ENV unset");
			assert!(config.dev_mode);
		}

		#[test]
		fn production_mode_works_without_dev_mode() {
			let result = with_env_vars(
				&[(DEV_MODE_ENV_VAR, "0"), (LOOM_ENV_VAR, "production")],
				AuthConfig::from_env,
			);
			let config = result.expect("Should not panic when dev mode disabled");
			assert!(!config.dev_mode);
		}
	}

	mod extract_session_cookie {
		use super::*;

		#[test]
		fn extracts_session_from_single_cookie() {
			let mut headers = HeaderMap::new();
			headers.insert(COOKIE, HeaderValue::from_static("loom_session=abc123"));

			assert_eq!(extract_session_cookie(&headers), Some("abc123".to_string()));
		}

		#[test]
		fn extracts_session_from_multiple_cookies() {
			let mut headers = HeaderMap::new();
			headers.insert(
				COOKIE,
				HeaderValue::from_static("other=value; loom_session=xyz789; another=test"),
			);

			assert_eq!(extract_session_cookie(&headers), Some("xyz789".to_string()));
		}

		#[test]
		fn returns_none_when_no_cookie_header() {
			let headers = HeaderMap::new();
			assert_eq!(extract_session_cookie(&headers), None);
		}

		#[test]
		fn returns_none_when_session_cookie_missing() {
			let mut headers = HeaderMap::new();
			headers.insert(
				COOKIE,
				HeaderValue::from_static("other=value; another=test"),
			);

			assert_eq!(extract_session_cookie(&headers), None);
		}

		#[test]
		fn handles_whitespace_around_cookies() {
			let mut headers = HeaderMap::new();
			headers.insert(
				COOKIE,
				HeaderValue::from_static("  loom_session=token123  ; other=val  "),
			);

			assert_eq!(
				extract_session_cookie(&headers),
				Some("token123".to_string())
			);
		}

		#[test]
		fn extracts_with_custom_cookie_name() {
			let mut headers = HeaderMap::new();
			headers.insert(
				COOKIE,
				HeaderValue::from_static("custom_session=mytoken; loom_session=other"),
			);

			assert_eq!(
				extract_session_cookie_with_name(&headers, "custom_session"),
				Some("mytoken".to_string())
			);
		}
	}

	mod extract_bearer_token {
		use super::*;

		#[test]
		fn extracts_bearer_token() {
			let mut headers = HeaderMap::new();
			headers.insert(
				AUTHORIZATION,
				HeaderValue::from_static("Bearer lt_0123456789abcdef"),
			);

			assert_eq!(
				extract_bearer_token(&headers),
				Some("lt_0123456789abcdef".to_string())
			);
		}

		#[test]
		fn returns_none_when_no_auth_header() {
			let headers = HeaderMap::new();
			assert_eq!(extract_bearer_token(&headers), None);
		}

		#[test]
		fn returns_none_for_basic_auth() {
			let mut headers = HeaderMap::new();
			headers.insert(
				AUTHORIZATION,
				HeaderValue::from_static("Basic dXNlcjpwYXNz"),
			);

			assert_eq!(extract_bearer_token(&headers), None);
		}

		#[test]
		fn returns_none_for_missing_space() {
			let mut headers = HeaderMap::new();
			headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer"));

			// No space after "Bearer", so strip_prefix("Bearer ") fails
			assert_eq!(extract_bearer_token(&headers), None);
		}

		#[test]
		fn returns_empty_string_for_bearer_with_trailing_space() {
			let mut headers = HeaderMap::new();
			headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer "));

			// "Bearer " with trailing space returns empty token
			assert_eq!(extract_bearer_token(&headers), Some("".to_string()));
		}

		#[test]
		fn is_case_sensitive_for_bearer_prefix() {
			let mut headers = HeaderMap::new();
			headers.insert(AUTHORIZATION, HeaderValue::from_static("bearer token123"));

			assert_eq!(extract_bearer_token(&headers), None);
		}
	}

	mod token_type_detection {
		use super::*;

		#[test]
		fn is_api_key_token_detects_api_keys() {
			assert!(is_api_key_token("lk_0123456789abcdef"));
			assert!(!is_api_key_token("lt_0123456789abcdef"));
			assert!(!is_api_key_token("random_token"));
		}

		#[test]
		fn is_access_token_detects_access_tokens() {
			assert!(is_access_token("lt_0123456789abcdef"));
			assert!(!is_access_token("lk_0123456789abcdef"));
			assert!(!is_access_token("random_token"));
		}

		#[test]
		fn identify_bearer_token_returns_correct_type() {
			assert_eq!(
				identify_bearer_token("lk_0123456789abcdef"),
				BearerTokenType::ApiKey
			);
			assert_eq!(
				identify_bearer_token("lt_0123456789abcdef"),
				BearerTokenType::AccessToken
			);
			assert_eq!(
				identify_bearer_token("random_token"),
				BearerTokenType::Unknown
			);
		}
	}
}
