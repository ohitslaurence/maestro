// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{extract::State, Json};
use loom_scim::patch::PatchRequest;
use loom_scim::{ScimError, ScimGroup, ScimUser};
use loom_server_audit::{AuditEventType, AuditLogEntry};
use loom_server_auth::{TeamId, TeamRole, UserId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::ScimApiError;
use crate::handlers::users::ScimState;
use crate::mapping::{scim_user_to_display_name, scim_user_to_email};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkRequest {
	pub schemas: Vec<String>,
	#[serde(rename = "Operations")]
	pub operations: Vec<BulkOperation>,
	#[serde(default)]
	pub fail_on_errors: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOperation {
	pub method: String,
	pub path: String,
	#[serde(default)]
	pub bulk_id: Option<String>,
	#[serde(default)]
	pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkResponse {
	pub schemas: Vec<String>,
	#[serde(rename = "Operations")]
	pub operations: Vec<BulkOperationResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOperationResponse {
	pub method: String,
	pub bulk_id: Option<String>,
	pub status: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub location: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub response: Option<serde_json::Value>,
}

#[derive(Debug)]
pub(crate) enum ResourcePath {
	Users,
	UsersId(String),
	Groups,
	GroupsId(String),
}

pub(crate) fn parse_path(path: &str) -> Option<ResourcePath> {
	let path = path.trim_start_matches('/');

	if path == "Users" {
		return Some(ResourcePath::Users);
	}
	if path == "Groups" {
		return Some(ResourcePath::Groups);
	}
	if let Some(id) = path.strip_prefix("Users/") {
		return Some(ResourcePath::UsersId(id.to_string()));
	}
	if let Some(id) = path.strip_prefix("Groups/") {
		return Some(ResourcePath::GroupsId(id.to_string()));
	}
	None
}

pub(crate) fn resolve_bulk_id_refs(
	value: &mut serde_json::Value,
	bulk_id_map: &HashMap<String, String>,
) {
	match value {
		serde_json::Value::String(s) => {
			if let Some(bulk_id) = s.strip_prefix("bulkId:") {
				if let Some(resolved) = bulk_id_map.get(bulk_id) {
					*s = resolved.clone();
				}
			}
		}
		serde_json::Value::Array(arr) => {
			for item in arr {
				resolve_bulk_id_refs(item, bulk_id_map);
			}
		}
		serde_json::Value::Object(map) => {
			for (_, v) in map {
				resolve_bulk_id_refs(v, bulk_id_map);
			}
		}
		_ => {}
	}
}

pub async fn bulk_operations(
	State(state): State<ScimState>,
	Json(request): Json<BulkRequest>,
) -> Result<Json<BulkResponse>, ScimApiError> {
	if request.operations.len() > 1000 {
		return Err(ScimApiError::Scim(ScimError::TooMany));
	}

	let fail_on_errors = request.fail_on_errors.unwrap_or(0);
	let mut responses = Vec::new();
	let mut error_count = 0;
	let mut bulk_id_map: HashMap<String, String> = HashMap::new();

	for op in request.operations {
		if fail_on_errors > 0 && error_count >= fail_on_errors {
			responses.push(BulkOperationResponse {
				method: op.method.clone(),
				bulk_id: op.bulk_id.clone(),
				status: "412".to_string(),
				location: None,
				response: Some(serde_json::json!({
					"detail": "Operation skipped due to previous errors"
				})),
			});
			continue;
		}

		let mut data = op.data.clone();
		if let Some(ref mut d) = data {
			resolve_bulk_id_refs(d, &bulk_id_map);
		}

		let result = process_operation(&state, &op.method, &op.path, data).await;

		match result {
			Ok((status, location, response)) => {
				if let (Some(ref bulk_id), Some(ref loc)) = (&op.bulk_id, &location) {
					if let Some(id) = loc.rsplit('/').next() {
						bulk_id_map.insert(bulk_id.clone(), id.to_string());
					}
				}
				responses.push(BulkOperationResponse {
					method: op.method.clone(),
					bulk_id: op.bulk_id.clone(),
					status,
					location,
					response,
				});
			}
			Err((status, detail)) => {
				error_count += 1;
				responses.push(BulkOperationResponse {
					method: op.method.clone(),
					bulk_id: op.bulk_id.clone(),
					status,
					location: None,
					response: Some(serde_json::json!({ "detail": detail })),
				});
			}
		}
	}

	let success_count = responses
		.iter()
		.filter(|r| r.status.starts_with("2"))
		.count();

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimBulkOperation)
			.action("SCIM bulk operation completed")
			.details(json!({
				"total_operations": responses.len(),
				"success_count": success_count,
				"error_count": error_count,
				"org_id": state.org_id.to_string(),
			}))
			.build(),
	);

	Ok(Json(BulkResponse {
		schemas: vec!["urn:ietf:params:scim:api:messages:2.0:BulkResponse".to_string()],
		operations: responses,
	}))
}

async fn process_operation(
	state: &ScimState,
	method: &str,
	path: &str,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let resource_path =
		parse_path(path).ok_or_else(|| ("400".to_string(), "Invalid path".to_string()))?;

	match (method.to_uppercase().as_str(), resource_path) {
		("POST", ResourcePath::Users) => create_user(state, data).await,
		("PUT", ResourcePath::UsersId(id)) => replace_user(state, &id, data).await,
		("PATCH", ResourcePath::UsersId(id)) => patch_user(state, &id, data).await,
		("DELETE", ResourcePath::UsersId(id)) => delete_user(state, &id).await,
		("POST", ResourcePath::Groups) => create_group(state, data).await,
		("PUT", ResourcePath::GroupsId(id)) => replace_group(state, &id, data).await,
		("PATCH", ResourcePath::GroupsId(id)) => patch_group(state, &id, data).await,
		("DELETE", ResourcePath::GroupsId(id)) => delete_group(state, &id).await,
		_ => Err((
			"400".to_string(),
			format!("Unsupported operation: {} {}", method, path),
		)),
	}
}

async fn create_user(
	state: &ScimState,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let data = data.ok_or_else(|| ("400".to_string(), "Missing data".to_string()))?;
	let scim_user: ScimUser = serde_json::from_value(data)
		.map_err(|e| ("400".to_string(), format!("Invalid user data: {}", e)))?;

	let email = scim_user_to_email(&scim_user)
		.ok_or_else(|| ("400".to_string(), "userName or email required".to_string()))?;
	let display_name = scim_user_to_display_name(&scim_user).unwrap_or_else(|| email.clone());

	let request = loom_server_provisioning::ProvisioningRequest::scim(
		&email,
		&display_name,
		scim_user.external_id.clone(),
		scim_user.locale.clone(),
		state.org_id,
	);

	let user = state
		.provisioning
		.provision_user(request)
		.await
		.map_err(|e| ("500".to_string(), format!("Provisioning failed: {}", e)))?;

	let location = format!("/api/scim/Users/{}", user.id);
	Ok((
		"201".to_string(),
		Some(location),
		Some(serde_json::json!({ "id": user.id.to_string() })),
	))
}

async fn replace_user(
	state: &ScimState,
	id: &str,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let data = data.ok_or_else(|| ("400".to_string(), "Missing data".to_string()))?;
	let scim_user: ScimUser = serde_json::from_value(data)
		.map_err(|e| ("400".to_string(), format!("Invalid user data: {}", e)))?;

	let user_id = parse_uuid(id)?;
	let user_id = UserId::new(user_id);

	let display_name = scim_user_to_display_name(&scim_user);
	let external_id = scim_user.external_id.as_deref();
	let locale = scim_user.locale.as_deref();
	let deleted_at: Option<String> = if scim_user.active {
		None
	} else {
		Some(chrono::Utc::now().to_rfc3339())
	};

	state
		.user_repo
		.update_user_for_scim(
			&user_id,
			display_name.as_deref(),
			external_id,
			locale,
			deleted_at.as_deref(),
		)
		.await
		.map_err(|e| ("500".to_string(), format!("Update failed: {}", e)))?;

	let location = format!("/api/scim/Users/{}", id);
	Ok(("200".to_string(), Some(location), None))
}

async fn patch_user(
	state: &ScimState,
	id: &str,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let data = data.ok_or_else(|| ("400".to_string(), "Missing data".to_string()))?;
	let patch: PatchRequest = serde_json::from_value(data)
		.map_err(|e| ("400".to_string(), format!("Invalid patch data: {}", e)))?;

	patch
		.validate()
		.map_err(|e| ("400".to_string(), e.to_string()))?;

	let user_id = parse_uuid(id)?;
	let user_id = UserId::new(user_id);

	for op in &patch.operations {
		match op.path.as_deref() {
			Some("active") | None if op.value.as_ref().and_then(|v| v.get("active")).is_some() => {
				let active = op
					.value
					.as_ref()
					.and_then(|v| v.get("active"))
					.and_then(|v| v.as_bool())
					.unwrap_or(true);
				if active {
					state
						.user_repo
						.restore_user(&user_id)
						.await
						.map_err(|e| ("500".to_string(), e.to_string()))?;
				} else {
					state
						.user_repo
						.soft_delete_user(&user_id)
						.await
						.map_err(|e| ("500".to_string(), e.to_string()))?;
				}
			}
			Some("displayName") => {
				if let Some(value) = op.value.as_ref().and_then(|v| v.as_str()) {
					state
						.user_repo
						.update_display_name(&user_id, value)
						.await
						.map_err(|e| ("500".to_string(), e.to_string()))?;
				}
			}
			_ => {}
		}
	}

	let location = format!("/api/scim/Users/{}", id);
	Ok(("200".to_string(), Some(location), None))
}

async fn delete_user(
	state: &ScimState,
	id: &str,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let user_id = parse_uuid(id)?;
	let user_id = UserId::new(user_id);

	state
		.user_repo
		.soft_delete_user(&user_id)
		.await
		.map_err(|e| ("500".to_string(), e.to_string()))?;

	state
		.provisioning
		.deprovision_from_org(&user_id, &state.org_id)
		.await
		.map_err(|e| ("500".to_string(), e.to_string()))?;

	Ok(("204".to_string(), None, None))
}

async fn create_group(
	state: &ScimState,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let data = data.ok_or_else(|| ("400".to_string(), "Missing data".to_string()))?;
	let scim_group: ScimGroup = serde_json::from_value(data)
		.map_err(|e| ("400".to_string(), format!("Invalid group data: {}", e)))?;

	let team_id = state
		.team_repo
		.create_scim_team(
			&state.org_id,
			&scim_group.display_name,
			scim_group.external_id.as_deref(),
		)
		.await
		.map_err(|e| ("500".to_string(), format!("Create failed: {}", e)))?;

	let user_ids: Result<Vec<UserId>, _> = scim_group
		.members
		.iter()
		.map(|m| parse_uuid(&m.value).map(UserId::new))
		.collect();
	let user_ids = user_ids?;

	state
		.team_repo
		.set_team_members(&team_id, &user_ids)
		.await
		.map_err(|e| ("500".to_string(), e.to_string()))?;

	let location = format!("/api/scim/Groups/{}", team_id);
	Ok((
		"201".to_string(),
		Some(location),
		Some(serde_json::json!({ "id": team_id.to_string() })),
	))
}

async fn replace_group(
	state: &ScimState,
	id: &str,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let data = data.ok_or_else(|| ("400".to_string(), "Missing data".to_string()))?;
	let scim_group: ScimGroup = serde_json::from_value(data)
		.map_err(|e| ("400".to_string(), format!("Invalid group data: {}", e)))?;

	let team_id = parse_uuid(id)?;
	let team_id = TeamId::new(team_id);

	state
		.team_repo
		.update_scim_team(
			&team_id,
			&scim_group.display_name,
			scim_group.external_id.as_deref(),
		)
		.await
		.map_err(|e| ("500".to_string(), e.to_string()))?;

	let user_ids: Result<Vec<UserId>, _> = scim_group
		.members
		.iter()
		.map(|m| parse_uuid(&m.value).map(UserId::new))
		.collect();
	let user_ids = user_ids?;

	state
		.team_repo
		.set_team_members(&team_id, &user_ids)
		.await
		.map_err(|e| ("500".to_string(), e.to_string()))?;

	let location = format!("/api/scim/Groups/{}", id);
	Ok(("200".to_string(), Some(location), None))
}

async fn patch_group(
	state: &ScimState,
	id: &str,
	data: Option<serde_json::Value>,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let data = data.ok_or_else(|| ("400".to_string(), "Missing data".to_string()))?;
	let patch: PatchRequest = serde_json::from_value(data)
		.map_err(|e| ("400".to_string(), format!("Invalid patch data: {}", e)))?;

	patch
		.validate()
		.map_err(|e| ("400".to_string(), e.to_string()))?;

	let team_id = parse_uuid(id)?;
	let team_id = TeamId::new(team_id);

	for op in &patch.operations {
		match op.path.as_deref() {
			Some("displayName") => {
				if let Some(value) = op.value.as_ref().and_then(|v| v.as_str()) {
					let team = state
						.team_repo
						.get_team_with_scim_fields(&team_id, &state.org_id)
						.await
						.map_err(|e| ("500".to_string(), e.to_string()))?
						.ok_or_else(|| ("404".to_string(), format!("Group {} not found", id)))?;

					state
						.team_repo
						.update_scim_team(&team_id, value, team.scim_external_id.as_deref())
						.await
						.map_err(|e| ("500".to_string(), e.to_string()))?;
				}
			}
			Some("members") => {
				if let Some(members) = op.value.as_ref().and_then(|v| v.as_array()) {
					for member in members {
						if let Some(user_id_str) = member.get("value").and_then(|v| v.as_str()) {
							let user_id = parse_uuid(user_id_str)?;
							let user_id = UserId::new(user_id);
							match op.op {
								loom_scim::PatchOp::Add => {
									state
										.team_repo
										.add_member(&team_id, &user_id, TeamRole::Member)
										.await
										.map_err(|e| ("500".to_string(), e.to_string()))?;
								}
								loom_scim::PatchOp::Remove => {
									state
										.team_repo
										.remove_member(&team_id, &user_id)
										.await
										.map_err(|e| ("500".to_string(), e.to_string()))?;
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

	let location = format!("/api/scim/Groups/{}", id);
	Ok(("200".to_string(), Some(location), None))
}

async fn delete_group(
	state: &ScimState,
	id: &str,
) -> Result<(String, Option<String>, Option<serde_json::Value>), (String, String)> {
	let team_id = parse_uuid(id)?;
	let team_id = TeamId::new(team_id);

	state
		.team_repo
		.delete_scim_team(&team_id, &state.org_id)
		.await
		.map_err(|e| ("500".to_string(), e.to_string()))?;

	Ok(("204".to_string(), None, None))
}

fn parse_uuid(s: &str) -> Result<Uuid, (String, String)> {
	Uuid::parse_str(s).map_err(|e| ("400".to_string(), format!("Invalid ID: {}", e)))
}
