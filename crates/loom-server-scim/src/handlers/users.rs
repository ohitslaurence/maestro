// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	Json,
};
use chrono::Utc;
use loom_scim::patch::PatchRequest;
use loom_scim::{evaluate_filter, FilterParser, ListResponse, ScimUser};
use loom_server_audit::{AuditEventType, AuditLogEntry, AuditService};
use loom_server_auth::{OrgId, UserId};
use loom_server_db::{ScimUserRow, TeamRepository, UserRepository};
use loom_server_provisioning::UserProvisioningService;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

fn parse_user_id(s: &str) -> Result<UserId, ScimApiError> {
	let uuid =
		Uuid::parse_str(s).map_err(|e| ScimApiError::Internal(format!("Invalid user ID: {}", e)))?;
	Ok(UserId::new(uuid))
}

use crate::error::ScimApiError;
use crate::mapping::{scim_user_to_display_name, scim_user_to_email, LoomUser};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListUsersQuery {
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

#[derive(Clone)]
pub struct ScimState {
	pub org_id: OrgId,
	pub provisioning: Arc<UserProvisioningService>,
	pub user_repo: Arc<UserRepository>,
	pub team_repo: Arc<TeamRepository>,
	pub audit_service: Arc<AuditService>,
}

fn get_user_attr(user: &ScimUser, attr: &str) -> Option<String> {
	match attr.to_lowercase().as_str() {
		"username" => Some(user.user_name.clone()),
		"displayname" => user.display_name.clone(),
		"active" => Some(user.active.to_string()),
		"id" => user.id.clone(),
		"externalid" => user.external_id.clone(),
		"locale" => user.locale.clone(),
		"emails" | "emails.value" => user.emails.first().map(|e| e.value.clone()),
		_ => None,
	}
}

pub async fn list_users(
	State(state): State<ScimState>,
	Query(query): Query<ListUsersQuery>,
) -> Result<Json<ListResponse<ScimUser>>, ScimApiError> {
	let parsed_filter = if let Some(ref filter_str) = query.filter {
		Some(FilterParser::parse(filter_str)?)
	} else {
		None
	};

	let rows = state
		.user_repo
		.list_users_in_org(&state.org_id, 10000, 0)
		.await?;

	let all_users: Vec<ScimUser> = rows.into_iter().map(scim_user_row_to_scim_user).collect();

	let filtered_users: Vec<ScimUser> = if let Some(ref filter) = parsed_filter {
		all_users
			.into_iter()
			.filter(|user| evaluate_filter(filter, &|attr| get_user_attr(user, attr)))
			.collect()
	} else {
		all_users
	};

	let total = filtered_users.len() as i64;
	let count = query.count.min(1000);
	let offset = (query.start_index - 1).max(0) as usize;
	let paginated: Vec<ScimUser> = filtered_users
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

pub async fn create_user(
	State(state): State<ScimState>,
	Json(scim_user): Json<ScimUser>,
) -> Result<(StatusCode, Json<ScimUser>), ScimApiError> {
	let email = scim_user_to_email(&scim_user)
		.ok_or_else(|| ScimApiError::BadRequest("userName or email required".to_string()))?;
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
		.map_err(|e| ScimApiError::Internal(format!("Provisioning failed: {}", e)))?;

	info!(user_id = %user.id, email = %email, "SCIM: provisioned user");

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimUserCreated)
			.resource("user", user.id.to_string())
			.action("SCIM user created via provisioning")
			.details(json!({
				"email": email,
				"display_name": display_name,
				"org_id": state.org_id.to_string(),
			}))
			.build(),
	);

	let row = state
		.user_repo
		.get_user_in_org(&user.id, &state.org_id)
		.await?
		.ok_or_else(|| {
			ScimApiError::Internal(format!("User {} not found after provisioning", user.id))
		})?;

	let scim_user = scim_user_row_to_scim_user(row);
	Ok((StatusCode::CREATED, Json(scim_user)))
}

pub async fn get_user(
	State(state): State<ScimState>,
	Path(id): Path<String>,
) -> Result<Json<ScimUser>, ScimApiError> {
	let user_id = parse_user_id(&id)?;

	let row = state
		.user_repo
		.get_user_in_org(&user_id, &state.org_id)
		.await?
		.ok_or_else(|| ScimApiError::NotFound(format!("User {} not found", id)))?;

	let scim_user = scim_user_row_to_scim_user(row);
	Ok(Json(scim_user))
}

pub async fn replace_user(
	State(state): State<ScimState>,
	Path(id): Path<String>,
	Json(scim_user): Json<ScimUser>,
) -> Result<Json<ScimUser>, ScimApiError> {
	let user_id = parse_user_id(&id)?;
	let display_name = scim_user_to_display_name(&scim_user);
	let external_id = scim_user.external_id.as_deref();
	let locale = scim_user.locale.as_deref();
	let active = scim_user.active;

	let deleted_at: Option<String> = if active {
		None
	} else {
		Some(Utc::now().to_rfc3339())
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
		.await?;

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimUserUpdated)
			.resource("user", user_id.to_string())
			.action("SCIM user replaced")
			.details(json!({
				"display_name": display_name,
				"external_id": external_id,
				"active": active,
			}))
			.build(),
	);

	get_user(State(state), Path(id)).await
}

pub async fn patch_user(
	State(state): State<ScimState>,
	Path(id): Path<String>,
	Json(patch): Json<PatchRequest>,
) -> Result<Json<ScimUser>, ScimApiError> {
	patch.validate()?;
	let user_id = parse_user_id(&id)?;

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
					state.user_repo.restore_user(&user_id).await?;
				} else {
					state.user_repo.soft_delete_user(&user_id).await?;
				}
			}
			Some("displayName") => {
				if let Some(value) = op.value.as_ref().and_then(|v| v.as_str()) {
					state.user_repo.update_display_name(&user_id, value).await?;
				}
			}
			_ => {}
		}
	}

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimUserUpdated)
			.resource("user", user_id.to_string())
			.action("SCIM user patched")
			.details(json!({
				"operations_count": patch.operations.len(),
			}))
			.build(),
	);

	get_user(State(state), Path(id)).await
}

pub async fn delete_user(
	State(state): State<ScimState>,
	Path(id): Path<String>,
) -> Result<StatusCode, ScimApiError> {
	let user_id = parse_user_id(&id)?;

	state.user_repo.soft_delete_user(&user_id).await?;

	state
		.provisioning
		.deprovision_from_org(&user_id, &state.org_id)
		.await
		.map_err(|e| ScimApiError::Internal(e.to_string()))?;

	info!(user_id = %id, "SCIM: deprovisioned user");

	state.audit_service.log(
		AuditLogEntry::builder(AuditEventType::ScimUserDeprovisioned)
			.resource("user", user_id.to_string())
			.action("SCIM user deprovisioned")
			.details(json!({
				"org_id": state.org_id.to_string(),
			}))
			.build(),
	);

	Ok(StatusCode::NO_CONTENT)
}

fn scim_user_row_to_scim_user(row: ScimUserRow) -> ScimUser {
	let active = row.deleted_at.is_none();

	let loom_user = LoomUser {
		id: row.id,
		email: row.primary_email,
		display_name: row.display_name,
		avatar_url: row.avatar_url,
		locale: row.locale,
		scim_external_id: row.scim_external_id,
		active,
		created_at: row.created_at,
		updated_at: row.updated_at,
	};

	loom_user.into()
}
