// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// A user profile in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserProfileResponse {
	pub id: String,
	pub display_name: String,
	pub email: Option<String>,
	pub avatar_url: Option<String>,
}

/// Extended user profile for the current user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CurrentUserProfileResponse {
	pub id: String,
	pub display_name: String,
	pub username: Option<String>,
	pub primary_email: Option<String>,
	pub avatar_url: Option<String>,
	pub email_visible: bool,
	pub locale: Option<String>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

/// Request to update user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateUserProfileRequest {
	pub display_name: Option<String>,
	pub username: Option<String>,
	pub avatar_url: Option<String>,
	pub email_visible: Option<bool>,
	pub locale: Option<String>,
}

/// Success response for user operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserSuccessResponse {
	pub message: String,
}

/// Error response for user operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserErrorResponse {
	pub error: String,
	pub message: String,
}

/// Response for account deletion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AccountDeletionResponse {
	pub message: String,
	pub deletion_scheduled_at: DateTime<Utc>,
	pub grace_period_days: i32,
}

/// A linked identity (OAuth provider) for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct IdentityResponse {
	pub id: String,
	pub provider: String,
	pub email: String,
	pub email_verified: bool,
	pub created_at: DateTime<Utc>,
}

/// Response containing all linked identities for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListIdentitiesResponse {
	pub identities: Vec<IdentityResponse>,
}
