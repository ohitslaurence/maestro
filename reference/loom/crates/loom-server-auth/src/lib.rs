// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authentication and ABAC (Attribute-Based Access Control) for Loom.
//!
//! This crate provides:
//! - User identity and session management
//! - OAuth integration (GitHub, Google)
//! - Magic link passwordless authentication
//! - Device code flow for CLI/VS Code
//! - Organization and team management
//! - ABAC policy engine for fine-grained access control
//! - Audit logging
//!
//! # ABAC Design Rationale
//!
//! The Attribute-Based Access Control (ABAC) system provides fine-grained authorization
//! by evaluating policies based on:
//!
//! - **Subject attributes**: Who is making the request (user ID, org memberships, global roles)
//! - **Resource attributes**: What is being accessed (resource type, owner, org, visibility)
//! - **Action**: What operation is requested (read, write, delete, share, etc.)
//!
//! This approach offers several advantages over simpler RBAC:
//!
//! 1. **Flexibility**: Policies can consider any combination of attributes
//! 2. **Scalability**: No explosion of roles as permissions multiply
//! 3. **Context-awareness**: Decisions can factor in resource state (e.g., visibility)
//! 4. **Audit trail**: Every decision is based on explicit, loggable attributes
//!
//! # Security Considerations
//!
//! - Session tokens and API keys are stored as Argon2 hashes, never plaintext
//! - Secrets use [`loom_common_secret::SecretString`] to prevent accidental logging
//! - All authentication operations support structured logging with automatic redaction

pub mod abac;
pub mod access_token;
pub mod account_deletion;
pub mod admin;
pub mod api_key;
mod argon2_config;
pub mod audit;
pub mod email;
pub mod error;
pub mod middleware;
pub mod org;
pub mod session;
pub mod share_link;
pub mod support_access;
pub mod team;
pub mod types;
pub mod user;
pub mod websocket;
pub mod ws_token;

pub mod device_code {
	//! Re-export device code types from loom-server-auth-devicecode.
	pub use loom_server_auth_devicecode::*;
}

pub mod magic_link {
	//! Re-export magic link types from loom-server-auth-magiclink.
	pub use loom_server_auth_magiclink::*;
}

pub use abac::{
	is_allowed, Action, OrgMembershipAttr, ResourceAttrs, ResourceType, SubjectAttrs,
	TeamMembershipAttr,
};
pub use access_token::{
	generate_access_token, hash_access_token, is_valid_access_token_format, verify_access_token,
	AccessToken, ACCESS_TOKEN_BYTES, ACCESS_TOKEN_EXPIRY_DAYS, ACCESS_TOKEN_PREFIX,
};
pub use account_deletion::{
	can_restore, create_tombstone_id, is_past_grace_period, DeletionRequest, TombstoneUser,
	ACCOUNT_DELETION_GRACE_DAYS,
};
pub use admin::{check_can_demote, check_can_impersonate, check_can_promote, ImpersonationSession};
pub use api_key::{
	generate_api_key, hash_api_key, is_valid_api_key_format, verify_api_key, ApiKey, ApiKeyUsage,
	API_KEY_BYTES, API_KEY_PREFIX,
};
pub use audit::{AuditEventType, AuditLogBuilder, AuditLogEntry, AUDIT_RETENTION_DAYS};
pub use device_code::{
	generate_device_code, generate_user_code, is_valid_user_code_format, DeviceCode,
	DeviceCodeStatus, DEVICE_CODE_EXPIRY_MINUTES, POLL_INTERVAL_SECONDS,
};
pub use email::{render_email, EmailTemplate, SmtpConfig, TlsMode};
pub use error::AuthError;
pub use magic_link::{
	generate_magic_link_token, hash_magic_link_token, verify_magic_link_token, MagicLink,
	MAGIC_LINK_EXPIRY_MINUTES, MAGIC_LINK_TOKEN_BYTES,
};
pub use middleware::{
	extract_bearer_token, extract_session_cookie, extract_session_cookie_with_name,
	identify_bearer_token, is_access_token, is_api_key_token, is_ws_token, AuthConfig, AuthContext,
	AuthRequired, BearerTokenType, CurrentUser, DEV_MODE_ENV_VAR, LOOM_ENV_VAR, SESSION_COOKIE_NAME,
};
pub use org::{OrgInvitation, OrgJoinRequest, OrgMembership, OrgVisibility, Organization};
pub use session::{generate_session_token, Session, SESSION_EXPIRY_DAYS};
pub use share_link::{
	generate_share_token, hash_share_token, verify_share_token, ShareLink, SHARE_TOKEN_BYTES,
};
pub use support_access::{SupportAccess, SUPPORT_ACCESS_DAYS};
pub use team::{Team, TeamMembership};
pub use types::*;
pub use user::{
	generate_username_base, is_username_reserved, validate_username, Identity, Provider, User,
	UserProfile, RESERVED_USERNAMES,
};
pub use websocket::{
	auth_timeout, close_code_for_error, close_codes, WsAuthContext, WsAuthError, WsAuthMessage,
	WsAuthMethod, WsAuthResponse, WsAuthState, WS_AUTH_TIMEOUT_SECS,
};
pub use ws_token::{
	generate_ws_token, hash_ws_token, is_valid_ws_token_format, verify_ws_token, WsToken,
	WS_TOKEN_BYTES, WS_TOKEN_EXPIRY_SECONDS, WS_TOKEN_PREFIX,
};

/// Hash a token using SHA-256 and return the hex-encoded result.
///
/// This function is used to hash tokens before database lookup, ensuring
/// that raw tokens are never stored in the database. The SHA-256 hash is
/// one-way, so even if the database is compromised, the raw tokens cannot
/// be recovered.
///
/// # Security
///
/// - Input tokens are consumed and not logged
/// - Output hash is safe to log and store
pub fn hash_token(token: &str) -> String {
	use sha2::{Digest, Sha256};
	let mut hasher = Sha256::new();
	hasher.update(token.as_bytes());
	hex::encode(hasher.finalize())
}
