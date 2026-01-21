// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Admin HTTP handlers.
//!
//! Implements admin endpoints per the auth-abac-system.md specification section 26:
//! - List all users (system_admin only)
//! - Update global roles
//! - User impersonation
//! - Audit log queries
//!
//! # Security
//!
//! All endpoints in this module require system administrator privileges unless
//! otherwise noted. These are highly sensitive operations that should be:
//! - Monitored via audit logs
//! - Rate-limited in production
//! - Accessed only from trusted networks
//!
//! # Authorization Matrix
//!
//! | Endpoint                | Required Role                |
//! |------------------------|------------------------------|
//! | `list_users`           | `system_admin`              |
//! | `update_user_roles`    | `system_admin`              |
//! | `start_impersonation`  | `system_admin`              |
//! | `stop_impersonation`   | authenticated (any)         |
//! | `list_audit_logs`      | `system_admin` or `auditor` |

use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, AuditSeverity, UserId as AuditUserId};
use loom_server_auth::UserId;
use serde_json::json;
use uuid::Uuid;

pub use loom_server_api::admin::{
	AdminErrorResponse, AdminSuccessResponse, AdminUserResponse, AuditLogEntryResponse,
	DeleteUserResponse, ImpersonateRequest, ImpersonateResponse, ImpersonationState,
	ImpersonationUserInfo, ListAuditLogsParams, ListAuditLogsResponse, ListUsersParams,
	ListUsersResponse, UpdateRolesRequest,
};

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
};

fn parse_user_id(id: &str, locale: &str) -> Result<UserId, AdminErrorResponse> {
	Uuid::parse_str(id)
		.map(UserId::new)
		.map_err(|_| AdminErrorResponse {
			error: "bad_request".to_string(),
			message: t(locale, "server.api.user.invalid_id").to_string(),
		})
}

/// List all users in the system (paginated).
///
/// # Authorization
///
/// Requires `system_admin` role. Returns 403 Forbidden otherwise.
///
/// # Request
///
/// Query parameters:
/// - `limit` (optional): Maximum users to return (default: 50)
/// - `offset` (optional): Pagination offset (default: 0)
/// - `search` (optional): Filter by display name or email
///
/// # Response
///
/// Returns [`ListUsersResponse`] with paginated user list.
///
/// # Errors
///
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `system_admin` role
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - User emails are PII and should be handled accordingly
/// - Results are paginated to prevent large data exfiltration
#[utoipa::path(
    get,
    path = "/api/admin/users",
    params(ListUsersParams),
    responses(
        (status = 200, description = "List of users", body = ListUsersResponse),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 403, description = "Not authorized (system_admin required)", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		search = ?params.search,
		limit = params.limit,
		offset = params.offset
	)
)]
pub async fn list_users(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Query(params): Query<ListUsersParams>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized admin user list attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let search = params.search.as_deref();
	let (users, total) = match state
		.user_repo
		.list_users(params.limit, params.offset, search)
		.await
	{
		Ok(result) => result,
		Err(e) => {
			tracing::error!(error = %e, actor_id = %current_user.user.id, "Failed to list users");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.admin.user_list_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	let users: Vec<AdminUserResponse> = users
		.into_iter()
		.map(|u| AdminUserResponse {
			id: u.id.to_string(),
			display_name: u.display_name,
			primary_email: u.primary_email,
			avatar_url: u.avatar_url,
			is_system_admin: u.is_system_admin,
			is_support: u.is_support,
			is_auditor: u.is_auditor,
			created_at: u.created_at,
			updated_at: u.updated_at,
			deleted_at: u.deleted_at,
		})
		.collect();

	tracing::info!(
		actor_id = %current_user.user.id,
		user_count = users.len(),
		total = total,
		"Admin listed users"
	);

	(
		StatusCode::OK,
		Json(ListUsersResponse {
			users,
			total,
			limit: params.limit,
			offset: params.offset,
		}),
	)
		.into_response()
}

/// Update a user's global roles.
///
/// # Authorization
///
/// Requires `system_admin` role. Returns 403 Forbidden otherwise.
///
/// # Request
///
/// Path parameters:
/// - `id`: Target user's UUID
///
/// Body ([`UpdateRolesRequest`]):
/// - `is_system_admin` (optional): Grant/revoke system admin
/// - `is_support` (optional): Grant/revoke support role
/// - `is_auditor` (optional): Grant/revoke auditor role
///
/// # Response
///
/// Returns updated [`AdminUserResponse`].
///
/// # Errors
///
/// - `400 Bad Request`: Invalid user ID or attempting to remove own admin status
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `system_admin` role
/// - `404 Not Found`: Target user does not exist
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Admins cannot remove their own admin status (prevents lockout)
/// - All role changes are logged to audit trail
#[utoipa::path(
    patch,
    path = "/api/admin/users/{id}/roles",
    params(
        ("id" = String, Path, description = "User ID")
    ),
    request_body = UpdateRolesRequest,
    responses(
        (status = 200, description = "Roles updated", body = AdminUserResponse),
        (status = 400, description = "Invalid request", body = AdminErrorResponse),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 403, description = "Not authorized (system_admin required)", body = AdminErrorResponse),
        (status = 404, description = "User not found", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state, payload),
	fields(
		actor_id = %current_user.user.id,
		target_id = %user_id
	)
)]
pub async fn update_user_roles(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(user_id): Path<String>,
	Json(payload): Json<UpdateRolesRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(
			actor_id = %current_user.user.id,
			target_id = %user_id,
			"Unauthorized role update attempt"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let target_user_id = match parse_user_id(&user_id, locale) {
		Ok(id) => id,
		Err(e) => return (StatusCode::BAD_REQUEST, Json(e)).into_response(),
	};

	if current_user.user.id == target_user_id && payload.is_system_admin == Some(false) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			"Admin attempted to remove own admin status"
		);
		return (
			StatusCode::BAD_REQUEST,
			Json(AdminErrorResponse {
				error: "bad_request".to_string(),
				message: t(locale, "server.api.admin.cannot_remove_own_admin").to_string(),
			}),
		)
			.into_response();
	}

	let mut target_user = match state.user_repo.get_user_by_id(&target_user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(AdminErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.user.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				target_id = %target_user_id,
				"Failed to get user"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	// Prevent removing the last system admin
	if payload.is_system_admin == Some(false) && target_user.is_system_admin {
		let admin_count = match state.user_repo.count_system_admins().await {
			Ok(count) => count,
			Err(e) => {
				tracing::error!(error = %e, "Failed to count system admins");
				return (
					StatusCode::INTERNAL_SERVER_ERROR,
					Json(AdminErrorResponse {
						error: "internal_error".to_string(),
						message: t(locale, "server.api.error.internal").to_string(),
					}),
				)
					.into_response();
			}
		};

		if admin_count <= 1 {
			tracing::warn!(
				actor_id = %current_user.user.id,
				target_id = %target_user_id,
				"Attempted to remove the last system admin"
			);
			return (
				StatusCode::BAD_REQUEST,
				Json(AdminErrorResponse {
					error: "bad_request".to_string(),
					message: t(locale, "server.api.admin.cannot_remove_last_admin").to_string(),
				}),
			)
				.into_response();
		}
	}

	let old_roles = json!({
		"is_system_admin": target_user.is_system_admin,
		"is_support": target_user.is_support,
		"is_auditor": target_user.is_auditor,
	});

	if let Some(is_admin) = payload.is_system_admin {
		target_user.is_system_admin = is_admin;
	}
	if let Some(is_support) = payload.is_support {
		target_user.is_support = is_support;
	}
	if let Some(is_auditor) = payload.is_auditor {
		target_user.is_auditor = is_auditor;
	}

	if let Err(e) = state.user_repo.update_user(&target_user).await {
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			target_id = %target_user_id,
			"Failed to update user roles"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AdminErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.admin.user_update_failed").to_string(),
			}),
		)
			.into_response();
	}

	let new_roles = json!({
		"is_system_admin": target_user.is_system_admin,
		"is_support": target_user.is_support,
		"is_auditor": target_user.is_auditor,
	});

	tracing::info!(
		actor_id = %current_user.user.id,
		target_id = %target_user_id,
		old_roles = ?old_roles,
		new_roles = ?new_roles,
		"Admin updated user roles"
	);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::GlobalRoleChanged)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.severity(AuditSeverity::Warning)
			.resource("user", target_user.id.to_string())
			.details(json!({
				"old_roles": old_roles,
				"new_roles": new_roles,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(AdminUserResponse {
			id: target_user.id.to_string(),
			display_name: target_user.display_name,
			primary_email: target_user.primary_email,
			avatar_url: target_user.avatar_url,
			is_system_admin: target_user.is_system_admin,
			is_support: target_user.is_support,
			is_auditor: target_user.is_auditor,
			created_at: target_user.created_at,
			updated_at: target_user.updated_at,
			deleted_at: target_user.deleted_at,
		}),
	)
		.into_response()
}

/// Delete a user account (soft-delete).
///
/// # Authorization
///
/// Requires `system_admin` role. Returns 403 Forbidden otherwise.
///
/// # Request
///
/// Path parameters:
/// - `id`: Target user's UUID
///
/// # Response
///
/// Returns [`DeleteUserResponse`] confirming deletion.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid user ID or attempting to delete own account
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `system_admin` role
/// - `404 Not Found`: Target user does not exist
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Admins cannot delete their own account
/// - All deletions are logged to audit trail
/// - Uses soft-delete (user can be restored within grace period)
#[utoipa::path(
    delete,
    path = "/api/admin/users/{id}",
    params(
        ("id" = String, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User deleted", body = DeleteUserResponse),
        (status = 400, description = "Invalid request", body = AdminErrorResponse),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 403, description = "Not authorized (system_admin required)", body = AdminErrorResponse),
        (status = 404, description = "User not found", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		target_id = %user_id
	)
)]
pub async fn delete_user(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(user_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	// Check system admin
	if !current_user.user.is_system_admin {
		tracing::warn!(
			actor_id = %current_user.user.id,
			target_id = %user_id,
			"Unauthorized user delete attempt"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	// Parse user ID
	let target_user_id = match parse_user_id(&user_id, locale) {
		Ok(id) => id,
		Err(e) => return (StatusCode::BAD_REQUEST, Json(e)).into_response(),
	};

	// Cannot delete self
	if current_user.user.id == target_user_id {
		return (
			StatusCode::BAD_REQUEST,
			Json(AdminErrorResponse {
				error: "bad_request".to_string(),
				message: t(locale, "server.api.admin.cannot_delete_self").to_string(),
			}),
		)
			.into_response();
	}

	// Check user exists
	let target_user = match state.user_repo.get_user_by_id(&target_user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(AdminErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.admin.user_not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, target_id = %user_id, "Failed to get user");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	// Soft-delete the user
	if let Err(e) = state.user_repo.soft_delete_user(&target_user_id).await {
		tracing::error!(error = %e, target_id = %user_id, "Failed to delete user");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AdminErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		target_id = %user_id,
		target_email = ?target_user.primary_email,
		"Admin deleted user"
	);

	// Audit log
	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::UserDeleted)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.severity(AuditSeverity::Critical)
			.resource("user", user_id.clone())
			.details(json!({
				"action": "user_deleted_by_admin",
				"target_email": target_user.primary_email,
				"target_display_name": target_user.display_name,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(DeleteUserResponse {
			message: t(locale, "server.api.admin.user_deleted").to_string(),
			user_id,
		}),
	)
		.into_response()
}

/// Get current impersonation state.
///
/// Returns the current impersonation state for the authenticated admin user.
/// If the admin is currently impersonating another user, returns details about
/// both the original admin and the impersonated user.
///
/// Requires `system_admin` role.
#[utoipa::path(
    get,
    path = "/api/admin/impersonate/state",
    responses(
        (status = 200, description = "Impersonation state", body = ImpersonationState),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 403, description = "Not authorized (system_admin required)", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(
    skip(state),
    fields(actor_id = %current_user.user.id)
)]
pub async fn get_impersonation_state(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(
			actor_id = %current_user.user.id,
			"Unauthorized impersonation state access attempt"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	match state
		.session_repo
		.get_active_impersonation_session(&current_user.user.id)
		.await
	{
		Ok(Some((_session_id, target_user_id))) => {
			match state.user_repo.get_user_by_id(&target_user_id).await {
				Ok(Some(target_user)) => Json(ImpersonationState {
					is_impersonating: true,
					original_user: Some(ImpersonationUserInfo {
						id: current_user.user.id.to_string(),
						display_name: current_user.user.display_name.clone(),
					}),
					impersonated_user: Some(ImpersonationUserInfo {
						id: target_user.id.to_string(),
						display_name: target_user.display_name,
					}),
				})
				.into_response(),
				Ok(None) => {
					tracing::warn!(
						actor_id = %current_user.user.id,
						target_id = %target_user_id,
						"Impersonation session refers to missing user"
					);
					Json(ImpersonationState {
						is_impersonating: false,
						original_user: None,
						impersonated_user: None,
					})
					.into_response()
				}
				Err(e) => {
					tracing::error!(error = %e, "Failed to fetch impersonated user");
					(
						StatusCode::INTERNAL_SERVER_ERROR,
						Json(AdminErrorResponse {
							error: "internal_error".to_string(),
							message: t(locale, "server.api.error.internal").to_string(),
						}),
					)
						.into_response()
				}
			}
		}
		Ok(None) => Json(ImpersonationState {
			is_impersonating: false,
			original_user: None,
			impersonated_user: None,
		})
		.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to check impersonation state");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

/// Start impersonating a user.
///
/// # Authorization
///
/// Requires `system_admin` role. Returns 403 Forbidden otherwise.
///
/// # Request
///
/// Path parameters:
/// - `id`: Target user's UUID to impersonate
///
/// Body ([`ImpersonateRequest`]):
/// - `reason`: Required justification for impersonation (for audit)
///
/// # Response
///
/// Returns [`ImpersonateResponse`] with new session ID.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid user ID or attempting to impersonate self
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `system_admin` role
/// - `404 Not Found`: Target user does not exist
/// - `409 Conflict`: Already impersonating another user
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Creates an auditable impersonation session
/// - Original admin identity is preserved in audit logs
/// - Admin cannot impersonate themselves
/// - Must stop current impersonation before starting new one
#[utoipa::path(
    post,
    path = "/api/admin/users/{id}/impersonate",
    params(
        ("id" = String, Path, description = "User ID to impersonate")
    ),
    request_body = ImpersonateRequest,
    responses(
        (status = 200, description = "Impersonation started", body = ImpersonateResponse),
        (status = 400, description = "Invalid request", body = AdminErrorResponse),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 403, description = "Not authorized (system_admin required)", body = AdminErrorResponse),
        (status = 404, description = "User not found", body = AdminErrorResponse),
        (status = 409, description = "Already impersonating", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state, payload),
	fields(
		actor_id = %current_user.user.id,
		target_id = %user_id
	)
)]
pub async fn start_impersonation(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(user_id): Path<String>,
	Json(payload): Json<ImpersonateRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(
			actor_id = %current_user.user.id,
			target_id = %user_id,
			"Unauthorized impersonation attempt"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let target_user_id = match parse_user_id(&user_id, locale) {
		Ok(id) => id,
		Err(e) => return (StatusCode::BAD_REQUEST, Json(e)).into_response(),
	};

	if current_user.user.id == target_user_id {
		return (
			StatusCode::BAD_REQUEST,
			Json(AdminErrorResponse {
				error: "bad_request".to_string(),
				message: "Cannot impersonate yourself".to_string(),
			}),
		)
			.into_response();
	}

	if let Ok(Some(_)) = state
		.session_repo
		.get_active_impersonation_session(&current_user.user.id)
		.await
	{
		return (
			StatusCode::CONFLICT,
			Json(AdminErrorResponse {
				error: "conflict".to_string(),
				message: "Already impersonating another user. Stop current impersonation first."
					.to_string(),
			}),
		)
			.into_response();
	}

	let target_user = match state.user_repo.get_user_by_id(&target_user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(AdminErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.user.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				target_id = %target_user_id,
				"Failed to get user"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let session_id = match state
		.session_repo
		.create_impersonation_session(&current_user.user.id, &target_user_id, &payload.reason)
		.await
	{
		Ok(id) => id,
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				target_id = %target_user_id,
				"Failed to create impersonation session"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	tracing::info!(
		actor_id = %current_user.user.id,
		target_id = %target_user_id,
		session_id = %session_id,
		"Admin started impersonation"
	);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::ImpersonationStarted)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.severity(AuditSeverity::Warning)
			.resource("user", target_user.id.to_string())
			.details(json!({
				"reason": payload.reason,
				"session_id": session_id,
				"target_user_id": target_user_id.to_string(),
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(ImpersonateResponse {
			session_id,
			message: format!("Now impersonating user {}", target_user.display_name),
		}),
	)
		.into_response()
}

/// Stop impersonating a user.
///
/// # Authorization
///
/// Requires authentication. Any authenticated user with an active impersonation
/// session can stop it.
///
/// # Response
///
/// Returns [`AdminSuccessResponse`] confirming impersonation ended.
///
/// # Errors
///
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `404 Not Found`: No active impersonation session
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Ends the impersonation session and restores admin identity
/// - Session termination is logged to audit trail
#[utoipa::path(
    post,
    path = "/api/admin/impersonate/stop",
    responses(
        (status = 200, description = "Impersonation stopped", body = AdminSuccessResponse),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 404, description = "Not currently impersonating", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn stop_impersonation(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let (session_id, target_user_id) = match state
		.session_repo
		.get_active_impersonation_session(&current_user.user.id)
		.await
	{
		Ok(Some((id, target_id))) => (id, target_id),
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(AdminErrorResponse {
					error: "not_found".to_string(),
					message: "Not currently impersonating any user".to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				"Failed to get impersonation session"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Err(e) = state
		.session_repo
		.end_impersonation_session(&session_id)
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			session_id = %session_id,
			"Failed to end impersonation session"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(AdminErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		target_id = %target_user_id,
		session_id = %session_id,
		"Admin stopped impersonation"
	);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::ImpersonationEnded)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.severity(AuditSeverity::Warning)
			.resource("user", target_user_id.to_string())
			.details(json!({
				"session_id": session_id,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(AdminSuccessResponse {
			message: "Impersonation ended".to_string(),
		}),
	)
		.into_response()
}

/// Query audit logs.
///
/// # Authorization
///
/// Requires `system_admin` or `auditor` role. Returns 403 Forbidden otherwise.
///
/// # Request
///
/// Query parameters:
/// - `event_type` (optional): Filter by event type
/// - `actor_id` (optional): Filter by actor user ID
/// - `resource_type` (optional): Filter by resource type
/// - `resource_id` (optional): Filter by resource ID
/// - `from` (optional): Start of time range (ISO 8601)
/// - `to` (optional): End of time range (ISO 8601)
/// - `limit` (optional): Maximum logs to return (default: 50)
/// - `offset` (optional): Pagination offset (default: 0)
///
/// # Response
///
/// Returns [`ListAuditLogsResponse`] with paginated audit log entries.
///
/// # Errors
///
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks required role
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Audit logs contain sensitive operational data
/// - Access is restricted to admins and auditors only
#[utoipa::path(
    get,
    path = "/api/admin/audit-logs",
    params(ListAuditLogsParams),
    responses(
        (status = 200, description = "List of audit logs", body = ListAuditLogsResponse),
        (status = 401, description = "Not authenticated", body = AdminErrorResponse),
        (status = 403, description = "Not authorized (system_admin or auditor required)", body = AdminErrorResponse)
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		event_type = ?params.event_type,
		filter_actor_id = ?params.actor_id,
		resource_type = ?params.resource_type,
		limit = params.limit,
		offset = params.offset
	)
)]
pub async fn list_audit_logs(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Query(params): Query<ListAuditLogsParams>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin && !current_user.user.is_auditor {
		tracing::warn!(
			actor_id = %current_user.user.id,
			"Unauthorized audit log access attempt"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.error.forbidden").to_string(),
			}),
		)
			.into_response();
	}

	let (logs, total) = match state
		.audit_repo
		.query_logs(
			params.event_type.as_deref(),
			params.actor_id.as_deref(),
			params.resource_type.as_deref(),
			params.resource_id.as_deref(),
			params.from,
			params.to,
			Some(params.limit.into()),
			Some(params.offset.into()),
		)
		.await
	{
		Ok(result) => result,
		Err(e) => {
			tracing::error!(error = %e, "Failed to query audit logs");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let logs: Vec<AuditLogEntryResponse> = logs
		.into_iter()
		.map(|l| AuditLogEntryResponse {
			id: l.id.to_string(),
			timestamp: l.timestamp,
			event_type: l.event_type.to_string(),
			actor_user_id: l.actor_user_id.map(|id| id.to_string()),
			impersonating_user_id: l.impersonating_user_id.map(|id| id.to_string()),
			resource_type: l.resource_type,
			resource_id: l.resource_id,
			action: l.action,
			ip_address: l.ip_address,
			user_agent: l.user_agent,
			details: if l.details == serde_json::Value::Null {
				None
			} else {
				Some(l.details)
			},
		})
		.collect();

	tracing::info!(
		actor_id = %current_user.user.id,
		log_count = logs.len(),
		total = total,
		"Audit logs queried"
	);

	(
		StatusCode::OK,
		Json(ListAuditLogsResponse {
			logs,
			total,
			limit: params.limit,
			offset: params.offset,
		}),
	)
		.into_response()
}
