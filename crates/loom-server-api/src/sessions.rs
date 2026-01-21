// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// A session in the list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SessionResponse {
	pub id: String,
	pub session_type: String,
	pub created_at: DateTime<Utc>,
	pub last_used_at: DateTime<Utc>,
	pub expires_at: DateTime<Utc>,
	pub ip_address: Option<String>,
	pub user_agent: Option<String>,
	pub geo_city: Option<String>,
	pub geo_country: Option<String>,
	pub is_current: bool,
}

/// Response for listing sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListSessionsResponse {
	pub sessions: Vec<SessionResponse>,
}

/// Response for session operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SessionSuccessResponse {
	pub message: String,
}

/// Error response for session operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SessionErrorResponse {
	pub error: String,
	pub message: String,
}
