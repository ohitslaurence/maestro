// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! WebSocket authentication state machine.
//!
//! Implements first-message authentication with a 5-second timeout.
//!
//! # State Machine
//!
//! ```text
//! ┌─────────────┐  auth msg   ┌───────────────┐
//! │   Pending   │────────────▶│ Authenticated │
//! │  (5s timer) │             │   (active)    │
//! └─────────────┘             └───────────────┘
//!       │
//!       │ timeout/invalid
//!       ▼
//! ┌─────────────┐
//! │   Closed    │
//! └─────────────┘
//! ```
//!
//! # Protocol
//!
//! 1. Client connects to `/v1/ws/sessions/{session_id}` - NO auth required at HTTP level
//! 2. Server starts 5-second auth timer
//! 3. Client must send first message: `{"type": "auth", "token": "lt_xxx"}`
//! 4. Server validates token using existing auth logic (hash + DB lookup)
//! 5. Success: `{"type": "auth_ok", "user_id": "..."}`
//! 6. Failure: `{"type": "auth_error", "message": "..."}` then close
//! 7. After auth_ok, normal message flow proceeds

use loom_server_auth::CurrentUser;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const AUTH_TIMEOUT_SECS: u64 = 5;

pub fn auth_timeout() -> Duration {
	Duration::from_secs(AUTH_TIMEOUT_SECS)
}

#[derive(Debug, Clone)]
pub enum WebSocketAuthState {
	Pending,
	Authenticated(Box<AuthenticatedContext>),
	Closed,
}

impl WebSocketAuthState {
	pub fn is_pending(&self) -> bool {
		matches!(self, WebSocketAuthState::Pending)
	}

	pub fn is_authenticated(&self) -> bool {
		matches!(self, WebSocketAuthState::Authenticated(_))
	}

	pub fn is_closed(&self) -> bool {
		matches!(self, WebSocketAuthState::Closed)
	}

	pub fn user(&self) -> Option<&AuthenticatedContext> {
		match self {
			WebSocketAuthState::Authenticated(ctx) => Some(ctx.as_ref()),
			_ => None,
		}
	}
}

#[derive(Debug, Clone)]
pub struct AuthenticatedContext {
	pub current_user: CurrentUser,
}

impl AuthenticatedContext {
	pub fn new(current_user: CurrentUser) -> Self {
		Self { current_user }
	}

	pub fn user_id(&self) -> &loom_server_auth::UserId {
		&self.current_user.user.id
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMessage {
	Auth {
		token: Option<String>,
		session_token: Option<String>,
	},
}

impl AuthMessage {
	pub fn with_token(token: impl Into<String>) -> Self {
		AuthMessage::Auth {
			token: Some(token.into()),
			session_token: None,
		}
	}

	pub fn with_session_token(session_token: impl Into<String>) -> Self {
		AuthMessage::Auth {
			token: None,
			session_token: Some(session_token.into()),
		}
	}

	pub fn token(&self) -> Option<&str> {
		match self {
			AuthMessage::Auth { token, .. } => token.as_deref(),
		}
	}

	pub fn session_token(&self) -> Option<&str> {
		match self {
			AuthMessage::Auth { session_token, .. } => session_token.as_deref(),
		}
	}

	pub fn is_valid(&self) -> bool {
		match self {
			AuthMessage::Auth {
				token,
				session_token,
			} => {
				let has_token = token.as_ref().is_some_and(|t| !t.is_empty());
				let has_session = session_token.as_ref().is_some_and(|s| !s.is_empty());
				has_token || has_session
			}
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResponse {
	AuthOk { user_id: String },
	AuthError { message: String },
}

impl AuthResponse {
	pub fn ok(user_id: &loom_server_auth::UserId) -> Self {
		AuthResponse::AuthOk {
			user_id: user_id.to_string(),
		}
	}

	pub fn error(message: impl Into<String>) -> Self {
		AuthResponse::AuthError {
			message: message.into(),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
	Timeout,
	InvalidMessage,
	InvalidToken,
	TokenExpired,
	TokenRevoked,
	InvalidApiKey,
	ApiKeyRevoked,
	InvalidSession,
	SessionExpired,
	UserInactive,
	InternalError,
}

impl std::fmt::Display for AuthError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			AuthError::Timeout => write!(f, "authentication timeout"),
			AuthError::InvalidMessage => write!(f, "invalid authentication message"),
			AuthError::InvalidToken => write!(f, "invalid token"),
			AuthError::TokenExpired => write!(f, "token expired"),
			AuthError::TokenRevoked => write!(f, "token revoked"),
			AuthError::InvalidApiKey => write!(f, "invalid API key"),
			AuthError::ApiKeyRevoked => write!(f, "API key revoked"),
			AuthError::InvalidSession => write!(f, "invalid session"),
			AuthError::SessionExpired => write!(f, "session expired"),
			AuthError::UserInactive => write!(f, "user account inactive"),
			AuthError::InternalError => write!(f, "internal server error"),
		}
	}
}

impl std::error::Error for AuthError {}

pub mod close_codes {
	pub const AUTH_TIMEOUT: u16 = 4001;
	pub const AUTH_INVALID: u16 = 4002;
	pub const AUTH_EXPIRED: u16 = 4003;
	pub const AUTH_REVOKED: u16 = 4004;
	pub const USER_INACTIVE: u16 = 4005;
	pub const INVALID_MESSAGE: u16 = 4006;
}

pub fn close_code_for_error(error: &AuthError) -> u16 {
	match error {
		AuthError::Timeout => close_codes::AUTH_TIMEOUT,
		AuthError::InvalidMessage => close_codes::INVALID_MESSAGE,
		AuthError::InvalidToken | AuthError::InvalidApiKey | AuthError::InvalidSession => {
			close_codes::AUTH_INVALID
		}
		AuthError::TokenExpired | AuthError::SessionExpired => close_codes::AUTH_EXPIRED,
		AuthError::TokenRevoked | AuthError::ApiKeyRevoked => close_codes::AUTH_REVOKED,
		AuthError::UserInactive => close_codes::USER_INACTIVE,
		AuthError::InternalError => close_codes::AUTH_INVALID,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	mod auth_state {
		use super::*;
		use chrono::Utc;
		use loom_server_auth::{Session, SessionType, User, UserId};

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
		fn pending_state() {
			let state = WebSocketAuthState::Pending;
			assert!(state.is_pending());
			assert!(!state.is_authenticated());
			assert!(!state.is_closed());
			assert!(state.user().is_none());
		}

		#[test]
		fn authenticated_state() {
			let user = make_test_user();
			let session = Session::new(user.id, SessionType::Web);
			let current_user = CurrentUser::from_session(user, session.id);
			let ctx = AuthenticatedContext::new(current_user);
			let state = WebSocketAuthState::Authenticated(Box::new(ctx));

			assert!(!state.is_pending());
			assert!(state.is_authenticated());
			assert!(!state.is_closed());
			assert!(state.user().is_some());
		}

		#[test]
		fn closed_state() {
			let state = WebSocketAuthState::Closed;
			assert!(!state.is_pending());
			assert!(!state.is_authenticated());
			assert!(state.is_closed());
			assert!(state.user().is_none());
		}
	}

	mod auth_message {
		use super::*;

		#[test]
		fn with_token() {
			let msg = AuthMessage::with_token("lt_token123");
			assert_eq!(msg.token(), Some("lt_token123"));
			assert_eq!(msg.session_token(), None);
			assert!(msg.is_valid());
		}

		#[test]
		fn with_session_token() {
			let msg = AuthMessage::with_session_token("session_abc");
			assert_eq!(msg.token(), None);
			assert_eq!(msg.session_token(), Some("session_abc"));
			assert!(msg.is_valid());
		}

		#[test]
		fn empty_token_invalid() {
			let msg = AuthMessage::Auth {
				token: Some("".to_string()),
				session_token: None,
			};
			assert!(!msg.is_valid());
		}

		#[test]
		fn both_none_invalid() {
			let msg = AuthMessage::Auth {
				token: None,
				session_token: None,
			};
			assert!(!msg.is_valid());
		}

		#[test]
		fn serializes_with_token() {
			let msg = AuthMessage::with_token("lt_abc123");
			let json = serde_json::to_string(&msg).unwrap();
			assert!(json.contains(r#""type":"auth""#));
			assert!(json.contains(r#""token":"lt_abc123""#));
		}

		#[test]
		fn deserializes_with_token() {
			let json = r#"{"type":"auth","token":"lt_xyz789"}"#;
			let msg: AuthMessage = serde_json::from_str(json).unwrap();
			assert_eq!(msg.token(), Some("lt_xyz789"));
			assert!(msg.is_valid());
		}

		#[test]
		fn deserializes_with_session_token() {
			let json = r#"{"type":"auth","session_token":"cookie_value"}"#;
			let msg: AuthMessage = serde_json::from_str(json).unwrap();
			assert_eq!(msg.session_token(), Some("cookie_value"));
			assert!(msg.is_valid());
		}
	}

	mod auth_response {
		use super::*;
		use loom_server_auth::UserId;

		#[test]
		fn ok_response() {
			let user_id = UserId::generate();
			let resp = AuthResponse::ok(&user_id);
			let json = serde_json::to_string(&resp).unwrap();
			assert!(json.contains(r#""type":"auth_ok""#));
			assert!(json.contains(&user_id.to_string()));
		}

		#[test]
		fn error_response() {
			let resp = AuthResponse::error("authentication timeout");
			let json = serde_json::to_string(&resp).unwrap();
			assert!(json.contains(r#""type":"auth_error""#));
			assert!(json.contains(r#""message":"authentication timeout""#));
		}
	}

	mod close_codes {
		use super::*;

		#[test]
		fn timeout_code() {
			assert_eq!(close_code_for_error(&AuthError::Timeout), 4001);
		}

		#[test]
		fn invalid_token_code() {
			assert_eq!(close_code_for_error(&AuthError::InvalidToken), 4002);
		}

		#[test]
		fn expired_code() {
			assert_eq!(close_code_for_error(&AuthError::TokenExpired), 4003);
		}

		#[test]
		fn revoked_code() {
			assert_eq!(close_code_for_error(&AuthError::TokenRevoked), 4004);
		}
	}

	mod timeout {
		use super::*;

		#[test]
		fn timeout_is_5_seconds() {
			assert_eq!(AUTH_TIMEOUT_SECS, 5);
			assert_eq!(auth_timeout(), Duration::from_secs(5));
		}
	}

	mod property_tests {
		use super::*;

		proptest! {
			#[test]
			fn auth_message_with_any_nonempty_token_is_valid(token in "[a-zA-Z0-9_]{1,100}") {
				let msg = AuthMessage::with_token(token);
				prop_assert!(msg.is_valid());
			}

			#[test]
			fn auth_message_with_any_nonempty_session_is_valid(session in "[a-zA-Z0-9_]{1,100}") {
				let msg = AuthMessage::with_session_token(session);
				prop_assert!(msg.is_valid());
			}

			#[test]
			fn auth_message_roundtrips(token in "[a-zA-Z0-9_]{1,50}") {
				let msg = AuthMessage::with_token(&token);
				let json = serde_json::to_string(&msg).unwrap();
				let parsed: AuthMessage = serde_json::from_str(&json).unwrap();
				prop_assert_eq!(parsed.token(), Some(token.as_str()));
			}

			#[test]
			fn non_auth_message_types_fail_to_parse_as_auth(
				msg_type in "[a-z_]{1,20}".prop_filter("not auth", |t| t != "auth")
			) {
				let json = format!(r#"{{"type":"{}","data":"test"}}"#, msg_type);
				let result: Result<AuthMessage, _> = serde_json::from_str(&json);
				prop_assert!(result.is_err());
			}
		}
	}

	mod state_machine {
		use super::*;
		use chrono::Utc;
		use loom_server_auth::{Session, SessionType, User, UserId};

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
		fn pending_to_authenticated_transition() {
			let mut state = WebSocketAuthState::Pending;
			assert!(state.is_pending());

			let user = make_test_user();
			let session = Session::new(user.id, SessionType::Web);
			let current_user = CurrentUser::from_session(user, session.id);
			let ctx = AuthenticatedContext::new(current_user);
			state = WebSocketAuthState::Authenticated(Box::new(ctx));

			assert!(state.is_authenticated());
			assert!(!state.is_pending());
			assert!(!state.is_closed());
		}

		#[test]
		fn pending_to_closed_on_timeout() {
			let mut state = WebSocketAuthState::Pending;
			assert!(state.is_pending());

			state = WebSocketAuthState::Closed;

			assert!(state.is_closed());
			assert!(!state.is_pending());
			assert!(!state.is_authenticated());
		}

		#[test]
		fn pending_to_closed_on_invalid_auth() {
			let mut state = WebSocketAuthState::Pending;
			assert!(state.is_pending());

			state = WebSocketAuthState::Closed;

			assert!(state.is_closed());
		}

		#[test]
		fn authenticated_user_context_accessible() {
			let user = make_test_user();
			let user_id = user.id;
			let session = Session::new(user.id, SessionType::Web);
			let current_user = CurrentUser::from_session(user, session.id);
			let ctx = AuthenticatedContext::new(current_user);
			let state = WebSocketAuthState::Authenticated(Box::new(ctx));

			let user_ctx = state.user().expect("should have user");
			assert_eq!(*user_ctx.user_id(), user_id);
		}
	}
}
