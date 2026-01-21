// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! WebSocket authentication types and utilities.
//!
//! This module provides authentication support for WebSocket connections as specified
//! in the auth-abac-system.md specification (Section 7: WebSocket/SSE Authentication).
//!
//! ## Dual Authentication Support
//!
//! | Client      | Mechanism                                      |
//! |-------------|------------------------------------------------|
//! | Web browser | Session cookie (automatic with handshake)      |
//! | CLI/VS Code | First-message auth after connection (30s timeout) |
//!
//! ## First-Message Auth Flow
//!
//! 1. CLI connects: `wss://loom.example/ws` (unauthenticated)
//! 2. Server starts 30-second timeout
//! 3. CLI sends: `{ "type": "auth", "token": "bearer_xyz789" }`
//! 4. Server validates token
//!    - Success: connection authenticated
//!    - Failure/timeout: disconnect

use crate::{CurrentUser, UserId};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timeout for first-message authentication (30 seconds as per spec).
pub const WS_AUTH_TIMEOUT_SECS: u64 = 30;

/// Get the authentication timeout duration.
pub fn auth_timeout() -> Duration {
	Duration::from_secs(WS_AUTH_TIMEOUT_SECS)
}

/// Authentication state for a WebSocket connection.
#[derive(Debug, Clone)]
pub enum WsAuthState {
	/// Connection is unauthenticated, waiting for auth message.
	/// Contains the deadline for auth timeout.
	AwaitingAuth,

	/// Connection is authenticated with user context.
	Authenticated(Box<WsAuthContext>),

	/// Authentication failed (invalid token, timeout, etc.).
	Failed(WsAuthError),
}

impl WsAuthState {
	/// Returns true if the connection is authenticated.
	pub fn is_authenticated(&self) -> bool {
		matches!(self, WsAuthState::Authenticated(_))
	}

	/// Get the authenticated user context, if any.
	pub fn user(&self) -> Option<&WsAuthContext> {
		match self {
			WsAuthState::Authenticated(ctx) => Some(ctx.as_ref()),
			_ => None,
		}
	}
}

/// Authenticated user context for WebSocket connections.
#[derive(Debug, Clone)]
pub struct WsAuthContext {
	/// The authenticated user.
	pub current_user: CurrentUser,
	/// How the connection was authenticated.
	pub auth_method: WsAuthMethod,
}

impl WsAuthContext {
	/// Create a new authenticated context.
	pub fn new(current_user: CurrentUser, auth_method: WsAuthMethod) -> Self {
		Self {
			current_user,
			auth_method,
		}
	}

	/// Get the user ID.
	pub fn user_id(&self) -> &UserId {
		&self.current_user.user.id
	}
}

/// How the WebSocket connection was authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsAuthMethod {
	/// Authenticated via session cookie during upgrade handshake (web browser).
	SessionCookie,
	/// Authenticated via first-message bearer token (CLI/VS Code).
	BearerToken,
	/// Authenticated via API key in first message.
	ApiKey,
}

/// WebSocket authentication errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsAuthError {
	/// No authentication provided within timeout.
	Timeout,
	/// Invalid or missing auth message format.
	InvalidAuthMessage,
	/// Invalid bearer token.
	InvalidToken,
	/// Token has expired.
	TokenExpired,
	/// Token has been revoked.
	TokenRevoked,
	/// Invalid API key.
	InvalidApiKey,
	/// API key has been revoked.
	ApiKeyRevoked,
	/// Session not found or invalid.
	InvalidSession,
	/// Session has expired.
	SessionExpired,
	/// User account is suspended or deleted.
	UserInactive,
	/// Internal server error.
	InternalError,
}

impl std::fmt::Display for WsAuthError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			WsAuthError::Timeout => write!(f, "authentication timeout"),
			WsAuthError::InvalidAuthMessage => write!(f, "invalid authentication message"),
			WsAuthError::InvalidToken => write!(f, "invalid token"),
			WsAuthError::TokenExpired => write!(f, "token expired"),
			WsAuthError::TokenRevoked => write!(f, "token revoked"),
			WsAuthError::InvalidApiKey => write!(f, "invalid API key"),
			WsAuthError::ApiKeyRevoked => write!(f, "API key revoked"),
			WsAuthError::InvalidSession => write!(f, "invalid session"),
			WsAuthError::SessionExpired => write!(f, "session expired"),
			WsAuthError::UserInactive => write!(f, "user account inactive"),
			WsAuthError::InternalError => write!(f, "internal server error"),
		}
	}
}

impl std::error::Error for WsAuthError {}

/// First-message authentication request from client.
///
/// Sent by CLI/VS Code clients immediately after WebSocket connection is established.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsAuthMessage {
	/// Message type, must be "auth".
	#[serde(rename = "type")]
	pub message_type: String,
	/// Bearer token (lt_xxx) or API key (lk_xxx).
	pub token: String,
}

impl WsAuthMessage {
	/// Create a new auth message with a bearer token.
	pub fn new(token: impl Into<String>) -> Self {
		Self {
			message_type: "auth".to_string(),
			token: token.into(),
		}
	}

	/// Validate the message format.
	pub fn is_valid(&self) -> bool {
		self.message_type == "auth" && !self.token.is_empty()
	}
}

/// Response to authentication attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsAuthResponse {
	/// Message type: "auth_success" or "auth_error".
	#[serde(rename = "type")]
	pub message_type: String,
	/// True if authentication succeeded.
	pub success: bool,
	/// Error details if authentication failed.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
	/// User ID if authentication succeeded.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub user_id: Option<String>,
}

impl WsAuthResponse {
	/// Create a success response.
	pub fn success(user_id: &UserId) -> Self {
		Self {
			message_type: "auth_success".to_string(),
			success: true,
			error: None,
			user_id: Some(user_id.to_string()),
		}
	}

	/// Create an error response.
	pub fn error(err: WsAuthError) -> Self {
		Self {
			message_type: "auth_error".to_string(),
			success: false,
			error: Some(err.to_string()),
			user_id: None,
		}
	}
}

/// Close codes for WebSocket authentication failures.
///
/// These follow the WebSocket close code conventions:
/// - 4000-4999: Application-defined close codes
pub mod close_codes {
	/// Authentication timeout (client didn't send auth in time).
	pub const AUTH_TIMEOUT: u16 = 4001;
	/// Invalid authentication credentials.
	pub const AUTH_INVALID: u16 = 4002;
	/// Token or session expired.
	pub const AUTH_EXPIRED: u16 = 4003;
	/// Token or session revoked.
	pub const AUTH_REVOKED: u16 = 4004;
	/// User account inactive (suspended/deleted).
	pub const USER_INACTIVE: u16 = 4005;
	/// Invalid message format.
	pub const INVALID_MESSAGE: u16 = 4006;
}

/// Get the appropriate close code for an auth error.
pub fn close_code_for_error(error: &WsAuthError) -> u16 {
	match error {
		WsAuthError::Timeout => close_codes::AUTH_TIMEOUT,
		WsAuthError::InvalidAuthMessage => close_codes::INVALID_MESSAGE,
		WsAuthError::InvalidToken | WsAuthError::InvalidApiKey | WsAuthError::InvalidSession => {
			close_codes::AUTH_INVALID
		}
		WsAuthError::TokenExpired | WsAuthError::SessionExpired => close_codes::AUTH_EXPIRED,
		WsAuthError::TokenRevoked | WsAuthError::ApiKeyRevoked => close_codes::AUTH_REVOKED,
		WsAuthError::UserInactive => close_codes::USER_INACTIVE,
		WsAuthError::InternalError => close_codes::AUTH_INVALID,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	mod ws_auth_state {
		use super::*;
		use crate::{Session, SessionType, User};
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
		fn awaiting_auth_is_not_authenticated() {
			let state = WsAuthState::AwaitingAuth;
			assert!(!state.is_authenticated());
			assert!(state.user().is_none());
		}

		#[test]
		fn authenticated_state_has_user() {
			let user = make_test_user();
			let session = Session::new(user.id, SessionType::Web);
			let current_user = CurrentUser::from_session(user, session.id);
			let ctx = WsAuthContext::new(current_user, WsAuthMethod::SessionCookie);
			let state = WsAuthState::Authenticated(Box::new(ctx));

			assert!(state.is_authenticated());
			assert!(state.user().is_some());
		}

		#[test]
		fn failed_state_is_not_authenticated() {
			let state = WsAuthState::Failed(WsAuthError::Timeout);
			assert!(!state.is_authenticated());
			assert!(state.user().is_none());
		}
	}

	mod ws_auth_message {
		use super::*;

		#[test]
		fn new_creates_valid_message() {
			let msg = WsAuthMessage::new("lt_token123");
			assert_eq!(msg.message_type, "auth");
			assert_eq!(msg.token, "lt_token123");
			assert!(msg.is_valid());
		}

		#[test]
		fn empty_token_is_invalid() {
			let msg = WsAuthMessage {
				message_type: "auth".to_string(),
				token: "".to_string(),
			};
			assert!(!msg.is_valid());
		}

		#[test]
		fn wrong_type_is_invalid() {
			let msg = WsAuthMessage {
				message_type: "not_auth".to_string(),
				token: "lt_token123".to_string(),
			};
			assert!(!msg.is_valid());
		}

		#[test]
		fn serializes_correctly() {
			let msg = WsAuthMessage::new("lt_abc123");
			let json = serde_json::to_string(&msg).unwrap();
			assert!(json.contains("\"type\":\"auth\""));
			assert!(json.contains("\"token\":\"lt_abc123\""));
		}

		#[test]
		fn deserializes_correctly() {
			let json = r#"{"type":"auth","token":"lt_xyz789"}"#;
			let msg: WsAuthMessage = serde_json::from_str(json).unwrap();
			assert_eq!(msg.message_type, "auth");
			assert_eq!(msg.token, "lt_xyz789");
		}
	}

	mod ws_auth_response {
		use super::*;

		#[test]
		fn success_response() {
			let user_id = UserId::generate();
			let resp = WsAuthResponse::success(&user_id);
			assert!(resp.success);
			assert_eq!(resp.message_type, "auth_success");
			assert!(resp.error.is_none());
			assert!(resp.user_id.is_some());
		}

		#[test]
		fn error_response() {
			let resp = WsAuthResponse::error(WsAuthError::Timeout);
			assert!(!resp.success);
			assert_eq!(resp.message_type, "auth_error");
			assert!(resp.error.is_some());
			assert!(resp.user_id.is_none());
		}
	}

	mod close_codes {
		use super::*;

		#[test]
		fn timeout_uses_correct_code() {
			assert_eq!(close_code_for_error(&WsAuthError::Timeout), 4001);
		}

		#[test]
		fn invalid_token_uses_correct_code() {
			assert_eq!(close_code_for_error(&WsAuthError::InvalidToken), 4002);
		}

		#[test]
		fn expired_uses_correct_code() {
			assert_eq!(close_code_for_error(&WsAuthError::TokenExpired), 4003);
		}

		#[test]
		fn revoked_uses_correct_code() {
			assert_eq!(close_code_for_error(&WsAuthError::TokenRevoked), 4004);
		}
	}

	mod auth_timeout {
		use super::*;

		#[test]
		fn timeout_is_30_seconds() {
			assert_eq!(WS_AUTH_TIMEOUT_SECS, 30);
			assert_eq!(auth_timeout(), Duration::from_secs(30));
		}
	}
}
