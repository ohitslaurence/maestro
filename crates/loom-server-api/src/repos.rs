// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_scm::{OwnerType, RepoRole, Visibility};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OwnerTypeApi {
	User,
	Org,
}

impl From<OwnerType> for OwnerTypeApi {
	fn from(v: OwnerType) -> Self {
		match v {
			OwnerType::User => OwnerTypeApi::User,
			OwnerType::Org => OwnerTypeApi::Org,
		}
	}
}

impl From<OwnerTypeApi> for OwnerType {
	fn from(v: OwnerTypeApi) -> Self {
		match v {
			OwnerTypeApi::User => OwnerType::User,
			OwnerTypeApi::Org => OwnerType::Org,
		}
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum VisibilityApi {
	#[default]
	Private,
	Public,
}

impl From<Visibility> for VisibilityApi {
	fn from(v: Visibility) -> Self {
		match v {
			Visibility::Private => VisibilityApi::Private,
			Visibility::Public => VisibilityApi::Public,
		}
	}
}

impl From<VisibilityApi> for Visibility {
	fn from(v: VisibilityApi) -> Self {
		match v {
			VisibilityApi::Private => Visibility::Private,
			VisibilityApi::Public => Visibility::Public,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RepoRoleApi {
	Read,
	Write,
	Admin,
}

impl From<RepoRole> for RepoRoleApi {
	fn from(v: RepoRole) -> Self {
		match v {
			RepoRole::Read => RepoRoleApi::Read,
			RepoRole::Write => RepoRoleApi::Write,
			RepoRole::Admin => RepoRoleApi::Admin,
		}
	}
}

impl From<RepoRoleApi> for RepoRole {
	fn from(v: RepoRoleApi) -> Self {
		match v {
			RepoRoleApi::Read => RepoRole::Read,
			RepoRoleApi::Write => RepoRole::Write,
			RepoRoleApi::Admin => RepoRole::Admin,
		}
	}
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRepoRequest {
	pub owner_type: OwnerTypeApi,
	pub owner_id: Uuid,
	pub name: String,
	#[serde(default)]
	pub visibility: VisibilityApi,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateRepoRequest {
	pub name: Option<String>,
	pub visibility: Option<VisibilityApi>,
	pub default_branch: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoResponse {
	pub id: Uuid,
	pub owner_type: OwnerTypeApi,
	pub owner_id: Uuid,
	pub name: String,
	pub visibility: VisibilityApi,
	pub default_branch: String,
	pub clone_url: String,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListReposResponse {
	pub repos: Vec<RepoResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoSuccessResponse {
	pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoErrorResponse {
	pub error: String,
	pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoTeamAccessResponse {
	pub team_id: Uuid,
	pub role: RepoRoleApi,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListRepoTeamAccessResponse {
	pub teams: Vec<RepoTeamAccessResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantTeamAccessRequest {
	pub team_id: Uuid,
	pub role: RepoRoleApi,
}
