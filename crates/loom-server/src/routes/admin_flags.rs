// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Admin routes for platform-level feature flags.
//!
//! Provides endpoints for super admin management of platform-wide flags:
//! - List, create, update, archive platform flags
//! - List, create, update, delete platform kill switches
//! - Activate/deactivate platform kill switches
//!
//! Platform flags have `org_id = None` and override org-level flags with the
//! same key during evaluation.
//!
//! # Security
//!
//! All endpoints require `system_admin` role.

use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use chrono::Utc;
use loom_flags_core::{
	Flag, FlagId, FlagPrerequisite, KillSwitch, KillSwitchId, Strategy, UserId, Variant, VariantValue,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_flags::FlagsRepository;
use serde_json::json;

pub use loom_server_api::flags::{
	ActivateKillSwitchRequest, CreateFlagRequest, CreateKillSwitchRequest, CreateStrategyRequest,
	FlagPrerequisiteApi, FlagResponse, FlagsErrorResponse, FlagsSuccessResponse, KillSwitchResponse,
	ListFlagsResponse, ListKillSwitchesResponse, ListStrategiesResponse, StrategyResponse,
	UpdateFlagRequest, UpdateKillSwitchRequest, UpdateStrategyRequest, VariantApi, VariantValueApi,
};
use serde::Deserialize;

/// Query parameters for listing platform flags.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlatformFlagsQuery {
	/// Include archived flags (default: false).
	#[serde(default)]
	pub include_archived: bool,
}

use crate::{
	api::AppState,
	api_response::{bad_request, conflict, internal_error, not_found},
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	routes::admin::AdminErrorResponse,
};

// ============================================================================
// Helper Conversions
// ============================================================================

fn variant_from_api(v: &VariantApi) -> Variant {
	Variant {
		name: v.name.clone(),
		value: match &v.value {
			VariantValueApi::Boolean(b) => VariantValue::Boolean(*b),
			VariantValueApi::String(s) => VariantValue::String(s.clone()),
			VariantValueApi::Json(j) => VariantValue::Json(j.clone()),
		},
		weight: v.weight,
	}
}

fn variant_to_api(v: &Variant) -> VariantApi {
	VariantApi {
		name: v.name.clone(),
		value: match &v.value {
			VariantValue::Boolean(b) => VariantValueApi::Boolean(*b),
			VariantValue::String(s) => VariantValueApi::String(s.clone()),
			VariantValue::Json(j) => VariantValueApi::Json(j.clone()),
		},
		weight: v.weight,
	}
}

fn prerequisite_from_api(p: &FlagPrerequisiteApi) -> FlagPrerequisite {
	FlagPrerequisite {
		flag_key: p.flag_key.clone(),
		required_variant: p.required_variant.clone(),
	}
}

fn prerequisite_to_api(p: &FlagPrerequisite) -> FlagPrerequisiteApi {
	FlagPrerequisiteApi {
		flag_key: p.flag_key.clone(),
		required_variant: p.required_variant.clone(),
	}
}

fn flag_to_response(flag: &Flag) -> FlagResponse {
	FlagResponse {
		id: flag.id.to_string(),
		org_id: flag.org_id.map(|id| id.to_string()),
		key: flag.key.clone(),
		name: flag.name.clone(),
		description: flag.description.clone(),
		tags: flag.tags.clone(),
		maintainer_user_id: flag.maintainer_user_id.map(|id| id.to_string()),
		variants: flag.variants.iter().map(variant_to_api).collect(),
		default_variant: flag.default_variant.clone(),
		prerequisites: flag.prerequisites.iter().map(prerequisite_to_api).collect(),
		is_archived: flag.is_archived(),
		created_at: flag.created_at,
		updated_at: flag.updated_at,
		archived_at: flag.archived_at,
	}
}

fn kill_switch_to_response(ks: &KillSwitch) -> KillSwitchResponse {
	KillSwitchResponse {
		id: ks.id.to_string(),
		org_id: ks.org_id.map(|id| id.to_string()),
		key: ks.key.clone(),
		name: ks.name.clone(),
		description: ks.description.clone(),
		linked_flag_keys: ks.linked_flag_keys.clone(),
		is_active: ks.is_active,
		activated_at: ks.activated_at,
		activated_by: ks.activated_by.map(|id| id.to_string()),
		activation_reason: ks.activation_reason.clone(),
		created_at: ks.created_at,
		updated_at: ks.updated_at,
	}
}

// ============================================================================
// Platform Flag Routes
// ============================================================================

/// List all platform-level flags.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	get,
	path = "/api/admin/flags",
	params(
		("include_archived" = bool, Query, description = "Include archived flags")
	),
	responses(
		(status = 200, description = "List of platform flags", body = ListFlagsResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn list_platform_flags(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Query(query): Query<PlatformFlagsQuery>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform flags list attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let flags = match state
		.flags_repo
		.list_flags(None, query.include_archived)
		.await
	{
		Ok(flags) => flags,
		Err(e) => {
			tracing::error!(error = %e, error_debug = ?e, "Failed to list platform flags");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	tracing::info!(
		actor_id = %current_user.user.id,
		flag_count = flags.len(),
		"Listed platform flags"
	);

	let flags_response: Vec<FlagResponse> = flags.iter().map(flag_to_response).collect();

	(
		StatusCode::OK,
		Json(ListFlagsResponse {
			flags: flags_response,
		}),
	)
		.into_response()
}

/// Create a new platform-level flag.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	post,
	path = "/api/admin/flags",
	request_body = CreateFlagRequest,
	responses(
		(status = 201, description = "Platform flag created", body = FlagResponse),
		(status = 400, description = "Invalid request", body = FlagsErrorResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 409, description = "Flag key already exists", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state, payload), fields(actor_id = %current_user.user.id, flag_key = %payload.key))]
pub async fn create_platform_flag(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(payload): Json<CreateFlagRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform flag creation attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	// Validate flag key
	if !Flag::validate_key(&payload.key) {
		return bad_request::<FlagsErrorResponse>(
			"invalid_key",
			t(locale, "server.api.flags.invalid_flag_key"),
		)
		.into_response();
	}

	// Check for duplicate key
	if let Ok(Some(_)) = state.flags_repo.get_flag_by_key(None, &payload.key).await {
		return conflict::<FlagsErrorResponse>(
			"duplicate_key",
			t(locale, "server.api.flags.duplicate_flag_key"),
		)
		.into_response();
	}

	// Validate variants
	if payload.variants.is_empty() {
		return bad_request::<FlagsErrorResponse>(
			"no_variants",
			t(locale, "server.api.flags.variant_not_found"),
		)
		.into_response();
	}

	// Validate default variant exists
	if !payload
		.variants
		.iter()
		.any(|v| v.name == payload.default_variant)
	{
		return bad_request::<FlagsErrorResponse>(
			"invalid_default",
			t(locale, "server.api.flags.default_variant_missing"),
		)
		.into_response();
	}

	let now = Utc::now();
	let flag = Flag {
		id: FlagId::new(),
		org_id: None, // Platform flag
		key: payload.key.clone(),
		name: payload.name.clone(),
		description: payload.description.clone(),
		tags: payload.tags.clone(),
		maintainer_user_id: payload
			.maintainer_user_id
			.as_ref()
			.and_then(|id| id.parse().ok().map(UserId)),
		variants: payload.variants.iter().map(variant_from_api).collect(),
		default_variant: payload.default_variant.clone(),
		prerequisites: payload
			.prerequisites
			.iter()
			.map(prerequisite_from_api)
			.collect(),
		exposure_tracking_enabled: false,
		created_at: now,
		updated_at: now,
		archived_at: None,
	};

	if let Err(e) = state.flags_repo.create_flag(&flag).await {
		tracing::error!(error = %e, flag_key = %flag.key, "Failed to create platform flag");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::FlagCreated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_flag", flag.id.to_string())
			.details(json!({
				"key": flag.key,
				"name": flag.name,
				"is_platform_flag": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		flag_id = %flag.id,
		flag_key = %flag.key,
		"Created platform flag"
	);

	(StatusCode::CREATED, Json(flag_to_response(&flag))).into_response()
}

/// Get a platform-level flag by key.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	get,
	path = "/api/admin/flags/{key}",
	params(
		("key" = String, Path, description = "Flag key")
	),
	responses(
		(status = 200, description = "Platform flag", body = FlagResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Flag not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, flag_key = %key))]
pub async fn get_platform_flag(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform flag access attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let flag = match state.flags_repo.get_flag_by_key(None, &key).await {
		Ok(Some(flag)) => flag,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.flag_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, flag_key = %key, "Failed to get platform flag");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	(StatusCode::OK, Json(flag_to_response(&flag))).into_response()
}

/// Update a platform-level flag.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	patch,
	path = "/api/admin/flags/{key}",
	params(
		("key" = String, Path, description = "Flag key")
	),
	request_body = UpdateFlagRequest,
	responses(
		(status = 200, description = "Platform flag updated", body = FlagResponse),
		(status = 400, description = "Invalid request", body = FlagsErrorResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Flag not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state, payload), fields(actor_id = %current_user.user.id, flag_key = %key))]
pub async fn update_platform_flag(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
	Json(payload): Json<UpdateFlagRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform flag update attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let mut flag = match state.flags_repo.get_flag_by_key(None, &key).await {
		Ok(Some(flag)) => flag,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.flag_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, flag_key = %key, "Failed to get platform flag");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	// Update fields
	if let Some(name) = payload.name {
		flag.name = name;
	}
	if let Some(description) = payload.description {
		flag.description = Some(description);
	}
	if let Some(tags) = payload.tags {
		flag.tags = tags;
	}
	if let Some(maintainer_user_id) = payload.maintainer_user_id {
		flag.maintainer_user_id = maintainer_user_id.parse().ok().map(UserId);
	}
	if let Some(variants) = payload.variants {
		flag.variants = variants.iter().map(variant_from_api).collect();
	}
	if let Some(default_variant) = payload.default_variant {
		// Validate default variant exists
		if !flag.variants.iter().any(|v| v.name == default_variant) {
			return bad_request::<FlagsErrorResponse>(
				"invalid_default",
				t(locale, "server.api.flags.default_variant_missing"),
			)
			.into_response();
		}
		flag.default_variant = default_variant;
	}
	if let Some(prerequisites) = payload.prerequisites {
		flag.prerequisites = prerequisites.iter().map(prerequisite_from_api).collect();
	}

	flag.updated_at = Utc::now();

	if let Err(e) = state.flags_repo.update_flag(&flag).await {
		tracing::error!(error = %e, flag_key = %key, "Failed to update platform flag");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::FlagUpdated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_flag", flag.id.to_string())
			.details(json!({
				"key": flag.key,
				"is_platform_flag": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		flag_id = %flag.id,
		flag_key = %flag.key,
		"Updated platform flag"
	);

	(StatusCode::OK, Json(flag_to_response(&flag))).into_response()
}

/// Archive a platform-level flag.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	delete,
	path = "/api/admin/flags/{key}",
	params(
		("key" = String, Path, description = "Flag key")
	),
	responses(
		(status = 200, description = "Platform flag archived", body = FlagsSuccessResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Flag not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, flag_key = %key))]
pub async fn archive_platform_flag(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform flag archive attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let flag = match state.flags_repo.get_flag_by_key(None, &key).await {
		Ok(Some(flag)) => flag,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.flag_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, flag_key = %key, "Failed to get platform flag");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	if let Err(e) = state.flags_repo.archive_flag(flag.id).await {
		tracing::error!(error = %e, flag_key = %key, "Failed to archive platform flag");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::FlagArchived)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_flag", flag.id.to_string())
			.details(json!({
				"key": flag.key,
				"is_platform_flag": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		flag_id = %flag.id,
		flag_key = %flag.key,
		"Archived platform flag"
	);

	(
		StatusCode::OK,
		Json(FlagsSuccessResponse {
			message: t(locale, "server.api.flags.flag_archived").to_string(),
		}),
	)
		.into_response()
}

/// Restore an archived platform-level flag.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	post,
	path = "/api/admin/flags/{key}/restore",
	params(
		("key" = String, Path, description = "Flag key")
	),
	responses(
		(status = 200, description = "Platform flag restored", body = FlagsSuccessResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Flag not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, flag_key = %key))]
pub async fn restore_platform_flag(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform flag restore attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let flag = match state.flags_repo.get_flag_by_key(None, &key).await {
		Ok(Some(flag)) => flag,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.flag_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, flag_key = %key, "Failed to get platform flag");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	if let Err(e) = state.flags_repo.restore_flag(flag.id).await {
		tracing::error!(error = %e, flag_key = %key, "Failed to restore platform flag");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::FlagRestored)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_flag", flag.id.to_string())
			.details(json!({
				"key": flag.key,
				"is_platform_flag": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		flag_id = %flag.id,
		flag_key = %flag.key,
		"Restored platform flag"
	);

	(
		StatusCode::OK,
		Json(FlagsSuccessResponse {
			message: t(locale, "server.api.flags.flag_restored").to_string(),
		}),
	)
		.into_response()
}

// ============================================================================
// Platform Kill Switch Routes
// ============================================================================

/// List all platform-level kill switches.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	get,
	path = "/api/admin/flags/kill-switches",
	responses(
		(status = 200, description = "List of platform kill switches", body = ListKillSwitchesResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn list_platform_kill_switches(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switches list attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let kill_switches = match state.flags_repo.list_kill_switches(None).await {
		Ok(ks) => ks,
		Err(e) => {
			tracing::error!(error = %e, "Failed to list platform kill switches");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	tracing::info!(
		actor_id = %current_user.user.id,
		count = kill_switches.len(),
		"Listed platform kill switches"
	);

	let ks_response: Vec<KillSwitchResponse> =
		kill_switches.iter().map(kill_switch_to_response).collect();

	(
		StatusCode::OK,
		Json(ListKillSwitchesResponse {
			kill_switches: ks_response,
		}),
	)
		.into_response()
}

/// Create a new platform-level kill switch.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	post,
	path = "/api/admin/flags/kill-switches",
	request_body = CreateKillSwitchRequest,
	responses(
		(status = 201, description = "Platform kill switch created", body = KillSwitchResponse),
		(status = 400, description = "Invalid request", body = FlagsErrorResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 409, description = "Kill switch key already exists", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state, payload), fields(actor_id = %current_user.user.id, ks_key = %payload.key))]
pub async fn create_platform_kill_switch(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(payload): Json<CreateKillSwitchRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switch creation attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	// Validate kill switch key
	if !KillSwitch::validate_key(&payload.key) {
		return bad_request::<FlagsErrorResponse>(
			"invalid_key",
			t(locale, "server.api.flags.invalid_kill_switch_key"),
		)
		.into_response();
	}

	// Check for duplicate key
	if let Ok(Some(_)) = state
		.flags_repo
		.get_kill_switch_by_key(None, &payload.key)
		.await
	{
		return conflict::<FlagsErrorResponse>(
			"duplicate_key",
			t(locale, "server.api.flags.duplicate_kill_switch_key"),
		)
		.into_response();
	}

	let now = Utc::now();
	let kill_switch = KillSwitch {
		id: KillSwitchId::new(),
		org_id: None, // Platform kill switch
		key: payload.key.clone(),
		name: payload.name.clone(),
		description: payload.description.clone(),
		linked_flag_keys: payload.linked_flag_keys.clone(),
		is_active: false,
		activated_at: None,
		activated_by: None,
		activation_reason: None,
		created_at: now,
		updated_at: now,
	};

	if let Err(e) = state.flags_repo.create_kill_switch(&kill_switch).await {
		tracing::error!(error = %e, ks_key = %kill_switch.key, "Failed to create platform kill switch");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::KillSwitchCreated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_kill_switch", kill_switch.id.to_string())
			.details(json!({
				"key": kill_switch.key,
				"name": kill_switch.name,
				"linked_flag_keys": kill_switch.linked_flag_keys,
				"is_platform_kill_switch": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		ks_id = %kill_switch.id,
		ks_key = %kill_switch.key,
		"Created platform kill switch"
	);

	(
		StatusCode::CREATED,
		Json(kill_switch_to_response(&kill_switch)),
	)
		.into_response()
}

/// Get a platform-level kill switch by key.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	get,
	path = "/api/admin/flags/kill-switches/{key}",
	params(
		("key" = String, Path, description = "Kill switch key")
	),
	responses(
		(status = 200, description = "Platform kill switch", body = KillSwitchResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Kill switch not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, ks_key = %key))]
pub async fn get_platform_kill_switch(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switch access attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let kill_switch = match state.flags_repo.get_kill_switch_by_key(None, &key).await {
		Ok(Some(ks)) => ks,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.kill_switch_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, ks_key = %key, "Failed to get platform kill switch");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	(StatusCode::OK, Json(kill_switch_to_response(&kill_switch))).into_response()
}

/// Update a platform-level kill switch.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	patch,
	path = "/api/admin/flags/kill-switches/{key}",
	params(
		("key" = String, Path, description = "Kill switch key")
	),
	request_body = UpdateKillSwitchRequest,
	responses(
		(status = 200, description = "Platform kill switch updated", body = KillSwitchResponse),
		(status = 400, description = "Invalid request", body = FlagsErrorResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Kill switch not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state, payload), fields(actor_id = %current_user.user.id, ks_key = %key))]
pub async fn update_platform_kill_switch(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
	Json(payload): Json<UpdateKillSwitchRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switch update attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let mut kill_switch = match state.flags_repo.get_kill_switch_by_key(None, &key).await {
		Ok(Some(ks)) => ks,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.kill_switch_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, ks_key = %key, "Failed to get platform kill switch");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	// Update fields
	if let Some(name) = payload.name {
		kill_switch.name = name;
	}
	if let Some(description) = payload.description {
		kill_switch.description = Some(description);
	}
	if let Some(linked_flag_keys) = payload.linked_flag_keys {
		kill_switch.linked_flag_keys = linked_flag_keys;
	}

	kill_switch.updated_at = Utc::now();

	if let Err(e) = state.flags_repo.update_kill_switch(&kill_switch).await {
		tracing::error!(error = %e, ks_key = %key, "Failed to update platform kill switch");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::KillSwitchUpdated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_kill_switch", kill_switch.id.to_string())
			.details(json!({
				"key": kill_switch.key,
				"is_platform_kill_switch": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		ks_id = %kill_switch.id,
		ks_key = %kill_switch.key,
		"Updated platform kill switch"
	);

	(StatusCode::OK, Json(kill_switch_to_response(&kill_switch))).into_response()
}

/// Delete a platform-level kill switch.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	delete,
	path = "/api/admin/flags/kill-switches/{key}",
	params(
		("key" = String, Path, description = "Kill switch key")
	),
	responses(
		(status = 200, description = "Platform kill switch deleted", body = FlagsSuccessResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Kill switch not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, ks_key = %key))]
pub async fn delete_platform_kill_switch(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switch delete attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let kill_switch = match state.flags_repo.get_kill_switch_by_key(None, &key).await {
		Ok(Some(ks)) => ks,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.kill_switch_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, ks_key = %key, "Failed to get platform kill switch");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	if let Err(e) = state.flags_repo.delete_kill_switch(kill_switch.id).await {
		tracing::error!(error = %e, ks_key = %key, "Failed to delete platform kill switch");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::KillSwitchDeleted)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_kill_switch", kill_switch.id.to_string())
			.details(json!({
				"key": kill_switch.key,
				"is_platform_kill_switch": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		ks_id = %kill_switch.id,
		ks_key = %kill_switch.key,
		"Deleted platform kill switch"
	);

	(
		StatusCode::OK,
		Json(FlagsSuccessResponse {
			message: t(locale, "server.api.flags.kill_switch_deleted").to_string(),
		}),
	)
		.into_response()
}

/// Activate a platform-level kill switch.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	post,
	path = "/api/admin/flags/kill-switches/{key}/activate",
	params(
		("key" = String, Path, description = "Kill switch key")
	),
	request_body = ActivateKillSwitchRequest,
	responses(
		(status = 200, description = "Platform kill switch activated", body = KillSwitchResponse),
		(status = 400, description = "Activation reason required", body = FlagsErrorResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Kill switch not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state, payload), fields(actor_id = %current_user.user.id, ks_key = %key))]
pub async fn activate_platform_kill_switch(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
	Json(payload): Json<ActivateKillSwitchRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switch activation attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	// Validate reason is provided
	if payload.reason.trim().is_empty() {
		return bad_request::<FlagsErrorResponse>(
			"reason_required",
			t(locale, "server.api.flags.activation_reason_required"),
		)
		.into_response();
	}

	let mut kill_switch = match state.flags_repo.get_kill_switch_by_key(None, &key).await {
		Ok(Some(ks)) => ks,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.kill_switch_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, ks_key = %key, "Failed to get platform kill switch");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	let now = Utc::now();
	kill_switch.is_active = true;
	kill_switch.activated_at = Some(now);
	kill_switch.activated_by = Some(UserId(current_user.user.id.into_inner()));
	kill_switch.activation_reason = Some(payload.reason.clone());
	kill_switch.updated_at = now;

	if let Err(e) = state.flags_repo.update_kill_switch(&kill_switch).await {
		tracing::error!(error = %e, ks_key = %key, "Failed to activate platform kill switch");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	// Broadcast SSE event for platform kill switch activation to all connected clients
	let event = loom_flags_core::FlagStreamEvent::kill_switch_activated(
		kill_switch.key.clone(),
		kill_switch.linked_flag_keys.clone(),
		payload.reason.clone(),
	);
	state.flags_broadcaster.broadcast_to_all(event).await;

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::KillSwitchActivated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_kill_switch", kill_switch.id.to_string())
			.details(json!({
				"key": kill_switch.key,
				"reason": payload.reason,
				"linked_flag_keys": kill_switch.linked_flag_keys,
				"is_platform_kill_switch": true,
			}))
			.build(),
	);

	tracing::warn!(
		actor_id = %current_user.user.id,
		ks_id = %kill_switch.id,
		ks_key = %kill_switch.key,
		reason = %payload.reason,
		linked_flags = ?kill_switch.linked_flag_keys,
		"ACTIVATED platform kill switch"
	);

	(StatusCode::OK, Json(kill_switch_to_response(&kill_switch))).into_response()
}

/// Deactivate a platform-level kill switch.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	post,
	path = "/api/admin/flags/kill-switches/{key}/deactivate",
	params(
		("key" = String, Path, description = "Kill switch key")
	),
	responses(
		(status = 200, description = "Platform kill switch deactivated", body = KillSwitchResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Kill switch not found", body = FlagsErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, ks_key = %key))]
pub async fn deactivate_platform_kill_switch(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(key): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform kill switch deactivation attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let mut kill_switch = match state.flags_repo.get_kill_switch_by_key(None, &key).await {
		Ok(Some(ks)) => ks,
		Ok(None) => {
			return not_found::<FlagsErrorResponse>(t(locale, "server.api.flags.kill_switch_not_found"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, ks_key = %key, "Failed to get platform kill switch");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	kill_switch.is_active = false;
	kill_switch.activated_at = None;
	kill_switch.activated_by = None;
	kill_switch.activation_reason = None;
	kill_switch.updated_at = Utc::now();

	if let Err(e) = state.flags_repo.update_kill_switch(&kill_switch).await {
		tracing::error!(error = %e, ks_key = %key, "Failed to deactivate platform kill switch");
		return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
			.into_response();
	}

	// Broadcast SSE event for platform kill switch deactivation to all connected clients
	let event = loom_flags_core::FlagStreamEvent::kill_switch_deactivated(
		kill_switch.key.clone(),
		kill_switch.linked_flag_keys.clone(),
	);
	state.flags_broadcaster.broadcast_to_all(event).await;

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::KillSwitchDeactivated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("platform_kill_switch", kill_switch.id.to_string())
			.details(json!({
				"key": kill_switch.key,
				"is_platform_kill_switch": true,
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		ks_id = %kill_switch.id,
		ks_key = %kill_switch.key,
		"Deactivated platform kill switch"
	);

	(StatusCode::OK, Json(kill_switch_to_response(&kill_switch))).into_response()
}

// ============================================================================
// Platform Strategy Routes
// ============================================================================

/// List all platform-level strategies.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	get,
	path = "/api/admin/flags/strategies",
	responses(
		(status = 200, description = "List of platform strategies", body = ListStrategiesResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse)
	),
	tag = "admin-flags"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn list_platform_strategies(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized platform strategies list attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let strategies = match state.flags_repo.list_strategies(None).await {
		Ok(s) => s,
		Err(e) => {
			tracing::error!(error = %e, "Failed to list platform strategies");
			return internal_error::<FlagsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	tracing::info!(
		actor_id = %current_user.user.id,
		count = strategies.len(),
		"Listed platform strategies"
	);

	let strategies_response: Vec<StrategyResponse> =
		strategies.iter().map(strategy_to_response).collect();

	(
		StatusCode::OK,
		Json(ListStrategiesResponse {
			strategies: strategies_response,
		}),
	)
		.into_response()
}

fn strategy_to_response(s: &Strategy) -> StrategyResponse {
	use super::flags::{condition_to_api, percentage_key_to_api, schedule_to_api};
	StrategyResponse {
		id: s.id.to_string(),
		org_id: s.org_id.map(|id| id.to_string()),
		name: s.name.clone(),
		description: s.description.clone(),
		conditions: s.conditions.iter().map(condition_to_api).collect(),
		percentage: s.percentage,
		percentage_key: percentage_key_to_api(&s.percentage_key),
		schedule: s.schedule.as_ref().map(schedule_to_api),
		created_at: s.created_at,
		updated_at: s.updated_at,
	}
}
