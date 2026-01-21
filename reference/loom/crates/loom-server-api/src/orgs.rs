// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_auth::org::{OrgVisibility, Organization};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Organization visibility setting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OrgVisibilityApi {
	Public,
	Unlisted,
	Private,
}

impl From<OrgVisibility> for OrgVisibilityApi {
	fn from(v: OrgVisibility) -> Self {
		match v {
			OrgVisibility::Public => OrgVisibilityApi::Public,
			OrgVisibility::Unlisted => OrgVisibilityApi::Unlisted,
			OrgVisibility::Private => OrgVisibilityApi::Private,
		}
	}
}

impl From<OrgVisibilityApi> for OrgVisibility {
	fn from(v: OrgVisibilityApi) -> Self {
		match v {
			OrgVisibilityApi::Public => OrgVisibility::Public,
			OrgVisibilityApi::Unlisted => OrgVisibility::Unlisted,
			OrgVisibilityApi::Private => OrgVisibility::Private,
		}
	}
}

/// An organization in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct OrgResponse {
	pub id: String,
	pub name: String,
	pub slug: String,
	pub visibility: OrgVisibilityApi,
	pub is_personal: bool,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
	pub member_count: Option<i64>,
}

impl OrgResponse {
	pub fn from_org(org: Organization, member_count: Option<i64>) -> Self {
		Self {
			id: org.id.to_string(),
			name: org.name,
			slug: org.slug,
			visibility: org.visibility.into(),
			is_personal: org.is_personal,
			created_at: org.created_at,
			updated_at: org.updated_at,
			member_count,
		}
	}
}

/// Response for listing organizations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListOrgsResponse {
	pub orgs: Vec<OrgResponse>,
}

/// Request to create an organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateOrgRequest {
	pub name: String,
	pub slug: String,
	#[serde(default)]
	pub visibility: Option<OrgVisibilityApi>,
}

/// Request to update an organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateOrgRequest {
	pub name: Option<String>,
	pub slug: Option<String>,
	pub visibility: Option<OrgVisibilityApi>,
}

/// Success response for organization operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct OrgSuccessResponse {
	pub message: String,
}

/// Error response for organization operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct OrgErrorResponse {
	pub error: String,
	pub message: String,
}

/// A member in an organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct OrgMemberResponse {
	pub user_id: String,
	pub display_name: String,
	pub email: Option<String>,
	pub avatar_url: Option<String>,
	pub role: String,
	pub joined_at: DateTime<Utc>,
}

/// Response for listing organization members.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListOrgMembersResponse {
	pub members: Vec<OrgMemberResponse>,
}

pub use crate::invitations::{JoinRequestResponse, ListJoinRequestsResponse};

/// Request to add a member to an organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AddOrgMemberRequest {
	pub email: String,
	#[serde(default)]
	pub role: Option<String>,
}

/// Request to update a member's role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateOrgMemberRoleRequest {
	pub role: String,
}
