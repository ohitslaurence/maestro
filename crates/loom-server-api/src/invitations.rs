// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationResponse {
	pub id: String,
	pub org_id: String,
	pub org_name: String,
	pub email: String,
	pub role: String,
	pub invited_by: String,
	pub invited_by_name: String,
	pub created_at: DateTime<Utc>,
	pub expires_at: DateTime<Utc>,
	pub is_expired: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListInvitationsResponse {
	pub invitations: Vec<InvitationResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInvitationRequest {
	pub email: String,
	#[serde(default)]
	pub role: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateInvitationResponse {
	pub id: String,
	pub email: String,
	pub role: String,
	pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationSuccessResponse {
	pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationErrorResponse {
	pub error: String,
	pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AcceptInvitationRequest {
	pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AcceptInvitationResponse {
	pub org_id: String,
	pub org_name: String,
	pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinRequestResponse {
	pub id: String,
	pub user_id: String,
	pub display_name: String,
	pub email: Option<String>,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListJoinRequestsResponse {
	pub requests: Vec<JoinRequestResponse>,
}
