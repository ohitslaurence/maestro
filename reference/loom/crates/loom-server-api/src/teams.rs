// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_auth::{team::Team, types::TeamRole};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// A team in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TeamResponse {
	pub id: String,
	pub org_id: String,
	pub name: String,
	pub slug: String,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
	pub member_count: Option<i64>,
}

/// Response for listing teams.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListTeamsResponse {
	pub teams: Vec<TeamResponse>,
}

/// Request to create a team.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateTeamRequest {
	pub name: String,
	pub slug: String,
}

/// Request to update a team.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateTeamRequest {
	pub name: Option<String>,
	pub slug: Option<String>,
}

/// Success response for team operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TeamSuccessResponse {
	pub message: String,
}

/// Error response for team operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TeamErrorResponse {
	pub error: String,
	pub message: String,
}

/// Team role for API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TeamRoleApi {
	Maintainer,
	Member,
}

/// A member in a team.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TeamMemberResponse {
	pub user_id: String,
	pub display_name: String,
	pub email: Option<String>,
	pub avatar_url: Option<String>,
	pub role: TeamRoleApi,
	pub joined_at: DateTime<Utc>,
}

/// Response for listing team members.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListTeamMembersResponse {
	pub members: Vec<TeamMemberResponse>,
}

/// Request to add a member to a team.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AddTeamMemberRequest {
	pub user_id: String,
	#[serde(default = "default_team_role")]
	pub role: TeamRoleApi,
}

fn default_team_role() -> TeamRoleApi {
	TeamRoleApi::Member
}

impl TeamResponse {
	pub fn from_team(team: Team, member_count: Option<i64>) -> Self {
		Self {
			id: team.id.to_string(),
			org_id: team.org_id.to_string(),
			name: team.name,
			slug: team.slug,
			created_at: team.created_at,
			updated_at: team.updated_at,
			member_count,
		}
	}
}

impl From<TeamRole> for TeamRoleApi {
	fn from(role: TeamRole) -> Self {
		match role {
			TeamRole::Maintainer => TeamRoleApi::Maintainer,
			TeamRole::Member => TeamRoleApi::Member,
		}
	}
}

impl From<TeamRoleApi> for TeamRole {
	fn from(role: TeamRoleApi) -> Self {
		match role {
			TeamRoleApi::Maintainer => TeamRole::Maintainer,
			TeamRoleApi::Member => TeamRole::Member,
		}
	}
}
