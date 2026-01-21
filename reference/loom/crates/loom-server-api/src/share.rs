// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Request body for creating a share link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateShareLinkRequest {
	/// Optional expiration timestamp. If not provided, link never expires.
	#[cfg_attr(feature = "openapi", schema(example = "2025-02-01T00:00:00Z"))]
	pub expires_at: Option<String>,
}

/// Response for share link creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateShareLinkResponse {
	/// The full shareable URL.
	#[cfg_attr(
		feature = "openapi",
		schema(example = "https://loom.example/threads/T-123/share/abc123def456...")
	)]
	pub url: String,
	/// When the link expires, if set.
	pub expires_at: Option<String>,
	/// When the link was created.
	pub created_at: String,
}

/// Response for share link operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ShareLinkSuccessResponse {
	pub message: String,
}

/// Error response for share link operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ShareLinkErrorResponse {
	pub message: String,
	pub code: String,
}

/// Response containing shared thread data (read-only view).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SharedThreadResponse {
	/// Thread ID.
	pub id: String,
	/// Thread title.
	pub title: Option<String>,
	/// When the thread was created.
	pub created_at: String,
	/// When the thread was last updated.
	pub updated_at: String,
	/// Thread content (read-only snapshot).
	pub content: serde_json::Value,
}

/// Response for support access request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SupportAccessRequestResponse {
	/// Unique ID for this support access request.
	pub request_id: String,
	/// Thread ID the access was requested for.
	pub thread_id: String,
	/// When the request was made.
	pub requested_at: String,
	/// Current status of the request.
	#[cfg_attr(feature = "openapi", schema(example = "pending"))]
	pub status: String,
}

/// Response for support access approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SupportAccessApprovalResponse {
	/// Thread ID access was granted for.
	pub thread_id: String,
	/// User ID of the support staff granted access.
	pub granted_to: String,
	/// When access was approved.
	pub approved_at: String,
	/// When access expires (31 days after approval).
	pub expires_at: String,
}

/// Response for support access operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SupportAccessSuccessResponse {
	pub message: String,
}

/// Error response for support access operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SupportAccessErrorResponse {
	pub message: String,
	pub code: String,
}
