// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	Json,
};
use loom_scim::patch::PatchRequest;
use loom_scim::types::{GroupMember, Meta, SCHEMA_CORE_GROUP};
use loom_scim::{evaluate_filter, FilterParser, ListResponse, ScimGroup};
use loom_server_audit::{AuditEventType, AuditLogEntry};
use loom_server_auth::{TeamId, UserId};
use loom_server_db::ScimTeam;
use serde::Deserialize;
use serde_json::json;
use tracing::info;
use uuid::Uuid;

use crate::error::ScimApiError;
use crate::handlers::users::ScimState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGroupsQuery {
	#[serde(default = "default_start_index")]
	pub start_index: i64,
	#[serde(default = "default_count")]
	pub count: i64,
	pub filter: Option<String>,
}

fn default_start_index() -> i64 {
	1
}
fn default_count() -> i64 {
	100
}

fn parse_team_id(s: &str) -> Result<TeamId, ScimApiError> {
	let uuid =
		Uuid::parse_str(s).map_err(|e| ScimApiError::Internal(format!("Invalid team ID: {}", e)))?;
	Ok(TeamId::new(uuid))
}

fn parse_user_id(s: &str) -> Result<UserId, ScimApiError> {
	let uuid =
		Uuid::parse_str(s).map_err(|e| ScimApiError::Internal(format!("Invalid user ID: {}", e)))?;
	Ok(UserId::new(uuid))
}

fn scim_team_to_group(team: &ScimTeam, members: Vec<GroupMember>) -> ScimGroup {
	ScimGroup {
		schemas: vec![SCHEMA_CORE_GROUP.to_string()],
		id: Some(team.id.to_string()),
		external_id: team.scim_external_id.clone(),
		display_name: team.name.clone(),
		members,
		meta: Some(Meta {
			resource_type: "Group".to_string(),
			created: team.created_at,
			last_modified: team.updated_at,
			location: None,
			version: None,
		}),
	}
}

async fn get_group_members(
	state: &ScimState,
	team_id: &TeamId,
) -> Result<Vec<GroupMember>, ScimApiError> {
	let members = state.team_repo.list_scim_group_members(team_id).await?;
	Ok(
		members
			.into_iter()
			.map(|(user_id, display_name)| {
				let id = user_id.to_string();
				GroupMember {
					value: id.clone(),
					ref_: Some(format!("/api/scim/Users/{}", id)),
					display: display_name,
				}
			})
			.collect(),
	)
}

fn get_group_attr(group: &ScimGroup, attr: &str) -> Option<String> {
	match attr.to_lowercase().as_str() {
		"displayname" => Some(group.display_name.clone()),
		"id" => group.id.clone(),
		"externalid" => group.external_id.clone(),
		_ => None,
	}
}

pub async fn list_groups(
	State(state): State<ScimState>,
	Query(query): Query<ListGroupsQuery>,
) -> Result<Json<ListResponse<ScimGroup>>, ScimApiError> {
	let parsed_filter = if let Some(ref filter_str) = query.filter {
		Some(FilterParser::parse(filter_str)?)
	} else {
		None
	};

	let teams = state
		.team_repo
		.list_scim_teams(&state.org_id, 10000, 0)
		.await?;

	let mut all_groups = Vec::with_capacity(teams.len());
	for team in &teams {
		let members = get_group_members(&state, &team.id).await?;
		all_groups.push(scim_team_to_group(team, members));
	}

	let filtered_groups: Vec<ScimGroup> = if let Some(ref filter) = parsed_filter {
		all_groups
			.into_iter()
			.filter(|group| evaluate_filter(filter, &|attr| get_group_attr(group, attr)))
			.collect()
	} else {
		all_groups
	};

	let total = filtered_groups.len() as i64;
	let count = query.count.min(1000);
	let offset = (query.start_index - 1).max(0) as usize;
	let paginated: Vec<ScimGroup> = filtered_groups
		.into_iter()
		.skip(offset)
		.take(count as usize)
		.collect();

	Ok(Json(ListResponse::new(
		paginated,
		total,
		query.start_index,
		count,
	)))
}

pub async fn create_group(
	State(state): State<ScimState>,
	Json(scim_group): Json<ScimGroup>,
) -> Result<(StatusCode, Json<ScimGroup>), ScimApiError> {
	let team_id = state
		.team_repo
		.create_scim_team(
			&state.org_id,
			&scim_group.display_name,
			scim_group.external_id.as_deref(),
		)
		.await?;

	let user_ids: Result<Vec<UserId>, _> = scim_group
		.members
		.iter()
		.map(|m| parse_user_id(&m.value))
		.collect();
	let user_ids = user_ids?;

	state
		.team_repo
		.set_team_members(&team_id, &user_ids)
		.await?;

	info!(team_id = %team_id, name = %scim_group.display_name, "SCIM: created group");

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimGroupCreated)
			.resource("team", team_id.to_string())
			.action("SCIM group created")
			.details(json!({
				"display_name": scim_group.display_name,
				"member_count": user_ids.len(),
				"org_id": state.org_id.to_string(),
			}))
			.build(),
	);

	get_group(State(state), Path(team_id.to_string()))
		.await
		.map(|g| (StatusCode::CREATED, g))
}

pub async fn get_group(
	State(state): State<ScimState>,
	Path(id): Path<String>,
) -> Result<Json<ScimGroup>, ScimApiError> {
	let team_id = parse_team_id(&id)?;

	let team = state
		.team_repo
		.get_team_with_scim_fields(&team_id, &state.org_id)
		.await?
		.ok_or_else(|| ScimApiError::NotFound(format!("Group {} not found", id)))?;

	let members = get_group_members(&state, &team_id).await?;

	Ok(Json(scim_team_to_group(&team, members)))
}

pub async fn replace_group(
	State(state): State<ScimState>,
	Path(id): Path<String>,
	Json(scim_group): Json<ScimGroup>,
) -> Result<Json<ScimGroup>, ScimApiError> {
	let team_id = parse_team_id(&id)?;

	state
		.team_repo
		.update_scim_team(
			&team_id,
			&scim_group.display_name,
			scim_group.external_id.as_deref(),
		)
		.await?;

	let user_ids: Result<Vec<UserId>, _> = scim_group
		.members
		.iter()
		.map(|m| parse_user_id(&m.value))
		.collect();
	let user_ids = user_ids?;

	state
		.team_repo
		.set_team_members(&team_id, &user_ids)
		.await?;

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimGroupUpdated)
			.resource("team", team_id.to_string())
			.action("SCIM group replaced")
			.details(json!({
				"display_name": scim_group.display_name,
				"member_count": user_ids.len(),
			}))
			.build(),
	);

	get_group(State(state), Path(id)).await
}

pub async fn patch_group(
	State(state): State<ScimState>,
	Path(id): Path<String>,
	Json(patch): Json<PatchRequest>,
) -> Result<Json<ScimGroup>, ScimApiError> {
	patch.validate()?;
	let team_id = parse_team_id(&id)?;

	for op in &patch.operations {
		match op.path.as_deref() {
			Some("displayName") => {
				if let Some(value) = op.value.as_ref().and_then(|v| v.as_str()) {
					let team = state
						.team_repo
						.get_team_with_scim_fields(&team_id, &state.org_id)
						.await?
						.ok_or_else(|| ScimApiError::NotFound(format!("Group {} not found", id)))?;

					state
						.team_repo
						.update_scim_team(&team_id, value, team.scim_external_id.as_deref())
						.await?;
				}
			}
			Some("members") => {
				if let Some(members) = op.value.as_ref().and_then(|v| v.as_array()) {
					for member in members {
						if let Some(user_id_str) = member.get("value").and_then(|v| v.as_str()) {
							let user_id = parse_user_id(user_id_str)?;
							match op.op {
								loom_scim::PatchOp::Add => {
									state
										.team_repo
										.add_member(&team_id, &user_id, loom_server_auth::TeamRole::Member)
										.await?;

									state.audit_service.log(
										AuditLogEntry::builder(AuditEventType::ScimGroupMemberAdded)
											.resource("team", team_id.to_string())
											.action("SCIM group member added")
											.details(json!({
												"user_id": user_id.to_string(),
											}))
											.build(),
									);
								}
								loom_scim::PatchOp::Remove => {
									state.team_repo.remove_member(&team_id, &user_id).await?;

									state.audit_service.log(
										AuditLogEntry::builder(AuditEventType::ScimGroupMemberRemoved)
											.resource("team", team_id.to_string())
											.action("SCIM group member removed")
											.details(json!({
												"user_id": user_id.to_string(),
											}))
											.build(),
									);
								}
								_ => {}
							}
						}
					}
				}
			}
			_ => {}
		}
	}

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimGroupUpdated)
			.resource("team", team_id.to_string())
			.action("SCIM group patched")
			.details(json!({
				"operations_count": patch.operations.len(),
			}))
			.build(),
	);

	get_group(State(state), Path(id)).await
}

pub async fn delete_group(
	State(state): State<ScimState>,
	Path(id): Path<String>,
) -> Result<StatusCode, ScimApiError> {
	let team_id = parse_team_id(&id)?;

	state
		.team_repo
		.delete_scim_team(&team_id, &state.org_id)
		.await?;

	info!(team_id = %id, "SCIM: deleted group");

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimGroupDeleted)
			.resource("team", team_id.to_string())
			.action("SCIM group deleted")
			.details(json!({
				"org_id": state.org_id.to_string(),
			}))
			.build(),
	);

	Ok(StatusCode::NO_CONTENT)
}
