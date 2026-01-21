// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authentication and authorization error types.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during authentication and authorization.
#[derive(Debug, Error)]
pub enum AuthError {
	// =========================================================================
	// Authentication Errors
	// =========================================================================
	/// No authentication credentials provided.
	#[error("authentication required")]
	AuthenticationRequired,

	/// The provided credentials are invalid.
	#[error("invalid credentials")]
	InvalidCredentials,

	/// The session has expired.
	#[error("session expired")]
	SessionExpired,

	/// The session was revoked.
	#[error("session revoked")]
	SessionRevoked,

	/// The session was not found.
	#[error("session not found")]
	SessionNotFound,

	/// The access token is invalid.
	#[error("invalid access token")]
	InvalidAccessToken,

	/// The access token has expired.
	#[error("access token expired")]
	AccessTokenExpired,

	/// The API key is invalid.
	#[error("invalid API key")]
	InvalidApiKey,

	/// The API key has been revoked.
	#[error("API key revoked")]
	ApiKeyRevoked,

	/// The API key does not have the required scope.
	#[error("API key missing required scope: {0}")]
	ApiKeyMissingScope(String),

	// =========================================================================
	// OAuth Errors
	// =========================================================================
	/// OAuth authorization failed.
	#[error("OAuth authorization failed: {0}")]
	OAuthFailed(String),

	/// OAuth state mismatch (possible CSRF attack).
	#[error("OAuth state mismatch")]
	OAuthStateMismatch,

	/// OAuth provider returned an error.
	#[error("OAuth provider error: {0}")]
	OAuthProviderError(String),

	/// Failed to exchange OAuth code for tokens.
	#[error("OAuth token exchange failed: {0}")]
	OAuthTokenExchangeFailed(String),

	/// Failed to fetch user info from OAuth provider.
	#[error("failed to fetch user info from OAuth provider: {0}")]
	OAuthUserInfoFailed(String),

	// =========================================================================
	// Magic Link Errors
	// =========================================================================
	/// The magic link token is invalid.
	#[error("invalid magic link")]
	InvalidMagicLink,

	/// The magic link has expired.
	#[error("magic link expired")]
	MagicLinkExpired,

	/// The magic link has already been used.
	#[error("magic link already used")]
	MagicLinkAlreadyUsed,

	/// Failed to send magic link email.
	#[error("failed to send magic link email: {0}")]
	MagicLinkEmailFailed(String),

	// =========================================================================
	// Device Code Flow Errors
	// =========================================================================
	/// The device code is invalid.
	#[error("invalid device code")]
	InvalidDeviceCode,

	/// The device code has expired.
	#[error("device code expired")]
	DeviceCodeExpired,

	/// The device code authorization is still pending.
	#[error("authorization pending")]
	DeviceCodePending,

	/// The device code authorization was denied by the user.
	#[error("authorization denied")]
	DeviceCodeDenied,

	// =========================================================================
	// User Errors
	// =========================================================================
	/// The user was not found.
	#[error("user not found: {0}")]
	UserNotFound(Uuid),

	/// The user account has been suspended.
	#[error("user account suspended")]
	UserSuspended,

	/// The user account has been deleted.
	#[error("user account deleted")]
	UserDeleted,

	/// Email already in use by another account.
	#[error("email already in use by another account")]
	EmailAlreadyInUse,

	/// Cannot remove last identity from account.
	#[error("cannot remove last identity from account")]
	CannotRemoveLastIdentity,

	// =========================================================================
	// Authorization Errors
	// =========================================================================
	/// Access denied by ABAC policy.
	#[error("access denied")]
	AccessDenied,

	/// Forbidden operation with a specific reason.
	#[error("forbidden: {0}")]
	Forbidden(String),

	/// The resource was not found (or access is denied).
	#[error("resource not found")]
	ResourceNotFound,

	/// Insufficient permissions for this operation.
	#[error("insufficient permissions")]
	InsufficientPermissions,

	// =========================================================================
	// Organization Errors
	// =========================================================================
	/// The organization was not found.
	#[error("organization not found: {0}")]
	OrgNotFound(Uuid),

	/// The user is not a member of this organization.
	#[error("not a member of this organization")]
	NotOrgMember,

	/// The user is already a member of this organization.
	#[error("already a member of this organization")]
	AlreadyOrgMember,

	/// Cannot remove the last owner of an organization.
	#[error("cannot remove last owner of organization")]
	CannotRemoveLastOwner,

	/// Organization name is already taken.
	#[error("organization name already taken")]
	OrgNameTaken,

	/// Join request not found.
	#[error("join request not found")]
	JoinRequestNotFound,

	/// Join request already exists.
	#[error("join request already pending")]
	JoinRequestAlreadyPending,

	// =========================================================================
	// Team Errors
	// =========================================================================
	/// The team was not found.
	#[error("team not found: {0}")]
	TeamNotFound(Uuid),

	/// The user is not a member of this team.
	#[error("not a member of this team")]
	NotTeamMember,

	/// The user is already a member of this team.
	#[error("already a member of this team")]
	AlreadyTeamMember,

	/// Team name already exists in this organization.
	#[error("team name already exists in this organization")]
	TeamNameTaken,

	// =========================================================================
	// Invitation Errors
	// =========================================================================
	/// The invitation was not found or is invalid.
	#[error("invalid invitation")]
	InvalidInvitation,

	/// The invitation has expired.
	#[error("invitation expired")]
	InvitationExpired,

	/// The invitation has already been used.
	#[error("invitation already used")]
	InvitationAlreadyUsed,

	// =========================================================================
	// Share Link Errors
	// =========================================================================
	/// The share link is invalid.
	#[error("invalid share link")]
	InvalidShareLink,

	/// The share link has expired.
	#[error("share link expired")]
	ShareLinkExpired,

	/// The share link has been revoked.
	#[error("share link revoked")]
	ShareLinkRevoked,

	// =========================================================================
	// Support Access Errors
	// =========================================================================
	/// Support access has not been granted.
	#[error("support access not granted")]
	SupportAccessNotGranted,

	/// Support access has expired.
	#[error("support access expired")]
	SupportAccessExpired,

	// =========================================================================
	// Infrastructure Errors
	// =========================================================================
	/// Database error.
	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	/// Token hashing error.
	#[error("token hashing error: {0}")]
	HashingError(String),

	/// Internal error.
	#[error("internal error: {0}")]
	Internal(String),

	/// Configuration error.
	#[error("configuration error: {0}")]
	Configuration(String),
}

impl AuthError {
	/// Returns true if this error should be logged at error level.
	pub fn is_internal(&self) -> bool {
		matches!(
			self,
			AuthError::Database(_)
				| AuthError::HashingError(_)
				| AuthError::Internal(_)
				| AuthError::Configuration(_)
		)
	}

	/// Returns the HTTP status code for this error.
	pub fn status_code(&self) -> u16 {
		match self {
			// 401 Unauthorized
			AuthError::AuthenticationRequired
			| AuthError::InvalidCredentials
			| AuthError::SessionExpired
			| AuthError::SessionRevoked
			| AuthError::SessionNotFound
			| AuthError::InvalidAccessToken
			| AuthError::AccessTokenExpired
			| AuthError::InvalidApiKey
			| AuthError::ApiKeyRevoked
			| AuthError::InvalidMagicLink
			| AuthError::MagicLinkExpired
			| AuthError::MagicLinkAlreadyUsed
			| AuthError::InvalidDeviceCode
			| AuthError::DeviceCodeExpired
			| AuthError::DeviceCodeDenied
			| AuthError::InvalidInvitation
			| AuthError::InvitationExpired
			| AuthError::InvitationAlreadyUsed
			| AuthError::InvalidShareLink
			| AuthError::ShareLinkExpired
			| AuthError::ShareLinkRevoked => 401,

			// 403 Forbidden
			AuthError::AccessDenied
			| AuthError::Forbidden(_)
			| AuthError::InsufficientPermissions
			| AuthError::ApiKeyMissingScope(_)
			| AuthError::NotOrgMember
			| AuthError::NotTeamMember
			| AuthError::UserSuspended
			| AuthError::UserDeleted
			| AuthError::SupportAccessNotGranted
			| AuthError::SupportAccessExpired => 403,

			// 404 Not Found
			AuthError::UserNotFound(_)
			| AuthError::OrgNotFound(_)
			| AuthError::TeamNotFound(_)
			| AuthError::ResourceNotFound
			| AuthError::JoinRequestNotFound => 404,

			// 409 Conflict
			AuthError::EmailAlreadyInUse
			| AuthError::AlreadyOrgMember
			| AuthError::AlreadyTeamMember
			| AuthError::OrgNameTaken
			| AuthError::TeamNameTaken
			| AuthError::JoinRequestAlreadyPending => 409,

			// 422 Unprocessable Entity
			AuthError::CannotRemoveLastIdentity | AuthError::CannotRemoveLastOwner => 422,

			// 202 Accepted (for pending states)
			AuthError::DeviceCodePending => 202,

			// OAuth errors - could be various causes
			AuthError::OAuthFailed(_)
			| AuthError::OAuthStateMismatch
			| AuthError::OAuthProviderError(_)
			| AuthError::OAuthTokenExchangeFailed(_)
			| AuthError::OAuthUserInfoFailed(_)
			| AuthError::MagicLinkEmailFailed(_) => 400,

			// 500 Internal Server Error
			AuthError::Database(_)
			| AuthError::HashingError(_)
			| AuthError::Internal(_)
			| AuthError::Configuration(_) => 500,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn auth_required_is_401() {
		assert_eq!(AuthError::AuthenticationRequired.status_code(), 401);
	}

	#[test]
	fn access_denied_is_403() {
		assert_eq!(AuthError::AccessDenied.status_code(), 403);
	}

	#[test]
	fn user_not_found_is_404() {
		assert_eq!(AuthError::UserNotFound(Uuid::nil()).status_code(), 404);
	}

	#[test]
	fn email_conflict_is_409() {
		assert_eq!(AuthError::EmailAlreadyInUse.status_code(), 409);
	}

	#[test]
	fn device_code_pending_is_202() {
		assert_eq!(AuthError::DeviceCodePending.status_code(), 202);
	}

	#[test]
	fn internal_errors_are_flagged() {
		assert!(AuthError::Internal("test".into()).is_internal());
		assert!(AuthError::HashingError("test".into()).is_internal());
		assert!(!AuthError::AccessDenied.is_internal());
	}
}
