// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authentication API types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// List of available authentication providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AuthProvidersResponse {
	pub providers: Vec<String>,
}

/// Current authenticated user information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CurrentUserResponse {
	pub id: String,
	pub display_name: String,
	pub username: Option<String>,
	pub email: Option<String>,
	pub avatar_url: Option<String>,
	pub locale: Option<String>,
	pub global_roles: Vec<String>,
	pub created_at: DateTime<Utc>,
}

/// Generic success response for auth operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AuthSuccessResponse {
	pub message: String,
}

/// Error response for auth operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AuthErrorResponse {
	pub error: String,
	pub message: String,
}

/// Request body for magic link authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MagicLinkRequest {
	pub email: String,
}

/// Response for starting device code flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeviceCodeStartResponse {
	pub device_code: String,
	pub user_code: String,
	pub verification_url: String,
	pub expires_in: i64,
}

/// Request body for polling device code status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeviceCodePollRequest {
	pub device_code: String,
}

/// Response for polling device code status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "status")]
pub enum DeviceCodePollResponse {
	#[serde(rename = "pending")]
	Pending,
	#[serde(rename = "completed")]
	Completed { access_token: String },
	#[serde(rename = "expired")]
	Expired,
}

/// Request body for completing device code flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeviceCodeCompleteRequest {
	pub user_code: String,
}

/// Response for completing device code flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeviceCodeCompleteResponse {
	pub message: String,
}

/// Query parameters for OAuth callback.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct OAuthCallbackQuery {
	pub code: Option<String>,
	pub state: Option<String>,
	pub error: Option<String>,
	pub error_description: Option<String>,
}

/// Response for WebSocket token request.
///
/// Returns a short-lived token that can be used for WebSocket first-message authentication.
/// The token is valid for 30 seconds and can only be used once.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WsTokenResponse {
	/// The WebSocket authentication token (prefix: ws_).
	pub token: String,
	/// Token expiry time in seconds (30).
	pub expires_in: i64,
}
